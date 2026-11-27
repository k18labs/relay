# Roadmap

## M0 — Foundation (implementation present; cross-platform acceptance incomplete)

- [x] Rust crate, pinned toolchain, lockfile, strict config parser, and `run`/`check`/`version` CLI.
- [x] CI and dependency/license policy files.
- [ ] Linux x86-64 and ARM64 builds and CI evidence. ARM64 locked tests, formatting, and Clippy pass locally on 2026-11-26; x86-64 evidence for this change set is pending CI.

## M1 — HTTP proxy (implemented; broader validation remains)

- [x] TCP listener, bounded connection admission, HTTP/1.1 forwarding, fixed routes, streaming bodies, and hop-by-hop cleanup.
- [x] Header/body bounds, request concurrency limit, progress timeouts, upstream pooling bounds, and graceful shutdown deadline.
- [x] Local regression coverage for routing, forwarding, limits, timeouts, shutdown, and upstream reuse.
- [x] Fuzz targets for config/routing, host normalization, and header policy; three Linux ARM64 smoke runs completed at 1,000 iterations each on 2026-11-26.
- [x] Expanded raw framing/authority regressions, including malformed chunk payload, TE/CL canonicalization, duplicate Host, unsupported transfer codings, request-target forms, CONNECT, and Upgrade.
- [ ] Sustained load testing and comparative performance results. A single 15-second WSL ARM64 smoke comparison is recorded, but it is not release evidence.

## M2 — Secure transport (implemented; review remains)

- [x] TLS 1.3 termination, PEM loading, and TLS configuration validation.
- [x] HTTPS upstream with certificate-chain and hostname verification; no bypass or early data.
- [x] Local transport security integration tests.
- [ ] Linux platform verification and independent review. ARM64 unit/integration tests and lint pass locally; x86-64 current-change CI and an external independent review remain open.

## M3 — Hardening (in progress)

- [x] Fuzz configuration/routing and protocol-facing policy paths with checked-in seed corpus and scheduled CI; bounded local smoke runs pass on ARM64.
- [x] Run locked dependency advisory/license checks and review the direct dependency/features policy; first-party unsafe Rust remains forbidden. Replace the unmaintained PEM parser with `rustls-pki-types`.
- [ ] Add focused cancellation regression and sustained stress checks for limits and cleanup under load.
- [x] Update security model and deployment guidance to state verified behavior and remaining release gaps.

## M4 — Release candidate (not started)

- [ ] Linux x86-64/ARM64 release builds, full benchmark results, reproducibility checks, SBOM, checksums, provenance, and signing. Draft-release workflow is implemented but has not run on a protected tag; no release artifact has been verified.
- [x] Add a release checklist and a tag workflow that validates, builds both Linux architectures, generates SPDX, checksums, and provenance, then creates a draft only.
- [ ] Complete independent external security review and all release gates; only then consider v0.1 RC.
