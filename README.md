# Relay

Relay is a small Rust HTTP/1.1 reverse proxy by k18 Labs. It routes only to configured upstream origins and uses Hyper, Tokio, and rustls for protocol and transport handling.

## Development status

The current implementation supports inbound TLS 1.3, verified HTTPS upstreams, strict TOML configuration, host/path routing, streaming HTTP bodies, connection reuse, request and connection admission limits, header/body limits, timeouts, and bounded graceful shutdown. ARM64 Linux locked tests, formatting, Clippy, and three 1,000-iteration fuzz smoke runs passed locally. Linux x86-64 evidence for the current changes, sustained load and comparative performance results, external independent security review, and verified release artifacts remain incomplete. This is not a production release.

## Build and run

Use the pinned toolchain in `rust-toolchain.toml`.

```sh
cargo build --locked
cargo test --locked
cargo run -- check --config relay.example.toml
cargo run -- run --config relay.example.toml
```

TLS is enabled by default. Set `tls.certificate` and `tls.private_key` to PEM files. Set `server.tls = false` only for local plaintext development. HTTPS upstream certificates and hostnames are verified using the system-independent WebPKI roots plus optional configured CA files.

Routes match a normalized host and a segment-boundary path prefix; the longest matching path wins. Upstream destinations come only from configuration. `CONNECT` and protocol upgrades return 501. Forwarded client identity headers are stripped rather than trusted or generated.

See [configuration](docs/CONFIGURATION.md), [design](DESIGN.md), [threat model](docs/THREAT_MODEL.md), [testing](docs/TESTING.md), and [roadmap](ROADMAP.md).

## License

Apache-2.0. See [LICENSE](LICENSE).
