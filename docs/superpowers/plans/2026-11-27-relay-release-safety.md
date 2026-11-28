# Relay Release-Safety Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete Relay's release-safety and P1 reliability work with verified HTTP/3 cancellation, bounded multiplexed resource use, physical benchmark evidence, verified draft artifacts, and an independent security review.

**Architecture:** Keep Relay's current Hyper/rustls/Quinn architecture. Patch the pinned `h3-quinn` adapter locally only for the pending-read cancellation defect, retain configured limits, and add verification scripts/evidence around the existing benchmark and tag-release workflows. Produce only a draft release; continue the four P3 projects through separate designs after this phase.

**Tech Stack:** Rust 1.96, Tokio, Hyper, rustls, Quinn, `h3`/`h3-quinn`, Bash, Python 3, GitHub Actions, Cosign, GitHub CLI, Caddy, NGINX, Apache httpd.

**Spec:** `docs/superpowers/specs/2026-11-27-relay-release-safety-design.md`; roadmap requirements in `ROADMAP.md`, `docs/RELEASE_CHECKLIST.md`, and `docs/FEATURE_PROPOSALS.md`.

## Global Constraints

- Preserve `#![forbid(unsafe_code)]`; do not add unsafe Rust.
- Use Hyper/rustls/Quinn implementations; do not write HTTP parsing or cryptography.
- Keep upstream destinations configuration-controlled; reject unknown config and conflicting routes before binding sockets.
- Stream bodies with explicit byte/time limits; strip hop-by-hop and untrusted forwarding headers; never log request data or secrets.
- Bound connections, concurrent work, buffers, pools, and shutdown; avoid unbounded queues or detached cleanup work.
- Keep dependencies minimal, locked, and license-compatible; review features and license changes.
- Add a regression for each behavior change; run formatting, Clippy with warnings denied, and the full locked suite.
- Do not substitute WSL2, QEMU, or hosted runners for the physical Linux benchmark gate; do not self-certify the independent-review gate.
- Create a draft release only; never publish it as part of this plan.
- Follow the user's direct-to-main integration request for implementation commits: complete local validation first, use only the previously authorized temporary user-specific ruleset bypass for the push, restore the ruleset immediately, then wait for and fix any remote CI failure before the next task. Never leave a bypass configured or bypass tag rules.
- Do not stage or remove the existing untracked `.tmp-actionlint-1.7.7/`, `artifacts-de76/`, or `artifacts-f3625a4/` directories.

## Review Focus

- A timed-out H3 body must return 408 and send `H3_REQUEST_CANCELLED`, without panicking the connection or losing follow-up traffic; pin this in `tests/http3_proxy.rs`.
- H2/H3 reset, cancellation, and timeouts must return permits and stream capacity; test the cap-one and 64-stream profiles in `tests/tls_proxy.rs` and `tests/http3_proxy.rs`.
- A slow body at the configured aggregate stream limit must not cause body buffering to grow with total body bytes; measure RSS in the physical resource profile.
- Release provenance and Cosign verification must reject a wrong tag identity/issuer and modified binary, checksum, or SBOM; test tampered inputs and identity construction in `scripts/release/test-verify-artifacts.sh`, then verify real signatures in a clean environment.
- Unsupported protocol/server combinations or failed negotiation must be recorded as unavailable/error, never reported as successful benchmark data; test the physical benchmark harness.

---

### Task 1: Apply the reviewed dependency updates directly to main

**Files:**
- Source PR #3: `.github/workflows/release.yml`
- Source PR #4: `Cargo.toml`, `Cargo.lock`

**Interfaces:**
- Consumes: current `origin/main` at `c2748eb`; proposed changes in Dependabot PRs #3 and #4.
- Produces: direct commits on `main` with the action and TOML dependency changes reviewed and checks green; no PRs are created or merged for this work.

- [x] **Step 1: Recheck PR #3 evidence and source compatibility**

Run: `gh pr checks 3` and inspect the exact PR diff and `action.yml` at the pinned action commit.
Expected: audit, x86-64, ARM64, and fuzz checks pass; v4 still accepts `subject-path`; release permissions remain `contents: read`, `id-token: write`, and `attestations: write`.

