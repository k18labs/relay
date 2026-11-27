use std::{
    convert::Infallible,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use bytes::Bytes;
use http::{Request, Response};
use http_body_util::Full;
use hyper::{body::Incoming, service::service_fn};
use hyper_util::rt::TokioIo;
use relay::{config::Config, proxy};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

async fn start_relay(upstream: std::net::SocketAddr) -> std::net::SocketAddr {
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve(
        listener,
        config,
        std::future::pending::<()>(),
    )));
    addr
}

async fn request(addr: std::net::SocketAddr, host: &str, extra: &str) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            format!("GET /hello HTTP/1.1\r\nHost: {host}\r\n{extra}Connection: close\r\n\r\n")
                .as_bytes(),
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

async fn request_with_body(addr: std::net::SocketAddr, body: &[u8]) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream
        .write_all(
            format!(
                "POST /upload HTTP/1.1\r\nHost: example.org\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .as_bytes(),
        )
        .await
        .unwrap();
    stream.write_all(body).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

async fn raw_request(addr: std::net::SocketAddr, request: &[u8]) -> String {
    let mut stream = TcpStream::connect(addr).await.unwrap();
    stream.write_all(request).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8_lossy(&response).into_owned()
}

async fn relay_with_limits(upstream: std::net::SocketAddr, limits: &str) -> std::net::SocketAddr {
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[limits]\n{limits}\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve(
        listener,
        config,
        std::future::pending::<()>(),
    )));
    addr
}

#[tokio::test]
async fn forwards_to_the_fixed_upstream_and_strips_connection_named_headers() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (tx, rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut data = vec![0; 2048];
        let count = stream.read(&mut data).await.unwrap();
        let _ = tx.send(String::from_utf8_lossy(&data[..count]).into_owned());
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .unwrap();
    });
    let relay = start_relay(upstream_addr).await;
    let response = request(
        relay,
        "example.org",
        "Connection: x-secret\r\nx-secret: hidden\r\nX-Forwarded-For: 203.0.113.8\r\nX-Forwarded-By: proxy.attacker\r\nX-Real-IP: 203.0.113.8\r\nForwarded: for=203.0.113.8\r\n",
    )
    .await;
    assert!(response.contains("200 OK"));
    let forwarded = rx.await.unwrap().to_ascii_lowercase();
    assert!(forwarded.contains("get /hello http/1.1"));
    assert!(!forwarded.contains("x-secret"));
    assert!(!forwarded.contains("x-forwarded-for"));
    assert!(!forwarded.contains("x-forwarded-by"));
    assert!(!forwarded.contains("x-real-ip"));
    assert!(!forwarded.contains("forwarded:"));
}

#[tokio::test]
async fn rejects_unknown_hosts_without_contacting_an_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let relay = start_relay(upstream_addr).await;
    let response = request(relay, "attacker.example", "").await;
    assert!(response.contains("421"));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_declared_oversized_bodies_before_contacting_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[limits]\nmax_body_bytes = 4\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream_addr}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = listener.local_addr().unwrap();
    drop(tokio::spawn(proxy::serve(
        listener,
        config,
        std::future::pending::<()>(),
    )));

    let response = request_with_body(relay_addr, b"12345").await;
    assert!(response.contains("413"));
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_chunked_request_bodies_over_the_limit() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = relay_with_limits(upstream.local_addr().unwrap(), "max_body_bytes = 4").await;
    let response = raw_request(
        relay_addr,
        b"POST /upload HTTP/1.1\r\nHost: example.org\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n5\r\n12345\r\n0\r\n\r\n",
    )
    .await;
    assert!(response.contains("413"), "unexpected response: {response}");
}

#[tokio::test]
async fn rejects_known_oversized_upstream_responses_before_forwarding_them() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n12345")
            .await
            .unwrap();
    });
    let relay_addr = relay_with_limits(upstream_addr, "max_body_bytes = 4").await;
    let response = request(relay_addr, "example.org", "").await;
    assert!(response.contains("502"), "unexpected response: {response}");
    assert!(!response.ends_with("12345"));
}

