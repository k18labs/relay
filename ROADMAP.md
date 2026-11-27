# Roadmap

## M0 — Foundation (implementation present; cross-platform acceptance incomplete)

- [x] Rust crate, pinned toolchain, lockfile, strict config parser, and `run`/`check`/`version` CLI.
- [x] CI and dependency/license policy files.
- [x] Linux x86-64 and ARM64 builds and CI evidence for PR commit `e849e30`: both GitHub Actions jobs passed formatting, Clippy, locked tests, optimized builds, and clean repeat-build hash comparisons on 2026-11-27. The worker-count update still needs fresh CI evidence.

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
- [x] Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, optimized builds, and repeat-build checks on the current PR commit.
- [ ] Complete an external independent security review before the first production release.

## M3 — Hardening (in progress)

- [x] Fuzz configuration/routing and protocol-facing policy paths with checked-in seed corpus and scheduled CI; bounded local smoke runs pass on ARM64.
- [x] Run locked dependency advisory/license checks and review the direct dependency/features policy; first-party unsafe Rust remains forbidden. Replace the unmaintained PEM parser with `rustls-pki-types`.
- [x] Add a focused regression proving a canceled upstream exchange is not retried.
- [x] Run three-repeat normal and connection-cap stress profiles plus SIGTERM cleanup under 256-client load on both GitHub-hosted Linux architectures. Relay reported no errors or timeouts in the capped VM runs, and both shutdown checks exited within the 7-second watchdog. Separate WSL runs showed high variance and errors; all raw results are in `docs/perf-results/` and `docs/release-evidence/`.
- [x] Update security model and deployment guidance to state verified behavior and remaining release gaps.

## M4 — Release candidate (not started)

- [x] Linux x86-64/ARM64 optimized builds and clean repeat-build hash checks passed in CI on the current PR commit.
- [ ] Complete performance characterization on representative physical hosts, including binary size, memory, CPU, startup, TLS handshakes, and connection scaling. WSL2 smoke measurements now exist; they show startup and p99 overhead above provisional targets and do not satisfy this gate.
- [ ] Run the protected-tag workflow and verify the SBOM, checksums, provenance attestations, and signed release artifacts from a clean environment. No tagged artifact has been verified.
- [x] Add a release checklist and a tag workflow that validates, builds both Linux architectures, generates SPDX, checksums, and provenance, then creates a draft only.
- [ ] Complete independent external security review and all release gates; only then consider v0.1 RC.
