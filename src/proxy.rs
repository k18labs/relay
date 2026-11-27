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
use hyper::service::service_fn;
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
use tower_service::Service;

use crate::{
    config::{self, Config, Route},
    tls,
};

type BoxError = Box<dyn Error + Send + Sync>;
type ProxyBody = UnsyncBoxBody<Bytes, BoxError>;
type UpstreamConnector = BoundedConnector<hyper_rustls::HttpsConnector<HttpConnector>>;
type ProxyClient = Client<UpstreamConnector, ProxyBody>;

#[derive(Clone)]
struct ProxyState {
    config: Arc<Config>,
    client: ProxyClient,
    request_permits: Arc<tokio::sync::Semaphore>,
    connection_permits: Arc<tokio::sync::Semaphore>,
    handshake_permits: Arc<tokio::sync::Semaphore>,
    shutting_down: Arc<AtomicBool>,
    http3_port: Option<u16>,
}

#[derive(Clone)]
struct BoundedConnector<C> {
    inner: C,
    timeout: Duration,
    permits: Arc<tokio::sync::Semaphore>,
}

impl<C> BoundedConnector<C> {
    fn new(inner: C, timeout: Duration, max_in_flight: usize) -> Self {
        Self {
            inner,
            timeout,
            permits: Arc::new(tokio::sync::Semaphore::new(max_in_flight)),
        }
    }
}

impl<C> Service<Uri> for BoundedConnector<C>
where
    C: Service<Uri, Error = BoxError> + Send + Sync + 'static,
    C::Future: Send + 'static,
    C::Response: Send + 'static,
{
    type Response = C::Response;
    type Error = io::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, Self::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx).map_err(io::Error::other)
    }

    fn call(&mut self, destination: Uri) -> Self::Future {
        let future = self.inner.call(destination);
        let deadline = Instant::now() + self.timeout;
        let permits = self.permits.clone();
        Box::pin(async move {
            let _permit = permits.try_acquire_owned().map_err(io::Error::other)?;
            tokio::time::timeout_at(deadline, future)
                .await
                .map_err(|_| {
                    io::Error::new(io::ErrorKind::TimedOut, "upstream connection timed out")
                })?
                .map_err(io::Error::other)
        })
    }
}

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
    run_server(listener, config, None, client_tls, None, shutdown).await
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
    let http3_endpoint = server_tls
        .as_ref()
        .map(|tls| build_http3_endpoint(listener.local_addr()?, tls, &config))
        .transpose()?;
    run_server(
        listener,
        config,
        server_tls,
        client_tls,
        http3_endpoint,
        shutdown,
    )
    .await
}

fn build_http3_endpoint(
    address: std::net::SocketAddr,
    server_tls: &ServerConfig,
    config: &Config,
) -> io::Result<quinn::Endpoint> {
    let mut tls = server_tls.clone();
    tls.alpn_protocols = vec![b"h3".to_vec()];
    tls.max_early_data_size = 0;
    let crypto = quinn::crypto::rustls::QuicServerConfig::try_from(Arc::new(tls))
        .map_err(io::Error::other)?;
    let mut server = quinn::ServerConfig::with_crypto(Arc::new(crypto));
    let mut transport = quinn::TransportConfig::default();
    transport
        .max_concurrent_bidi_streams(quinn::VarInt::from_u32(
            config.limits.max_concurrent_streams_per_connection as u32,
        ))
        .max_concurrent_uni_streams(quinn::VarInt::from_u32(3))
        .stream_receive_window(quinn::VarInt::from_u32(256 * 1024))
        .receive_window(quinn::VarInt::from_u32(4 * 1024 * 1024))
        .send_window(4 * 1024 * 1024);
    server
        .max_incoming(config.server.max_connections.min(256))
        .incoming_buffer_size(64 * 1024)
        .incoming_buffer_size_total(16 * 1024 * 1024)
        .transport_config(Arc::new(transport));
    quinn::Endpoint::server(server, address)
}

async fn accept_with_retry<F, Fut, T>(mut accept: F) -> std::io::Result<T>
where
    F: FnMut() -> Fut,
    Fut: Future<Output = std::io::Result<T>>,
{
    loop {
        match accept().await {
            Ok(value) => return Ok(value),
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::InvalidInput
                        | std::io::ErrorKind::Unsupported
                        | std::io::ErrorKind::NotConnected
                ) =>
            {
                return Err(error);
            }
            Err(error) => {
                tracing::warn!(error = %error, "listener accept failed; retrying");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
        }
    }
}

