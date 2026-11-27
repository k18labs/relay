# Contributing

Contributions should be small, tested, and consistent with `AGENTS.md`, `SPEC.md`, and the threat model. For protocol behavior, include a regression test and document the trust boundary and remaining limits.

Before opening a change, run:

```sh
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

Do not add dependencies without describing why the existing stack cannot do the job. Do not submit real credentials, keys, or certificates. Use synthetic test fixtures. By contributing, you agree that your contributions are offered under Apache-2.0.
