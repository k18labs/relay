# Security model

Relay trusts the operator, validated local configuration, TLS credential files, and configured upstream names. Client requests and upstream responses are untrusted. Client requests can select only among configured routes.

Hyper owns HTTP parsing and serialization. Relay uses typed headers, removes hop-by-hop and untrusted forwarding fields, rejects missing or duplicate Host values, enforces streaming body limits, and does not follow redirects. rustls provides TLS 1.3 inbound and verified upstream transport; upstream certificate-chain and hostname checks are enabled. TLS early data is disabled.

Connection, request, and TLS handshake concurrency are capped. Headers and bodies have configured limits. Header, connect, response, handshake, body-progress, upstream-pool idle, and shutdown deadlines bound time spent on slow peers. The header deadline also applies while waiting for the next request on keep-alive connections.

Relay accepts only origin-form request targets, validates explicit authority ports, and rejects transfer-coding chains it does not support. Hyper parses inbound messages and canonicalizes transfer-encoding plus content-length according to its HTTP/1 parser; Relay does not inspect or re-parse raw wire headers. A regression test checks that the forwarded request has one unambiguous chunked framing.

Connection writes, the full upstream connector operation (including TLS negotiation), keep-alive request admission, listener retries, and shutdown are covered by bounded deadlines or explicit drain behavior. These safeguards have local regression coverage; they do not establish a production security claim.

Current PR CI passed locked tests, formatting, Clippy, optimized builds, and repeat-build checks on Linux x86-64 and ARM64; three 1,000-iteration fuzz smoke runs passed on ARM64. Three-repeat normal and connection-cap tests plus SIGTERM-under-load checks are recorded for both GitHub-hosted Linux architectures. These virtualized runs do not establish production performance; full performance characterization, external independent security review, and published SBOM/provenance/checksums remain open. Rust memory safety does not guarantee proxy correctness; do not treat this as a production release.