#[tokio::test]
async fn rejects_conflicting_content_lengths_before_contacting_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"POST / HTTP/1.1\r\nHost: example.org\r\nContent-Length: 1\r\nContent-Length: 2\r\nConnection: close\r\n\r\nx",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_asterisk_form_targets_before_routing() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"OPTIONS * HTTP/1.1\r\nHost: example.org\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_connect_requests_before_contacting_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"CONNECT example.org:443 HTTP/1.1\r\nHost: example.org:443\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.contains("501"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_upgrade_requests_before_contacting_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"GET / HTTP/1.1\r\nHost: example.org\r\nConnection: Upgrade\r\nUpgrade: websocket\r\n\r\n",
    )
    .await;
    assert!(response.contains("501"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_host_authority_userinfo() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"GET / HTTP/1.1\r\nHost: user@example.org\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_host_authorities_with_out_of_range_ports() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"GET / HTTP/1.1\r\nHost: example.org:65536\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_duplicate_host_fields_before_contacting_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"GET / HTTP/1.1\r\nHost: example.org\r\nHost: example.org\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn normalizes_transfer_encoding_and_content_length_conflicts_before_forwarding() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (forwarded_tx, forwarded_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = Vec::new();
        let mut buffer = [0; 512];
        while !request.windows(5).any(|window| window == b"0\r\n\r\n") && request.len() < 4096 {
            let count = stream.read(&mut buffer).await.unwrap();
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
        }
        let _ = forwarded_tx.send(String::from_utf8_lossy(&request).to_ascii_lowercase());
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .unwrap();
    });
    let relay_addr = start_relay(upstream_addr).await;
    let response = raw_request(
        relay_addr,
        b"POST / HTTP/1.1\r\nHost: example.org\r\nContent-Length: 0\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n0\r\n\r\n",
    )
    .await;
    assert!(
        response.contains("200 OK"),
        "unexpected response: {response}"
    );
    let forwarded = forwarded_rx.await.unwrap();
    assert!(forwarded.starts_with("post / http/1.1"));
    assert!(!forwarded.contains("content-length:"));
    assert!(forwarded.contains("transfer-encoding: chunked"));
}

#[tokio::test]
async fn does_not_forward_invalid_chunk_payload() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"POST / HTTP/1.1\r\nHost: example.org\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\nZ\r\nnot-hex\r\n0\r\n\r\n",
    )
    .await;
    assert!(response.contains("502"), "unexpected response: {response}");
    let (mut forwarded, _) = tokio::time::timeout(Duration::from_secs(1), upstream.accept())
        .await
        .expect("proxy did not close the failed upstream exchange")
        .unwrap();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), forwarded.read_to_end(&mut bytes))
        .await
        .expect("failed upstream exchange stayed open")
        .unwrap();
    let request = String::from_utf8_lossy(&bytes).to_ascii_lowercase();
    assert!(!request.contains("not-hex"));
}

#[tokio::test]
async fn rejects_absolute_form_request_targets() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"GET http://example.org/ HTTP/1.1\r\nHost: example.org\r\nConnection: close\r\n\r\n",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_unsupported_request_transfer_codings() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = start_relay(upstream.local_addr().unwrap()).await;
    let response = raw_request(
        relay_addr,
        b"POST / HTTP/1.1\r\nHost: example.org\r\nTransfer-Encoding: gzip, chunked\r\nConnection: close\r\n\r\n0\r\n\r\n",
    )
    .await;
    assert!(response.contains("400"), "unexpected response: {response}");
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn rejects_unsupported_upstream_response_transfer_codings() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: gzip, chunked\r\nConnection: close\r\n\r\n0\r\n\r\n",
            )
            .await
            .unwrap();
    });
    let relay_addr = start_relay(upstream_addr).await;
    let response = request(relay_addr, "example.org", "").await;
    assert!(response.contains("502"), "unexpected response: {response}");
}

