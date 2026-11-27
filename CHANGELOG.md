# Changelog

## Unreleased

- Added downstream TLS 1.3 and verified HTTPS upstream transport using rustls.
- Added strict TLS credential and trust-root loading, bounded request/connection handling, body and header limits, timeouts, and graceful shutdown.
- Expanded local HTTP, TLS, CLI, and configuration regression tests.
- Hardened request-target, authority, transfer-coding, upstream connector/cancellation, downstream write-timeout, connection-pool, and graceful-shutdown handling.
- Replaced the unmaintained `rustls-pemfile` dependency with the PEM API in `rustls-pki-types`.
- Added bounded fuzz targets, a Relay/NGINX benchmark smoke harness, draft release automation, and an evidence-based release checklist.
- Updated project documentation to describe the current Rust implementation and remaining release work.

## 0.1.0-alpha.1

- Initial public development snapshot. Not a production release.
