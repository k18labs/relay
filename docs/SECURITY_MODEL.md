# Security model

Relay trusts the operator, validated local configuration, TLS credential files, and configured upstream names. Client requests and upstream responses are untrusted. Client requests can select only among configured routes.

Hyper owns HTTP parsing and serialization. Relay uses typed headers, removes hop-by-hop and untrusted forwarding fields, rejects missing or duplicate Host values, enforces streaming body limits, and does not follow redirects. rustls provides TLS 1.3 inbound and verified upstream transport; upstream certificate-chain and hostname checks are enabled. TLS early data is disabled.

Connection, request, and TLS handshake concurrency are capped. Headers and bodies have configured limits. Header, connect, response, handshake, body-progress, upstream-pool idle, and shutdown deadlines bound time spent on slow peers. The header deadline also applies while waiting for the next request on keep-alive connections.

Relay accepts only origin-form request targets, validates explicit authority ports, and rejects transfer-coding chains it does not support. Hyper parses inbound messages and canonicalizes transfer-encoding plus content-length according to its HTTP/1 parser; Relay does not inspect or re-parse raw wire headers. A regression test checks that the forwarded request has one unambiguous chunked framing.

Connection writes, the full upstream connector operation (including TLS negotiation), keep-alive request admission, listener retries, and shutdown are covered by bounded deadlines or explicit drain behavior. These safeguards have local regression coverage; they do not establish a production security claim.

Current PR CI passed locked tests, formatting, Clippy, and optimized builds on Linux x86-64 and ARM64; three 1,000-iteration fuzz smoke runs passed on ARM64. WSL ARM64 normal-load and connection-cap runs are recorded, but their high run-to-run variation, timeouts, and socket errors make them unsuitable as release evidence. A WSL SIGTERM-under-load check exited cleanly within its 7-second watchdog. Native sustained load tests, repeatable comparative performance results, external independent security review, and published SBOM/provenance/checksums have not yet been completed. Rust memory safety does not guarantee proxy correctness; do not treat this as a production release.
