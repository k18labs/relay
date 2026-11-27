# Relay Roadmap and Release Readiness Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task. Keep this plan and its progress ledger current.

**Goal:** Complete the open roadmap work and prepare a verifiable v0.1.0 release candidate without claiming evidence that has not been produced.

**Architecture:** Keep Relay as one locked Rust crate using Hyper and rustls. Put fuzzing and benchmark drivers in separate development tooling so production dependencies stay small. Release automation builds only authorized tags and publishes binary, SBOM, checksum, and provenance evidence.

**Tech Stack:** Rust 1.96.0, Cargo, Tokio, Hyper, rustls, cargo-fuzz/libFuzzer, GitHub Actions, Syft or CycloneDX SBOM tooling, GitHub artifact attestation.

**Spec:** `SPEC.md`, `docs/THREAT_MODEL.md`, `docs/SECURITY_MODEL.md`, `ROADMAP.md`.

## Global Constraints

- Preserve `#![forbid(unsafe_code)` in first-party Rust code; no custom crypto or HTTP parser.
- Use fixed, validated upstream destinations; reject unknown configuration fields and validate before binding.
- Stream bodies and enforce configured byte and time limits.
- Keep all externally controlled resource use bounded and avoid sensitive logging.
- Keep dependencies minimal, locked, and license-compatible; review new dependency features.
- Do not call a milestone or release ready without its acceptance evidence.

## Review Focus

- Malformed request framing and conflicting transfer metadata must be rejected or handled consistently.
- Fuzz inputs must have bounded execution and only target first-party parsing/policy entry points.
- Concurrent clients and slow bodies must not exceed admission, buffer, or cleanup limits.
- Release workflows must restrict publication to protected version tags and use least-privilege permissions.
- Performance results must name exact hardware, versions, configs, and commands and must not imply unmeasured production readiness.

---

### Task 1: Protocol Regression Coverage

**Files:** `tests/proxy.rs`, `docs/TESTING.md`

- [x] Add raw request regressions for duplicate/invalid Host, request-target edge cases, `Transfer-Encoding` conflicts, malformed chunk framing, connection-token stripping, and unsupported protocol upgrades.
- [x] Run each focused regression and verify the expected status/connection behavior.
- [x] Run full locked test suite (57 tests pass on Linux ARM64).

### Task 2: Fuzzing

**Files:** `fuzz/Cargo.toml`, `fuzz/fuzz_targets/*.rs`, `.github/workflows/fuzz.yml`, `.gitignore`, `docs/TESTING.md`

- [x] Add bounded libFuzzer targets for config parsing and route/host normalization using the public library API; header policy target also added.
- [x] Add a bounded scheduled/manual CI workflow with a pinned nightly toolchain and corpus artifact upload on failure.
- [x] Verify all fuzz targets compile and execute for 1,000 runs each on Linux ARM64 with the pinned nightly.

### Task 3: Resource and Load Checks

**Files:** `tests/proxy.rs`, `scripts/load/`, `.github/workflows/benchmark.yml`, `docs/PERFORMANCE.md`

- [x] Add regressions for connection admission, capacity recovery after body completion/stalled downstream writes, and no retry after a canceled upstream exchange.
- [ ] Add equivalent load configurations for Relay, NGINX, HAProxy, Caddy, and Pingora. A repeatable Relay/NGINX harness is implemented; HAProxy, Caddy, and Pingora are outstanding.
- [ ] Run comparative tests on native Linux x86-64 and ARM64; one WSL ARM64 smoke result is recorded but is not release evidence.

### Task 4: Dependency and Security Review

**Files:** `.github/workflows/security.yml`, `deny.toml`, `docs/THREAT_MODEL.md`, `docs/SECURITY_MODEL.md`

- [x] Run `cargo audit` and `cargo deny check`; there are no known vulnerability/license/source failures. `cargo deny` warns about duplicate `syn` and `winnow` versions with distinct major-version requirements.
- [x] Review first-party unsafe-code policy, dependency features, timeout behavior, and resource bounds. `rustls-pemfile` was replaced with `rustls-pki-types` PEM parsing.
- [x] Record an internal independent source review and finding dispositions in `docs/SECURITY_REVIEW.md`; external independent review remains open.

### Task 5: Release Supply Chain

**Files:** `.github/workflows/release.yml`, `rust-toolchain.toml`, `docs/DEPLOYMENT.md`, `docs/TESTING.md`

- [ ] Build Linux x86-64 and ARM64 artifacts only for authorized protected version tags. Workflow exists but has not run on a protected tag.
- [x] Add SPDX SBOM, SHA-256 checksum, and GitHub build provenance generation to the draft release workflow.
- [ ] Verify generated artifact checksums and attestations from a clean environment; no release artifacts exist yet.
- [x] Enable active `main` and `v*` tag rulesets; `main` requires one approval, status checks, and resolved review threads, while version tags require the repository admin to create/update/delete. Release jobs use scoped permissions.

### Task 6: Reproducibility and RC Decision

**Files:** `.github/workflows/ci.yml`, `docs/PERFORMANCE.md`, `ROADMAP.md`, `CHANGELOG.md`, `README.md`

- [ ] Verify repeat builds on supported Linux targets and record reproducibility results and any host-dependent differences.
- [ ] Reconcile every roadmap/spec acceptance item against test, CI, review, benchmark, and artifact evidence.
- [x] Update status, performance, security, deployment, and release documentation. Version metadata remains at 0.1.0 until gates pass.
- [ ] Create a pre-release candidate only after no release-blocking item remains.
