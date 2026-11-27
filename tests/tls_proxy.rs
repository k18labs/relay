use std::{path::PathBuf, sync::Arc, time::Duration};

use bytes::Bytes;
use http::{Request, header};
use http_body_util::{BodyExt, Full};
use hyper_util::rt::{TokioExecutor, TokioIo};
use relay::{
    config::{Config, TlsConfig},
    proxy, tls,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{Semaphore, mpsc},
    task::JoinHandle,
};
use tokio_rustls::TlsAcceptor;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tls")
        .join(name)
}

fn fixture_toml_path(name: &str) -> String {
    fixture(name).to_string_lossy().replace('\\', "/")
}

fn test_server_tls(certificate: &str, private_key: &str) -> Arc<rustls::ServerConfig> {
    tls::load_server_config(&TlsConfig {
        certificate: Some(fixture(certificate)),
        private_key: Some(fixture(private_key)),
        upstream_ca_files: Vec::new(),
    })
    .unwrap()
}

async fn read_headers<R: AsyncReadExt + Unpin>(stream: &mut R) {
    let mut bytes = Vec::new();
    while !bytes.ends_with(b"\r\n\r\n") && bytes.len() < 16 * 1024 {
        let mut byte = [0; 1];
        if stream.read_exact(&mut byte).await.is_err() {
            break;
        }
        bytes.push(byte[0]);
    }
}

async fn start_tls_upstream(
    certificate: &str,
    private_key: &str,
) -> (std::net::SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let acceptor = TlsAcceptor::from(test_server_tls(certificate, private_key));
    let task = tokio::spawn(async move {
        let Ok(Ok((stream, _))) =
            tokio::time::timeout(Duration::from_secs(2), listener.accept()).await
        else {
            return;
        };
        if let Ok(Ok(mut stream)) =
            tokio::time::timeout(Duration::from_secs(2), acceptor.accept(stream)).await
        {
            read_headers(&mut stream).await;
            let _ = stream
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
                .await;
        }
    });
    (address, task)
}

async fn start_http_upstream() -> (std::net::SocketAddr, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        let Ok(Ok((mut stream, _))) =
            tokio::time::timeout(Duration::from_secs(2), listener.accept()).await
        else {
            return;
        };
        read_headers(&mut stream).await;
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await;
    });
    (address, task)
}

fn config(upstream: &str, ca_file: Option<&str>) -> Arc<Config> {
    let ca_config = ca_file.map_or_else(String::new, |path| {
        format!("\n[tls]\nupstream_ca_files = [\"{path}\"]\n")
    });
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"{upstream}\"\n{ca_config}"
    );
    Arc::new(Config::parse(&text).unwrap())
}

async fn start_plain_relay(config: Arc<Config>) -> std::net::SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve(
        listener,
        config,
        std::future::pending::<()>(),
    )));
    address
}

async fn request(address: std::net::SocketAddr) -> String {
    let mut stream = TcpStream::connect(address).await.unwrap();
    stream
        .write_all(b"GET / HTTP/1.1\r\nHost: example.org\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

#[tokio::test]
async fn verifies_a_configured_https_upstream_and_forwards_the_response() {
    let (upstream, upstream_task) =
        start_tls_upstream("localhost-cert.pem", "localhost-key.pem").await;
    let ca_file = fixture_toml_path("test-ca.pem");
    let config = config(
        &format!("https://127.0.0.1:{}", upstream.port()),
        Some(&ca_file),
    );
    let relay_address = start_plain_relay(config).await;
    let response = request(relay_address).await;
    assert!(
        response.contains("200 OK"),
        "unexpected response: {response}"
    );
    upstream_task.await.unwrap();
}

#[tokio::test]
async fn rejects_an_untrusted_https_upstream_certificate() {
    let (upstream, upstream_task) =
        start_tls_upstream("localhost-cert.pem", "localhost-key.pem").await;
    let config = config(&format!("https://127.0.0.1:{}", upstream.port()), None);
    let relay_address = start_plain_relay(config).await;
    let response = request(relay_address).await;
    assert!(response.contains("502"), "unexpected response: {response}");
    upstream_task.await.unwrap();
}

#[tokio::test]
async fn rejects_an_https_upstream_hostname_mismatch() {
    let (upstream, upstream_task) =
        start_tls_upstream("wrong-host-cert.pem", "wrong-host-key.pem").await;
    let ca_file = fixture_toml_path("test-ca.pem");
    let config = config(
        &format!("https://127.0.0.1:{}", upstream.port()),
        Some(&ca_file),
    );
    let relay_address = start_plain_relay(config).await;
    let response = request(relay_address).await;
    assert!(response.contains("502"), "unexpected response: {response}");
    upstream_task.await.unwrap();
}

#[tokio::test]
async fn bounds_the_upstream_tls_handshake_with_the_connect_deadline() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        if let Ok(Ok((stream, _))) =
            tokio::time::timeout(Duration::from_secs(2), upstream.accept()).await
        {
            tokio::time::sleep(Duration::from_secs(2)).await;
            drop(stream);
        }
    });
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[limits]\nconnect_timeout_ms = 100\nresponse_timeout_ms = 5000\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"https://127.0.0.1:{}\"\n",
        upstream_addr.port()
    );
    let relay_address = start_plain_relay(Arc::new(Config::parse(&text).unwrap())).await;
    let started = tokio::time::Instant::now();
    let response = request(relay_address).await;
    assert!(response.contains("502"), "unexpected response: {response}");
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[tokio::test]
async fn terminates_tls_1_3_for_clients_and_forwards_to_http_upstream() {
    let (upstream, upstream_task) = start_http_upstream().await;
    let certificate = fixture_toml_path("localhost-cert.pem");
    let private_key = fixture_toml_path("localhost-key.pem");
    let ca_file = fixture_toml_path("test-ca.pem");
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\nupstream_ca_files = [\"{ca_file}\"]\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let server_tls = tls::load_server_config(&config.tls).unwrap();
    let client_tls = tls::build_upstream_client_config(&config.tls).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_address = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve_with_tls(
        listener,
        config,
        Some(server_tls),
        client_tls,
        std::future::pending::<()>(),
    )));

    let tcp = TcpStream::connect(relay_address).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut client = tokio_rustls::TlsConnector::from(
        tls::build_upstream_client_config(&TlsConfig {
            certificate: None,
            private_key: None,
            upstream_ca_files: vec![fixture("test-ca.pem")],
        })
        .unwrap(),
    )
    .connect(server_name, tcp)
    .await
    .unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.org\r\nConnection: close\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(3), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&response).contains("200 OK"));
    upstream_task.await.unwrap();
}

