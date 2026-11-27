# Competitor benchmark report

## Scope

This is a diagnostic WSL2 ARM64 comparison of Relay 0.1.0, Caddy 2.11.4, NGINX 1.30.4, and Apache httpd 2.4.68. It is not release performance evidence and does not support a general ranking. Raw `h2load` output, exact versions, host/kernel information, and per-run results are in [the raw log](perf-results/2026-11-27-kali-wsl-arm64-competitor-comparison.txt).

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

HTTP/3 was excluded from the throughput matrix. Relay's in-process HTTP/3 integration test passes for forwarding, streaming request bodies, body-size rejection, and UDP bind conflicts. During an external curl 8.21.0 smoke probe with a 1 KiB upstream response, curl received the response headers and full payload but then reported `HTTP/3 stream 0 reset by server (error 0x0 unknown)`. A separate `h2load --h3` attempt started and completed zero requests, so it produced no valid throughput result. The benchmark script therefore defaults to HTTP/1.1 and HTTP/2; opting into HTTP/3 fails closed if the client reports no successful requests. Treat external HTTP/3 response termination as an open interoperability issue; do not make an HTTP/3 performance or release-readiness claim until this is resolved and retested with an external client. Apache's installed build did not provide an HTTP/3 module.

## Limitations

- WSL2 virtualization, host scheduling, the Windows-mounted checkout, and the local Python threaded upstream affect results.
- TLS used one shared test certificate; these numbers do not measure certificate validation or production certificate costs.
- Only one workload shape, response size, concurrency level, host, and run date were measured.
- NGINX HTTP/2 results varied sharply; NGINX HTTP/1.1 also differed substantially from the other servers.
- These results do not satisfy the roadmap's physical-host release characterization gate.

## Reproduction

On Linux, install `python3`, `openssl`, `nginx` with its HTTP/2 module, `apache2` with `mod_http2`, Caddy, curl with HTTP/3 support, and `h2load` with HTTP/3 support. Build Relay and run:

```sh
cargo build --locked --release
PROTOCOLS="h1 h2" DURATION=10s WARM_UP=2s REPETITIONS=3 \
  RESULTS=docs/perf-results/competitor-comparison.txt \
  bash scripts/load/run-competitor-comparison.sh target/release/relay
```

The harness uses ports 18000 and 18443–18446. Run it on an otherwise idle host and retain raw stdout. For an HTTP/3 comparison, first resolve Relay's external-client stream reset, then add `h3` to `PROTOCOLS`; Apache HTTP/3 is reported unavailable unless its installed build provides that protocol.
