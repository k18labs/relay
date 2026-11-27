# Configuration

Relay reads TOML. Unknown fields are rejected. `relay check --config FILE` parses and validates configuration and TLS files without binding the listener.

```toml
[server]
listen = "127.0.0.1:8443"
max_connections = 512
# Tokio worker threads; defaults to 2 and must be between 1 and 64.
workers = 2
# TLS defaults to true. Set false only for local plaintext development.
tls = true

[tls]
certificate = "certs/relay.pem"
private_key = "certs/relay-key.pem"
# Optional additional trust roots for private HTTPS upstreams.
# upstream_ca_files = ["certs/internal-ca.pem"]

[limits]
max_header_bytes = 16384
max_header_count = 64
max_body_bytes = 10485760
max_concurrent_requests = 1024
# Per HTTP/2 or HTTP/3 connection; must be between 1 and 1024.
max_concurrent_streams_per_connection = 64
max_concurrent_tls_handshakes = 128
header_timeout_ms = 5000
handshake_timeout_ms = 5000
connect_timeout_ms = 5000
response_timeout_ms = 30000
body_idle_timeout_ms = 30000
upstream_idle_timeout_ms = 30000
shutdown_timeout_ms = 5000
max_idle_upstream_per_host = 8

[[routes]]
host = "example.org"
path = "/"
upstream = "https://127.0.0.1:3000"
```

TLS uses TLS 1.3. When enabled, both certificate and private-key paths are required. Upstream HTTPS uses built-in WebPKI roots plus optional PEM trust roots, and validates the certificate chain and hostname. Relative paths are resolved from the process working directory. `server.tls = false` disables inbound TLS and does not accept inbound TLS credentials.

With inbound TLS enabled, Relay negotiates HTTP/1.1 or HTTP/2 over the configured TCP listener and HTTP/3 over QUIC/UDP on the same numeric port. The UDP socket must be available at startup. `max_concurrent_streams_per_connection` bounds HTTP/2 and HTTP/3 streams and must be between 1 and 1024. HTTP/3 external-client response termination is still being validated; see [benchmark notes](BENCHMARKS.md).

Route hosts are case-insensitive DNS names or IPv4 literals; a final DNS dot is ignored. Paths are absolute prefixes matched on segment boundaries (`/api` matches `/api/x`, not `/apix`). The longest matching path wins. Duplicate normalized host/path pairs are rejected. Upstreams must be HTTP or HTTPS origins with no credentials, path, or query.

Limits have validated bounds. Bodies stream through Relay and are capped at `max_body_bytes`; connection admission, concurrent requests, TLS handshakes, headers, and idle upstream pooling are bounded. Header, body progress, connection, response, handshake, and shutdown timeouts are in milliseconds and must be nonzero. The header deadline also limits idle keep-alive connections. See `src/config.rs` for exact supported ranges and defaults.