- [x] **Step 2: Apply the verified action update directly**

Apply the reviewed `.github/workflows/release.yml` diff from PR #3 to the current checkout. Verify the `subject-path` input and the workflow permissions remain correct. Do not create, approve, or merge a PR.
Expected: the release workflow update is present in the working tree and the focused action review remains satisfied.

- [x] **Step 3: Commit and push the action update directly**

Commit only `.github/workflows/release.yml` and push the commit directly to `main` using the authorized temporary ruleset bypass procedure if needed; restore rules immediately and wait for remote CI. Close superseded Dependabot PR #3 after verifying the direct commit contains its reviewed change.
Expected: `main` contains the action update; no PR is used to integrate it.

- [x] **Step 4: Apply and revalidate the TOML update directly**

Review the TOML 1.2.2 diff from PR #4, apply its `Cargo.toml`/`Cargo.lock` changes on the updated `main`, and confirm unknown fields still fail in `tests/config.rs`. Run local locked tests, audit, and license checks; wait for remote CI before committing.
Expected: the direct dependency update passes the locked suite and policy checks.

- [x] **Step 5: Commit and push the TOML update directly**

Commit only `Cargo.toml` and `Cargo.lock` and push directly to `main` using the authorized temporary bypass procedure if needed; restore rules immediately and wait for remote CI. Close superseded Dependabot PR #4 after verifying the direct commit contains its reviewed change.
Expected: `main` contains TOML 1.2.2, the lockfile is current, and the locked suite/build checks pass.

- [x] **Step 6: Record and commit the dependency review**

Update the dependency section of `ROADMAP.md` or release evidence with both direct commit SHAs, audit/license results, and CI runs. Do not claim a dependency review based only on green checks. Commit this evidence and wait for main's documentation CI checks.

### Task 2: Fix HTTP/3 pending-read cancellation

**Files:**
- Create: `vendor/h3-quinn/Cargo.toml`, `vendor/h3-quinn/LICENSE`, `vendor/h3-quinn/README.md`, `vendor/h3-quinn/src/lib.rs`, `vendor/h3-quinn/PATCH.md`
- Modify: `Cargo.toml`, `Cargo.lock`
- Test: `tests/http3_proxy.rs::stalled_http3_upload_times_out_and_releases_request_capacity`

**Interfaces:**
- Consumes: existing `stop_http3_request_body` timeout path and H3 client request-stream APIs.
- Produces: the `h3-quinn 0.0.10` crates.io patch with identical public API and corrected pending-read cancellation.

- [ ] **Step 1: Enable and strengthen the failing regression**

Remove `#[ignore]` from `stalled_http3_upload_times_out_and_releases_request_capacity`. After receiving 408, attempt `send_data` on the still-open upload and assert `Err(h3::error::StreamError::RemoteTerminate { code: h3::error::Code::H3_REQUEST_CANCELLED })`; keep the same-connection follow-up request and assert its 200 body. Run before the patch.

Run: `cargo +1.96.0 test --locked --test http3_proxy stalled_http3_upload_times_out_and_releases_request_capacity -- --nocapture`
Expected before patch: reproduce the current panic or wrong cancellation result; preserve the output as the regression baseline.

- [ ] **Step 2: Vendor the exact locked crate source and license**

Copy the crate's normalized manifest, README, MIT license, and `src/lib.rs` from the locked `h3-quinn 0.0.10` source into `vendor/h3-quinn/`. Do not copy Cargo's generated lockfile or registry bookkeeping files. Add `PATCH.md` with upstream issue #361, patch rationale, source version/checksum, and the removal condition.

- [ ] **Step 3: Patch stop delivery without changing the public API**

In `vendor/h3-quinn/src/lib.rs`, add a `futures::channel::oneshot::Sender<u64>` to `RecvStream` and change `ReadChunkFuture` to return the stream plus a data/stopped outcome. When `poll_data` moves Quinn's stream into the `read_chunk` future, select that future against the stop receiver. If stop wins, drop the borrowing read future, call Quinn `RecvStream::stop` with the requested code, and return the stream to its owner. If a read wins, clear the sender and preserve the existing result conversion. `stop_sending` sends the code while a read is pending and calls `stop` directly when the stream is present. A completed stop returns EOF to any later `poll_data` call. Remove the panic path that unwraps an absent stream; do not add dependencies or Tokio features.