async fn run_server<F>(
    listener: TcpListener,
    config: Arc<Config>,
    server_tls: Option<Arc<ServerConfig>>,
    client_tls: Arc<ClientConfig>,
    http3_endpoint: Option<quinn::Endpoint>,
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
    let connector = BoundedConnector::new(
        connector,
        Duration::from_millis(config.limits.connect_timeout_ms),
        config.limits.max_concurrent_requests,
    );
    let mut client_builder = Client::builder(TokioExecutor::new());
    client_builder
        .pool_timer(TokioTimer::new())
        .pool_max_idle_per_host(config.limits.max_idle_upstream_per_host)
        .pool_idle_timeout(Duration::from_millis(
            config.limits.upstream_idle_timeout_ms,
        ))
        .retry_canceled_requests(false)
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
    let (connection_shutdown_tx, connection_shutdown_rx) = tokio::sync::watch::channel(false);
    let shutting_down = Arc::new(AtomicBool::new(false));
    let http3_port = http3_endpoint
        .as_ref()
        .map(quinn::Endpoint::local_addr)
        .transpose()?
        .map(|address| address.port());
    let state = ProxyState {
        config: config.clone(),
        client: client.clone(),
        request_permits: request_permits.clone(),
        connection_permits: permits.clone(),
        handshake_permits: handshake_permits.clone(),
        shutting_down: shutting_down.clone(),
        http3_port,
    };
    if let Some(endpoint) = http3_endpoint {
        let shutdown = connection_shutdown_rx.clone();
        let state = state.clone();
        connections.spawn(async move {
            serve_http3_listener(endpoint, state, shutdown).await;
        });
    }
    tokio::pin!(shutdown);

    loop {
        tokio::select! {
            _ = &mut shutdown => {
                shutting_down.store(true, Ordering::Release);
                connection_shutdown_tx.send_replace(true);
                break;
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    tracing::debug!(error = %error, "connection task ended");
                }
            }
            accepted = accept_with_retry(|| listener.accept()) => {
                let (stream, _) = accepted?;
                let Ok(permit) = permits.clone().try_acquire_owned() else { drop(stream); continue; };
                let state = state.clone();
                let tls_acceptor = tls_acceptor.clone();
                let connection_shutdown = connection_shutdown_rx.clone();
                connections.spawn(async move {
                    if let Some(acceptor) = tls_acceptor {
                        let Ok(handshake_permit) = state.handshake_permits.clone().try_acquire_owned() else { return; };
                        let result = tokio::time::timeout(
                            Duration::from_millis(state.config.limits.handshake_timeout_ms),
                            acceptor.accept(stream),
                        ).await;
                        drop(handshake_permit);
                        match result {
                            Ok(Ok(stream)) => {
                                if stream.get_ref().1.alpn_protocol() == Some(b"h2") {
                                    serve_http2(stream, permit, state, connection_shutdown).await;
                                } else {
                                    serve_http(stream, permit, state, connection_shutdown).await;
                                }
                            },
                            Ok(Err(error)) => tracing::debug!(error = %error, "TLS handshake failed"),
                            Err(_) => tracing::debug!("TLS handshake timed out"),
                        }
                    } else {
                        serve_http(stream, permit, state, connection_shutdown).await;
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
    state: ProxyState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(WriteTimeoutIo::new(
        stream,
        Duration::from_millis(state.config.limits.downstream_write_timeout_ms),
    ));
    let service_state = state.clone();
    let service = service_fn(move |request| forward(request, service_state.clone()));
    let limits = state.config.limits.clone();
    let mut builder = hyper::server::conn::http1::Builder::new();
    builder
        .timer(TokioTimer::new())
        .max_headers(limits.max_header_count)
        .max_header_size(limits.max_header_bytes)
        .max_buf_size(limits.max_header_bytes.max(16 * 1024))
        .header_read_timeout(Duration::from_millis(limits.header_timeout_ms));
    let connection = builder.serve_connection(io, service);
    tokio::pin!(connection);
    tokio::select! {
        result = &mut connection => {
            if let Err(error) = result {
                tracing::debug!(error = %error, "client connection ended");
            }
        }
        result = shutdown.changed() => {
            if result.is_ok() && *shutdown.borrow() {
                connection.as_mut().graceful_shutdown();
            }
            if let Err(error) = connection.await {
                tracing::debug!(error = %error, "client connection ended during graceful shutdown");
            }
        }
    }
}

async fn serve_http2<I>(
    stream: I,
    _connection_permit: tokio::sync::OwnedSemaphorePermit,
    state: ProxyState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    let io = TokioIo::new(WriteTimeoutIo::new(
        stream,
        Duration::from_millis(state.config.limits.downstream_write_timeout_ms),
    ));
    let service_state = state.clone();
    let service = service_fn(move |request| forward(request, service_state.clone()));
    let mut builder = hyper::server::conn::http2::Builder::new(TokioExecutor::new());
    builder
        .timer(TokioTimer::new())
        .max_concurrent_streams(Some(
            state.config.limits.max_concurrent_streams_per_connection as u32,
        ))
        .max_header_list_size(state.config.limits.max_header_bytes as u32)
        .max_pending_accept_reset_streams(Some(16))
        .max_send_buf_size(16 * 1024);
    let connection = builder.serve_connection(io, service);
    tokio::pin!(connection);
    tokio::select! {
        result = &mut connection => {
            if let Err(error) = result {
                tracing::debug!(error = %error, "HTTP/2 connection ended");
            }
        }
        result = shutdown.changed() => {
            if result.is_ok() && *shutdown.borrow() {
                connection.as_mut().graceful_shutdown();
            }
            if let Err(error) = connection.await {
                tracing::debug!(error = %error, "HTTP/2 connection ended during graceful shutdown");
            }
        }
    }
}

async fn serve_http3_listener(
    endpoint: quinn::Endpoint,
    state: ProxyState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut connections = JoinSet::new();
    loop {
        tokio::select! {
            result = shutdown.changed() => {
                if result.is_ok() && *shutdown.borrow() {
                    break;
                }
            }
            Some(result) = connections.join_next(), if !connections.is_empty() => {
                if let Err(error) = result {
                    tracing::debug!(error = %error, "HTTP/3 connection task ended");
                }
            }
            incoming = endpoint.accept() => {
                let Some(incoming) = incoming else { break; };
                let Ok(connection_permit) = state.connection_permits.clone().try_acquire_owned() else {
                    drop(incoming);
                    continue;
                };
                let Ok(handshake_permit) = state.handshake_permits.clone().try_acquire_owned() else {
                    drop(incoming);
                    drop(connection_permit);
                    continue;
                };
                let state = state.clone();
                let connection_shutdown = shutdown.clone();
                connections.spawn(async move {
                    let result = tokio::time::timeout(
                        Duration::from_millis(state.config.limits.handshake_timeout_ms),
                        incoming,
                    ).await;
                    drop(handshake_permit);
                    match result {
                        Ok(Ok(connection)) => {
                            serve_http3_connection(
                                connection,
                                connection_permit,
                                state,
                                connection_shutdown,
                            ).await;
                        }
                        Ok(Err(error)) => tracing::debug!(error = %error, "HTTP/3 handshake failed"),
                        Err(_) => tracing::debug!("HTTP/3 handshake timed out"),
                    }
                });
            }
        }
    }

    let drain = async {
        while let Some(result) = connections.join_next().await {
            if let Err(error) = result {
                tracing::debug!(error = %error, "HTTP/3 connection task ended during shutdown");
            }
        }
    };
    if tokio::time::timeout(
        Duration::from_millis(state.config.limits.shutdown_timeout_ms),
        drain,
    )
    .await
    .is_err()
    {
        connections.abort_all();
        while connections.join_next().await.is_some() {}
    }
    endpoint.close(0u32.into(), b"server shutdown");
}

async fn serve_http3_connection(
    connection: quinn::Connection,
    _connection_permit: tokio::sync::OwnedSemaphorePermit,
    state: ProxyState,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut builder = h3::server::builder();
    builder.max_field_section_size(state.config.limits.max_header_bytes as u64);
    let Ok(mut connection) = builder
        .build::<_, Bytes>(h3_quinn::Connection::new(connection))
        .await
    else {
        return;
    };
    let mut streams = JoinSet::new();
    loop {
        tokio::select! {
            result = shutdown.changed() => {
                if result.is_ok() && *shutdown.borrow() {
                    let _ = connection.shutdown(0).await;
                    break;
                }
            }
            Some(result) = streams.join_next(), if !streams.is_empty() => {
                if let Err(error) = result {
                    tracing::debug!(error = %error, "HTTP/3 request task ended");
                }
            }
            request = connection.accept() => match request {
                Ok(Some(resolver)) => {
                    let state = state.clone();
                    streams.spawn(async move {
                        let Ok(Ok((request, stream))) = tokio::time::timeout(
                            Duration::from_millis(state.config.limits.header_timeout_ms),
                            resolver.resolve_request(),
                        ).await else { return; };
                        let (send_stream, receive_stream) = stream.split();
                        let request = request.map(|()| H3RequestBody::new(receive_stream));
                        handle_http3_request(
                            request,
                            send_stream,
                            state,
                        ).await;
                    });
                }
                Ok(None) | Err(_) => break,
            }
        }
    }

    let drain = async {
        while let Some(result) = streams.join_next().await {
            if let Err(error) = result {
                tracing::debug!(error = %error, "HTTP/3 request task ended during shutdown");
            }
        }
    };
    if tokio::time::timeout(
        Duration::from_millis(state.config.limits.shutdown_timeout_ms),
        drain,
    )
    .await
    .is_err()
    {
        streams.abort_all();
        while streams.join_next().await.is_some() {}
    }
}

struct H3RequestBody<S> {
    stream: h3::server::RequestStream<S, Bytes>,
    trailers: bool,
    done: bool,
}

async fn handle_http3_request<S, B>(
    request: Request<B>,
    mut stream: h3::server::RequestStream<S, Bytes>,
    state: ProxyState,
) where
    S: h3::quic::SendStream<Bytes> + Send + 'static,
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    let write_timeout = Duration::from_millis(state.config.limits.downstream_write_timeout_ms);
    let response = match forward(request, state).await {
        Ok(response) => response,
        Err(never) => match never {},
    };
    let (parts, mut body) = response.into_parts();
    match tokio::time::timeout(
        write_timeout,
        stream.send_response(Response::from_parts(parts, ())),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            tracing::debug!(error = %error, "HTTP/3 response headers failed");
            return;
        }
        Err(_) => {
            tracing::debug!("HTTP/3 response headers timed out");
            return;
        }
    }
    while let Some(frame) = body.frame().await {
        match frame {
            Ok(frame) => match frame.into_data() {
                Ok(data) => {
                    match tokio::time::timeout(write_timeout, stream.send_data(data)).await {
                        Ok(Ok(())) => {}
                        Ok(Err(error)) => {
                            tracing::debug!(error = %error, "HTTP/3 response data failed");
                            return;
                        }
                        Err(_) => {
                            tracing::debug!("HTTP/3 response data timed out");
                            return;
                        }
                    }
                }
                Err(frame) => {
                    if let Ok(trailers) = frame.into_trailers() {
                        match tokio::time::timeout(write_timeout, stream.send_trailers(trailers))
                            .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                tracing::debug!(error = %error, "HTTP/3 response trailers failed");
                                return;
                            }
                            Err(_) => {
                                tracing::debug!("HTTP/3 response trailers timed out");
                                return;
                            }
                        }
                    }
                }
            },
            Err(_) => {
                stream.stop_stream(h3::error::Code::H3_INTERNAL_ERROR);
                return;
            }
        }
    }
    match tokio::time::timeout(write_timeout, stream.finish()).await {
        Ok(Ok(())) => {}
        Ok(Err(error)) => tracing::debug!(error = %error, "HTTP/3 response stream finish failed"),
        Err(_) => tracing::debug!("HTTP/3 response stream finish timed out"),
    }
}

impl<S> H3RequestBody<S> {
    fn new(stream: h3::server::RequestStream<S, Bytes>) -> Self {
        Self {
            stream,
            trailers: false,
            done: false,
        }
    }
}

impl<S> Unpin for H3RequestBody<S> {}

impl<S> Body for H3RequestBody<S>
where
    S: h3::quic::RecvStream + Send + 'static,
{
    type Data = Bytes;
    type Error = BoxError;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        if this.done {
            return Poll::Ready(None);
        }
        if !this.trailers {
            match this.stream.poll_recv_data(cx) {
                Poll::Ready(Ok(Some(mut data))) => {
                    let bytes = data.copy_to_bytes(data.remaining());
                    return Poll::Ready(Some(Ok(Frame::data(bytes))));
                }
                Poll::Ready(Ok(None)) => this.trailers = true,
                Poll::Ready(Err(error)) => {
                    this.done = true;
                    return Poll::Ready(Some(Err(Box::new(error))));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
        match this.stream.poll_recv_trailers(cx) {
            Poll::Ready(Ok(Some(trailers))) => {
                this.done = true;
                Poll::Ready(Some(Ok(Frame::trailers(trailers))))
            }
            Poll::Ready(Ok(None)) => {
                this.done = true;
                Poll::Ready(None)
            }
            Poll::Ready(Err(error)) => {
                this.done = true;
                Poll::Ready(Some(Err(Box::new(error))))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

struct WriteTimeoutIo<T> {
    inner: T,
    timeout: Duration,
    deadline: Option<Pin<Box<Sleep>>>,
}

impl<T> WriteTimeoutIo<T> {
    fn new(inner: T, timeout: Duration) -> Self {
        Self {
            inner,
            timeout,
            deadline: None,
        }
    }

    fn poll_write_timeout(&mut self, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let deadline = self
            .deadline
            .get_or_insert_with(|| Box::pin(tokio::time::sleep(self.timeout)));
        if deadline.as_mut().poll(cx).is_ready() {
            Poll::Ready(Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "downstream write made no progress",
            )))
        } else {
            Poll::Pending
        }
    }
}

impl<T: AsyncRead + Unpin> AsyncRead for WriteTimeoutIo<T> {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.inner).poll_read(cx, buffer)
    }
}

impl<T: AsyncWrite + Unpin> AsyncWrite for WriteTimeoutIo<T> {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_write(cx, buffer) {
            Poll::Ready(Ok(written)) => {
                if written > 0 {
                    self.deadline = None;
                }
                Poll::Ready(Ok(written))
            }
            Poll::Ready(Err(error)) => {
                self.deadline = None;
                Poll::Ready(Err(error))
            }
            Poll::Pending => self.poll_write_timeout(cx).map(|result| result.map(|()| 0)),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_flush(cx) {
            Poll::Ready(Ok(())) => {
                self.deadline = None;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => {
                self.deadline = None;
                Poll::Ready(Err(error))
            }
            Poll::Pending => self.poll_write_timeout(cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match Pin::new(&mut self.inner).poll_shutdown(cx) {
            Poll::Ready(Ok(())) => {
                self.deadline = None;
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(error)) => {
                self.deadline = None;
                Poll::Ready(Err(error))
            }
            Poll::Pending => self.poll_write_timeout(cx),
        }
    }
}

async fn forward<B>(
    mut request: Request<B>,
    state: ProxyState,
) -> Result<Response<ProxyBody>, Infallible>
where
    B: Body<Data = Bytes> + Send + 'static,
    B::Error: Into<BoxError>,
{
    if state.shutting_down.load(Ordering::Acquire) {
        return Ok(simple_response(StatusCode::SERVICE_UNAVAILABLE, true));
    }
    let config = &state.config;
    if request.method() == http::Method::CONNECT || request.headers().contains_key(header::UPGRADE)
    {
        return Ok(simple_response(StatusCode::NOT_IMPLEMENTED, true));
    }
    if !request.uri().path().starts_with('/') {
        return Ok(simple_response(StatusCode::BAD_REQUEST, true));
    }
    let hosts: Vec<_> = request.headers().get_all(header::HOST).iter().collect();
    if request.headers().len() > config.limits.max_header_count {
        return Ok(simple_response(
            StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            true,
        ));
    }
    let authority = match request.version() {
        http::Version::HTTP_2 | http::Version::HTTP_3 => {
            if request.uri().scheme_str() != Some("https") {
                return Ok(simple_response(StatusCode::BAD_REQUEST, true));
            }
            let Some(authority) = request.uri().authority() else {
                return Ok(simple_response(StatusCode::BAD_REQUEST, true));
            };
            if hosts.len() > 1
                || hosts.first().is_some_and(|host| {
                    host.to_str()
                        .ok()
                        .and_then(|host| host.parse::<http::uri::Authority>().ok())
                        .is_none_or(|host| !same_authority(&host, authority, 443))
                })
            {
                return Ok(simple_response(StatusCode::BAD_REQUEST, true));
            }
            authority.clone()
        }
        _ => {
            if request.uri().scheme().is_some()
                || request.uri().authority().is_some()
                || hosts.len() != 1
            {
                return Ok(simple_response(StatusCode::BAD_REQUEST, true));
            }
            let Ok(host_value) = hosts[0].to_str() else {
                return Ok(simple_response(StatusCode::BAD_REQUEST, true));
            };
            let Ok(authority) = host_value.parse::<http::uri::Authority>() else {
                return Ok(simple_response(StatusCode::BAD_REQUEST, true));
            };
            authority
        }
    };
    if authority.as_str().contains('@') || !config::authority_port_is_valid(&authority) {
        return Ok(simple_response(StatusCode::BAD_REQUEST, true));
    }
    let host = authority.host().to_owned();
    let transfer_encoding = request.headers().get_all(header::TRANSFER_ENCODING);
    if transfer_encoding.iter().count() > 1
        || transfer_encoding
            .iter()
            .next()
            .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"chunked"))
    {
        return Ok(simple_response(StatusCode::BAD_REQUEST, true));
    }
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
    let Ok(request_permit) = state.request_permits.clone().try_acquire_owned() else {
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
    *request.version_mut() = http::Version::HTTP_11;

    let body_limit_hit = Arc::new(AtomicBool::new(false));
    let body_limit_for_error = body_limit_hit.clone();
    let body_timeout_hit = Arc::new(AtomicBool::new(false));
    let body_timeout_for_error = body_timeout_hit.clone();
    let (parts, body) = request.into_parts();
    let body = body
        .map_err(|error| -> BoxError { error.into() })
        .boxed_unsync();
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
        state.client.request(request),
    )
    .await;

    match response {
        Ok(Ok(response)) => {
            let transfer_encoding = response.headers().get_all(header::TRANSFER_ENCODING);
            if transfer_encoding.iter().count() > 1
                || transfer_encoding
                    .iter()
                    .next()
                    .is_some_and(|value| !value.as_bytes().eq_ignore_ascii_case(b"chunked"))
            {
                return Ok(simple_response(StatusCode::BAD_GATEWAY, true));
            }
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
            let mut response = Response::from_parts(parts, body);
            if let Some(port) = state.http3_port
                && let Ok(value) = HeaderValue::from_str(&format!("h3=\":{port}\"; ma=86400"))
            {
                response.headers_mut().insert(header::ALT_SVC, value);
            }
            Ok(response)
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

fn same_authority(
    left: &http::uri::Authority,
    right: &http::uri::Authority,
    default_port: u16,
) -> bool {
    config::normalize_host(left.host()) == config::normalize_host(right.host())
        && left.port_u16().unwrap_or(default_port) == right.port_u16().unwrap_or(default_port)
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

fn upstream_uri<B>(route: &Route, request: &Request<B>) -> Result<Uri, ()> {
    let origin = route.upstream.parse::<Uri>().map_err(|_| ())?;
    let scheme = origin.scheme_str().ok_or(())?;
    let authority = origin.authority().ok_or(())?;
    let path = request.uri().path_and_query().ok_or(())?;
    let mut builder = Uri::builder();
    builder = builder.scheme(scheme).authority(authority.clone());
    builder.path_and_query(path.clone()).build().map_err(|_| ())
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

pub fn strip_hop_headers(headers: &mut http::HeaderMap) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn accept_retries_after_a_transient_listener_error() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let client = tokio::spawn(TcpStream::connect(address));
        let fail_once = AtomicBool::new(true);

        let (stream, _) = accept_with_retry(|| async {
            if fail_once.swap(false, Ordering::Relaxed) {
                Err(io::Error::from(io::ErrorKind::Interrupted))
            } else {
                listener.accept().await
            }
        })
        .await
        .unwrap();

        assert!(stream.peer_addr().is_ok());
        assert!(client.await.unwrap().is_ok());
    }

    #[tokio::test]
    async fn accept_returns_nonrecoverable_listener_errors() {
        let error = accept_with_retry(|| async {
            Err::<(), _>(io::Error::from(io::ErrorKind::InvalidInput))
        })
        .await
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidInput);
    }
}
