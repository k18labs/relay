use std::{
    convert::Infallible,
    error::Error,
    future::Future,
    io,
    pin::Pin,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    task::{Context, Poll},
    time::Duration,
};

use bytes::{Buf, Bytes};
use http::{
    Request, Response, StatusCode, Uri,
    header::{self, HeaderName, HeaderValue},
};
use http_body::{Body, Frame};
use http_body_util::{BodyExt, Full, Limited, combinators::UnsyncBoxBody};
use hyper::{body::Incoming, service::service_fn};
use hyper_rustls::HttpsConnectorBuilder;
use hyper_util::{
    client::legacy::{Client, connect::HttpConnector},
    rt::{TokioExecutor, TokioIo, TokioTimer},
};
use rustls::{ClientConfig, ServerConfig};
use tokio::{
    io::{AsyncRead, AsyncWrite},
    net::TcpListener,
    task::JoinSet,
    time::{Instant, Sleep},
};
use tokio_rustls::TlsAcceptor;

use crate::{
    config::{Config, Route},
    tls,
};

type BoxError = Box<dyn Error + Send + Sync>;
type ProxyBody = UnsyncBoxBody<Bytes, BoxError>;
type UpstreamConnector = hyper_rustls::HttpsConnector<HttpConnector>;
type ProxyClient = Client<UpstreamConnector, ProxyBody>;

pub async fn serve<F>(
    listener: TcpListener,
    config: Arc<Config>,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    if config.server.tls {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "TLS is enabled; call serve_with_tls with validated TLS configuration",
        ));
    }
    let client_tls =
        tls::build_upstream_client_config(&config.tls).map_err(std::io::Error::other)?;
    run_server(listener, config, None, client_tls, shutdown).await
}

pub async fn serve_with_tls<F>(
    listener: TcpListener,
    config: Arc<Config>,
    server_tls: Option<Arc<ServerConfig>>,
    client_tls: Arc<ClientConfig>,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    if config.server.tls != server_tls.is_some() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "server TLS configuration does not match server.tls",
        ));
    }
    run_server(listener, config, server_tls, client_tls, shutdown).await
}

async fn run_server<F>(
    listener: TcpListener,
    config: Arc<Config>,
    server_tls: Option<Arc<ServerConfig>>,
    client_tls: Arc<ClientConfig>,
    shutdown: F,
) -> std::io::Result<()>
where
    F: Future<Output = ()> + Send + 'static,
{
    let mut connector = HttpConnector::new();
    connector.set_connect_timeout(Some(Duration::from_millis(
        config.limits.connect_timeout_ms,
    )));
    connector.enforce_http(false);
    let connector = HttpsConnectorBuilder::new()
        .with_tls_config((*client_tls).clone())
        .https_or_http()
        .enable_http1()
        .wrap_connector(connector);
    let mut client_builder = Client::builder(TokioExecutor::new());
    client_builder
        .pool_max_idle_per_host(config.limits.max_idle_upstream_per_host)
        .pool_idle_timeout(Duration::from_millis(
            config.limits.upstream_idle_timeout_ms,
        ))
        .http1_max_headers(config.limits.max_header_count)
        .http1_max_buf_size(config.limits.max_header_bytes.max(16 * 1024));
    let client: ProxyClient = client_builder.build(connector);
    let permits = Arc::new(tokio::sync::Semaphore::new(config.server.max_connections));
    let request_permits = Arc::new(tokio::sync::Semaphore::new(
        config.limits.max_concurrent_requests,
    ));
    let handshake_permits = Arc::new(tokio::sync::Semaphore::new(
        config.limits.max_concurrent_tls_handshakes,
    ));
    let tls_acceptor = server_tls.map(TlsAcceptor::from);
    let mut connections = JoinSet::new();
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => break,
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    tracing::debug!(error = %error, "connection task ended");
                }
            }
            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                let Ok(permit) = permits.clone().try_acquire_owned() else { drop(stream); continue; };
                let client = client.clone();
                let config = config.clone();
                let request_permits = request_permits.clone();
                let handshake_permits = handshake_permits.clone();
                let tls_acceptor = tls_acceptor.clone();
                connections.spawn(async move {
                    if let Some(acceptor) = tls_acceptor {
                        let Ok(handshake_permit) = handshake_permits.try_acquire_owned() else { return; };
                        let result = tokio::time::timeout(
                            Duration::from_millis(config.limits.handshake_timeout_ms),
                            acceptor.accept(stream),
                        ).await;
                        drop(handshake_permit);
                        match result {
                            Ok(Ok(stream)) => serve_http(stream, permit, config, client, request_permits).await,
                            Ok(Err(error)) => tracing::debug!(error = %error, "TLS handshake failed"),
                            Err(_) => tracing::debug!("TLS handshake timed out"),
                        }
                    } else {
                        serve_http(stream, permit, config, client, request_permits).await;
                    }
                });
            }
        }
    }

    let drain = async {
        while let Some(result) = connections.join_next().await {
            if let Err(error) = result {
                tracing::debug!(error = %error, "connection task ended during shutdown");
            }
        }
    };
    if tokio::time::timeout(
        Duration::from_millis(config.limits.shutdown_timeout_ms),
        drain,
    )
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    Ok(())
}