- [ ] **Step 4: Apply the local crate patch and lock it**

Add `[patch.crates-io] h3-quinn = { path = "vendor/h3-quinn" }` to `Cargo.toml`, preserving the crate version and dependency features. Regenerate only the relevant `Cargo.lock` entry with `cargo +1.96.0 update -p h3-quinn --precise 0.0.10`.

- [ ] **Step 5: Run the H3 regression, full suite, lint, and dependency policy**

Run the targeted test, `cargo +1.96.0 fmt --all -- --check`, `cargo +1.96.0 clippy --all-targets --locked -- -D warnings`, `cargo +1.96.0 test --locked`, `cargo +1.96.0 build --release --locked`, `cargo audit`, and `cargo deny check`.
Expected: all pass; the regression observes the exact stop code, 408, no panic, capacity recovery, and same-connection follow-up.

- [ ] **Step 6: Commit, push, and verify remote CI**

Commit only the vendored patch, lockfile, integration test, and `PATCH.md` with a focused message. Push directly to `main` using the authorized temporary bypass procedure, restore the ruleset immediately, and wait for x86-64, ARM64, audit, fuzz, and benchmark checks to pass before continuing.

### Task 3: Complete multiplexed resource-limit evidence

**Files:**
- Modify: `tests/tls_proxy.rs`, `tests/http3_proxy.rs`
- Create: `scripts/load/run-multiplexed-resource-profile.sh`
- Create: `docs/perf-results/<date>-multiplexed-resource-profile-<arch>.txt`

**Interfaces:**
- Consumes: configured `max_concurrent_streams_per_connection = 64`, `max_concurrent_requests = 1024`, and existing upstream/test helpers.
- Produces: repeatable H2/H3 active-stream, cancellation, memory, and shutdown evidence.

- [ ] **Step 1: Add deterministic H2 reset/cancel permit-recovery coverage**

Add a new test beside `limits_concurrent_http2_streams_on_a_single_connection` in `tests/tls_proxy.rs`. With one stream/request permit, cancel or reset the active stream, then assert the next request reaches upstream and completes. Keep bounded timeouts on every wait.

- [ ] **Step 2: Add deterministic H3 reset/cancel permit-recovery coverage**

Add a test beside `limits_concurrent_http3_streams_on_a_single_connection`. Verify reset/cancel returns both request and stream capacity and a follow-up request on the same QUIC connection completes.

- [ ] **Step 3: Exercise the configured cross-platform profile**

Run H2 and H3 separately with 8 client connections × 64 streams, global request cap 1024, and 64 KiB bodies to a local upstream that drains without retaining them. Gate upstream replies and assert no more than 512 requests are active; release the gate and verify all complete. Repeat with cancellation and SIGTERM while the gate is closed.

- [ ] **Step 4: Add the bounded physical memory/shutdown profile**

The script runs 16 connections × 64 streams (1,024 requests), each streaming a fixed 1 MiB body to a discard upstream. Record active requests, RSS, CPU, and shutdown time. Acceptance: RSS plateaus below 1 GiB during the 1 GiB aggregate body load, returns to within 64 MiB of baseline within 10 seconds, and shutdown completes within the configured 5-second bound. Keep the script's request count, body size, and durations bounded and validate its arguments.

- [ ] **Step 5: Run the profile on both supported CI architectures and a physical host**

Run the correctness/stress tests on Linux x86-64 and ARM64. Run and save the memory profile on each available physical Linux host; do not label the Windows Surface, WSL2, QEMU, or hosted runners as physical Linux evidence.

- [ ] **Step 6: Commit and verify**

Commit the H2/H3 regressions and bounded profile script, push to `main` under the authorized process, and wait for remote checks on both architectures. Keep physical memory evidence separate from CI stress output.

### Task 4: Produce accepted physical performance baselines

**Files:**
- Modify: `scripts/load/run-competitor-comparison.sh`, `scripts/load/run-linux-local.sh`
- Create: `scripts/load/run-windows-native.ps1`
- Modify: `docs/BENCHMARKS.md`, `docs/PERFORMANCE.md`
- Create: `docs/perf-results/<date>-physical-<arch>-<server>-<protocol>.txt`

