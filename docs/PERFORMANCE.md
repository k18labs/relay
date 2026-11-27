# Performance

The one-run ARM64 WSL smoke result is saved in [`perf-results/2026-11-27-kali-wsl-arm64-smoke.txt`](perf-results/2026-11-27-kali-wsl-arm64-smoke.txt). It measured 15 seconds each with 4 `wrk` threads and 50 connections against a local threaded Python server returning a fixed 1 KiB file. The observed rates were 995.73 req/s direct to the upstream, 881.11 req/s through Relay, and 1,345.02 req/s through NGINX; Relay p50/p90/p99 latency was 48.23/78.24/123.57 ms, and NGINX was 34.40/41.00/62.78 ms.

The current PR's GitHub Actions matrix also completed one 15-second smoke run on each architecture. Raw outputs are saved for [Linux x86-64](perf-results/2026-11-26-github-actions-linux-x86_64-smoke.txt) and [Linux ARM64](perf-results/2026-11-26-github-actions-linux-aarch64-smoke.txt). Both used 4 threads and 50 connections on 4-vCPU Azure-hosted runners. Relay measured 1,156.14 req/s on x86-64 and 1,155.61 req/s on ARM64; NGINX measured 1,765.04 and 1,857.00 req/s, respectively. These are single-run smoke measurements with a Python threaded upstream, not sustained or release-grade comparisons.

## Repeated Linux VM load results

The PR benchmark workflow completed three 30-second runs at 50 clients and three at Relay's configured 256-connection limit on 4-vCPU GitHub-hosted Linux VMs. Raw outputs include kernel, CPU model, memory, and tool versions: [x86-64 normal](perf-results/2026-11-27-github-actions-linux-x86_64-normal.txt), [x86-64 connection cap](perf-results/2026-11-27-github-actions-linux-x86_64-connection-cap.txt), [ARM64 normal](perf-results/2026-11-27-github-actions-linux-aarch64-normal.txt), and [ARM64 connection cap](perf-results/2026-11-27-github-actions-linux-aarch64-connection-cap.txt).

Median requests/second across the three runs:

| Architecture | Clients | Direct upstream | Relay | NGINX |
| --- | ---: | ---: | ---: | ---: |
| x86-64 | 50 | 1,167.78 | 1,159.85 | 2,828.61 |
| ARM64 | 50 | 1,167.86 | 1,155.91 | 1,877.47 |
| x86-64 | 256 | 6,100.50 | 5,141.28 | 2,811.52* |
| ARM64 | 256 | 3,660.53 | 3,221.87 | 1,561.40* |

At 50 clients, the Python upstream limits Relay throughput to about 1,160 requests per second. At 256 clients, NGINX emitted client-side read socket errors in all three runs (*); Relay had no reported `wrk` socket errors or timeouts in those runs. Because the NGINX capped runs had errors and the VM architectures differ, these figures are diagnostic only and do not establish performance parity or superiority.

The workflow also sent SIGTERM during 256-client load on both architectures. Relay exited cleanly inside the 7-second watchdog; the raw results are [x86-64](release-evidence/2026-11-27-github-actions-linux-x86_64-shutdown-load.txt) and [ARM64](release-evidence/2026-11-27-github-actions-linux-aarch64-shutdown-load.txt). `wrk` socket errors in those two files are expected because the test deliberately stops Relay during the run.

## Post-worker-count Linux VM measurements

Commit `de76c72` changed Relay to a two-worker default. The benchmark matrix passed again on both four-vCPU GitHub-hosted VMs: three 30-second runs at 50 clients and three at the configured 256-connection cap. Raw stdout is preserved for [x86-64 normal](perf-results/2026-11-27-github-actions-de76-x86_64-normal.txt), [x86-64 cap](perf-results/2026-11-27-github-actions-de76-x86_64-connection-cap.txt), [ARM64 normal](perf-results/2026-11-27-github-actions-de76-aarch64-normal.txt), and [ARM64 cap](perf-results/2026-11-27-github-actions-de76-aarch64-connection-cap.txt). The CI run also recorded [TLS](perf-results/2026-11-27-github-actions-de76-x86_64-tls.txt) and [ARM64 TLS](perf-results/2026-11-27-github-actions-de76-aarch64-tls.txt), but only one repetition each. OpenSSL 3.0.13's `s_time -new` reports completed stats and exits with status 1 because that code path skips the later status reset ([OpenSSL 3.0.13 source](https://github.com/openssl/openssl/blob/openssl-3.0.13/apps/s_time.c)). The corrected harness validates a full timed connection summary before accepting that known status; current-commit CI must confirm three recorded repetitions before the TLS evidence is considered complete.

