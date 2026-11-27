# Relay Foundation Design

## Intent

Create the initial Relay foundation as a small Rust reverse proxy with fixed, validated routes and no client-selected upstreams. The project is new and the target directory has no Git metadata or existing source.

## Goals and non-goals

The first iteration covers M0 and the HTTP portion of M1: strict TOML configuration parsing, `run`/`check`/`version` CLI, a TCP HTTP/1.1 listener, deterministic host/path routing, and forwarding to explicitly configured HTTP upstreams. HTTPS upstreams, inbound TLS termination, reloads, retries, and HTTP/2 are out of scope until implemented and separately verified.

## Constraints

Rust stable; Tokio, Hyper, rustls, Serde/TOML, and tracing remain the intended stack. Unsafe code is forbidden in first-party code. Avoid unsupported security or performance claims. This Windows workspace is a development environment; Linux x86-64 and ARM64 remain release targets.

## Approaches

1. Use Hyper/Tokio directly: few layers and explicit control, but more protocol and lifecycle details to implement.
2. Use a higher-level web framework: less code, but adds routing behavior and abstractions that do not fit the constrained proxy core.
3. Build an HTTP parser and proxy from scratch: rejected because it increases protocol risk and duplicates mature implementations.

**Selected:** Hyper/Tokio directly. It preserves control while relying on a mature HTTP implementation.

## Architecture and calls

Configuration is deserialized with unknown fields rejected, validated before binding, and shared immutably. Routes match normalized DNS host names exactly and select the longest configured path prefix; ties are invalid. A route maps to one fixed upstream. Connection admission uses a Tokio semaphore; each accepted connection is served by Hyper. Forwarded requests strip hop-by-hop headers, including names listed by `Connection`, before sending to the chosen upstream. No access log includes request headers or body.

The first iteration will not claim full request-body byte limiting, TLS, or hardened framing behavior beyond Hyper's parser. These are explicit gaps to close before treating Relay as production-ready.

## Acceptance

`relay check` rejects malformed, unknown-field, and ambiguous configuration. Local integration tests demonstrate fixed-route forwarding, deterministic route selection, and rejection of unknown hosts. `cargo fmt --check`, `cargo clippy -- -D warnings`, and `cargo test --locked` pass on the available environment.