**Interfaces:**
- Consumes: optimized Relay and native Caddy/NGINX/Apache builds, `h2load`, curl with HTTP/3 support, `wrk`/system metrics tools.
- Produces: raw and summarized measurements for the exact supported host/protocol matrix.

- [ ] **Step 1: Extend the Linux competitor harness to the accepted matrix**

Support client counts 1, 50, and 256; 15-second warm-up; five repetitions; and 60-second runs. For H3, send repeated 1,000-request curl batches until the measured interval reaches 60 seconds and report actual elapsed time. Set server worker/connection ceilings above 256, keep one shared 1 KiB response and one streaming 1 MiB body profile, and report binary size, startup, RSS, CPU, TLS handshake rate, throughput, and p50/p95/p99. Verify the negotiated protocol before every measured case and record unsupported modes explicitly.

- [ ] **Step 2: Add fail-closed harness tests**

Test invalid arguments, missing tools, protocol negotiation failure, server startup failure, and unsupported Apache H3. A failed or unavailable mode must not produce a success row. Run `bash -n scripts/load/run-competitor-comparison.sh` and the script's 1-second smoke profile.

- [ ] **Step 3: Add the Windows ARM64 supplemental host recorder**

Create `scripts/load/run-windows-native.ps1` to record OS build, CPU model/core count, memory, power mode, Relay binary size/startup/RSS/CPU, and supported Caddy/NGINX/Apache protocol checks. Use native ARM64 builds only; mark unsupported server/protocol combinations `unavailable`. Do not count these results toward the physical Linux gate.

Validate PowerShell syntax with `System.Management.Automation.Language.Parser::ParseFile`, then run the script once and verify that metadata and each detected protocol are recorded with explicit status.

- [ ] **Step 4: Run the physical Linux matrix**

Run the accepted matrix on physical Linux x86-64 and ARM64 hosts. For each protocol/server case, keep five raw 60-second runs after a 15-second warm-up, report median and range, and calculate throughput/p95-latency coefficient of variation. If either exceeds 10%, repeat five times and label still-unstable cases inconclusive. Record host identity, kernel, tools, commands, config, and protocol support. Do not claim a ranking from unsupported or unlike cases.

- [ ] **Step 5: Record Windows supplemental data and Linux results**

Run the Windows script on the physical Surface where tools are supported. Save raw logs and update `docs/BENCHMARKS.md`/`docs/PERFORMANCE.md`. If physical Linux hosts are unavailable, leave those gates open rather than substituting virtualized output.

- [ ] **Step 6: Commit harness and accepted evidence**

Commit the tested harness changes and only the measurements that meet their host/protocol acceptance criteria. Push under the authorized direct-main process and wait for remote checks. Keep supplementary Windows data and any inconclusive Linux cases labeled as such.

### Task 5: Run final-candidate validation and prepare external review

**Files:**
- Modify: `.github/workflows/fuzz.yml` only if the current six-target workflow fails to emit per-target run evidence
- Modify: `docs/INDEPENDENT_SECURITY_REVIEW_PACKET.md`, `docs/SECURITY_REVIEW.md`
- Modify: `docs/RELEASE_CHECKLIST.md`

**Interfaces:**
- Consumes: one exact candidate commit after Tasks 1–4.
- Produces: final CI/fuzz evidence and a reviewer packet tied to the exact commit.

- [ ] **Step 1: Run all required checks on the exact candidate commit**

Run CI on Linux x86-64/ARM64, dependency audit/license checks, all six fuzz targets, full locked tests, Clippy, release builds, clean repeat-build checks, and deployment-unit verification. Save run links, fuzz target durations/counts/corpus, and build hashes.

- [ ] **Step 2: Validate the deployment kit on the target Linux distribution**

Use Ubuntu 24.04 LTS (the current CI distribution) unless the deployment docs identify a different target. Run `scripts/release/verify-systemd-unit.sh` and the documented account, config/key permissions, firewall, run/check, restart, and SIGTERM flow on that distribution. Record the distribution/version and exact evidence in `docs/DEPLOYMENT.md`; do not claim deployment validation from a different distro or WSL2.

