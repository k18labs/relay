# Relay

Relay is a small Rust reverse proxy by k18 Labs. It accepts HTTP/1.1 and HTTP/2 over TCP/TLS and HTTP/3 over QUIC/TLS, routes only to configured upstream origins, and uses Hyper, Tokio, rustls, Quinn, and `h3` for protocol and transport handling. Upstream requests currently use HTTP/1.1.

## Development status

The current implementation supports inbound TLS 1.3, HTTP/1.1 and HTTP/2 over TCP, HTTP/3 over UDP when TLS is enabled, verified HTTPS upstreams, strict TOML configuration, host/path routing, streaming HTTP bodies, connection reuse, request and connection admission limits, header/body limits, timeouts, and bounded graceful shutdown. After fixing early-response stream resets, curl 8.21.0 completed one external HTTP/3 smoke run successfully; broader HTTP/3 interoperability and throughput characterization remain open. Full performance characterization, an external independent security review, and verified release artifacts remain incomplete. This is not a production release.

## Build and run

Use the pinned toolchain in `rust-toolchain.toml`.

```sh
cargo build --locked
cargo test --locked
cargo run -- check --config relay.example.toml
cargo run -- run --config relay.example.toml
```

TLS is enabled by default. Set `tls.certificate` and `tls.private_key` to PEM files. With TLS enabled, HTTP/2 is negotiated over TCP and HTTP/3 binds UDP on the same numeric port as `server.listen`; both transports use the same configured routes and limits. Set `server.tls = false` only for local plaintext development. HTTPS upstream certificates and hostnames are verified using the system-independent WebPKI roots plus optional configured CA files.

Routes match a normalized host and a segment-boundary path prefix; the longest matching path wins. Upstream destinations come only from configuration. `CONNECT` and protocol upgrades return 501. Forwarded client identity headers are stripped rather than trusted or generated.

See [configuration](docs/CONFIGURATION.md), [design](DESIGN.md), [threat model](docs/THREAT_MODEL.md), [benchmark report](docs/BENCHMARKS.md), [testing](docs/TESTING.md), and [roadmap](ROADMAP.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
