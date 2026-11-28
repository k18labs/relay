# Relay release-safety phase design

**Status:** proposed; awaiting maintainer approval
**Roadmap scope:** release gates and P1 reliability work, delivered before the P3 proposals

## Intent

Make Relay's existing HTTP/1.1, HTTP/2, and HTTP/3 reverse-proxy behavior reliable enough to support an evidence-based draft release. Close the known HTTP/3 stalled-body timeout failure, complete the outstanding resource-limit and release-artifact validation, gather physical-host measurements, and prepare the code and evidence for an independent security review. Keep the roadmap honest: emulated or virtual-machine data cannot satisfy physical-host gates, and an internal review cannot satisfy the independent-review gate.

This is the first phase of the user's full-roadmap goal. It does not discard the P3 proposals; those will be designed and delivered as separate projects after the release-safety phase.

## Goals

- Make an HTTP/3 request with a stalled body return 408, send the intended HTTP/3 cancellation code, avoid panics, release request and stream capacity, and allow follow-up traffic on the same connection.
- Complete multiplexed-resource acceptance for HTTP/2 and HTTP/3, including reset/cancellation, memory bounds, and shutdown under load.
- Review and resolve the open Dependabot action and Rust dependency updates using their locked audit, formatting, Clippy, test, and release-build evidence.
- Exercise the protected-tag workflow end to end; create only a draft release and verify its binary, SBOM, checksums, Cosign bundles, and GitHub provenance from a clean environment.
- Collect repeatable physical-host measurements for size, memory, CPU, startup, TLS handshakes, and connection scaling; compare Relay with Caddy, NGINX, and Apache for HTTP/1.1, HTTP/2, and HTTP/3 where each server supports it.
- Derive regression thresholds only from stable physical-host baselines.
- Prepare a current, self-contained security-review packet and obtain a documented review from a reviewer who did not author the implementation. Do not count a self-review as completion.
- Update `ROADMAP.md`, `SPEC.md`, `DESIGN.md`, `docs/THREAT_MODEL.md`, protocol/performance docs, and release evidence only when the behavior or acceptance evidence justifies it.

## Non-goals for this phase

- Implementing the four P3 proposals (configuration/certificate reload, optional upstream HTTP/2, multiple upstreams with health checks/balancing, and operator diagnostics). Each is a later project with its own design and acceptance plan.
- Publishing a release. The workflow remains draft-only; publication requires the verified artifacts and review gates.
- Treating WSL2, QEMU, or hosted CI runners as physical-host evidence.
- Adding unsafe Rust, custom HTTP parsing or cryptography, arbitrary client-selected upstreams, unbounded work, or request-data logging.

## Approaches considered

1. **Release safety first — selected by the maintainer.** Fix the known protocol failure and complete evidence and review gates first, then deliver the P3 projects separately. This limits new behavior while the current proxy has an unresolved HTTP/3 failure and makes later feature measurements comparable.
2. **Features first.** Implement P3 before release gates. This would add mutable configuration, new upstream protocol behavior, balancing, and diagnostics before current safety evidence is complete, and would invalidate existing baselines.
3. **Stop at the original release scope.** Close M4 and leave P3 proposals deferred. This does not meet the full-roadmap goal and is rejected.

## Design choices

### HTTP/3 timeout cancellation

