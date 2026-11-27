use std::{path::PathBuf, sync::Arc, time::Duration};

use relay::{
    config::{Config, TlsConfig},
    proxy, tls,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
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
