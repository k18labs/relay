use std::{path::PathBuf, sync::Arc, time::Duration};

use bytes::{Buf, Bytes};
use http::Request;
use relay::{
    config::{Config, TlsConfig},
    proxy, tls,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, UdpSocket},
};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tls")
        .join(name)
}

async fn start_http_upstream() -> (std::net::SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let task = tokio::spawn(async move {
        for index in 0..3 {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(Duration::from_secs(3), listener.accept()).await
            else {
                return;
            };
            if index < 2 {
                if read_complete_request(&mut stream).await.is_err() {
                    return;
                }
                let response_body = if index == 0 {
                    vec![b'x'; 1024]
                } else {
                    b"ok".to_vec()
                };
                let response = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    response_body.len()
                );
                let _ = stream.write_all(response.as_bytes()).await;
                let _ = stream.write_all(&response_body).await;
            } else {
                let mut request = Vec::new();
                let _ =
                    tokio::time::timeout(Duration::from_secs(3), stream.read_to_end(&mut request))
                        .await;
            }
        }
    });
    (address, task)
}

async fn read_complete_request(stream: &mut tokio::net::TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    loop {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Ok(request);
        }
        request.extend_from_slice(&buffer[..count]);
        let Some(header_end) = request.windows(4).position(|window| window == b"\r\n\r\n") else {
            continue;
        };
        let headers = String::from_utf8_lossy(&request[..header_end]).to_ascii_lowercase();
        if headers.contains("transfer-encoding: chunked") {
            if request[header_end + 4..]
                .windows(5)
                .any(|window| window == b"0\r\n\r\n")
            {
                return Ok(request);
            }
        } else if let Some(length) = headers.lines().find_map(|line| {
            line.strip_prefix("content-length:")
                .and_then(|value| value.trim().parse::<usize>().ok())
        }) {
            if request.len() >= header_end + 4 + length {
                return Ok(request);
            }
        } else {
            return Ok(request);
        }
    }
}

#[tokio::test]
async fn serves_http3_and_forwards_to_the_configured_upstream() {
    let (upstream, upstream_task) = start_http_upstream().await;
    let certificate = fixture("localhost-cert.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let private_key = fixture("localhost-key.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let ca_file = fixture("test-ca.pem");
    let config_text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[limits]\nmax_body_bytes = 1024\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
    );
    let config = Arc::new(Config::parse(&config_text).unwrap());
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

    let mut client_tls = (*tls::build_upstream_client_config(&TlsConfig {
        certificate: None,
        private_key: None,
        upstream_ca_files: vec![ca_file],
    })
    .unwrap())
    .clone();
    client_tls.alpn_protocols = vec![b"h3".to_vec()];
    let crypto = quinn::crypto::rustls::QuicClientConfig::try_from(Arc::new(client_tls)).unwrap();
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(quinn::ClientConfig::new(Arc::new(crypto)));
    let connection = tokio::time::timeout(
        Duration::from_secs(3),
        endpoint.connect(relay_address, "localhost").unwrap(),
    )
    .await
    .expect("QUIC handshake timed out")
    .expect("QUIC connection failed");
    let mut builder = h3::client::builder();
    let (mut driver, mut sender) = builder
        .build::<_, _, bytes::Bytes>(h3_quinn::Connection::new(connection))
        .await
        .unwrap();
    tokio::spawn(async move {
        let _ = driver.wait_idle().await;
    });
    let request = Request::get("https://example.org/hello").body(()).unwrap();
    let mut stream = sender.send_request(request).await.unwrap();
    stream.finish().await.unwrap();
    let response = stream.recv_response().await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut body = Vec::new();
    while let Some(mut chunk) = stream.recv_data().await.unwrap() {
        while chunk.has_remaining() {
            let part = chunk.chunk();
            body.extend_from_slice(part);
            let count = part.len();
            chunk.advance(count);
        }
    }
    assert_eq!(body, vec![b'x'; 1024]);

    let request = Request::post("https://example.org/upload")
        .body(())
        .unwrap();
    let mut stream = sender.send_request(request).await.unwrap();
    stream.send_data(Bytes::from_static(b"abc")).await.unwrap();
    stream.finish().await.unwrap();
    let response = stream.recv_response().await.unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    while let Some(_chunk) = stream.recv_data().await.unwrap() {}

    let request = Request::post("https://example.org/upload")
        .body(())
        .unwrap();
    let mut stream = sender.send_request(request).await.unwrap();
    stream
        .send_data(Bytes::from(vec![b'a'; 1025]))
        .await
        .unwrap();
    stream.finish().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), stream.recv_response())
        .await
        .expect("oversized request timed out")
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::PAYLOAD_TOO_LARGE);
    endpoint.close(0u32.into(), b"test complete");
    let _ = shutdown_tx.send(());
    upstream_task.await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .expect("server shutdown timed out")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn fails_to_start_when_the_http3_udp_port_is_unavailable() {
    let certificate = fixture("localhost-cert.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let private_key = fixture("localhost-key.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let _occupied_udp = UdpSocket::bind(address).await.unwrap();
    let config_text = format!(
        "[server]\nlisten = \"{address}\"\nmax_connections = 8\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://127.0.0.1:3000\"\n"
    );
    let config = Arc::new(Config::parse(&config_text).unwrap());
    let server_tls = tls::load_server_config(&config.tls).unwrap();
    let client_tls = tls::build_upstream_client_config(&config.tls).unwrap();
    let error = proxy::serve_with_tls(
        listener,
        config,
        Some(server_tls),
        client_tls,
        std::future::pending::<()>(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::AddrInUse);
}
