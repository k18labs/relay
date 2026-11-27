#![no_main]

use std::{path::PathBuf, sync::Arc, time::Duration};

use bytes::Bytes;
use h3::error::Code;
use http::{HeaderName, HeaderValue, Method, Request};
use libfuzzer_sys::fuzz_target;
use quinn::{ClientConfig, Endpoint, VarInt, crypto::rustls::QuicClientConfig};
use relay::{
    config::{Config, TlsConfig},
    proxy, tls,
};
use tokio::{net::TcpListener, sync::oneshot, time::timeout};

const MAX_INPUT_BYTES: usize = 65_536;
const OPERATION_TIMEOUT: Duration = Duration::from_millis(100);

fuzz_target!(|input: &[u8]| {
    let input = &input[..input.len().min(MAX_INPUT_BYTES)];
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("create fuzz runtime");

    runtime.block_on(async {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("../tests/fixtures/tls");
        let certificate = fixture.join("localhost-cert.pem");
        let private_key = fixture.join("localhost-key.pem");
        let config_text = format!(
            "[server]\nlisten = \"127.0.0.1:0\"\nmax_connections = 2\n\n[limits]\nmax_concurrent_requests = 2\nmax_concurrent_streams_per_connection = 2\nheader_timeout_ms = 50\nconnect_timeout_ms = 50\nresponse_timeout_ms = 50\nbody_idle_timeout_ms = 50\nshutdown_timeout_ms = 100\nmax_body_bytes = 65536\n\n[tls]\ncertificate = \"{}\"\nprivate_key = \"{}\"\n\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://127.0.0.1:1\"\n",
            certificate.display(),
            private_key.display()
        );
        let Ok(config) = Config::parse(&config_text) else { return };
        let Ok(server_tls) = tls::load_server_config(&config.tls) else { return };
        let upstream_tls = TlsConfig {
            certificate: None,
            private_key: None,
            upstream_ca_files: Vec::new(),
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
            .alpn_protocols = vec![b"h3".to_vec()];
        let Ok(crypto) = QuicClientConfig::try_from(client_tls) else { return };
        let Ok(mut endpoint) = Endpoint::client("127.0.0.1:0".parse().expect("loopback address"))
        else {
            return;
        };
        endpoint.set_default_client_config(ClientConfig::new(Arc::new(crypto)));

        let Ok(listener) = TcpListener::bind("127.0.0.1:0").await else { return };
        let Ok(address) = listener.local_addr() else { return };
        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let server = tokio::spawn(proxy::serve_with_tls(
            listener,
            Arc::new(config),
            Some(server_tls),
            upstream_tls,
            async move {
                let _ = shutdown_rx.await;
            },
        ));

        let mut driver_task = None;
        if let Ok(connecting) = endpoint.connect(address, "localhost")
            && let Ok(Ok(connection)) = timeout(OPERATION_TIMEOUT, connecting).await
        {
            let field_section_size = u16::from_be_bytes([
                input.first().copied().unwrap_or(0),
                input.get(1).copied().unwrap_or(0),
            ]) as u64;
            let reset_stream = input.get(2).is_some_and(|byte| byte & 1 == 1);
            let body = input.get(3..).unwrap_or_default();
            let mut builder = h3::client::builder();
            builder
                .max_field_section_size(field_section_size)
                .send_grease(input.get(2).is_some_and(|byte| byte & 2 == 2));

            if let Ok(Ok((mut driver, mut sender))) = timeout(
                OPERATION_TIMEOUT,
                builder.build::<_, _, Bytes>(h3_quinn::Connection::new(connection)),
            )
            .await
            {
                driver_task = Some(tokio::spawn(async move { driver.wait_idle().await }));
                let mut request = Request::builder()
                    .method(Method::POST)
                    .uri("https://example.org/fuzz")
                    .body(())
                    .expect("fixed request is valid");
                if let Ok(value) = HeaderValue::from_bytes(body) {
                    request
                        .headers_mut()
                        .insert(HeaderName::from_static("x-relay-fuzz"), value);
                }

                if let Ok(Ok(mut stream)) =
                    timeout(OPERATION_TIMEOUT, sender.send_request(request)).await
                {
                    if reset_stream {
                        let prefix = &body[..body.len().min(1024)];
                        let _ = timeout(
                            OPERATION_TIMEOUT,
                            stream.send_data(Bytes::copy_from_slice(prefix)),
                        )
                        .await;
                        stream.stop_stream(Code::H3_REQUEST_CANCELLED);
                    } else {
                        let _ = timeout(
                            OPERATION_TIMEOUT,
                            stream.send_data(Bytes::copy_from_slice(body)),
                        )
                        .await;
                        let _ = timeout(OPERATION_TIMEOUT, stream.finish()).await;
                        if let Ok(Ok(_response)) =
                            timeout(OPERATION_TIMEOUT, stream.recv_response()).await
                        {
                            let _ = timeout(OPERATION_TIMEOUT, async {
                                while stream.recv_data().await?.is_some() {}
                                Ok::<(), h3::error::StreamError>(())
                            })
                            .await;
                        }
                    }
                }
            }
        }

        endpoint.close(VarInt::from_u32(0), b"fuzz iteration complete");
        if let Some(driver) = driver_task {
            let _ = timeout(OPERATION_TIMEOUT, driver).await;
        }
        let _ = shutdown_tx.send(());
        let _ = timeout(OPERATION_TIMEOUT, server).await;
    });
});
