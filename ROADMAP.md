# Roadmap

## M0 — Foundation (implementation present; cross-platform acceptance incomplete)

- [x] Rust crate, pinned toolchain, lockfile, strict config parser, and `run`/`check`/`version` CLI.
- [x] CI and dependency/license policy files.
- [x] Linux x86-64 and ARM64 builds and CI evidence for PR commit `71810ea`: both GitHub Actions jobs passed formatting, Clippy, locked tests, optimized builds, and clean repeat-build hash comparisons on 2026-11-27.

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
- [x] Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, optimized builds, and repeat-build checks on PR commit `71810ea`.
- [ ] Complete an external independent security review before the first production release.

## M3 — Hardening (in progress)

- [x] Fuzz configuration/routing and protocol-facing policy paths with checked-in seed corpus and scheduled CI. On WSL2 ARM64, 30-second bounded runs completed without crashes: `config_routes` 42,073 runs (42 seconds including shutdown), `host_normalization` 955,933 runs (31 seconds), and `header_policy` 198,015 runs (32 seconds); [raw output](docs/perf-results/2026-11-27-wsl-arm64-fuzz-smoke.txt).
- [x] Run locked dependency advisory/license checks and review the direct dependency/features policy; first-party unsafe Rust remains forbidden. Replace the unmaintained PEM parser with `rustls-pki-types`.
- [x] Add a focused regression proving a canceled upstream exchange is not retried.
- [x] Run three-repeat normal and connection-cap stress profiles plus SIGTERM cleanup under 256-client load on both GitHub-hosted Linux architectures, including after HTTP/2/HTTP/3 integration. PR commit `71810ea` completed all profiles and both shutdown checks. Relay recorded no cap-run request timeouts; NGINX had client read errors in capped runs. Separate WSL runs showed high variance; all raw results are in `docs/perf-results/` and `docs/release-evidence/`.
- [x] Update security model and deployment guidance to state verified behavior and remaining release gaps.

## M4 — Release candidate (not started)

- [x] Linux x86-64/ARM64 optimized builds and clean repeat-build hash checks passed in CI on PR commit `71810ea`.
- [ ] Complete performance characterization on representative physical hosts, including binary size, memory, CPU, startup, TLS handshakes, and connection scaling. WSL2 smoke measurements now exist; they show startup and p99 overhead above provisional targets and do not satisfy this gate.
- [ ] Complete native-host competitor benchmarks for HTTP/1.1, HTTP/2, and HTTP/3 where supported. Diagnostic WSL2 H1/H2 results and a three-repeat curl H3 comparison are recorded against Caddy, NGINX, and Apache (H3 unavailable in the installed Apache build). NGINX HTTP/2 was inconsistent. Representative physical-host runs remain open.
- [ ] Run the protected-tag workflow and verify the SBOM, checksums, provenance attestations, and signed release artifacts from a clean environment. No tagged artifact has been verified.
- [x] Add a release checklist and a tag workflow that validates, builds both Linux architectures, generates SPDX, checksums, GitHub provenance attestations, and Cosign keyless signatures, then creates a draft only.
- [ ] Complete independent external security review and all release gates; only then consider v0.1 RC.
