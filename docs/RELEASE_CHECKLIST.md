# Release checklist

Do not publish a Relay release until every required gate below has evidence attached to the release review.

## Required gates

- [x] Review `SPEC.md`, `ROADMAP.md`, `docs/THREAT_MODEL.md`, and `docs/SECURITY_MODEL.md`; remaining release limitations are listed in this checklist and the performance/security docs.
- [x] CI passes on Linux x86-64 and ARM64 for PR commit `285143c`: formatting, Clippy with warnings denied, locked tests, optimized builds, and clean repeat-build hashes. Rerun on the final release commit.
- [x] `cargo audit` and `cargo deny check` pass on the locked dependency graph; review changes to direct dependencies and enabled features. PR commit `285143c` passed the dependency security workflow.
- [ ] Run every target in the current fuzz workflow on the final release candidate and attach corpus, duration, and run-count evidence. Earlier evidence covers three WSL2 ARM64 targets ([raw output](perf-results/2026-11-27-wsl-arm64-fuzz-smoke.txt)) and the four-target workflow on earlier commits (including run `46059738470`). Local H2 and H3 smokes are recorded in [the H2 log](perf-results/2026-11-28-wsl-arm64-http2-fuzz-smoke.txt) and [the H3 log](perf-results/2026-11-27-wsl-arm64-http3-fuzz-smoke.txt). The current workflow has six targets; it still needs a passing remote run on the release candidate. H3 coverage uses the maintained client stack and does not fuzz malformed raw control frames.
- [x] Rerun repeatable load tests at normal and configured-limit workloads on both architectures after the HTTP/2/HTTP/3 change. Three 30-second runs at 50 and 256 clients completed on x86-64 and ARM64 GitHub-hosted Linux VMs on `285143c`, along with shutdown-under-load checks. Raw results are in [the x86-64 logs](perf-results/2026-11-27-github-actions-285143c-x86_64-normal.txt) and [ARM64 logs](perf-results/2026-11-27-github-actions-285143c-aarch64-normal.txt), with connection-cap, shutdown, and TLS logs alongside them. These are diagnostic VM results, not a performance claim.
- [x] Record three-repeat TLS 1.3 new-connection measurements on both CI architectures on `285143c`; each workflow reported three complete repetitions. These are VM measurements; representative physical-host characterization remains open.
- [ ] Record binary size, idle and loaded RSS, CPU, startup, TLS handshake, latency, and scaling results on representative physical hosts. WSL2 measurements and one QEMU user-mode smoke run are available as diagnostics; neither meets the physical-host gate. The QEMU RSS includes emulator overhead.
- [x] Resolve the external HTTP/3 client stream-reset report and attach successful interoperability evidence: curl 8.21.0 negotiated H3, received all 1,024 response bytes, and exited successfully on WSL2 ARM64 after Relay began draining the retained request stream. The regression test covers an upstream early response during an open upload. [Raw smoke output](perf-results/2026-11-27-wsl-arm64-h3-curl-smoke.txt). Diagnostic curl-based H3 throughput runs are recorded; representative native-host performance evidence remains open.
- [x] Record the first WSL2 ARM64 comparison against Caddy, NGINX, and Apache in `docs/BENCHMARKS.md`; the NGINX HTTP/2 runs were inconsistent and this data is diagnostic only.
- [ ] An external independent security reviewer has reviewed the release candidate and all findings have a documented disposition.
- Review preparation: [`INDEPENDENT_SECURITY_REVIEW_PACKET.md`](INDEPENDENT_SECURITY_REVIEW_PACKET.md) defines the proposed scope and report evidence. This preparation is not an external review and does not close the gate.
- [ ] The release build has a source SBOM, per-binary SHA-256 checksums, GitHub build provenance attestations, and Cosign keyless signatures for binaries, checksum manifests, and SBOM. Verify all of them from a clean environment.
- [x] Verify repeat-build results on both supported architectures and explain any expected non-reproducible fields. CI matched clean release build hashes on `285143c`; rerun for the final release commit.
- Unit preflight: `scripts/release/verify-systemd-unit.sh` checks the deployment unit in a temporary root after the release build; it passed locally with systemd 261, and the CI workflow now runs it on both architectures. Current workflow evidence is pending. Target-distribution account, permissions, firewall, and runtime validation is still required before deployment.
- [ ] Review the draft release, attached files, notes, version, and upgrade/configuration implications. Publish only after maintainer approval.

## Draft artifact verification

The tag workflow is intentionally a draft-release producer. In a clean Linux environment, check out the source tag and install `gh`, Cosign, Python 3, and `sha256sum`:

```sh
TAG=vX.Y.Z-rc.N
gh release download "$TAG" --repo k18labs/relay --dir release-check
./scripts/release/verify-artifacts.sh "$TAG" release-check 2>&1 | tee "release-check/verification-$TAG.txt"
```

The verifier checks binary checksums, Cosign identity and issuer for binaries/manifests/SBOM, GitHub provenance for both binaries, and required SPDX JSON fields. Its local tamper-rejection checks can be run with `bash scripts/release/test-verify-artifacts.sh`; these mock the external signing and attestation tools and do not replace a real verification. Save the clean-environment output with the release review. A draft release and successful workflow are not themselves approval to publish.

## Competitor benchmark evidence

The available comparison data is virtualized WSL2 ARM64 output with a local Python upstream. It does not support a defensible performance ranking. Repeat competitor benchmarks on documented physical Linux hosts with stable results before making performance claims. Relay's external HTTP/3 reset was resolved and confirmed with curl; diagnostic H3 throughput results exist, while representative physical-host performance evidence remains open.

## PR benchmarked-code evidence (not release approval)

At PR commit `285143c`, Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, optimized builds, and clean repeat-build checks; dependency audit and fuzz smoke also passed. The benchmark matrix completed three normal and three capped runs plus shutdown-under-load on each architecture, and the corrected TLS harness recorded all three TLS 1.3 runs on both. Raw outputs are checked into `docs/perf-results/2026-11-27-github-actions-285143c-*.txt`. The HTTP/3 early-response regression passes, curl 8.21.0 negotiated H3 and received the full response, and diagnostic H3 comparison runs are recorded in `docs/perf-results/`. Clean-environment artifact verification, representative physical-host measurements, native-host H3 performance, external independent review, and maintainer approval remain open.

## Current release status

The project is still pre-release. The tagged workflow and draft checklist do not substitute for actual successful tagged runs, clean-environment verification, benchmark evidence, or an external review. Do not create or publish an RC tag while any required gate is open.
