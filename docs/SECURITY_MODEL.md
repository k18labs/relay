# Security model

Relay trusts the operator, validated local configuration, TLS credential files, and configured upstream names. Client requests and upstream responses are untrusted. Client requests can select only among configured routes.

Hyper owns HTTP parsing and serialization. Relay uses typed headers, removes hop-by-hop and untrusted forwarding fields, rejects missing or duplicate Host values, enforces streaming body limits, and does not follow redirects. rustls provides TLS 1.3 inbound and verified upstream transport; upstream certificate-chain and hostname checks are enabled. TLS early data is disabled.

Connection, request, and TLS handshake concurrency are capped. Headers and bodies have configured limits. Header, connect, response, handshake, body-progress, upstream-pool idle, and shutdown deadlines bound time spent on slow peers. The header deadline also applies while waiting for the next request on keep-alive connections.

This development build has local regression tests for the above behaviors but lacks fuzzing, multi-platform Linux verification, load testing, and independent security review. Rust memory safety does not guarantee proxy correctness; do not treat this as a production release.