Median requests/second across the three repetitions:

| Architecture | Clients | Direct upstream | Relay | NGINX |
| --- | ---: | ---: | ---: | ---: |
| x86-64 | 50 | 1,167.52 | 1,162.56 | 2,837.42 |
| ARM64 | 50 | 1,167.81 | 1,155.04 | 1,754.83 |
| x86-64 | 256 | 6,113.29 | 5,150.73 | 2,783.22* |
| ARM64 | 256 | 3,643.73 | 3,240.01 | 1,526.47* |

The 50-client median p99 latencies were 42.06 ms for Relay on x86-64 (42.03 ms direct) and 43.04 ms on ARM64 (42.03 ms direct). At 256 clients Relay p99 was 118.86 ms on x86-64 and 909.99 ms on ARM64; latency increased sharply at the configured connection cap. NGINX emitted client read errors in every capped run (*), so the cap comparison does not support a ranking.

The same logs report median Relay CPU and peak sampled RSS: x86-64 was 8% CPU and 8,936 KiB RSS at 50 clients, and 30% CPU and 20,624 KiB at 256; ARM64 was 16% and 7,076 KiB at 50, and 42% and 19,748 KiB at 256. The optimized stripped binaries were 5,980,952 bytes (x86-64) and 4,659,520 bytes (ARM64). Median startup-to-first-HTTP measurements were 49 ms and 62 ms, respectively; idle RSS was 6,052 KiB and 4,304 KiB. These are VM observations, not physical-host release evidence. The x86-64 size is above the provisional ~5 MB target and ARM64 startup is above the provisional ~50 ms target.

Shutdown-under-load passed on both architectures; raw logs are [x86-64](release-evidence/2026-11-27-github-actions-de76-x86_64-shutdown-load.txt) and [ARM64](release-evidence/2026-11-27-github-actions-de76-aarch64-shutdown-load.txt). Socket errors there are expected because Relay is deliberately stopped while clients are active.

### Final PR run and TLS 1.3 handshakes

Commit `f3625a4` reran the load and TLS matrix after fixing the OpenSSL 3.0 `-new` exit-status handling. All three 15-second TLS runs completed on each architecture. Using the tool's 16-second real-time window, median new TLS 1.3 connections were 1,099.8/sec on x86-64 (17,597 connections) and 467.5/sec on ARM64 (7,480 connections). TLS-listener startup-to-first-handshake was 42 ms / 62 ms, with idle RSS 7,532 / 5,640 KiB. Raw runs: [x86-64 TLS](perf-results/2026-11-27-github-actions-f3625a4-x86_64-tls.txt) and [ARM64 TLS](perf-results/2026-11-27-github-actions-f3625a4-aarch64-tls.txt).

The same commit's repeated HTTP results were similar to `de76c72`: median Relay throughput at 50 clients was 1,162.16 req/s on x86-64 and 1,154.45 req/s on ARM64; at 256 clients it was 5,146.45 and 3,139.15 req/s. Median p99 at 50 clients was 42.06 ms / 43.07 ms (direct upstream 42.03 ms on both). At 256 clients, Relay p99 reached 130.84 ms / 1.27 s; one ARM64 run recorded 15 client timeouts. NGINX again had client read errors at the configured cap. The raw [normal](perf-results/2026-11-27-github-actions-f3625a4-x86_64-normal.txt) and [connection-cap](perf-results/2026-11-27-github-actions-f3625a4-aarch64-connection-cap.txt) logs are retained for both architectures. These results do not support a performance claim, and physical-host characterization remains open.

For `f3625a4`, optimized stripped binaries measured 5,980,952 bytes (x86-64) and 4,659,520 bytes (ARM64). Median plaintext startup was 48 ms / 62 ms; idle RSS was 6,036 / 4,312 KiB. Mean sampled CPU and peak sampled RSS in the normal runs were 7.9% / 8,692 KiB on x86-64 and 16.7% / 7,076 KiB on ARM64. At the configured cap these were 30.5% / 21,972 KiB and 42.6% / 19,876 KiB. The VM run still exceeds the provisional ~5 MB x86-64 binary target and ~50 ms ARM64 startup target.

