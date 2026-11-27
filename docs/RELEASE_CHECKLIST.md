# Release checklist

Do not publish a Relay release until every required gate below has evidence attached to the release review.

## Required gates

- [ ] Review `SPEC.md`, `ROADMAP.md`, `docs/THREAT_MODEL.md`, and `docs/SECURITY_MODEL.md`; close every release-scoped requirement or list the exact limitation.
- [ ] CI passes on Linux x86-64 and ARM64 for the release commit: formatting, Clippy with warnings denied, locked tests, and optimized builds.
- [ ] `cargo audit` and `cargo deny check` pass on the locked dependency graph; review changes to direct dependencies and enabled features.
- [ ] Run each scheduled fuzz target and attach corpus, duration, and run-count evidence. Triage every crash.
- [ ] Run repeatable load tests at normal and configured-limit workloads on both architectures. Attach raw results and hardware/software versions.
- [ ] Compare equivalent reverse-proxy configurations against NGINX, HAProxy, Caddy, and Pingora, or record an explicit release-scope decision and defer the comparison.
- [ ] An external independent security reviewer has reviewed the release candidate and all findings have a documented disposition.
- [ ] The release build has a source SBOM, per-binary SHA-256 checksums, and GitHub build provenance attestations. Verify all three from a clean environment.
- [ ] Verify repeat-build results on both supported architectures and explain any expected non-reproducible fields.
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

## Latest PR evidence (not release approval)

At PR commit `dbe014932a37d85dd950ce0d10e7ed543499785d`, Linux x86-64 and ARM64 CI passed formatting, Clippy, locked tests, and optimized builds. Dependency audit, license policy, fuzz smoke, and one 15-second benchmark smoke on each architecture also passed. The local ARM64 repeat-build hashes match; the evidence is in [`release-evidence/2026-11-26-arm64-repeat-build.txt`](release-evidence/2026-11-26-arm64-repeat-build.txt). Repeated WSL load results are recorded, but had high variance and errors. These checks must be rerun for the final release commit; native load testing, clean-environment artifact verification, and independent review are still open.

## Current release status

The project is still pre-release. The tagged workflow and draft checklist do not substitute for actual successful tagged runs, clean-environment verification, benchmark evidence, or an external review. Do not create or publish an RC tag while any required gate is open.
