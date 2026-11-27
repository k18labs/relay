# Performance

The one-run ARM64 WSL smoke result is saved in [`perf-results/2026-11-27-kali-wsl-arm64-smoke.txt`](perf-results/2026-11-27-kali-wsl-arm64-smoke.txt). It measured 15 seconds each with 4 `wrk` threads and 50 connections against a local threaded Python server returning a fixed 1 KiB file. The observed rates were 995.73 req/s direct to the upstream, 881.11 req/s through Relay, and 1,345.02 req/s through NGINX; Relay p50/p90/p99 latency was 48.23/78.24/123.57 ms, and NGINX was 34.40/41.00/62.78 ms.

The current PR's GitHub Actions matrix also completed one 15-second smoke run on each architecture. Raw outputs are saved for [Linux x86-64](perf-results/2026-11-26-github-actions-linux-x86_64-smoke.txt) and [Linux ARM64](perf-results/2026-11-26-github-actions-linux-aarch64-smoke.txt). Both used 4 threads and 50 connections on 4-vCPU Azure-hosted runners. Relay measured 1,156.14 req/s on x86-64 and 1,155.61 req/s on ARM64; NGINX measured 1,765.04 and 1,857.00 req/s, respectively. These are single-run smoke measurements with a Python threaded upstream, not sustained or release-grade comparisons.

Three 30-second WSL ARM64 repetitions at 50 clients and three at the configured 256-connection cap are recorded in [normal-load results](perf-results/2026-11-26-kali-wsl-arm64-load-normal.txt) and [connection-cap results](perf-results/2026-11-26-kali-wsl-arm64-load-connection-cap.txt). At 50 clients Relay varied from 428.53 to 960.25 req/s across the three runs. At 256 clients Relay varied from 369.52 to 1,699.31 req/s; the runs also recorded `wrk` timeouts and socket read errors. The upstream and NGINX results varied substantially too. This host's WSL2 scheduling and Python threaded upstream make these results unsuitable for release performance conclusions. They do provide repeat-run load evidence and show that a native-host stress run and limit/cleanup investigation are still needed.

The WSL measurements above are smoke and stress observations on an Oryon ARM64 laptop under WSL2 with a Windows-mounted checkout. They are not native-host release evidence. No release-grade binary-size, memory, startup, CPU, TLS-handshake, scaling, or at-limit results are available.

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