#[tokio::test]
async fn closes_idle_upstream_connections_after_the_pool_deadline() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (closed_tx, closed_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await
            .unwrap();
        let mut probe = [0; 1];
        let closed = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut probe))
            .await
            .is_ok_and(|result| result.is_ok_and(|count| count == 0));
        let _ = closed_tx.send(closed);
    });
    let relay_addr = relay_with_limits(upstream_addr, "upstream_idle_timeout_ms = 100").await;
    let response = request(relay_addr, "example.org", "").await;
    assert!(
        response.contains("200 OK"),
        "unexpected response: {response}"
    );
    assert!(
        tokio::time::timeout(Duration::from_secs(2), closed_rx)
            .await
            .expect("upstream should be evicted")
            .expect("upstream observer should complete")
    );
}

#[tokio::test]
async fn times_out_stalled_client_writes_and_releases_request_capacity() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut first, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = first.read(&mut request).await;
        first
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 67108864\r\n\r\n")
            .await
            .unwrap();
        let body = vec![b'x'; 64 * 1024 * 1024];
        let _ = first.write_all(&body).await;

        let (mut second, _) = tokio::time::timeout(Duration::from_secs(3), upstream.accept())
            .await
            .expect("capacity should be released after the stalled write")
            .unwrap();
        let mut request = [0; 1024];
        let _ = second.read(&mut request).await;
        second
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok")
            .await
            .unwrap();
    });
    let relay_addr = relay_with_limits(
        upstream_addr,
        "max_concurrent_requests = 1\nmax_body_bytes = 134217728\ndownstream_write_timeout_ms = 500",
    )
    .await;
    let mut slow_client = TcpStream::connect(relay_addr).await.unwrap();
    slow_client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.org\r\n\r\n")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;

    let rejected = request(relay_addr, "example.org", "").await;
    assert!(rejected.contains("503"), "unexpected response: {rejected}");
    tokio::time::sleep(Duration::from_millis(500)).await;
    drop(slow_client);

    let recovered = request(relay_addr, "example.org", "").await;
    assert!(
        recovered.contains("200 OK"),
        "unexpected response: {recovered}"
    );
    assert!(recovered.ends_with("ok"));
}

#[tokio::test]
async fn closes_clients_that_do_not_finish_headers_before_the_deadline() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr =
        relay_with_limits(upstream.local_addr().unwrap(), "header_timeout_ms = 100").await;
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.org\r\nX-Pending: ")
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(250)).await;
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(1), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(response.is_empty());
    assert!(
        tokio::time::timeout(Duration::from_millis(100), upstream.accept())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn closes_idle_keep_alive_clients_after_the_header_deadline() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await
            .unwrap();
    });
    let relay_addr = relay_with_limits(upstream_addr, "header_timeout_ms = 100").await;
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    client
        .write_all(b"GET / HTTP/1.1\r\nHost: example.org\r\n\r\n")
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    let response = String::from_utf8_lossy(&response);
    assert!(
        response.contains("200 OK"),
        "unexpected response: {response}"
    );
    assert!(response.ends_with("ok"));
}

#[tokio::test]
async fn times_out_when_upstream_does_not_start_a_response() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    let relay_addr = relay_with_limits(upstream_addr, "response_timeout_ms = 100").await;
    let response = request(relay_addr, "example.org", "").await;
    assert!(response.contains("504"), "unexpected response: {response}");
}

#[tokio::test]
async fn ends_a_stalled_response_body_after_the_body_idle_deadline() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\n")
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(500)).await;
    });
    let relay_addr = relay_with_limits(upstream_addr, "body_idle_timeout_ms = 100").await;
    let started = tokio::time::Instant::now();
    let response = request(relay_addr, "example.org", "").await;
    assert!(response.contains("200 OK"));
    assert!(started.elapsed() < Duration::from_millis(400));
}

