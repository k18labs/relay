# Threat model

Relay trusts the operator, configuration, local certificate files, and configured upstream names. Clients, request data, and upstream responses are untrusted. The listener should run with only the privileges it needs.

| Threat | Current mitigation | Evidence and remaining limitation |
|---|---|---|
| Request smuggling or ambiguous framing | Hyper owns HTTP parsing/serialization; duplicate Content-Length is rejected by regression test; bodies are streamed and capped | Selected framing cases tested. Fuzz targets cover config/routing, host normalization, and header policy, not Hyper's HTTP parser; broader malformed-input coverage remains |
| Header spoofing or request splitting | Typed HTTP headers; hop-by-hop and connection-nominated fields, `Forwarded`, `X-Forwarded-*`, and common client-IP fields are removed | Integration test checks stripping; Relay does not generate trusted client identity headers |
| Host confusion / open proxy / SSRF | Exactly one Host value is required; normalized host and path select only a configured route; unknown host gets 421 | Route and proxy tests; DNS resolution and operator configuration remain trusted |
| TLS downgrade or server impersonation | TLS 1.3 only; inbound cert/key validation; upstream chain and hostname validation; TLS early data disabled | Local TLS integration tests cover valid, untrusted, and wrong-host certificates; no external review yet |
| Resource exhaustion and slow peers | Connection/request/handshake admission limits, bounded header/body sizes, header/body/response/connect/handshake deadlines, header deadline for idle keep-alive connections, bounded idle pool, bounded shutdown | Unit/integration regression coverage and three-repeat WSL ARM64 loads at 50 and 256 clients are recorded. The WSL runs showed large variation and socket errors/timeouts; native stress testing and a measured memory envelope remain |
| Pool exhaustion or unsafe retries | Per-host idle pool cap; no custom retry layer; configured origin only | Reuse tested; cancellation and retry semantics need further review |
| Sensitive information in logs | Logs report connection errors only; no request headers, bodies, credentials, or cookies are logged | Manual source review; no dedicated log capture test |
| Dependency compromise | Locked direct dependencies and pinned toolchain; CI includes dependency/license checks | Audit and license checks passed for PR commit `dbe0149`; repeat them on the release commit. No release SBOM or signed artifact exists yet |
| Configuration ambiguity | Unknown fields rejected; routes, upstream origins, TLS pairs, and bounds validated before listener bind | Config tests; paths resolve from current working directory |
| Release artifact tampering | No public release artifacts are produced by this development tree | Checksums, provenance, reproducible build verification, and signing remain release blockers |

Rust prevents many memory-safety errors in first-party code (`#![forbid(unsafe_code)]`) but does not guarantee proxy or protocol correctness. Do not treat this development build as production-ready. Current PR CI passed on Linux x86-64 and ARM64 and the three fuzz smoke targets passed, but broader parser fuzzing, native stress/performance testing, and independent security review remain outstanding.
