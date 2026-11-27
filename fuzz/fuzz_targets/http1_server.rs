#![no_main]

use std::{sync::Arc, time::Duration};

use libfuzzer_sys::fuzz_target;
use relay::{config::Config, proxy};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};

const CONFIG: &str = r#"
[server]
listen = "127.0.0.1:0"
max_connections = 2
tls = false

[limits]
max_header_bytes = 16384
max_header_count = 32
max_body_bytes = 65536
max_concurrent_requests = 2
header_timeout_ms = 10
connect_timeout_ms = 10
response_timeout_ms = 20
body_idle_timeout_ms = 10
upstream_idle_timeout_ms = 1000
shutdown_timeout_ms = 20
max_idle_upstream_per_host = 1

[[routes]]
host = "example.org"
path = "/"
upstream = "http://127.0.0.1:1"
"#;

fuzz_target!(|input: &[u8]| {
    let input = &input[..input.len().min(65_536)];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create fuzz runtime");

    runtime.block_on(async {
        let config = Arc::new(Config::parse(CONFIG).expect("parse fixed fuzz config"));
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind ephemeral listener");
        let address = listener.local_addr().expect("read listener address");
        let server = tokio::spawn(proxy::serve(listener, config, std::future::pending::<()>()));

        if let Ok(Ok(mut client)) =
            timeout(Duration::from_millis(50), TcpStream::connect(address)).await
        {
            let _ = timeout(Duration::from_millis(50), client.write_all(input)).await;
            let _ = client.shutdown().await;
            let mut response = Vec::new();
            let _ = timeout(Duration::from_millis(50), client.read_to_end(&mut response)).await;
        }

        server.abort();
    });
});
