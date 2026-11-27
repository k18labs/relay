# Inbound HTTP/2 and HTTP/3 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add bounded inbound HTTP/2 and HTTP/3 to Relay and publish reproducible protocol-specific comparisons against Caddy, NGINX, and Apache.

**Architecture:** Use Hyper for HTTP/1.1 and HTTP/2 on TCP, TLS 1.3 ALPN to select the protocol, and Quinn plus `h3`/`h3-quinn` for HTTP/3 on UDP at the same configured port. Adapt all protocols to the same proxy request policy and configured HTTP/1.1 upstream client. Do not enable HTTP/3 discovery until the UDP service is live.

**Tech Stack:** Rust, Tokio, Hyper, rustls, Quinn, `h3`, `h3-quinn`; curl/nghttp or equivalent protocol-capable clients; Caddy, NGINX, Apache httpd.

**Spec:** `SPEC.md`, `DESIGN.md`, `docs/THREAT_MODEL.md`, and `docs/adr/0002-inbound-http2-http3.md`.

## Global Constraints

- Preserve `#![forbid(unsafe_code)]`; do not add unsafe Rust.
- Use Hyper/rustls protocol implementations; never implement cryptography or HTTP parsing from scratch.
- Keep upstream destinations configuration-controlled.
- Reject unknown configuration fields and conflicting routes; validate before binding sockets.
- Keep client bodies streaming, with explicit byte/time limits before any production claim.
- Remove hop-by-hop and untrusted forwarding headers; do not log request headers, bodies, credentials, or cookies.
- Bound connections, concurrent work, buffers, pools, and shutdown duration.
- Add regression tests for behavior changes and run fmt, Clippy with warnings denied, and full locked tests.
- Keep dependencies minimal, locked, and license-compatible; review new dependencies and features.

## Review Focus

- Malformed HTTP/2/3 requests and pseudo-header/authority inconsistencies: pin adapter validation and route behavior with protocol integration tests.
- Excess concurrent streams and QUIC handshakes: test admission limits and bounded shutdown.
- Slow or oversized streamed bodies: verify byte and idle-time limits remain enforced without whole-body buffering.
- UDP bind or TLS ALPN setup failure: test startup fails before any listener begins serving.
- HTTP/3 advertisement without a working UDP listener: verify startup ordering and response headers.

---

### Task 1: Protocol-neutral request dispatch

**Files:**
- Modify: `src/proxy.rs`
- Test: `src/proxy.rs` protocol integration tests

**Interfaces:**
- Consumes: current `forward(Request<Incoming>, ...)` policy and configured `ProxyClient`.
- Produces: a protocol-neutral request-body adapter used by H1/H2/H3 without weakening streaming bounds.

- [ ] Add a regression test proving both HTTP/1 and HTTP/2 use identical host/path routing and header stripping.
- [ ] Run the focused test and confirm failure for missing HTTP/2 support.
- [ ] Implement Hyper HTTP/2 ALPN serving and shared policy dispatch.
- [ ] Run focused tests and full locked suite.

### Task 2: Bounded HTTP/3 transport

**Files:**
- Modify: `Cargo.toml`, `Cargo.lock`, `src/proxy.rs`, `src/tls.rs`, `src/config.rs`, `src/main.rs`
- Test: protocol integration tests and configuration tests

**Interfaces:**
- Consumes: task 1 request dispatch; existing TLS material and limits.
- Produces: a validated UDP HTTP/3 listener sharing routing and upstream behavior.

- [ ] Add tests for HTTP/3 forwarding, policy parity, body limits, admission, and shutdown.
- [ ] Verify those tests fail before implementation.
- [ ] Add the smallest safe QUIC/HTTP/3 dependency feature set; configure TLS 1.3, ALPN `h3`, and no early data.
- [ ] Bind TCP and UDP before serving; bound QUIC connections, streams, buffers, deadlines, and shutdown.
- [ ] Run protocol tests, `cargo fmt`, `cargo clippy --locked --all-targets -- -D warnings`, and `cargo test --locked --all-targets`.

### Task 3: Reproducible competitor benchmarks

**Files:**
- Modify: `.github/workflows/benchmark.yml`, `scripts/load/`
- Create: `docs/BENCHMARKS.md`, committed raw result files under `docs/perf-results/`

**Interfaces:**
- Consumes: working H1/H2/H3 listeners and protocol-aware benchmark clients.
- Produces: versioned raw and summarized measurements for Relay, Caddy, NGINX, and Apache, with unsupported protocol combinations marked unavailable.

- [ ] Write smoke assertions for each benchmark server/protocol and identical response payload.
- [ ] Run the benchmark workflow on x86-64 and ARM64; retain exact versions, settings, raw output, and errors.
- [ ] Document methodology, protocol availability, results, caveats, and reproduction commands.

### Task 4: Release documentation and evidence

**Files:**
- Modify: `SPEC.md`, `DESIGN.md`, `README.md`, `ROADMAP.md`, `docs/THREAT_MODEL.md`, `docs/CONFIGURATION.md`, `docs/RELEASE_CHECKLIST.md`, `CHANGELOG.md`

- [ ] Update behavior and configuration docs only after implementation evidence exists.
- [ ] Run all available release checks, scheduled fuzz targets, and repeat-build checks.
- [ ] Push the branch and verify GitHub checks. Keep external-review, physical-host, and clean-tagged-artifact gates open until evidence exists.
- [ ] Merge only after the required review gate passes; do not bypass branch protection.
