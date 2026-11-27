# Changelog

## Unreleased

- Added inbound HTTP/2 over TLS/TCP and HTTP/3 over QUIC/UDP, with bounded per-connection streams and shared route, request, body, and shutdown policy. Fixed HTTP/3 response stream resets when an upstream responds before the client upload reaches EOF; external curl smoke and diagnostic three-repeat throughput runs now complete successfully. Broader interoperability and native-host performance characterization remain open.
- Added a reproducible Caddy, NGINX, and Apache HTTP/1.1 and HTTP/2 comparison harness and WSL2 ARM64 raw results. These are diagnostic only and make no performance ranking claim.
- Added downstream TLS 1.3 and verified HTTPS upstream transport using rustls.
- Added strict TLS credential and trust-root loading, bounded request/connection handling, body and header limits, timeouts, and graceful shutdown.
- Expanded local HTTP, TLS, CLI, and configuration regression tests.
- Hardened request-target, authority, transfer-coding, upstream connector/cancellation, downstream write-timeout, connection-pool, and graceful-shutdown handling.
- Replaced the unmaintained `rustls-pemfile` dependency with the PEM API in `rustls-pki-types`.
- Added bounded fuzz targets, a Relay/NGINX benchmark smoke harness, draft release automation, and an evidence-based release checklist.
- Updated project documentation to describe the current Rust implementation and remaining release work.

## 0.1.0-alpha.1

- Initial public development snapshot. Not a production release.
