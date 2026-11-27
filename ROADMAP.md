# Roadmap

## M0 — Foundation (implementation present; cross-platform acceptance incomplete)

- [x] Rust crate, pinned toolchain, lockfile, strict config parser, and `run`/`check`/`version` CLI.
- [x] CI and dependency/license policy files.
- [x] Linux x86-64 and ARM64 builds and CI evidence for PR commit `285143c`: both GitHub Actions jobs passed formatting, Clippy, locked tests, optimized builds, and clean repeat-build hash comparisons on 2026-11-27.

## M1 — HTTP proxy (implemented; broader validation remains)

- [x] TCP listener, bounded connection admission, HTTP/1.1 forwarding, fixed routes, streaming bodies, and hop-by-hop cleanup.
- [x] Header/body bounds, request concurrency limit, progress timeouts, upstream pooling bounds, and graceful shutdown deadline.
- [x] Local regression coverage for routing, forwarding, limits, timeouts, shutdown, and upstream reuse.
- [x] Fuzz targets for config/routing, host normalization, and header policy; three Linux ARM64 smoke runs completed at 1,000 iterations each on 2026-11-26.
- [x] Expanded raw framing/authority regressions, including malformed chunk payload, TE/CL canonicalization, duplicate Host, unsupported transfer codings, request-target forms, CONNECT, and Upgrade.
- [x] Initial repeated load testing and Relay/NGINX comparison: three 30-second runs at 50 and 256 clients on both GitHub-hosted Linux architectures are recorded. The capped NGINX runs had read errors; these VM results are diagnostic and support no performance claim.

## M2 — Secure transport (implemented; external review remains)

- [x] TLS 1.3 termination, PEM loading, and TLS configuration validation.
- [x] HTTPS upstream with certificate-chain and hostname verification; no bypass or early data.
- [x] Local transport security integration tests.
- [x] Negotiate HTTP/2 on TLS/TCP and serve HTTP/3 over QUIC/UDP on the configured numeric port; keep upstream requests on HTTP/1.1 and apply shared routing and bounds.
- [x] Resolve external HTTP/3 client stream-reset report: Quinn trace identified an unconsumed request receive stream being stopped with code 0 after an early upstream response. Relay now drains the receive side within body limits/timeouts; the early-response regression passes, and curl 8.21.0 received the full 1 KiB body and exited successfully on WSL2 ARM64. Smoke output: `docs/perf-results/2026-11-27-wsl-arm64-h3-curl-smoke.txt`. `h2load --h3` produces zero requests in this environment; diagnostic curl-based H3 throughput is recorded in `docs/perf-results/2026-11-27-wsl-arm64-http3-curl-comparison.txt`.
- [x] Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, optimized builds, and repeat-build checks on PR commit `285143c`.
- [ ] Complete an external independent security review before the first production release.

## M3 — Hardening (in progress)

- [x] Fuzz configuration/routing and protocol-facing policy paths with checked-in seed corpus and scheduled CI. On WSL2 ARM64, 30-second bounded runs completed without crashes: `config_routes` 42,073 runs (42 seconds including shutdown), `host_normalization` 955,933 runs (31 seconds), and `header_policy` 198,015 runs (32 seconds); [raw output](docs/perf-results/2026-11-27-wsl-arm64-fuzz-smoke.txt).
- [x] Run locked dependency advisory/license checks and review the direct dependency/features policy; first-party unsafe Rust remains forbidden. Replace the unmaintained PEM parser with `rustls-pki-types`.
- [x] Add a focused regression proving a canceled upstream exchange is not retried.
- [x] Run three-repeat normal and connection-cap stress profiles plus SIGTERM cleanup under 256-client load on both GitHub-hosted Linux architectures, including after HTTP/2/HTTP/3 integration. PR commit `285143c` completed all profiles and both shutdown checks; the three-repeat TLS 1.3 harness also completed on both architectures. Raw output is in `docs/perf-results/2026-11-27-github-actions-285143c-*.txt`. These hosted VM results remain diagnostic, not representative physical-host characterization.
- [x] Update security model and deployment guidance to state verified behavior and remaining release gaps.

## M4 — Release candidate (in progress; not release-ready)

- [x] Linux x86-64/ARM64 optimized builds and clean repeat-build hash checks passed in CI on PR commit `285143c`.
- [ ] Complete performance characterization on representative physical hosts, including binary size, memory, CPU, startup, TLS handshakes, and connection scaling. WSL2 and one QEMU user-mode ARM64 smoke measurement are recorded; neither satisfies this gate. The QEMU run is diagnostic and its RSS includes emulator overhead.
- [ ] Complete native-host competitor benchmarks for HTTP/1.1, HTTP/2, and HTTP/3 where supported. Diagnostic WSL2 H1/H2 results and a three-repeat curl H3 comparison are recorded against Caddy, NGINX, and Apache (H3 unavailable in the installed Apache build). NGINX HTTP/2 was inconsistent. Representative physical-host runs remain open.
- [ ] Run the protected-tag workflow and verify the SBOM, checksums, provenance attestations, and signed release artifacts from a clean environment. No tagged artifact has been verified.
- [x] Add a release checklist and a tag workflow that validates, builds both Linux architectures, generates SPDX, checksums, GitHub provenance attestations, and Cosign keyless signatures, then creates a draft only.
- [ ] Complete independent external security review and all release gates; only then consider v0.1 RC.

## Proposed follow-up backlog

The open M4 items remain the gates for the first release. The items below are proposed improvements for broader deployment and later releases; they do not change the initial product scope or claim that the open release gates are complete.