The final PR run's [x86-64](release-evidence/2026-11-27-github-actions-f3625a4-x86_64-shutdown-load.txt) and [ARM64](release-evidence/2026-11-27-github-actions-f3625a4-aarch64-shutdown-load.txt) shutdown-under-load checks passed within the 7-second watchdog. The ARM64 cap profile had 15 request timeouts in one of three repetitions; the other two had none. This is recorded as an observed stress result, not hidden as harness noise.

Three 30-second WSL ARM64 repetitions at 50 clients and three at the configured 256-connection cap are recorded in [normal-load results](perf-results/2026-11-26-kali-wsl-arm64-load-normal.txt) and [connection-cap results](perf-results/2026-11-26-kali-wsl-arm64-load-connection-cap.txt). At 50 clients Relay varied from 428.53 to 960.25 req/s across the three runs. At 256 clients Relay varied from 369.52 to 1,699.31 req/s; the runs also recorded `wrk` timeouts and socket read errors. The upstream and NGINX results varied substantially too. This host's WSL2 scheduling and Python threaded upstream make these results unsuitable for release performance conclusions.

A separate 256-client WSL test sent SIGTERM during active load and confirmed that Relay exited cleanly within the 7-second watchdog; [raw output](release-evidence/2026-11-27-wsl-arm64-shutdown-load.txt) includes the client-side disconnects caused by stopping the proxy.

The WSL measurements above are smoke and stress observations on an Oryon ARM64 laptop under WSL2 with a Windows-mounted checkout, not native-host release evidence. The GitHub results are virtualized cloud VM data, not dedicated physical-host measurements.

## Resource and TLS smoke measurements

After adding a configurable Tokio worker count (default 2, allowed range 1–64), a local WSL2 ARM64 measurement recorded a 4,659,624-byte optimized binary, 85 ms to first successful plaintext HTTP response, and 8,456 KiB idle RSS. During a 15-second 50-client run, `pidstat` sampled Relay at 15–18% CPU and 8,536 KiB RSS. Direct upstream and Relay throughput were 1,078.30 and 1,053.35 requests/second; p99 latency was 51.25 ms direct and 56.09 ms through Relay. A separate TLS 1.3 run measured 90 ms to the first completed handshake, 9,676 KiB idle RSS, and three `openssl s_time` repetitions. Raw outputs are [HTTP/load](perf-results/2026-11-27-wsl-arm64-workers2-normal.txt) and [TLS](perf-results/2026-11-27-wsl-arm64-workers2-tls.txt).

These are single local WSL2 smoke runs, not physical-host release measurements. The observed 4.84 ms p99 difference exceeds the provisional 1 ms target, and the startup observations exceed the provisional 50 ms target; those targets are not met by this evidence. The two-worker default reduces observed idle RSS below the provisional 10 MiB target on this host, while the TLS idle sample is close to that threshold. Confirm all resource figures on supported physical hosts before making a release claim.

## PR commit `71810ea` CI benchmark rerun

The PR commit `71810ea` reran the three-repeat normal-load, connection-cap, shutdown-under-load, and TLS 1.3 profiles on four-vCPU GitHub-hosted Linux VMs. Raw outputs are preserved for [x86-64 normal](perf-results/2026-11-27-github-actions-71810ea-x86_64-normal.txt), [x86-64 cap](perf-results/2026-11-27-github-actions-71810ea-x86_64-connection-cap.txt), [x86-64 shutdown](perf-results/2026-11-27-github-actions-71810ea-x86_64-shutdown.txt), [x86-64 TLS](perf-results/2026-11-27-github-actions-71810ea-x86_64-tls.txt), [ARM64 normal](perf-results/2026-11-27-github-actions-71810ea-aarch64-normal.txt), [ARM64 cap](perf-results/2026-11-27-github-actions-71810ea-aarch64-connection-cap.txt), [ARM64 shutdown](perf-results/2026-11-27-github-actions-71810ea-aarch64-shutdown.txt), and [ARM64 TLS](perf-results/2026-11-27-github-actions-71810ea-aarch64-tls.txt).

