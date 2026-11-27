# Independent security review packet

**Status:** prepared for a future reviewer; no independent reviewer is assigned
and no external review has taken place. This document does not satisfy the
release security-review gate.

## Review target

Review the exact release-candidate commit selected by the maintainer. Record
the full commit SHA, build/toolchain versions, operating system, and date before
starting. Re-run the review if the reviewed commit changes materially.

Relay is a fixed-route reverse proxy for Linux x86-64 and ARM64. It accepts
HTTP/1.1 and TLS-negotiated HTTP/2 over TCP, and HTTP/3 over QUIC when TLS is
enabled. Upstreams are configured by the operator; upstream requests currently
use HTTP/1.1. The intended trust boundary, exclusions, and security assumptions
are in [`THREAT_MODEL.md`](THREAT_MODEL.md), [`SECURITY_MODEL.md`](SECURITY_MODEL.md),
[`SPEC.md`](../SPEC.md), and [`DESIGN.md`](../DESIGN.md).

## Review scope

Please independently inspect and test:

- Configuration parsing, validation, route conflicts, host/path matching, and
  validation before socket binding (`src/config.rs`, CLI tests).
- HTTP/1.1 parsing boundaries, authority handling, header normalization,
  hop-by-hop and forwarding-header policy, request/response streaming, and
  body limits (`src/proxy.rs`, `tests/proxy.rs`).
- HTTP/2 and HTTP/3 stream admission, cancellation/reset behavior, flow control,
  malformed protocol input, and interaction with shared request limits
  (`tests/tls_proxy.rs`, `tests/http3_proxy.rs`, `fuzz/`).
- TLS 1.3 setup, certificate/key handling, upstream certificate-chain and
  hostname validation, ALPN, QUIC setup, and rejection of early data
  (`src/tls.rs`, TLS integration tests).
- Resource bounds and lifecycle: connection/request/handshake admission,
  timeouts, pools, slow peers, shutdown deadlines, task cleanup, and resource
  behavior under concurrent load.
- Fixed configured upstreams and SSRF boundaries; verify request authority,
  path, and query cannot select or alter the configured origin.
- Logging and error paths for leakage of headers, bodies, credentials, cookies,
  or other request-derived secrets.
- Dependency and feature selection, release build outputs, artifact integrity,
  and deployment guidance.

The reviewer may narrow or extend this scope, but should document any omitted
area and why. Features excluded from `SPEC.md`, including a remote admin API,
general forward proxying, caching, and load balancing, are not part of this
release candidate.

## Evidence to provide

The review report should include:

1. Reviewer identity/organization, review date, exact commit SHA, and test
environment.
2. Areas reviewed, methods/tools used, and any areas not reviewed.
3. Findings with severity, affected code, reproduction steps, impact, and a
   proposed or required fix.
4. A disposition for every finding: fixed with commit and regression evidence,
   accepted with maintainer rationale, or still open.
5. Retest results for fixes and a clear statement of residual risks.
6. A final assessment that does not imply production readiness unless the
   reviewer explicitly supports that conclusion.

Use [`SECURITY_REVIEW.md`](SECURITY_REVIEW.md) to record findings after review.
Do not put credentials, private keys, or sensitive customer data in the report.

## Suggested local validation

Run the commands required by `AGENTS.md` on the exact reviewed commit:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo audit
cargo deny check
```

The repository's scheduled fuzz workflow, supported-architecture CI, and
release checklist provide additional evidence. Passing automated checks do not
replace manual review, exploitability analysis, or a documented finding
disposition.