- [ ] **Step 3: Refresh the independent review packet**

Update the exact commit, toolchain, test/benchmark results, patch notes, threat model, and remaining limitations in `docs/INDEPENDENT_SECURITY_REVIEW_PACKET.md`. Keep secrets and customer data out. Record that no reviewer is assigned until the user supplies one.

- [ ] **Step 4: Obtain and resolve the independent review**

An external reviewer with write access reviews the exact candidate commit. Record findings in `docs/SECURITY_REVIEW.md`; fix every required finding with a regression and rerun affected checks. If the commit changes, rerun candidate validation and refresh the packet. Do not count a self-review as completion.

- [ ] **Step 5: Commit review preparation and final-candidate evidence**

Commit the packet, target-distribution results, and exact candidate validation links. Do not close the external-review checkbox until the reviewer identity, findings, dispositions, and retest evidence are present.

### Task 6: Verify protected-tag draft artifacts and close release gates

**Files:**
- Modify: `.github/workflows/release.yml` only for verified workflow defects
- Modify: `scripts/release/verify-artifacts.sh`, `scripts/release/test-verify-artifacts.sh` only for verified verifier defects
- Modify: `docs/RELEASE_CHECKLIST.md`, `ROADMAP.md`, `docs/BENCHMARKS.md`, `docs/PERFORMANCE.md`
- Create: `docs/release-evidence/<tag>-verification.txt`, `docs/release-evidence/<tag>-manifest.txt`

**Interfaces:**
- Consumes: pre-tag gates complete, final code/commit externally reviewed, and protected-tag rules verified.
- Produces: verified draft-only release assets and roadmap status backed by evidence.

- [ ] **Step 1: Resolve the tag/draft checklist sequence**

Update `docs/RELEASE_CHECKLIST.md` to distinguish pre-tag gates from the draft-artifact gate: no candidate tag until CI, fuzz, physical benchmark, deployment, and independent-review gates pass; after that, the protected tag may create a draft solely so assets can be verified. Publishing remains prohibited until every artifact check and maintainer review passes.

- [ ] **Step 2: Create the protected RC verification tag after all pre-tag gates pass**

Confirm the tag ruleset and exact reviewed commit. Push the agreed `v0.1.0-rc.1` tag only after Task 5 passes. Confirm the workflow creates a draft release and does not publish.

- [ ] **Step 3: Verify the downloaded assets in a clean Linux environment**

Download the draft assets with `gh release download`, then run `bash scripts/release/verify-artifacts.sh "$TAG" "$RELEASE_DIR"`. Confirm binary checksums, Cosign identity/issuer, SPDX JSON/signature, and GitHub provenance for both binaries. Save stdout/stderr and the exact tag/commit manifest. Run `bash scripts/release/test-verify-artifacts.sh` and prove tampered files and wrong identities are rejected.

- [ ] **Step 4: Commit verifier regression and evidence**

Extend `scripts/release/test-verify-artifacts.sh` with explicit wrong-tag-identity, altered binary, altered checksum manifest, and altered SBOM/signature rejection cases if any are missing. Run the test script, then commit the verifier test and clean-environment output after all checks pass.

- [ ] **Step 5: Set thresholds and close only evidence-backed checkboxes**

Use the accepted physical baseline to set per-architecture throughput, p95 latency, startup, binary-size, and RSS thresholds. Update roadmap/checklist/docs with links to raw evidence, independent review findings/dispositions, release artifacts, and unsupported combinations. Leave any missing host, protocol, reviewer, or deployment evidence open; leave the draft unpublished.

### Follow-on scope: complete the P3 proposals

After Tasks 1–6, continue the full-roadmap goal with four separate approved designs and plans, one at a time:

1. Atomic configuration and certificate reload.
2. Optional upstream HTTP/2.
3. Multiple configured upstreams with health checks and balancing.
4. Operator diagnostics.

Each follow-on design must update `SPEC.md`, `DESIGN.md`, `docs/THREAT_MODEL.md`, and an ADR before implementation, then supply its own regression, resource-bound, security-review, and deployment evidence. Do not mark the overall roadmap complete until all four are implemented and their acceptance evidence is verified.
