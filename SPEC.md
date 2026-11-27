# Relay requirements

Relay is a lightweight, security-focused HTTP reverse proxy written in Rust. The intended release targets Linux x86-64 and ARM64. Secure defaults and predictable resource use take priority over added features or binary-size targets.

## Implemented development scope

- CLI commands: `run`, `check`, and `version`.
- Strict TOML schema and validation before listener binding.
- TCP HTTP/1.1 and HTTP/2, plus HTTP/3 over QUIC/UDP when TLS is enabled, using fixed configured destinations, normalized host matching, and longest segment-boundary path selection. HTTP/2 and HTTP/3 requests use the same routing and policy path; upstream connections remain HTTP/1.1.
- TLS 1.3 inbound termination and HTTPS upstream verification, with no early data.
- Streaming request and response bodies; bounded connections, requests, handshakes, headers, bodies, upstream idle pool, timeouts, and shutdown.
- Hop-by-hop and untrusted forwarding header removal; `CONNECT` and upgrades rejected.

This is an early development implementation, not a release candidate. Current PR Linux builds and bounded fuzz/load smoke data exist, but release-grade sustained performance evidence, independent security review, SBOM, provenance, repeat-build evidence, and signed release artifacts remain outstanding.

## Product requirements

- Use established Tokio, Hyper, and rustls protocol implementations; no custom cryptography or HTTP parser.
- Do not derive upstream destinations from client-controlled input; do not follow redirects or retry requests automatically.
- Reject malformed, conflicting, or unsupported configuration before binding.
- Bound externally controlled resource use and clean up tasks predictably.
- Do not log request headers, bodies, credentials, or cookies. No telemetry or external administration interface.
- Keep dependencies minimal, locked, and license-compatible.

## Excluded from initial scope

Scripting, plugins, general forward proxying, arbitrary destinations, Kubernetes controller, database, general WAF, caching, automatic ACME, dashboard, remote admin API, load balancing, health checks, and configuration/certificate reload.

HTTP/3 external-client interoperability and performance characterization remain open release work; passing in-process tests alone is not release acceptance.

All size, memory, throughput, and latency goals remain unverified until measured on documented Linux hardware.
