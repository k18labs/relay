# Release checklist

Do not publish a Relay release until every required gate below has evidence attached to the release review.

## Required gates

- [x] Review `SPEC.md`, `ROADMAP.md`, `docs/THREAT_MODEL.md`, and `docs/SECURITY_MODEL.md`; remaining release limitations are listed in this checklist and the performance/security docs.
- [ ] CI passes on Linux x86-64 and ARM64 for the final release commit: formatting, Clippy with warnings denied, locked tests, and optimized builds. PR commit `de76c72` passed; the benchmark harness correction is pending.
- [x] `cargo audit` and `cargo deny check` pass on the locked dependency graph; review changes to direct dependencies and enabled features. PR commit `de76c72` passed.
- [ ] Run each scheduled fuzz target and attach corpus, duration, and run-count evidence. Triage every crash.
- [x] Rerun repeatable load tests at normal and configured-limit workloads on both architectures after the worker-count change. Three 30-second runs at 50 and 256 clients completed on x86-64 and ARM64 GitHub-hosted Linux VMs on `de76c72`; Relay had no client errors, while NGINX emitted read errors at the cap. These results are diagnostic, not a performance claim.
- [ ] Record binary size, idle and loaded RSS, CPU, startup, TLS handshake, latency, and scaling results on representative physical hosts. WSL2 smoke data is available but does not meet the provisional startup or p99-overhead targets.
- [x] Record the v0.1 scope decision below: compare with NGINX only, and defer HAProxy/Caddy/Pingora comparisons until after v0.1.
- [ ] An external independent security reviewer has reviewed the release candidate and all findings have a documented disposition.
- [ ] The release build has a source SBOM, per-binary SHA-256 checksums, and GitHub build provenance attestations. Verify all three from a clean environment.
- [x] Verify repeat-build results on both supported architectures and explain any expected non-reproducible fields. CI matched clean release build hashes on `de76c72`; rerun for the final release commit.
- [ ] Review the draft release, attached files, notes, version, and upgrade/configuration implications. Publish only after maintainer approval.

## Draft artifact verification

The tag workflow is intentionally a draft-release producer. On a clean Linux environment with `gh` authenticated and the GitHub CLI attestation extension available:

```sh
gh release download vX.Y.Z-rc.N --repo k18labs/relay --dir release-check
cd release-check
sha256sum --check relay-linux-x86_64.sha256
sha256sum --check relay-linux-aarch64.sha256
gh attestation verify relay-linux-x86_64 --repo k18labs/relay
gh attestation verify relay-linux-aarch64 --repo k18labs/relay
```

Confirm the SPDX JSON file is present and parses with an SPDX-compatible validator. Save the verification output with the release review. A draft release and successful workflow are not themselves approval to publish.

## v0.1 competitor benchmark scope decision

For v0.1, keep the Relay/NGINX matrix as a diagnostic baseline and defer HAProxy, Caddy, and Pingora comparisons until after v0.1. The available x86-64 and ARM64 runners are virtualized cloud VMs, and NGINX reported client-side read errors in all capped runs. The current results do not support a defensible performance ranking. Release notes must make no throughput or performance-superiority claim.

## Latest PR evidence (not release approval)

At PR commit `de76c72534c78dd2a4141b0604eb3db708399cfa`, Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, optimized builds, and clean repeat-build hashes; dependency audit and fuzz smoke also passed. The benchmark matrix completed three normal and three capped runs on each architecture and a shutdown-under-load test on each. The TLS job exposed an early-exit issue in the measurement wrapper: only one handshake repetition was recorded, so the harness is being corrected before those results are accepted. Raw results are committed under `docs/perf-results/` and `docs/release-evidence/`. Clean-environment artifact verification and external independent review remain open.

## Current release status

The project is still pre-release. The tagged workflow and draft checklist do not substitute for actual successful tagged runs, clean-environment verification, benchmark evidence, or an external review. Do not create or publish an RC tag while any required gate is open.
