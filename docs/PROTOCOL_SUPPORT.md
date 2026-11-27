# Protocol support matrix

This table describes the current Relay implementation and the exact interoperability evidence in this repository. It is not a broad compatibility guarantee; repeat external-client checks when updating the toolchain or protocol dependencies.

| Protocol | Listener | TLS and negotiation | Evidence | Known limits |
| --- | --- | --- | --- | --- |
| HTTP/1.1 | TCP | Plaintext for development, or TLS 1.3 | Raw TCP integration coverage in `tests/proxy.rs`; competitor benchmark clients included curl 8.21.0 | Request bodies are streamed and capped by configured limits. `CONNECT` and protocol upgrades return 501. |
| HTTP/2 | TCP | TLS 1.3; ALPN `h2` | ALPN integration coverage in `tests/tls_proxy.rs`; HTTP/2 requests and streams are covered by tests. Diagnostic competitor benchmark used h2load/nghttp2 1.70.0. | TLS is required; cleartext HTTP/2 is not supported. Upstream requests use HTTP/1.1. NGINX H2 benchmark runs were inconsistent; see [benchmark evidence](BENCHMARKS.md). |
| HTTP/3 | UDP/QUIC on the same numeric port as TCP | TLS 1.3; HTTP/3 ALPN; 0-RTT disabled | In-process `h3` integration tests in `tests/http3_proxy.rs`; external curl 8.21.0 H3 smoke and diagnostic runs on WSL2 ARM64 | TLS is required and the UDP bind must succeed at startup. External interoperability evidence is currently limited to curl 8.21.0. Upstream requests use HTTP/1.1. Apache 2.4.68 in the recorded benchmark environment had no HTTP/3 module. |

## Versions in the recorded external comparison

The 2026-11-27 WSL2 ARM64 comparison used Relay 0.1.0, Caddy 2.11.4, NGINX 1.30.4, Apache 2.4.68, curl 8.21.0, and h2load/nghttp2 1.70.0. These are benchmark-environment versions, not a minimum supported version policy. The environment and protocol-specific caveats are recorded in [`docs/perf-results/2026-11-27-wsl-arm64-http3-curl-comparison.txt`](perf-results/2026-11-27-wsl-arm64-http3-curl-comparison.txt).

When updating this matrix, record the tested client and server versions, transport, TLS mode, and whether the evidence is an integration test or an external interoperability run. Do not claim a protocol for a server/version combination unless that exact combination completed the check.
