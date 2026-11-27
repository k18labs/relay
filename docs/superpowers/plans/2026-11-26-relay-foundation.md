# Relay Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Establish a validated Rust HTTP/1.1 reverse proxy foundation (M0 plus an HTTP-only M1 slice).

**Architecture:** Tokio accepts bounded TCP connections; Hyper parses HTTP/1.1 and forwards to a configured upstream selected by exact host and longest path prefix. Configuration is immutable and validated before the listener starts.

**Tech Stack:** Rust stable, Tokio, Hyper, Hyper-util, Serde, TOML, tracing, Clap.

**Spec:** `docs/superpowers/specs/2026-11-26-relay-foundation-design.md`

## Global Constraints

- `#![forbid(unsafe_code)]` in first-party Rust code.
- No client-controlled upstreams, retries, redirects, logging of secrets, or external telemetry.
- Reject unknown config fields and ambiguous route ties.
- Do not claim TLS, body-size enforcement, or release readiness until implemented and verified.

## Review Focus

- Duplicate or malformed Host headers: Hyper parsing and explicit single-host extraction reject malformed input.
- Unknown Host: return 421 and never contact an upstream.
- Equal host/path routes: config validation rejects ties.
- Hop-by-hop headers named by `Connection`: remove them before forwarding.
- Upstream failure: return 502 without exposing internal error details.

---

### Task 1: Foundation and configuration

**Files:** `Cargo.toml`, `rust-toolchain.toml`, `src/lib.rs`, `src/config.rs`, `src/main.rs`, `tests/config.rs`, docs at repository root.

- [ ] Add strict config types and validation.
- [ ] Test valid configuration, unknown keys, duplicate route precedence, and invalid upstream schemes.
- [ ] Implement CLI `check` and `version`.
- [ ] Generate and commit `Cargo.lock`; run formatting, tests, and lint.

### Task 2: HTTP forwarding

**Files:** `src/proxy.rs`, `src/main.rs`, `tests/proxy.rs`, `relay.example.toml`, `docs/CONFIGURATION.md`.

- [ ] Test forwarding, route choice, unknown host, and hop-by-hop header stripping using local fixtures.
- [ ] Implement bounded connection admission and Hyper HTTP/1 serving.
- [ ] Add graceful SIGINT/SIGTERM shutdown where supported.
- [ ] Run the full requested verification suite and record limitations.