async fn serve_http<I>(
    stream: I,
    _connection_permit: tokio::sync::OwnedSemaphorePermit,
    config: Arc<Config>,
    client: ProxyClient,
    request_permits: Arc<tokio::sync::Semaphore>,
) where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(stream);
    let limits = config.limits.clone();
    let service_config = config.clone();
    let service = service_fn(move |request| {
        forward(
            request,
            service_config.clone(),
            client.clone(),
            request_permits.clone(),
        )
    });
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .max_headers(limits.max_header_count)
        .max_header_size(limits.max_header_bytes)
        .max_buf_size(limits.max_header_bytes.max(16 * 1024))
        .header_read_timeout(Duration::from_millis(limits.header_timeout_ms));
    if let Err(error) = builder.serve_connection(io, service).await {
        tracing::debug!(error = %error, "client connection ended");
    }
}

async fn forward(
    mut request: Request<Incoming>,
    config: Arc<Config>,
    client: ProxyClient,
    request_permits: Arc<tokio::sync::Semaphore>,
) -> Result<Response<ProxyBody>, Infallible> {
    if request.method() == http::Method::CONNECT || request.headers().contains_key(header::UPGRADE)
    {
        return Ok(simple_response(StatusCode::NOT_IMPLEMENTED, true));
    }
    if request.uri().scheme().is_some() || request.uri().authority().is_some() {
        return Ok(simple_response(StatusCode::BAD_REQUEST, true));
    }
    let hosts: Vec<_> = request.headers().get_all(header::HOST).iter().collect();
    if hosts.len() != 1 {
        return Ok(simple_response(StatusCode::BAD_REQUEST, true));
    }
    let Ok(host_value) = hosts[0].to_str() else {
        return Ok(simple_response(StatusCode::BAD_REQUEST, true));
    };
    let host = match host_value.parse::<http::uri::Authority>() {
        Ok(authority) => authority.host().to_owned(),
        Err(_) => return Ok(simple_response(StatusCode::BAD_REQUEST, true)),
    };
    let Some(route) = config.route(&host, request.uri().path()) else {
        return Ok(simple_response(StatusCode::MISDIRECTED_REQUEST, true));
    };
    if request
        .body()
        .size_hint()
        .upper()
        .is_some_and(|size| size > config.limits.max_body_bytes as u64)
    {
        return Ok(simple_response(StatusCode::PAYLOAD_TOO_LARGE, true));
    }
    let Ok(request_permit) = request_permits.try_acquire_owned() else {
        return Ok(simple_response(StatusCode::SERVICE_UNAVAILABLE, true));
    };

    let upstream = match upstream_uri(route, &request) {
        Ok(uri) => uri,
        Err(()) => return Ok(simple_response(StatusCode::BAD_REQUEST, true)),
    };
    strip_hop_headers(request.headers_mut());
    for name in [
        "forwarded",
        "x-real-ip",
        "x-client-ip",
        "x-cluster-client-ip",
        "true-client-ip",
        "x-original-forwarded-for",
    ] {
        request.headers_mut().remove(name);
    }
    let forwarded: Vec<_> = request
        .headers()
        .keys()
        .filter(|name| name.as_str().starts_with("x-forwarded-"))
        .cloned()
        .collect();
    for name in forwarded {
        request.headers_mut().remove(name);
    }
    let Ok(authority) = route.upstream.parse::<Uri>() else {
        return Ok(simple_response(StatusCode::BAD_GATEWAY, true));
    };
    let Some(authority) = authority.authority() else {
        return Ok(simple_response(StatusCode::BAD_GATEWAY, true));
    };
    let Ok(value) = HeaderValue::from_str(authority.as_str()) else {
        return Ok(simple_response(StatusCode::BAD_GATEWAY, true));
    };
    request.headers_mut().insert(header::HOST, value);
    *request.uri_mut() = upstream;

    let body_limit_hit = Arc::new(AtomicBool::new(false));
    let body_limit_for_error = body_limit_hit.clone();
    let body_timeout_hit = Arc::new(AtomicBool::new(false));
    let body_timeout_for_error = body_timeout_hit.clone();
    let (parts, body) = request.into_parts();
    let limited = Limited::new(body, config.limits.max_body_bytes).map_err(move |error| {
        if error
            .downcast_ref::<http_body_util::LengthLimitError>()
            .is_some()
        {
            body_limit_for_error.store(true, Ordering::Relaxed);
        }
        error
    });
    let body = DeadlineBody::new(
        limited,
        Duration::from_millis(config.limits.body_idle_timeout_ms),
        Some(body_timeout_for_error),
        None,
    )
    .boxed_unsync();
    let request = Request::from_parts(parts, body);
    let response = tokio::time::timeout(
        Duration::from_millis(config.limits.response_timeout_ms),
        client.request(request),
    )
    .await;

    match response {
        Ok(Ok(response)) => {
            if response
                .body()
                .size_hint()
                .upper()
                .is_some_and(|size| size > config.limits.max_body_bytes as u64)
            {
                return Ok(simple_response(StatusCode::BAD_GATEWAY, true));
            }
            let (mut parts, body) = response.into_parts();
            strip_hop_headers(&mut parts.headers);
            let limited = Limited::new(body, config.limits.max_body_bytes);
            let body = DeadlineBody::new(
                limited,
                Duration::from_millis(config.limits.body_idle_timeout_ms),
                None,
                Some(request_permit),
            )
            .boxed_unsync();
            Ok(Response::from_parts(parts, body))
        }
        Ok(Err(error)) => {
            if body_limit_hit.load(Ordering::Relaxed) {
                Ok(simple_response(StatusCode::PAYLOAD_TOO_LARGE, true))
            } else if body_timeout_hit.load(Ordering::Relaxed) {
                Ok(simple_response(StatusCode::REQUEST_TIMEOUT, true))
            } else {
                tracing::debug!(error = %error, source = ?error.source(), "upstream request failed");
                Ok(simple_response(StatusCode::BAD_GATEWAY, true))
            }
        }
        Err(_) => Ok(simple_response(StatusCode::GATEWAY_TIMEOUT, true)),
    }
}