#[tokio::test]
async fn graceful_shutdown_waits_for_active_request_completion() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        let _ = started_tx.send(());
        tokio::time::sleep(Duration::from_millis(100)).await;
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await;
    });
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[limits]\nshutdown_timeout_ms = 1500\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream_addr}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(proxy::serve(listener, config, async move {
        let _ = shutdown_rx.await;
    }));
    let client = tokio::spawn(request(relay_addr, "example.org", ""));
    started_rx.await.unwrap();
    shutdown_tx.send(()).unwrap();
    let response = client.await.unwrap();
    assert!(response.contains("200 OK"));
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn graceful_shutdown_stops_keep_alive_requests_from_reaching_upstream() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    let (second_tx, second_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        let _ = started_tx.send(());
        let _ = release_rx.await;
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await;
        let mut probe = [0; 1024];
        let received_second =
            tokio::time::timeout(Duration::from_millis(300), stream.read(&mut probe))
                .await
                .is_ok_and(|result| result.is_ok_and(|count| count > 0));
        let _ = second_tx.send(received_second);
    });
    let text = format!(
        "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 8\ntls = false\n\n[limits]\nshutdown_timeout_ms = 1500\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://{upstream_addr}\"\n"
    );
    let config = Arc::new(Config::parse(&text).unwrap());
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr = listener.local_addr().unwrap();
    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(proxy::serve(listener, config, async move {
        let _ = shutdown_rx.await;
    }));
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    client
        .write_all(b"GET /first HTTP/1.1\r\nHost: example.org\r\n\r\n")
        .await
        .unwrap();
    started_rx.await.unwrap();
    shutdown_tx.send(()).unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;
    client
        .write_all(b"GET /second HTTP/1.1\r\nHost: example.org\r\n\r\n")
        .await
        .unwrap();
    release_tx.send(()).unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&response).contains("200 OK"));
    assert!(
        !second_rx.await.unwrap(),
        "a request reached upstream during drain"
    );
    server.await.unwrap().unwrap();
}

#[tokio::test]
async fn returns_request_timeout_when_the_client_stalls_during_body_upload() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let relay_addr =
        relay_with_limits(upstream.local_addr().unwrap(), "body_idle_timeout_ms = 100").await;
    let mut client = TcpStream::connect(relay_addr).await.unwrap();
    client
        .write_all(
            b"POST /upload HTTP/1.1\r\nHost: example.org\r\nContent-Length: 1\r\nConnection: close\r\n\r\n",
        )
        .await
        .unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), client.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    assert!(String::from_utf8_lossy(&response).contains("408"));
}

#[tokio::test]
async fn caps_concurrent_requests_and_releases_capacity_after_response_body() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let (received_tx, received_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = tokio::sync::oneshot::channel();
    tokio::spawn(async move {
        let (mut stream, _) = upstream.accept().await.unwrap();
        let mut request = [0; 1024];
        let _ = stream.read(&mut request).await;
        let _ = received_tx.send(());
        let _ = release_rx.await;
        let _ = stream
            .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nok")
            .await;
    });
    let relay_addr = relay_with_limits(
        upstream_addr,
        "max_concurrent_requests = 1\nresponse_timeout_ms = 2000",
    )
    .await;
    let first = tokio::spawn(request(relay_addr, "example.org", ""));
    received_rx.await.unwrap();
    let second = request(relay_addr, "example.org", "").await;
    assert!(second.contains("503"), "unexpected response: {second}");
    release_tx.send(()).unwrap();
    assert!(first.await.unwrap().contains("200 OK"));
}

#[tokio::test]
async fn reuses_a_pooled_upstream_connection_across_client_connections() {
    let upstream = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let upstream_addr = upstream.local_addr().unwrap();
    let accepted = Arc::new(AtomicUsize::new(0));
    let handled = Arc::new(AtomicUsize::new(0));
    let accepted_counter = accepted.clone();
    let handled_counter = handled.clone();
    tokio::spawn(async move {
        let (stream, _) = upstream.accept().await.unwrap();
        accepted_counter.fetch_add(1, Ordering::Relaxed);
        let service = service_fn(move |_request: Request<Incoming>| {
            handled_counter.fetch_add(1, Ordering::Relaxed);
            async { Ok::<_, Infallible>(Response::new(Full::new(Bytes::from_static(b"ok")))) }
        });
        let _ = hyper::server::conn::http1::Builder::new()
            .serve_connection(TokioIo::new(stream), service)
            .await;
    });
    let relay_addr = start_relay(upstream_addr).await;

    assert!(
        request(relay_addr, "example.org", "")
            .await
            .contains("200 OK")
    );
    assert!(
        request(relay_addr, "example.org", "")
            .await
            .contains("200 OK")
    );
    assert_eq!(handled.load(Ordering::Relaxed), 2);
    assert_eq!(accepted.load(Ordering::Relaxed), 1);
}
