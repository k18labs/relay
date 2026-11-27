# Release checklist

Do not publish a Relay release until every required gate below has evidence attached to the release review.

## Required gates

- [x] Review `SPEC.md`, `ROADMAP.md`, `docs/THREAT_MODEL.md`, and `docs/SECURITY_MODEL.md`; remaining release limitations are listed in this checklist and the performance/security docs.
- [x] CI passes on Linux x86-64 and ARM64 for PR commit `71810ea`: formatting, Clippy with warnings denied, locked tests, optimized builds, and clean repeat-build hashes. Rerun for the eventual release commit.
- [x] `cargo audit` and `cargo deny check` pass on the locked dependency graph; review changes to direct dependencies and enabled features. PR commit `71810ea` passed after updating Quinn to patched releases.
- [x] Run each scheduled fuzz target and attach corpus, duration, and run-count evidence. All three targets completed bounded WSL2 ARM64 runs without crashes; see [raw output](perf-results/2026-11-27-wsl-arm64-fuzz-smoke.txt). Rerun on the release commit.
- [x] Rerun repeatable load tests at normal and configured-limit workloads on both architectures after the HTTP/2/HTTP/3 change. Three 30-second runs at 50 and 256 clients completed on x86-64 and ARM64 GitHub-hosted Linux VMs on `71810ea`; Relay had no client timeouts, while NGINX emitted read errors at the cap. Raw results are in `docs/perf-results/2026-11-27-github-actions-71810ea-*.txt`. These are diagnostic VM results, not a performance claim.
- [x] Record three-repeat TLS 1.3 new-connection measurements on both CI architectures on `71810ea`. These are VM measurements; representative physical-host characterization remains open.
- [ ] Record binary size, idle and loaded RSS, CPU, startup, TLS handshake, latency, and scaling results on representative physical hosts. WSL2 smoke data is available but does not meet the provisional startup or p99-overhead targets.
- [x] Resolve the external HTTP/3 client stream-reset report and attach successful interoperability evidence: curl 8.21.0 negotiated H3, received all 1,024 response bytes, and exited successfully on WSL2 ARM64 after Relay began draining the retained request stream. The regression test covers an upstream early response during an open upload. [Raw smoke output](perf-results/2026-11-27-wsl-arm64-h3-curl-smoke.txt). Diagnostic curl-based H3 throughput runs are recorded; representative native-host performance evidence remains open.
- [x] Record the first WSL2 ARM64 comparison against Caddy, NGINX, and Apache in `docs/BENCHMARKS.md`; the NGINX HTTP/2 runs were inconsistent and this data is diagnostic only.
- [ ] An external independent security reviewer has reviewed the release candidate and all findings have a documented disposition.
- [ ] The release build has a source SBOM, per-binary SHA-256 checksums, GitHub build provenance attestations, and Cosign keyless signatures for binaries, checksum manifests, and SBOM. Verify all of them from a clean environment.
- [x] Verify repeat-build results on both supported architectures and explain any expected non-reproducible fields. CI matched clean release build hashes on `71810ea`; rerun for the final release commit.
- [ ] Review the draft release, attached files, notes, version, and upgrade/configuration implications. Publish only after maintainer approval.

## Draft artifact verification

The tag workflow is intentionally a draft-release producer. On a clean Linux environment with `gh`, Cosign, and the GitHub CLI attestation extension available:

```sh
gh release download vX.Y.Z-rc.N --repo k18labs/relay --dir release-check
cd release-check
TAG=vX.Y.Z-rc.N
SIGNING_IDENTITY="https://github.com/k18labs/relay/.github/workflows/release.yml@refs/tags/$TAG"
sha256sum --check relay-linux-x86_64.sha256
sha256sum --check relay-linux-aarch64.sha256
for asset in relay-linux-x86_64 relay-linux-aarch64; do
  cosign verify-blob "$asset" --bundle "$asset.sigstore.json" \
    --certificate-identity "$SIGNING_IDENTITY" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com
done
for manifest in relay-linux-x86_64.sha256 relay-linux-aarch64.sha256 relay-sbom.spdx.json; do
  cosign verify-blob "$manifest" --bundle "$manifest.sigstore.json" \
    --certificate-identity "$SIGNING_IDENTITY" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com
done
gh attestation verify relay-linux-x86_64 --repo k18labs/relay
gh attestation verify relay-linux-aarch64 --repo k18labs/relay
```

Confirm the SPDX JSON file is present and parses with an SPDX-compatible validator. Save the verification output with the release review. A draft release and successful workflow are not themselves approval to publish.

## Competitor benchmark evidence

The available comparison data is virtualized WSL2 ARM64 output with a local Python upstream. It does not support a defensible performance ranking. Repeat competitor benchmarks on documented physical Linux hosts with stable results before making performance claims. Relay's external HTTP/3 reset was resolved and confirmed with curl; diagnostic H3 throughput results exist, while representative physical-host performance evidence remains open.

## Latest PR evidence (not release approval)

At PR commit `67ee7f0`, Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, optimized builds, and clean repeat-build checks; dependency audit and fuzz smoke also passed. The benchmark matrix completed three normal and three capped runs plus shutdown-under-load on each architecture; the corrected TLS harness recorded all three TLS 1.3 runs on both. The HTTP/3 early-response regression passes, curl 8.21.0 negotiated H3 and received the full response, and diagnostic H3 comparison runs are recorded in `docs/perf-results/`. The newer documentation and benchmark harness commits are awaiting CI. Clean-environment artifact verification, representative physical-host measurements, native-host H3 performance, external independent review, and maintainer approval remain open.

## Current release status

The project is still pre-release. The tagged workflow and draft checklist do not substitute for actual successful tagged runs, clean-environment verification, benchmark evidence, or an external review. Do not create or publish an RC tag while any required gate is open.