### P1 — Reliability and operations

- [ ] **Fuzz the real protocol boundary.** `fuzz_targets/http1_server.rs` feeds bounded arbitrary client bytes through Relay's actual Hyper HTTP/1 listener and has seeds for malformed framing and authority. `fuzz_targets/http2_server.rs` negotiates TLS/ALPN with Relay's actual listener, sends the HTTP/2 preface and SETTINGS frame, then fuzzes frame bytes through Hyper. Its tracked corpus includes a valid GET and an immediate RST_STREAM after request headers. A local WSL ARM64 smoke completed 824 executions in 31 seconds with 11,384 coverage edges and no crash; see [`docs/perf-results/2026-11-28-wsl-arm64-http2-fuzz-smoke.txt`](docs/perf-results/2026-11-28-wsl-arm64-http2-fuzz-smoke.txt). `fuzz_targets/http3_server.rs` drives SETTINGS values, grease, request headers/body, and stream cancellation through the H3 client stack into Relay's actual QUIC listener. A local WSL ARM64 smoke completed 117 executions in 33 seconds with 17,636 coverage edges and no crash; see [`docs/perf-results/2026-11-27-wsl-arm64-http3-fuzz-smoke.txt`](docs/perf-results/2026-11-27-wsl-arm64-http3-fuzz-smoke.txt). These H3 inputs use the maintained client stack and do not fuzz malformed raw control frames. The scheduled workflow includes both H2 and H3 but has not yet run remotely. Keep protocol parsing in maintained libraries and add regressions for any discovered failure.
- [x] **Test log redaction.** `tests/proxy.rs` captures success, malformed-client, and upstream-failure logs while sending unique sentinels in queries, authorization headers, cookies, private headers, and a body. The regression checks retain safe connection/upstream diagnostics and reject every sentinel. Commit `b44593d` passed formatting, Clippy, locked tests, release builds, and repeat-build checks on Linux x86-64 and ARM64 (CI run `46061251048`).
- [ ] **Measure multiplexed resource limits.** Gated tests exercise eight requests over a single TLS HTTP/2 connection and one QUIC HTTP/3 connection with the per-connection cap set to one; H2 excess streams are refused and a later request succeeds after capacity frees up. Both tests passed in x86-64/ARM64 CI run `46064928298`. A new WSL2 ARM64 H3 integration regression cancels a partial upload and verifies a follow-up request succeeds on the same connection with both request limits set to one. Add slow-body, reset, memory-bound, and shutdown-under-load coverage before closing this item.
- [x] **Add a Linux deployment kit.** `deploy/systemd/` provides a unit and operator checklist for a dedicated unprivileged account, protected config and key files, TCP/UDP firewall rules, restart behavior, file-descriptor limits, and graceful shutdown. Its `relay check` and `relay run` commands and configuration paths match the current CLI/configuration. The staged unit check passed locally with systemd 261 and is now included in both architecture CI jobs; those jobs have not run for the current uncommitted changes. Validate account, permissions, and runtime behavior on the target Linux distribution before deployment.
- [ ] **Automate release artifact verification.** `scripts/release/verify-artifacts.sh` checks checksums, Cosign identity and issuer, GitHub provenance attestations, and required SPDX JSON fields; `test-verify-artifacts.sh` rejects altered binaries and checksum-manifest signatures using mocked external tools. Run the verifier against actual draft assets in a clean environment and save the output with release evidence.

### P2 — Performance and maintenance

- [ ] **Establish regression thresholds from physical-host baselines.** After the M4 host measurements are complete, select stable throughput, p99 latency, startup, binary-size, and RSS thresholds per architecture. Run repeated comparable workloads and report host/tool versions; avoid failing CI on noisy virtual-machine measurements.
- [x] **Publish a protocol support matrix.** `docs/PROTOCOL_SUPPORT.md` lists the tested client/server versions and known limits for HTTP/1.1, HTTP/2, and HTTP/3, including Apache's H3 limitation in the benchmark environment. Refresh it with each supported release.
- [x] **Set a dependency and action update cadence.** Dependabot opens weekly grouped Cargo and GitHub Actions update PRs; review each with the locked dependency audit, license, formatting, Clippy, tests, and release-build checks.

### P3 — Feature proposals requiring a separate design

Design questions and acceptance evidence for these proposals are collected in
[`docs/FEATURE_PROPOSALS.md`](docs/FEATURE_PROPOSALS.md). The proposals are not
approved implementation scope and are not prerequisites for the initial
release.

- [ ] **Atomic configuration and certificate reload.** Validate a complete replacement before swapping it in, keep the last-known-good configuration on failure, and test in-flight requests and certificate rotation.
- [ ] **Optional upstream HTTP/2.** Prototype only after workload evidence shows a benefit; preserve verified TLS, fixed configured origins, bounded pooling, and the current HTTP/1.1 upstream default unless a separate design justifies changing it.
- [ ] **Multiple configured upstreams with health checks and balancing.** Keep destinations configuration-controlled, bound per-upstream pools and health-check work, and define failure/retry behavior before implementation.
- [ ] **Operator diagnostics.** Confirm a concrete operator need before adding metrics. Preserve the no-request-data logging rule and keep a network administration endpoint out of scope unless separately threat-modeled.

Features excluded by `SPEC.md`—including general forward proxying, arbitrary destinations, caching, WAF, automatic certificate issuance, dashboards, and a remote admin API—remain out of scope until a separate product and threat-model review approves them.
