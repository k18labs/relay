# Architecture

Relay is a single Rust crate. Configuration is parsed and validated before the listener is bound. TLS credentials and upstream trust roots are loaded into rustls configurations before startup.

Each accepted socket consumes a bounded connection permit. TLS handshakes consume a separate bounded permit and have a deadline. Hyper serves HTTP/1.1 over the resulting stream. Each request requires one Host field, then routes by normalized host and longest segment-boundary path prefix. The configured origin determines the upstream scheme and authority; client request data cannot choose a destination.

Hyper and `hyper-rustls` own HTTP and TLS protocol work. Request and response bodies stream through bounded wrappers. Request concurrency, body size, headers, connection setup, response start, body progress, idle keep-alive header reads, upstream idle pool size, and graceful shutdown are bounded by configuration. Request and response hop-by-hop fields are removed. Client forwarding identity is not trusted or generated.

`CONNECT` and protocol upgrades are unsupported. There is no reload, HTTP/2, load balancing, health checking, admin interface, access log, or automatic certificate issuance. See `DESIGN.md` and `docs/CONFIGURATION.md`.