#[tokio::test]
async fn negotiates_http2_and_forwards_using_the_authority_for_routing() {
    let (upstream, upstream_task) = start_http_upstream().await;
    let certificate = fixture_toml_path("localhost-cert.pem");
    let private_key = fixture_toml_path("localhost-key.pem");
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let server_tls = tls::load_server_config(&config.tls).unwrap();
    let client_tls = tls::build_upstream_client_config(&config.tls).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_address = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve_with_tls(
        listener,
        config,
        Some(server_tls),
        client_tls,
        std::future::pending::<()>(),
    )));

    let tcp = TcpStream::connect(relay_address).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut client_tls = (*tls::build_upstream_client_config(&TlsConfig {
        certificate: None,
        private_key: None,
        upstream_ca_files: vec![fixture("test-ca.pem")],
    })
    .unwrap())
    .clone();
    client_tls.alpn_protocols = vec![b"h2".to_vec()];
    let client = tokio_rustls::TlsConnector::from(Arc::new(client_tls))
        .connect(server_name, tcp)
        .await
        .unwrap();
    assert_eq!(client.get_ref().1.alpn_protocol(), Some(b"h2".as_slice()));
    let (mut sender, connection) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(client))
            .await
            .unwrap();
    tokio::spawn(async move {
        let _ = connection.await;
    });
    let request = Request::builder()
        .uri("https://example.org/hello")
        .header(header::HOST, "example.org")
        .body(Full::new(Bytes::new()))
        .unwrap();
    let response = sender.send_request(request).await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    assert_eq!(
        response
            .headers()
            .get(header::ALT_SVC)
            .unwrap()
            .to_str()
            .unwrap(),
        format!("h3=\":{}\"; ma=86400", relay_address.port())
    );
    assert_eq!(
        response.into_body().collect().await.unwrap().to_bytes(),
        &b"ok"[..]
    );
    upstream_task.await.unwrap();
}

