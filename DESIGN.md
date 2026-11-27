# Relay design

## Request path

Client TCP connection → bounded connection admission → optional TLS 1.3 termination → Hyper HTTP/1 or HTTP/2 parsing, or QUIC/HTTP/3 parsing over UDP → shared validation and policy → configured host/path route → HTTP or verified HTTPS upstream over HTTP/1.1 → streamed response.

## Current design

Configuration is parsed, validated, and TLS material loaded before listener binding. The immutable config is shared across connection tasks. Semaphores bound connections, concurrent requests, and TLS handshakes. Hyper owns HTTP/1 and HTTP/2 parsing/framing; the `h3` and Quinn implementations own HTTP/3 and QUIC; the legacy client provides upstream pooling. HTTP/2 is negotiated with ALPN on TCP. With TLS enabled, the QUIC endpoint binds UDP to the same configured numeric port and negotiates HTTP/3. `hyper-rustls` and rustls provide upstream TLS with chain and hostname checks. TLS uses TLS 1.3 and early data is disabled.

Bodies stream through byte limits. Header sizes/counts, connection establishment, response start, body progress, idle keep-alive header reads, upstream pool idle time, and shutdown are time-bounded. HTTP/2 and HTTP/3 use a configured per-connection stream bound. Route selection uses the HTTP/1 Host field or the HTTP/2/3 authority, normalized host matching, and the longest segment-boundary path prefix. Unknown hosts receive 421; upstream failures receive 502. `CONNECT` and upgrades receive 501.

Hop-by-hop fields and values named by `Connection` are stripped. Untrusted forwarding fields are removed; Relay does not currently create replacement client identity headers. Logs avoid request details and secrets.

## Trust boundaries

- Client bytes are untrusted and parsed by Hyper.
- Configuration and TLS files are operator-controlled, then validated before the listener starts.
- Upstream origins are fixed in configuration; requests cannot select an origin.
- Upstream responses are untrusted and pass through Hyper's HTTP implementation and configured body bounds.
- Operational logs exclude request headers, bodies, credentials, and cookies.

External curl HTTP/3 response termination and three-repeat diagnostic throughput runs now succeed after the early-response stream fix. Current PR fuzz smoke and Linux x86-64/ARM64 CI passed on `285143c`; rerun validation on the final release commit. Broader interoperability, native-host HTTP/3 performance characterization, physical-host load measurements, independent review, and release artifact verification remain incomplete. See `docs/ARCHITECTURE.md`, `docs/THREAT_MODEL.md`, and the architecture decisions under `docs/adr/`.
