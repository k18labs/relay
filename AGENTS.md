# Engineering rules for Relay contributors and agents

- Read `SPEC.md`, `DESIGN.md`, `docs/THREAT_MODEL.md`, and this file before changing proxy behavior.
- Preserve `#![forbid(unsafe_code)]`; do not add unsafe Rust.
- Use Hyper/rustls protocol implementations; never implement cryptography or HTTP parsing from scratch.
- Keep upstream destinations configuration-controlled. Never derive them from request headers, paths, or query values.
- Reject unknown configuration fields and conflicting routes; validate before binding sockets.
- Keep client bodies streaming, with explicit byte/time limits before any production claim.
- Remove hop-by-hop and untrusted forwarding headers; do not log request headers, bodies, credentials, or cookies.
- Bound connections, concurrent work, buffers, pools, and shutdown duration. Avoid unbounded queues and detached work where cleanup is needed.
- Add a regression test for each behavior change. Run formatting, Clippy with warnings denied, and the full locked test suite.
- Update docs when actual behavior changes. State unimplemented behavior plainly; never mark a milestone complete without its acceptance evidence.
- Keep dependencies minimal, locked, and license-compatible. Review new dependencies and their feature sets.