#[tokio::test]
async fn limits_concurrent_http2_streams_on_a_single_connection() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_address = upstream.local_addr().unwrap();
    let gates = Arc::new(Semaphore::new(0));
    let (started_tx, mut started_rx) = mpsc::unbounded_channel();
    let upstream_gates = gates.clone();
    let upstream_task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = upstream.accept().await else {
                return;
            };
            let gates = upstream_gates.clone();
            let started = started_tx.clone();
            tokio::spawn(async move {
                read_headers(&mut stream).await;
                if started.send(()).is_err() {
                    return;
                }
                let Ok(permit) = gates.acquire_owned().await else {
                    return;
                };
                permit.forget();
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await;
            });
        }
    });

    let certificate = fixture_toml_path("localhost-cert.pem");
    let private_key = fixture_toml_path("localhost-key.pem");
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[limits]\nmax_concurrent_requests = 8\nmax_concurrent_streams_per_connection = 1\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream_address}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let server_tls = tls::load_server_config(&config.tls).unwrap();
    let client_tls = tls::build_upstream_client_config(&config.tls).unwrap();
    let listener = TcpListener::bind(config.server.listen).await.unwrap();
    let relay_address = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server_task = tokio::spawn(proxy::serve_with_tls(
        listener,
        config,
        Some(server_tls),
        client_tls,
        async move {
            let _ = shutdown_rx.await;
        },
    ));

    let tcp = TcpStream::connect(relay_address).await.unwrap();
    let server_name = rustls::pki_types::ServerName::try_from("localhost").unwrap();
    let mut client_tls = (*tls::build_upstream_client_config(&TlsConfig {
        certificate: None,
        private_key: None,
        upstream_ca_files: vec![fixture("test-ca.pem")],
    })
    .unwrap())
    .clone();
    client_tls.alpn_protocols = vec![b"h2".to_vec()];
    let client = tokio_rustls::TlsConnector::from(Arc::new(client_tls))
        .connect(server_name, tcp)
        .await
        .unwrap();
    let (sender, connection) =
        hyper::client::conn::http2::handshake(TokioExecutor::new(), TokioIo::new(client))
            .await
            .unwrap();
    let connection_task = tokio::spawn(async move {
        let _ = connection.await;
    });

    let requests = (0..8)
        .map(|_| {
            let mut sender = sender.clone();
            tokio::spawn(async move {
                let request = Request::builder()
                    .uri("https://example.org/stream")
                    .header(header::HOST, "example.org")
                    .body(Full::new(Bytes::new()))
                    .unwrap();
                match sender.send_request(request).await {
                    Ok(response) => {
                        assert_eq!(response.status(), http::StatusCode::OK);
                        assert_eq!(
                            response.into_body().collect().await.unwrap().to_bytes(),
                            &b"ok"[..]
                        );
                        true
                    }
                    Err(error) => {
                        assert!(
                            format!("{error:?}").contains("REFUSED_STREAM"),
                            "unexpected HTTP/2 stream error: {error:?}"
                        );
                        false
                    }
                }
            })
        })
        .collect::<Vec<_>>();

    tokio::time::timeout(Duration::from_secs(3), started_rx.recv())
        .await
        .expect("first upstream request timed out")
        .expect("upstream task stopped");
    assert!(
        tokio::time::timeout(Duration::from_millis(150), started_rx.recv())
            .await
            .is_err(),
        "a second HTTP/2 stream exceeded the configured per-connection limit"
    );

    gates.add_permits(1);
    let mut successful_requests = 0;
    for request in requests {
        successful_requests += usize::from(
            tokio::time::timeout(Duration::from_secs(3), request)
                .await
                .expect("HTTP/2 request timed out")
                .unwrap(),
        );
    }
    assert_eq!(successful_requests, 1);

    let mut sender = sender.clone();
    let next_request = tokio::spawn(async move {
        let request = Request::builder()
            .uri("https://example.org/after-capacity")
            .header(header::HOST, "example.org")
            .body(Full::new(Bytes::new()))
            .unwrap();
        let response = sender.send_request(request).await.unwrap();
        assert_eq!(response.status(), http::StatusCode::OK);
        assert_eq!(
            response.into_body().collect().await.unwrap().to_bytes(),
            &b"ok"[..]
        );
    });
    tokio::time::timeout(Duration::from_secs(3), started_rx.recv())
        .await
        .expect("request did not start after stream capacity was available")
        .expect("upstream task stopped");
    gates.add_permits(1);
    tokio::time::timeout(Duration::from_secs(3), next_request)
        .await
        .expect("request after stream capacity was available timed out")
        .unwrap();

    drop(sender);
    connection_task.abort();
    upstream_task.abort();
    let _ = shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .expect("Relay shutdown timed out")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn rejects_clients_that_do_not_finish_tls_handshake() {
    let (upstream, _upstream_task) = start_http_upstream().await;
    let certificate = fixture_toml_path("localhost-cert.pem");
    let private_key = fixture_toml_path("localhost-key.pem");
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[limits]\nhandshake_timeout_ms = 100\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let server_tls = tls::load_server_config(&config.tls).unwrap();
    let client_tls = tls::build_upstream_client_config(&config.tls).unwrap();
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_address = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve_with_tls(
        listener,
        config,
        Some(server_tls),
        client_tls,
        std::future::pending::<()>(),
    )));
    let mut stream = TcpStream::connect(relay_address).await.unwrap();
    let mut response = Vec::new();
    let result = tokio::time::timeout(Duration::from_secs(1), stream.read_to_end(&mut response))
        .await
        .unwrap();
    assert!(
        result.is_ok()
            || result
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::ConnectionReset)
    );
    assert!(response.is_empty());
}