| Architecture | CPU | Binary bytes | Median startup | Median idle RSS | Median Relay throughput at 50 clients | Median p99 at 50 clients | Median p99 at 256 clients |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| x86-64 | AMD EPYC 7763 | 7,835,752 | 81 ms | 7,004 KiB | 1,157.34 req/s | 43.00 ms | 1,090 ms |
| ARM64 | Neoverse-N2 | 6,374,712 | 54 ms | 5,320 KiB | 1,157.87 req/s | 42.98 ms | 568.82 ms |

The capped runs had median Relay throughput of 3,139.11 req/s on x86-64 and 3,633.80 req/s on ARM64. Mean sampled Relay CPU and peak sampled RSS were 16.06% / 9,700 KiB (x86-64) and 14.08% / 8,100 KiB (ARM64) at 50 clients; at 256 clients they were 42.51% / 23,400 KiB and 39.90% / 21,504 KiB. Both shutdown-under-load checks passed. The TLS 1.3 harness recorded three successful 15-second runs per architecture; median connections per 16-second measurement window were 9,329 on x86-64 and 7,583 on ARM64. Startup to the first handshake was 63 ms / 60 ms, with TLS idle RSS of 8,232 KiB / 6,716 KiB.

The x86-64 binary remains above the provisional 5 MB target and startup remains above 50 ms. NGINX reported client read errors in the configured-cap runs. These hosted VMs are not physical-host evidence and the results do not support a performance-superiority claim.

## HTTP/3 interoperability smoke

The first external curl 8.21.0 probe received the full response body but reported a stream reset. Quinn tracing identified Relay stopping the request receive stream with code 0 when the upstream responded before the request body reached EOF. Relay now drains the retained receive stream under the configured body-size and idle-time limits before completing the response. The regression test covers an upstream early response while the client upload remains open. After the fix, curl negotiated HTTP/3, received the 1,024-byte body, and exited successfully on WSL2 ARM64; see the [raw smoke output](perf-results/2026-11-27-wsl-arm64-h3-curl-smoke.txt). `h2load --h3` still reports zero started requests, so no HTTP/3 throughput number is available. This smoke run does not satisfy native-host performance characterization.

## Repeatable local smoke comparison

On Linux with `nginx`, `wrk`, Python 3, and a release build of Relay installed, run:

```sh
cargo build --locked --release
DURATION=15s THREADS=4 CONCURRENCY=50 \
  bash scripts/load/run-linux-local.sh target/release/relay /usr/sbin/nginx \
  | tee arm64-wsl-smoke.txt
```

The harness sends the same fixed 1 KiB response directly from one local Python HTTP server and through Relay and NGINX reverse proxy configurations. It prints the kernel, CPU model, logical CPU count, memory, software versions, binary size, startup time, idle RSS, and `wrk` latency percentiles and request rates. During the Relay run, `pidstat` records CPU and RSS samples. The companion TLS harness measures TLS 1.3 new connections. The service logs and temporary configs are deleted at exit. Change `DURATION`, `THREADS`, and `CONCURRENCY` for repeated load levels. Run at least five repetitions after warm-up and preserve raw stdout separately.

This harness is a local smoke comparison only. The current available environment is Kali Linux ARM64 under WSL, so its timings are affected by virtualization, host scheduling, and the Windows-mounted checkout. No results from that environment should be treated as native-host release evidence. A three-repeat HTTP/1.1 and HTTP/2 comparison against Caddy, NGINX, and Apache is recorded in [BENCHMARKS.md](BENCHMARKS.md). NGINX's HTTP/2 result was inconsistent. Relay's external HTTP/3 response termination now succeeds with curl, but `h2load --h3` produces no successful requests, so no H3 throughput number is available. The broader physical-host performance gate remains open.

## Release measurements still required

Before making any performance claim, measure stripped binary size, idle and loaded RSS, startup time, throughput, CPU, p50/p95/p99 latency, TLS handshakes, connection scaling, and behavior at configured limits on documented physical Linux x86-64 and ARM64 hosts. Record exact versions, hardware, kernel, configuration, command lines, raw repeated results, and any host-dependent reproducibility differences. Do not present results from unlike workloads as directly comparable.
