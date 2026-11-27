#![no_main]

use std::{path::PathBuf, sync::Arc, time::Duration};

use libfuzzer_sys::fuzz_target;
use relay::{
    config::{Config, TlsConfig},
    proxy, tls,
};
use rustls::pki_types::ServerName;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time::timeout,
};
use tokio_rustls::TlsConnector;

const HTTP2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";
const HTTP2_SETTINGS: &[u8] = &[0, 0, 0, 4, 0, 0, 0, 0, 0];

fuzz_target!(|input: &[u8]| {
    let input = &input[..input.len().min(65_536)];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create fuzz runtime");

    runtime.block_on(async {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("../tests/fixtures/tls");
        let certificate = fixture.join("localhost-cert.pem");
        let private_key = fixture.join("localhost-key.pem");
        let ca = fixture.join("test-ca.pem");
        let config_text = format!(
            "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 2\n\n[limits]\nmax_concurrent_requests = 2\nmax_concurrent_streams_per_connection = 2\nheader_timeout_ms = 20\nconnect_timeout_ms = 20\nresponse_timeout_ms = 20\nbody_idle_timeout_ms = 20\nshutdown_timeout_ms = 20\nmax_body_bytes = 65536\n\n[tls]\ncertificate = \"{}\"\nprivate_key = \"{}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://127.0.0.1:1\"\n",
            certificate.display(),
            private_key.display()
        );
        let Ok(config) = Config::parse(&config_text) else { return };
        let Ok(server_tls) = tls::load_server_config(&config.tls) else { return };
        let upstream_tls = TlsConfig {
            certificate: None,
            private_key: None,
            upstream_ca_files: vec![ca],
        };
        let Ok(upstream_tls) = tls::build_upstream_client_config(&upstream_tls) else { return };
        let client_trust = TlsConfig {
            certificate: None,
            private_key: None,
            upstream_ca_files: vec![fixture.join("test-ca.pem")],
        };
        let Ok(mut client_tls) = tls::build_upstream_client_config(&client_trust) else { return };
        Arc::get_mut(&mut client_tls)
            .expect("fresh client TLS config")
            .alpn_protocols = vec![b"h2".to_vec()];
        let connector = TlsConnector::from(client_tls.clone());

        let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else { return };
        let Ok(address) = listener.local_addr() else { return };
        let server = tokio::spawn(proxy::serve_with_tls(
            listener,
            Arc::new(config),
            Some(server_tls),
            upstream_tls,
            std::future::pending::<()>(),
        ));

        if let Ok(Ok(socket)) = timeout(Duration::from_millis(100), TcpStream::connect(address)).await
            && let Ok(Ok(mut stream)) = timeout(
                Duration::from_millis(100),
                connector.connect(ServerName::try_from("localhost").unwrap(), socket),
            ).await
        {
            let _ = timeout(Duration::from_millis(50), stream.write_all(HTTP2_PREFACE)).await;
            let _ = timeout(Duration::from_millis(50), stream.write_all(HTTP2_SETTINGS)).await;
            let _ = timeout(Duration::from_millis(50), stream.write_all(input)).await;
            let _ = stream.shutdown().await;
            let mut response = Vec::new();
            let _ = timeout(Duration::from_millis(50), stream.read_to_end(&mut response)).await;
        }

        server.abort();
    });
});
