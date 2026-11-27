# Internal security review log

Date: 2026-11-26

This is an internal source review, not an external security assessment or penetration test. An independent internal reviewer examined proxy parsing, connection lifecycle, shutdown, TLS, and resource bounds. The findings below were addressed and have local regression coverage.

| Finding | Change | Regression evidence |
| --- | --- | --- |
| Asterisk and absolute-form targets could reach route URI construction. | Accept origin-form targets only; build upstream URIs from typed scheme and authority components. | `rejects_asterisk_form_targets_before_routing`, `rejects_absolute_form_request_targets` |
| A downstream peer that stopped reading could retain request capacity. | Apply a downstream write-progress deadline and release request permits when the stream ends. | `times_out_stalled_client_writes_and_releases_request_capacity` |
| Transient listener errors terminated the proxy. | Retry transient accept errors with a fixed backoff; surface configured nonrecoverable error kinds. | `accept_retries_after_a_transient_listener_error`, `accept_returns_nonrecoverable_listener_errors` |
| Upstream TLS handshakes were not bounded by the TCP connect deadline. | Bound the full connector future and cap in-flight connector operations. | `bounds_the_upstream_tls_handshake_with_the_connect_deadline` |
| Idle pool timer was missing, and canceled requests could be retried. | Configure Hyper's pool timer and disable canceled-request retries. | `closes_idle_upstream_connections_after_the_pool_deadline`, `does_not_retry_a_canceled_upstream_exchange` |
| Keep-alive requests could continue during drain. | Signal Hyper graceful shutdown and reject newly dispatched requests once draining begins. | `graceful_shutdown_stops_keep_alive_requests_from_reaching_upstream`, `graceful_shutdown_waits_for_active_request_completion` |
| Unsupported transfer codings and malformed chunk data needed explicit coverage. | Reject unsupported transfer-coding chains; rely on Hyper for parsing and framing normalization. | `rejects_unsupported_request_transfer_codings`, `rejects_unsupported_upstream_response_transfer_codings`, `normalizes_transfer_encoding_and_content_length_conflicts_before_forwarding`, `does_not_forward_invalid_chunk_payload` |
| Explicit authority ports and userinfo needed strict rejection. | Validate configured upstream and client Host authorities, including port range and userinfo. | Config invalid-port tests plus `rejects_host_authorities_with_out_of_range_ports` and `rejects_host_authority_userinfo` |

First-party Rust continues to use `#![forbid(unsafe_code)]`. The obsolete `rustls-pemfile` dependency was replaced by the PEM API supplied by `rustls-pki-types`; certificate/key integration tests pass. Current advisory and license checks pass. `cargo deny check` reports duplicate `syn` and `winnow` versions as warnings; these come through distinct major-version requirements in the current locked graph.

## Remaining independent review

An external security reviewer has not reviewed this release candidate. The external review, its scope, the reviewer identity, and disposition of any new findings remain release gates. Do not interpret this internal log as approval to deploy or publish.
