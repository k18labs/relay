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
    sync::{Semaphore, mpsc},
    task::JoinHandle,
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
        for index in 0..4 {
            let Ok(Ok((mut stream, _))) =
                tokio::time::timeout(Duration::from_secs(3), listener.accept()).await
            else {
                return;
            };
            if index < 3 {
                let request_read = if index == 2 {
                    read_request_headers(&mut stream).await
                } else {
                    read_complete_request(&mut stream).await
                };
                if request_read.is_err() {
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

async fn start_gated_upstream() -> (
    std::net::SocketAddr,
    mpsc::UnboundedReceiver<()>,
    Arc<Semaphore>,
    JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let gates = Arc::new(Semaphore::new(0));
    let upstream_gates = gates.clone();
    let (started_tx, started_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let gates = upstream_gates.clone();
            let started = started_tx.clone();
            tokio::spawn(async move {
                if read_request_headers(&mut stream).await.is_err() {
                    return;
                }
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
    (address, started_rx, gates, task)
}

async fn start_immediate_upstream() -> (
    std::net::SocketAddr,
    mpsc::UnboundedReceiver<()>,
    JoinHandle<()>,
) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started_tx, started_rx) = mpsc::unbounded_channel();
    let task = tokio::spawn(async move {
        loop {
            let Ok((mut stream, _)) = listener.accept().await else {
                return;
            };
            let started = started_tx.clone();
            tokio::spawn(async move {
                if read_request_headers(&mut stream).await.is_err() {
                    return;
                }
                if started.send(()).is_err() {
                    return;
                }
                let _ = stream
                    .write_all(
                        b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok",
                    )
                    .await;
            });
        }
    });
    (address, started_rx, task)
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

async fn read_request_headers(stream: &mut tokio::net::TcpStream) -> std::io::Result<Vec<u8>> {
    let mut request = Vec::new();
    let mut buffer = [0; 1024];
    while !request.windows(4).any(|window| window == b"\r\n\r\n") {
        let count = stream.read(&mut buffer).await?;
        if count == 0 {
            return Err(std::io::Error::from(std::io::ErrorKind::UnexpectedEof));
        }
        request.extend_from_slice(&buffer[..count]);
    }
    Ok(request)
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

    let request = Request::post("https://example.org/early-response")
        .body(())
        .unwrap();
    let mut stream = sender.send_request(request).await.unwrap();
    stream
        .send_data(Bytes::from_static(b"first chunk"))
        .await
        .unwrap();
    let (mut send_stream, mut receive_stream) = stream.split();
    let upload_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        send_stream
            .send_data(Bytes::from_static(b"second chunk"))
            .await?;
        send_stream.finish().await
    });
    let response = tokio::time::timeout(Duration::from_secs(3), receive_stream.recv_response())
        .await
        .expect("early upstream response timed out")
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut body = Vec::new();
    while let Some(mut chunk) = receive_stream.recv_data().await.unwrap() {
        while chunk.has_remaining() {
            let part = chunk.chunk();
            body.extend_from_slice(part);
            let count = part.len();
            chunk.advance(count);
        }
    }
    assert_eq!(body, b"ok");
    upload_task.await.unwrap().unwrap();

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
async fn canceled_http3_upload_releases_request_capacity_on_the_connection() {
    let (upstream, mut started_rx, upstream_task) = start_immediate_upstream().await;
    let certificate = fixture("localhost-cert.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let private_key = fixture("localhost-key.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let config_text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[limits]\nmax_concurrent_requests = 1\nmax_concurrent_streams_per_connection = 1\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
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
        upstream_ca_files: vec![fixture("test-ca.pem")],
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
        .build::<_, _, Bytes>(h3_quinn::Connection::new(connection))
        .await
        .unwrap();
    let driver_task = tokio::spawn(async move { driver.wait_idle().await });

    let request = Request::post("https://example.org/cancel")
        .body(())
        .unwrap();
    let mut canceled = sender.send_request(request).await.unwrap();
    canceled
        .send_data(Bytes::from_static(b"partial upload"))
        .await
        .unwrap();
    tokio::time::timeout(Duration::from_secs(3), started_rx.recv())
        .await
        .expect("canceled request did not reach upstream")
        .expect("upstream task stopped");
    canceled.stop_stream(h3::error::Code::H3_REQUEST_CANCELLED);
    drop(canceled);

    let request = Request::get("https://example.org/after-cancel")
        .body(())
        .unwrap();
    let mut next = tokio::time::timeout(Duration::from_secs(3), sender.send_request(request))
        .await
        .expect("second request was blocked by the per-connection stream limit")
        .expect("second request could not open a stream");
    next.finish().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), next.recv_response())
        .await
        .expect("second request did not receive a response")
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut body = Vec::new();
    while let Some(mut chunk) = next.recv_data().await.unwrap() {
        while chunk.has_remaining() {
            let part = chunk.chunk();
            body.extend_from_slice(part);
            let count = part.len();
            chunk.advance(count);
        }
    }
    assert_eq!(body, b"ok");

    endpoint.close(0u32.into(), b"test complete");
    let _driver_result = tokio::time::timeout(Duration::from_secs(3), driver_task)
        .await
        .expect("HTTP/3 client driver shutdown timed out")
        .unwrap();
    upstream_task.abort();
    let _ = shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .expect("Relay shutdown timed out")
        .unwrap()
        .unwrap();
}

#[tokio::test]
#[ignore = "Known h3-quinn 0.0.10 panic when stop_sending races a pending read; see ROADMAP"]
async fn stalled_http3_upload_times_out_and_releases_request_capacity() {
    let (upstream, mut started_rx, upstream_task) = start_immediate_upstream().await;
    let certificate = fixture("localhost-cert.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let private_key = fixture("localhost-key.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let config_text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[limits]\nbody_idle_timeout_ms = 100\nmax_concurrent_requests = 1\nmax_concurrent_streams_per_connection = 1\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
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
        upstream_ca_files: vec![fixture("test-ca.pem")],
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
        .build::<_, _, Bytes>(h3_quinn::Connection::new(connection))
        .await
        .unwrap();
    let driver_task = tokio::spawn(async move { driver.wait_idle().await });

    let request = Request::post("https://example.org/stalled-upload")
        .body(())
        .unwrap();
    let mut stalled = sender.send_request(request).await.unwrap();
    tokio::time::timeout(Duration::from_secs(3), started_rx.recv())
        .await
        .expect("stalled request did not reach upstream")
        .expect("upstream task stopped");
    let response = tokio::time::timeout(Duration::from_secs(3), stalled.recv_response())
        .await
        .expect("stalled request did not time out")
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::REQUEST_TIMEOUT);
    while let Some(_chunk) = stalled.recv_data().await.unwrap() {}

    let request = Request::get("https://example.org/after-timeout")
        .body(())
        .unwrap();
    let mut next = tokio::time::timeout(Duration::from_secs(3), sender.send_request(request))
        .await
        .expect("follow-up request was blocked by the per-connection stream limit")
        .expect("follow-up request could not open a stream");
    next.finish().await.unwrap();
    let response = tokio::time::timeout(Duration::from_secs(3), next.recv_response())
        .await
        .expect("follow-up request did not receive a response")
        .unwrap();
    assert_eq!(response.status(), http::StatusCode::OK);
    let mut body = Vec::new();
    while let Some(mut chunk) = next.recv_data().await.unwrap() {
        while chunk.has_remaining() {
            let part = chunk.chunk();
            body.extend_from_slice(part);
            let count = part.len();
            chunk.advance(count);
        }
    }
    assert_eq!(body, b"ok");

    endpoint.close(0u32.into(), b"test complete");
    let _driver_result = tokio::time::timeout(Duration::from_secs(3), driver_task)
        .await
        .expect("HTTP/3 client driver shutdown timed out")
        .unwrap();
    upstream_task.abort();
    let _ = shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .expect("Relay shutdown timed out")
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn limits_concurrent_http3_streams_on_a_single_connection() {
    let (upstream, mut started_rx, gates, upstream_task) = start_gated_upstream().await;
    let certificate = fixture("localhost-cert.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let private_key = fixture("localhost-key.pem")
        .to_string_lossy()
        .replace('\\', "/");
    let config_text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\n\n[limits]\nmax_concurrent_requests = 8\nmax_concurrent_streams_per_connection = 1\n\n[tls]\ncertificate = \"{certificate}\"\nprivate_key = \"{private_key}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
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
        upstream_ca_files: vec![fixture("test-ca.pem")],
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
    let (mut driver, sender) = builder
        .build::<_, _, Bytes>(h3_quinn::Connection::new(connection))
        .await
        .unwrap();
    let driver_task = tokio::spawn(async move { driver.wait_idle().await });

    let requests = (0..8)
        .map(|_| {
            let mut sender = sender.clone();
            tokio::spawn(async move {
                let request = Request::get("https://example.org/stream").body(()).unwrap();
                let mut stream = sender.send_request(request).await.unwrap();
                stream.finish().await.unwrap();
                let response = stream.recv_response().await.unwrap();
                assert_eq!(response.status(), http::StatusCode::OK);
                while let Some(mut chunk) = stream.recv_data().await.unwrap() {
                    while chunk.has_remaining() {
                        let count = chunk.chunk().len();
                        chunk.advance(count);
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
        "a second HTTP/3 stream exceeded the configured per-connection limit"
    );

    gates.add_permits(1);
    tokio::time::timeout(Duration::from_secs(3), started_rx.recv())
        .await
        .expect("next upstream request did not start after capacity was released")
        .expect("upstream task stopped");
    gates.add_permits(7);
    for request in requests {
        tokio::time::timeout(Duration::from_secs(3), request)
            .await
            .expect("HTTP/3 request timed out")
            .unwrap();
    }

    endpoint.close(0u32.into(), b"test complete");
    let _driver_result = tokio::time::timeout(Duration::from_secs(3), driver_task)
        .await
        .expect("HTTP/3 client driver shutdown timed out")
        .unwrap();
    upstream_task.abort();
    let _ = shutdown_tx.send(());
    tokio::time::timeout(Duration::from_secs(3), server_task)
        .await
        .expect("Relay shutdown timed out")
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
