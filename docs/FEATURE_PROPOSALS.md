# Feature proposals

These proposals describe possible work after the first release. They are not
approved scope, do not change the current implementation, and do not close any
release gate. Each proposal needs a maintainer decision and an independent
security review before implementation. The first release remains the small,
static-configuration reverse proxy described in `SPEC.md`.

## Configuration and certificate reload

**Problem.** Operators currently restart Relay to apply a route or certificate
change. A reload could reduce disruption, but it adds mutable shared state and
coordination to the request path.

**Proposed boundary.** Reload only a complete local configuration and its
referenced certificate files. Parse and validate the entire candidate, load and
validate its TLS material, and prepare any changed listeners before publishing
it. On any failure, retain the last known-good snapshot. Request handlers should
observe one immutable snapshot per request. Existing in-flight requests finish
under the snapshot they started with. No remote reload endpoint or arbitrary
file path supplied by a client is in scope.

**Required design decisions.** Define the signal or local control mechanism;
whether listener address changes are allowed; how file replacement races are
handled; how old listeners drain; and how reload results are reported without
logging secrets. Certificate and private-key bytes must never enter logs.

**Acceptance evidence.** Tests cover valid route replacement, invalid config,
invalid or mismatched certificate/key, missing files, concurrent requests
during reload, listener changes, repeated signals, and bounded drain. Failure
must leave the old configuration serving. Measure reload memory and prove old
snapshots are released after in-flight requests finish.

## Optional upstream HTTP/2

**Problem.** HTTP/1.1 upstream pooling may limit workloads where one configured
origin can efficiently multiplex requests. No current workload evidence shows
that this is a release need.

**Proposed boundary.** Keep HTTP/1.1 as the default. If measurements justify a
prototype, negotiate upstream HTTP/2 only through the existing verified TLS
connector. Keep configured origins fixed, preserve hostname and chain
verification, and apply existing body, request, timeout, and connection limits.
Do not add cleartext prior-knowledge mode or request retries.

**Required design decisions.** Establish the target workload and baseline;
define connection and stream caps, flow-control settings, protocol fallback,
pool partitioning, and cancellation semantics. Review all enabled dependency
features before adding any dependency or feature.

**Acceptance evidence.** Compare the same repeated workload over upstream H1 and
H2, including latency, throughput, CPU, memory, connection count, and tail
behavior under slow or canceled streams. Add tests for certificate failures,
pool limits, stream resets, cancellation without retry, and bounded shutdown.
Keep the option disabled by default unless results and review support changing
that policy.

## Multiple upstreams, health checks, and balancing

**Problem.** A route currently names one configured upstream. Multiple origins
could improve availability, but they expand failure and retry behavior and can
make request delivery ambiguous.

**Proposed boundary.** Any candidate origins remain explicitly listed in
configuration. Never select an origin from request data. Health-check work,
per-origin connections, and concurrent requests must be bounded. Do not retry a
request after it may have reached an upstream; retries of non-idempotent or
streaming requests are out of scope unless a separate protocol-safe design
proves otherwise.

**Required design decisions.** Define health-check method, interval, timeout,
startup behavior, recovery thresholds, balancing policy, draining, and behavior
when all origins are unhealthy. Decide whether health checks create a separate
network trust boundary and how operators see state without exposing an admin
endpoint.

**Acceptance evidence.** Test malformed and conflicting origins, DNS and
connection failures, slow checks, recovery, all-unhealthy behavior, load
distribution, request cancellation, and memory/connection bounds. Demonstrate
that client-controlled authority, path, and query cannot change the origin.

## Operator diagnostics

**Problem.** Operators may need aggregate visibility into load and failures.
No concrete metric consumer or deployment requirement has been established.

**Proposed boundary.** First assess whether existing process and service-manager
signals answer the operational question. If metrics are justified, prefer
aggregate counters and gauges with bounded label cardinality. Never include
request headers, paths, query values, bodies, credentials, cookies, or
unbounded client-controlled labels. A network administration endpoint remains
out of scope pending its own product and threat-model decision.

**Required design decisions.** Name the operator question, metric consumer,
collection interface, authentication and network exposure if any, retention,
cardinality limits, and behavior when collection fails. Do not add tracing of
request data as a substitute for metrics.

**Acceptance evidence.** Test that sensitive sentinels do not appear in emitted
metrics or logs, labels have fixed bounds, collection cannot stall proxy work,
and memory use remains bounded under changing client inputs.

## Cross-proposal review checklist

Before implementation, update `SPEC.md`, `DESIGN.md`, and `docs/THREAT_MODEL.md`
with the approved scope and trust boundaries; record the decision in an ADR;
review dependency and feature changes; add focused regression tests; and update
the release checklist with measurable acceptance evidence. Until then, the
proposals above remain unimplemented and excluded from the first release.
