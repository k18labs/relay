# Testing

Run formatting, lint, and locked tests with the pinned toolchain:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Current automated coverage includes configuration bounds and strict fields, route selection, CLI checks, fixed-upstream HTTP forwarding, header stripping, request/response body limits, framing cases, header/body/response/client idle deadlines, admission limits, graceful shutdown, and upstream connection reuse. TLS fixtures are synthetic and local; tests cover certificate/key loading, valid upstream trust, untrusted and wrong-host upstream certificates, inbound TLS, and stalled handshakes.

The tests do not replace fuzzing, stress/load testing, multi-platform CI, independent review, or production certificate management. Fuzz targets and performance measurements remain to be added.
