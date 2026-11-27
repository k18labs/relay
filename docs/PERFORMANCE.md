# Performance

The one-run ARM64 WSL smoke result is saved in [`perf-results/2026-11-27-kali-wsl-arm64-smoke.txt`](perf-results/2026-11-27-kali-wsl-arm64-smoke.txt). It measured 15 seconds each with 4 `wrk` threads and 50 connections against a local threaded Python server returning a fixed 1 KiB file. The observed rates were 995.73 req/s direct to the upstream, 881.11 req/s through Relay, and 1,345.02 req/s through NGINX; Relay p50/p90/p99 latency was 48.23/78.24/123.57 ms, and NGINX was 34.40/41.00/62.78 ms.

This is a single smoke run on an Oryon ARM64 laptop under WSL2 with a Windows-mounted checkout. It is not native-host release evidence and is too short for a performance claim. The result file preserves the complete raw output. No accepted binary-size, memory, startup, CPU, TLS-handshake, scaling, or at-limit results are available.

## Repeatable local smoke comparison

On Linux with `nginx`, `wrk`, Python 3, and a release build of Relay installed, run:

```sh
cargo build --locked --release
DURATION=15s THREADS=4 CONCURRENCY=50 \
  bash scripts/load/run-linux-local.sh target/release/relay /usr/sbin/nginx \
  | tee arm64-wsl-smoke.txt
```

The harness sends the same fixed 1 KiB response directly from one local Python HTTP server and through Relay and NGINX reverse proxy configurations. It prints the kernel, CPU model, logical CPU count, memory, software versions, and `wrk` latency percentiles and request rates. The service logs and temporary configs are deleted at exit. Change `DURATION`, `THREADS`, and `CONCURRENCY` for repeated load levels. Run at least five repetitions after warm-up and preserve raw stdout separately.

This harness is a local smoke comparison only. The current available environment is Kali Linux ARM64 under WSL, so its timings are affected by virtualization, host scheduling, and the Windows-mounted checkout. No results from that environment should be treated as native-host release evidence. It currently covers Relay and NGINX only; equivalent HAProxy, Caddy, and Pingora runs are outstanding.

## Release measurements still required

Measure stripped binary size, idle and loaded RSS, startup time, throughput, requests per second, CPU, p50/p95/p99 latency, TLS handshakes, connection scaling, and behavior at configured limits. Compare equivalent reverse-proxy configurations for Relay, NGINX, HAProxy, Caddy, and Pingora on documented native Linux x86-64 and ARM64 hosts. Record exact versions, hardware, kernel, configuration, command lines, raw repeated results, and any host-dependent reproducibility differences. Do not present results from unlike workloads as directly comparable.