The locked dependency is `h3-quinn 0.0.10`. Its registry source unwraps the Quinn receive stream in `stop_sending`; if `poll_data` has moved that stream into a pending read future, a timeout path can panic. Upstream issue [hyperium/h3#361](https://github.com/hyperium/h3/issues/361) describes the newer upstream behavior after the panic was avoided: `stop_sending` is deferred until the pending read completes, so a dropped stream may send code 0 rather than the requested HTTP/3 code. The nearby merged upstream change #357 addresses `recv_id()` during a pending read, not this stop behavior. The local patch must fix the locked-version panic and send the requested stop code; merely copying the deferred-stop behavior is insufficient.

**Proposed choice:** use a narrowly scoped, license-preserving local patch of the pinned `h3-quinn` adapter, with an upstreamable regression test and a clear removal condition when an upstream release includes the fix. Avoid a drop-only workaround, because it can still emit the wrong code; avoid replacing the QUIC adapter because that expands the change well beyond the failing operation. Keep the patch versioned and reviewable, and do not add new cryptographic or protocol parsing code.

Acceptance requires the existing stalled-upload reproduction to pass without `#[ignore]`, capture/verify the peer-visible stop code, verify no panic, verify 408 and same-connection follow-up behavior, and pass on Linux x86-64 and ARM64 CI. The test should also fail against the current unpatched dependency so it proves the defect rather than only the workaround.

### Multiplexed resource limits

Use two explicit profiles so correctness coverage stays fast while the pre-release run exercises configured defaults:

- **Cross-platform correctness/stress:** run HTTP/2 and HTTP/3 separately with `max_concurrent_streams_per_connection = 64`, `max_concurrent_requests = 1024`, and 8 client connections (512 simultaneous streams). Send fixed 64 KiB bodies to a local upstream that drains without retaining them. Gate upstream responses so the test can assert the active-work ceiling, then release all streams. Repeat with client reset/cancellation, server shutdown, and one stream timing out. Capacity must return and follow-up requests must succeed.
- **Physical-host memory/shutdown:** use 16 connections × 64 streams (1,024 simultaneous requests), each streaming a fixed 1 MiB body to the discard upstream. Record active requests, process RSS, CPU, and time to drain. RSS must plateau below 1 GiB while the 1 GiB aggregate body data is in flight, return within 64 MiB of the pre-run baseline within 10 seconds after completion, and the process must exit within the configured 5-second shutdown bound when signaled during load. If the proper streaming path exceeds these initial ceilings, record the measured floor and revise the threshold before closing the gate; do not hide instability by weakening the request/stream profile.

Retain the existing cap-one tests as small deterministic regressions. A test that only proves requests serialize under a stream limit is not sufficient to close this item.

### Dependency updates

Review the open bot-authored updates as separate changes. The actions update must preserve the `subject-path` input and the workflow's `contents`, `id-token`, and `attestations` permissions. The TOML update must keep the strict configuration behavior and reject unknown fields. Merge only after their checks and the update-specific source review pass; refresh any PR made stale by a preceding merge.

### Physical benchmarks

The available host is a physical ARM64 Surface Laptop with 32 GB of memory running Windows ARM64. Caddy and NGINX packages are discoverable through Windows Package Manager; Apache availability and native HTTP/2/HTTP/3 support must be confirmed before comparison. Use native builds on this host where available as supplemental physical ARM64 evidence. It does not substitute for the Linux matrix already required by `docs/PERFORMANCE.md` and `docs/RELEASE_CHECKLIST.md`: representative physical Linux x86-64 and ARM64 hosts are still required to close those gates.

**Minimum accepted matrix:** on each physical Linux architecture, benchmark Relay, Caddy, NGINX, and Apache for HTTP/1.1 and HTTP/2 where supported; include HTTP/3 for every server with a verified native H3 build, and mark unsupported combinations explicitly. Measure 1, 50, and 256 concurrent clients with a fixed 1 KiB response and a separate 1 MiB streaming-body case. Collect stripped binary size, startup-to-ready time, idle and loaded RSS, CPU, TLS 1.3 new-handshake rate, throughput, and p50/p95/p99 latency. Use a 15-second warm-up followed by five 60-second runs per case; publish the median and range. Treat a case as stable only when throughput and p95-latency coefficients of variation are each at most 10%; otherwise rerun five times and label it inconclusive if still unstable. Preserve every raw run and record host/kernel/OS, CPU and memory, software/client versions, power mode, commands, configuration, and protocol negotiation. Never combine unsupported protocols or unlike workloads into a parity claim.

Existing hosted x86-64/ARM64 results remain diagnostic references. The current Surface host alone does not satisfy the required physical Linux matrix; missing physical-host access remains an explicit open gate.

### Release artifacts

Run the existing `v*` tag workflow on a protected, reviewed release-candidate tag. Verify the generated assets in a clean environment with `scripts/release/verify-artifacts.sh`, including expected subject identity/issuer, checksums, SPDX content, and provenance. Save verifier output and a manifest of the exact tag and commit. Confirm the resulting release is a draft and do not publish it.

### Independent review

Refresh `docs/INDEPENDENT_SECURITY_REVIEW_PACKET.md` after the implementation and tests settle. Include scope, threat model, dependencies/features, relevant code paths, test/benchmark evidence, known limitations, and the exact commit. Obtain findings from an external reviewer with repository write access; track each finding and record the review result. The agent and PR author cannot complete this acceptance item for themselves.

## Constraints

- Read and preserve `AGENTS.md`, `SPEC.md`, `DESIGN.md`, and `docs/THREAT_MODEL.md` when changing proxy behavior.
- Preserve `#![forbid(unsafe_code)]`; use Hyper/rustls/Quinn implementations for HTTP and cryptography.
- Keep upstream destinations configuration-controlled; reject unknown configuration and conflicting routes before binding.
- Keep bodies streaming with explicit byte and time bounds; remove hop-by-hop and untrusted forwarding headers; keep request data and secrets out of logs.
- Bound connections, concurrent work, buffers, pools, and shutdown. Avoid detached work that cannot be cleaned up.
- Add regressions for behavior changes. Run formatting, Clippy with warnings denied, and the full locked suite; inspect dependency features/licenses before adding or changing dependencies.
- Never mark a roadmap checkbox complete without evidence that meets that checkbox's host, protocol, review, or artifact requirements.

## Acceptance sequence

1. Resolve the adapter defect and prove cancellation semantics on both CI architectures.
2. Finish the remaining H2/H3 stream, memory, cancellation, and shutdown evidence.
3. Resolve the open dependency PRs and rerun checks on the resulting main commit.
4. Gather native physical-host benchmark evidence; leave any unsupported platform/protocol matrix entries plainly open.
5. Complete and verify draft release artifacts from a protected tag.
6. Obtain and record the independent security review; address findings and rerun affected checks.
7. Set thresholds from the accepted physical baseline, then update the roadmap and release checklist.

Only the evidence-backed items above are in this phase. After it, continue the user's full-roadmap goal by designing and implementing each P3 proposal as its own reviewed project.

## Risks and unresolved gates

- A local adapter patch creates a small maintenance obligation until upstream releases a verified fix.
- The physical ARM64 Windows host may not provide native support for every comparison server/protocol, and no physical x86-64 Linux host is currently confirmed.
- An independent reviewer is not yet assigned. Preparing the review packet is possible here; completing the independent-review gate is not.
- No physical Linux x86-64 or ARM64 benchmark host has been confirmed. The local Windows ARM64 Surface is supplemental only; it cannot close the Linux-host acceptance matrix.
- Protected-tag execution depends on the repository's tag rules and GitHub Actions permissions; verify the effective policy before creating a tag.