struct DeadlineBody<B> {
    inner: Pin<Box<B>>,
    timeout: Duration,
    sleep: Pin<Box<Sleep>>,
    timeout_hit: Option<Arc<AtomicBool>>,
    _permit: Option<tokio::sync::OwnedSemaphorePermit>,
}

impl<B> DeadlineBody<B> {
    fn new(
        inner: B,
        timeout: Duration,
        timeout_hit: Option<Arc<AtomicBool>>,
        permit: Option<tokio::sync::OwnedSemaphorePermit>,
    ) -> Self {
        Self {
            inner: Box::pin(inner),
            timeout,
            sleep: Box::pin(tokio::time::sleep(timeout)),
            timeout_hit,
            _permit: permit,
        }
    }
}

impl<B> Unpin for DeadlineBody<B> {}

impl<B> Body for DeadlineBody<B>
where
    B: Body + 'static,
    B::Data: Buf,
    B::Error: Into<BoxError>,
{
    type Data = B::Data;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match this.inner.as_mut().poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                this.sleep.as_mut().reset(Instant::now() + this.timeout);
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(Some(Err(error))) => Poll::Ready(Some(Err(error.into()))),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => match this.sleep.as_mut().poll(cx) {
                Poll::Ready(()) => {
                    if let Some(flag) = &this.timeout_hit {
                        flag.store(true, Ordering::Relaxed);
                    }
                    Poll::Ready(Some(Err(Box::new(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "body idle timeout",
                    )))))
                }
                Poll::Pending => Poll::Pending,
            },
        }
    }
}

fn upstream_uri(route: &Route, request: &Request<Incoming>) -> Result<Uri, ()> {
    let origin = route.upstream.parse::<Uri>().map_err(|_| ())?;
    let scheme = origin.scheme_str().ok_or(())?;
    let authority = origin.authority().ok_or(())?;
    let path = request.uri().path_and_query().map_or("/", |pq| pq.as_str());
    format!("{scheme}://{authority}{path}")
        .parse()
        .map_err(|_| ())
}

fn simple_response(status: StatusCode, close: bool) -> Response<ProxyBody> {
    let body = Full::new(Bytes::new())
        .map_err(|never: Infallible| -> BoxError { match never {} })
        .boxed_unsync();
    let mut response = Response::builder()
        .status(status)
        .body(body)
        .expect("static response is valid");
    if close {
        response
            .headers_mut()
            .insert(header::CONNECTION, HeaderValue::from_static("close"));
    }
    response
}

fn strip_hop_headers(headers: &mut http::HeaderMap) {
    let mut nominated = Vec::new();
    for value in headers.get_all(header::CONNECTION).iter() {
        if let Ok(value) = value.to_str() {
            nominated.extend(
                value
                    .split(',')
                    .filter_map(|part| part.trim().parse::<HeaderName>().ok()),
            );
        }
    }
    for name in [
        header::CONNECTION,
        HeaderName::from_static("keep-alive"),
        header::PROXY_AUTHENTICATE,
        header::PROXY_AUTHORIZATION,
        header::TE,
        header::TRAILER,
        header::TRANSFER_ENCODING,
        header::UPGRADE,
    ] {
        headers.remove(name);
    }
    for name in nominated {
        headers.remove(name);
    }
}
