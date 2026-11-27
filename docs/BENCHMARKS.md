# Competitor benchmark report

## Scope

This is a diagnostic WSL2 ARM64 comparison of Relay 0.1.0, Caddy 2.11.4, NGINX 1.30.4, and Apache httpd 2.4.68. It is not release performance evidence and does not support a general ranking. Raw `h2load` HTTP/1.1 and HTTP/2 output, exact versions, host/kernel information, and per-run results are in [the H1/H2 raw log](perf-results/2026-11-27-kali-wsl-arm64-competitor-comparison.txt). The curl HTTP/3 output is in the [H3 raw log](perf-results/2026-11-27-wsl-arm64-http3-curl-comparison.txt).

All servers forwarded to the same local Python HTTP/1.1 upstream, which returned a fixed 1 KiB body. The workload used 4 `h2load` threads and 50 in-flight requests. HTTP/1.1 used 50 connections; HTTP/2 used 5 connections with 10 streams each. Each measured run lasted 10 seconds after a 2 second warm-up, with three repetitions. The harness checked the negotiated protocol before each set of runs. It used the same fixture certificate for every server.

Environment: Kali Linux ARM64 (`aarch64`) under WSL2, Linux 6.18.33.1-microsoft-standard-WSL2, 12 logical CPUs, approximately 15.4 GiB guest memory. The WSL guest did not expose a CPU model. Relay ran with two workers. The checkout was on a Windows-mounted filesystem.

## Results

Median aggregate requests/second (`h2load` completed requests divided by measured duration) and median run p95 request latency:

| Server | HTTP/1.1 req/s | HTTP/1.1 p95 | HTTP/2 req/s | HTTP/2 p95 | Notes |
| --- | ---: | ---: | ---: | ---: | --- |
| Relay | 1,135.1 | 48.22 ms | 832.8 | 92.00 ms | All requests succeeded in each run. |
| Caddy | 1,114.7 | 48.16 ms | 1,111.2 | 48.19 ms | All requests succeeded in each run. |
| NGINX | 2,607.7 | 2.95 ms | — | — | HTTP/1.1 runs completed; HTTP/2 varied from 0.5 to 73.4 req/s with runs reporting very different completion counts. HTTP/2 comparison is inconclusive. |
| Apache | 1,117.8 | 48.15 ms | 1,106.7 | 48.21 ms | All requests succeeded in each run. |

For Relay, the per-run aggregate rates were 1,135.1, 1,138.1, and 1,133.8 req/s for HTTP/1.1 and 832.8, 809.3, and 837.3 req/s for HTTP/2. Caddy and Apache were similarly consistent across repetitions. NGINX's HTTP/2 repetitions were inconsistent, so no median is presented for that mode. The large HTTP/1.1 difference between NGINX and the other implementations also needs investigation under a controlled native-host setup before any performance conclusion.

## HTTP/3 status

The initial external curl 8.21.0 probe received a 1 KiB response body and then reported `HTTP/3 stream 0 reset by server (error 0x0 unknown)`. Quinn trace logging showed Relay sending `STOP_SENDING` with error code 0 before the response stream finished: when the upstream replied before the request body reached EOF, the body wrapper was dropped and Quinn stopped the receive side. Relay now retains that receive side and drains it to EOF, enforcing the configured body limit and idle timeout before completing the downstream response. A regression test covers an upstream response arriving while the client is still uploading; it failed before the fix with `RemoteTerminate { code: 0x0 }` and passes after it.

After the fix, curl 8.21.0 negotiated HTTP/3, received all 1,024 response bytes, and exited successfully. `h2load --h3` produced no successful requests in this environment, so the harness now uses curl's parallel HTTP/3 client for H3 measurements. Three runs of 500 measured requests per server, with 50 warm-up requests and 20 concurrent transfers, completed with all responses successful. Median results were:

| Server | H3 req/s | H3 p50 | H3 p95 | Successful responses |
| --- | ---: | ---: | ---: | ---: |
| Relay | 450.77 | 43.731 ms | 47.836 ms | 500/500 in each run |
| Caddy | 441.30 | 43.633 ms | 47.818 ms | 500/500 in each run |
| NGINX | 391.94 | 2.439 ms | 14.009 ms | 500/500 in each run |
| Apache | — | — | — | HTTP/3 unavailable in installed build |

These are diagnostic WSL2 ARM64 curl measurements on one workload and one host. They are not native-host release evidence or a performance ranking. Exact versions, per-run throughput and latency, and client output are in the [raw HTTP/3 results](perf-results/2026-11-27-wsl-arm64-http3-curl-comparison.txt); the separate [curl interoperability smoke log](perf-results/2026-11-27-wsl-arm64-h3-curl-smoke.txt) records the initial protocol check and h2load attempt.

## Limitations

- WSL2 virtualization, host scheduling, the Windows-mounted checkout, and the local Python threaded upstream affect results.
- TLS used one shared test certificate; these numbers do not measure certificate validation or production certificate costs.
- Only one workload shape, response size, concurrency level, host, and run date were measured.
- NGINX HTTP/2 results varied sharply; NGINX HTTP/1.1 also differed substantially from the other servers. NGINX HTTP/3 throughput ranged from 391.35 to 392.81 req/s across these short runs.
- These results do not satisfy the roadmap's physical-host release characterization gate.

## Reproduction

On Linux, install `python3`, `openssl`, `nginx` with its HTTP/2 and HTTP/3 modules, `apache2` with `mod_http2`, Caddy, curl with HTTP/3 support, and `h2load`. Build Relay and run:

```sh
cargo build --locked --release
PROTOCOLS="h1 h2" DURATION=10s WARM_UP=2s REPETITIONS=3 \
  RESULTS=docs/perf-results/competitor-comparison.txt \
  bash scripts/load/run-competitor-comparison.sh target/release/relay
```

The harness uses ports 18000 and 18443–18446 by default. Set `PORT_OFFSET` to avoid local port conflicts. Run it on an otherwise idle host and retain raw stdout. To reproduce the HTTP/3 comparison, use `PROTOCOLS=h3 H3_REQUESTS=500 H3_WARM_UP_REQUESTS=50 REPETITIONS=3 CLIENTS=20 PORT_OFFSET=1000`; Apache HTTP/3 is reported unavailable unless its installed build provides that protocol.
