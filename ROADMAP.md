# Roadmap

## M0 — Foundation (implementation present; cross-platform acceptance incomplete)

- [x] Rust crate, pinned toolchain, lockfile, strict config parser, and `run`/`check`/`version` CLI.
- [x] CI and dependency/license policy files.
- [ ] Linux x86-64 and ARM64 builds and CI evidence.

## M1 — HTTP proxy (implemented; broader validation remains)

- [x] TCP listener, bounded connection admission, HTTP/1.1 forwarding, fixed routes, streaming bodies, and hop-by-hop cleanup.
- [x] Header/body bounds, request concurrency limit, progress timeouts, upstream pooling bounds, and graceful shutdown deadline.
- [x] Local regression coverage for routing, forwarding, limits, timeouts, shutdown, and upstream reuse.
- [ ] Fuzz targets, broader framing suite, and load testing.

## M2 — Secure transport (implemented; review remains)

- [x] TLS 1.3 termination, PEM loading, and TLS configuration validation.
- [x] HTTPS upstream with certificate-chain and hostname verification; no bypass or early data.
- [x] Local transport security integration tests.
- [ ] Linux platform verification and independent review.

## M3 — Hardening (in progress)

- [ ] Fuzz configuration, routing, and protocol-facing policy paths; add corpus/regression cases.
- [ ] Dependency and unsafe-code review; stress limits, cancellation, and cleanup under load.
- [ ] Complete security model verification and deployment guidance.

## M4 — Release candidate (not started)

- [ ] Linux x86-64/ARM64 builds, documented benchmarks, reproducibility checks, SBOM, checksums, provenance, and signing.
- [ ] Independent security review and release checklist; only then consider v0.1 RC.
