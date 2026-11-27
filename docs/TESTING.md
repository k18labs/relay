# Testing

Run formatting, lint, and locked tests with the pinned toolchain:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Current automated coverage includes configuration bounds and strict fields, route selection, CLI checks, fixed-upstream HTTP forwarding, header stripping, request/response body limits, framing cases, header/body/response/client idle deadlines, admission limits, graceful shutdown, and upstream connection reuse. TLS fixtures are synthetic and local; tests cover certificate/key loading, valid upstream trust, untrusted and wrong-host upstream certificates, inbound TLS, and stalled handshakes.

Fuzz targets live in `fuzz/` and can be run with a nightly toolchain and cargo-fuzz:

```sh
cargo +nightly-2026-11-25 fuzz run config_routes -- -max_total_time=60 -max_len=65536 -timeout=5
cargo +nightly-2026-11-25 fuzz run host_normalization -- -max_total_time=60 -max_len=65536 -timeout=5
cargo +nightly-2026-11-25 fuzz run header_policy -- -max_total_time=60 -max_len=65536 -timeout=5
```

The scheduled fuzz workflow runs bounded jobs and uploads corpora on failure. Local smoke runs are development checks, not proof of exhaustive protocol safety. The tests and fuzz targets do not replace sustained load testing, comparative performance measurements, independent external review, or production certificate management.
