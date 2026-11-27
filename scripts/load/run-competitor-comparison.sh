#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RELAY_BIN="$(realpath "${1:-$ROOT/target/release/relay}")"
DURATION="${DURATION:-10s}"
WARM_UP="${WARM_UP:-2s}"
REPETITIONS="${REPETITIONS:-3}"
THREADS="${THREADS:-4}"
CLIENTS="${CLIENTS:-50}"
STREAMS="${STREAMS:-10}"
H3_REQUESTS="${H3_REQUESTS:-1000}"
H3_WARM_UP_REQUESTS="${H3_WARM_UP_REQUESTS:-100}"
PORT_OFFSET="${PORT_OFFSET:-0}"
PROTOCOLS="${PROTOCOLS:-h1 h2}"
RESULTS="${RESULTS:-$ROOT/competitor-benchmark.txt}"

[[ "$DURATION" =~ ^([1-9]|[1-5][0-9]|60)s$ ]]
[[ "$WARM_UP" =~ ^([0-9]|[1-5][0-9]|60)s$ ]]
[[ "$REPETITIONS" =~ ^[1-5]$ ]]
[[ "$THREADS" =~ ^([1-9]|1[0-6])$ ]]
[[ "$CLIENTS" =~ ^([1-9][0-9]?|100)$ ]]
[[ "$STREAMS" =~ ^([1-9][0-9]?|100)$ ]]
[[ "$H3_REQUESTS" =~ ^([1-9][0-9]{0,3}|10000)$ ]]
[[ "$H3_WARM_UP_REQUESTS" =~ ^([0-9]{1,4}|10000)$ ]]
[[ "$PORT_OFFSET" =~ ^([0-9]{1,4}|[1-3][0-9]{4}|40000)$ ]]

UPSTREAM_PORT=$((18000 + PORT_OFFSET))
RELAY_PORT=$((18443 + PORT_OFFSET))
CADDY_PORT=$((18444 + PORT_OFFSET))
NGINX_PORT=$((18445 + PORT_OFFSET))
APACHE_PORT=$((18446 + PORT_OFFSET))

for command in python3 openssl nginx apache2 caddy h2load curl; do
  command -v "$command" >/dev/null || { echo "missing required benchmark tool: $command" >&2; exit 1; }
done
[[ -x "$RELAY_BIN" ]] || { echo "Relay binary not found: $RELAY_BIN" >&2; exit 1; }

TMP="$(mktemp -d "${TMPDIR:-/tmp}/relay-competitor-bench.XXXXXXXX")"
PIDS=()
cleanup() {
  local status=$?
  for pid in "${PIDS[@]}"; do kill "$pid" 2>/dev/null || true; done
  for pid in "${PIDS[@]}"; do wait "$pid" 2>/dev/null || true; done
  if [[ -s "$TMP/nginx.pid" ]]; then nginx -s stop -c "$TMP/nginx.conf" -p "$TMP" >/dev/null 2>&1 || true; fi
  if (( status == 0 )); then rm -rf -- "$TMP"; else echo "benchmark_debug_logs=$TMP" >&2; fi
}
trap cleanup EXIT INT TERM

mkdir -p "$(dirname "$RESULTS")" "$TMP/www"
: >"$RESULTS"
CERT="$ROOT/tests/fixtures/tls/localhost-cert.pem"
KEY="$ROOT/tests/fixtures/tls/localhost-key.pem"
CA="$ROOT/tests/fixtures/tls/test-ca.pem"
export SSL_CERT_FILE="$CA"

cat >"$TMP/upstream.py" <<PY
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

PAYLOAD = b"x" * 1024

class Handler(BaseHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def do_GET(self):
        self.send_response(200)
        self.send_header("Content-Length", str(len(PAYLOAD)))
        self.send_header("Content-Type", "application/octet-stream")
        self.end_headers()
        self.wfile.write(PAYLOAD)
    def log_message(self, *_args):
        pass

ThreadingHTTPServer(("127.0.0.1", $UPSTREAM_PORT), Handler).serve_forever()
PY

cat >"$TMP/relay.toml" <<EOF
[server]
listen = "127.0.0.1:$RELAY_PORT"
max_connections = 512
workers = 2

[tls]
certificate = "$CERT"
private_key = "$KEY"

[[routes]]
host = "localhost"
path = "/"
upstream = "http://127.0.0.1:$UPSTREAM_PORT"
EOF

cat >"$TMP/Caddyfile" <<EOF
{
	admin off
}
https://localhost:$CADDY_PORT {
	tls $CERT $KEY
	reverse_proxy 127.0.0.1:$UPSTREAM_PORT
}
EOF

cat >"$TMP/nginx.conf" <<EOF
worker_processes 2;
pid $TMP/nginx.pid;
error_log stderr warn;
events { worker_connections 2048; }
http {
    client_body_temp_path $TMP/client_body;
    fastcgi_temp_path $TMP/fastcgi_temp;
    proxy_temp_path $TMP/proxy_temp;
    scgi_temp_path $TMP/scgi_temp;
    uwsgi_temp_path $TMP/uwsgi_temp;
    access_log off;
    server {
        listen 127.0.0.1:$NGINX_PORT ssl;
        listen 127.0.0.1:$NGINX_PORT quic reuseport;
        http2 on;
        http3 on;
        ssl_certificate $CERT;
        ssl_certificate_key $KEY;
        ssl_protocols TLSv1.2 TLSv1.3;
        add_header Alt-Svc 'h3=":$NGINX_PORT"; ma=86400' always;
        location / {
            proxy_pass http://127.0.0.1:$UPSTREAM_PORT;
            proxy_set_header Host localhost;
        }
    }
}
EOF

cat >"$TMP/apache.conf" <<EOF
ServerRoot /etc/apache2
PidFile $TMP/apache.pid
Listen 127.0.0.1:$APACHE_PORT
LoadModule mpm_event_module /usr/lib/apache2/modules/mod_mpm_event.so
LoadModule authz_core_module /usr/lib/apache2/modules/mod_authz_core.so
LoadModule authz_host_module /usr/lib/apache2/modules/mod_authz_host.so
LoadModule socache_shmcb_module /usr/lib/apache2/modules/mod_socache_shmcb.so
LoadModule ssl_module /usr/lib/apache2/modules/mod_ssl.so
LoadModule http2_module /usr/lib/apache2/modules/mod_http2.so
LoadModule proxy_module /usr/lib/apache2/modules/mod_proxy.so
LoadModule proxy_http_module /usr/lib/apache2/modules/mod_proxy_http.so
ServerName localhost
ErrorLog $TMP/apache-error.log
LogLevel warn
Protocols h2 http/1.1
<VirtualHost 127.0.0.1:$APACHE_PORT>
    ServerName localhost
    SSLEngine on
    SSLCertificateFile $CERT
    SSLCertificateKeyFile $KEY
    SSLProtocol TLSv1.2 TLSv1.3
    ProxyPreserveHost On
    ProxyPass / http://127.0.0.1:$UPSTREAM_PORT/
    ProxyPassReverse / http://127.0.0.1:$UPSTREAM_PORT/
    <Location />
        Require all granted
    </Location>
</VirtualHost>
EOF

CPU="$(awk -F: '$1 ~ /model name|Hardware/ {sub(/^[^:]*:[[:space:]]*/, ""); print; exit}' /proc/cpuinfo)"
CPU="${CPU:-not reported by WSL2 guest}"
{
  echo "Relay competitor benchmark"
  echo "date_utc=$(date -u +%Y-%m-%dT%H:%M:%SZ)"
  echo "kernel=$(uname -srvmo)"
  echo "cpu=$CPU"
  echo "logical_cpus=$(getconf _NPROCESSORS_ONLN)"
  echo "memory=$(awk '/MemTotal/ {print $2 " kB"}' /proc/meminfo)"
  echo "protocols=$PROTOCOLS h1_h2_duration=$DURATION h1_h2_warm_up=$WARM_UP repetitions=$REPETITIONS h1_h2_threads=$THREADS h1_h2_clients=$CLIENTS h2_streams_per_connection=$STREAMS h3_requests=$H3_REQUESTS h3_warm_up_requests=$H3_WARM_UP_REQUESTS h3_max_parallel=$CLIENTS port_offset=$PORT_OFFSET"
  echo "relay=$($RELAY_BIN version)"
  echo "nginx=$(nginx -v 2>&1)"
  echo "caddy=$(caddy version)"
  echo "apache=$(apache2 -v | head -1)"
  echo "h2load=$(h2load --version)"
  echo "curl=$(curl --version | head -1)"
  echo ""
} | tee -a "$RESULTS"

python3 -u "$TMP/upstream.py" >"$TMP/upstream.log" 2>&1 & PIDS+=("$!")
"$RELAY_BIN" run --config "$TMP/relay.toml" >"$TMP/relay.log" 2>&1 & PIDS+=("$!")
caddy run --config "$TMP/Caddyfile" --adapter caddyfile >"$TMP/caddy.log" 2>&1 & PIDS+=("$!")
nginx -c "$TMP/nginx.conf" -p "$TMP" -g 'daemon off;' >"$TMP/nginx.log" 2>&1 & PIDS+=("$!")
apache2 -f "$TMP/apache.conf" -DFOREGROUND >"$TMP/apache.log" 2>&1 & PIDS+=("$!")

wait_http1() {
  local port="$1" attempt
  for attempt in $(seq 1 100); do
    if curl --silent --show-error --fail --insecure --http1.1 "https://localhost:$port/bench" --output /dev/null 2>/dev/null; then return 0; fi
    sleep 0.1
  done
  echo "server did not become ready on TCP port $port" >&2
  for log in "$TMP/relay.log" "$TMP/caddy.log" "$TMP/nginx.log" "$TMP/apache.log"; do
    if [[ -s "$log" ]]; then echo "--- $log" >&2; tail -40 "$log" >&2; fi
  done
  return 1
}

for port in "$RELAY_PORT" "$CADDY_PORT" "$NGINX_PORT" "$APACHE_PORT"; do wait_http1 "$port"; done

check_protocol() {
  local server="$1" mode="$2" port="$3" actual expected output curl_status
  case "$mode" in
    h1) actual="$(curl --silent --show-error --insecure --http1.1 --output /dev/null --write-out '%{http_version}' "https://localhost:$port/bench")"; expected=1.1 ;;
    h2) actual="$(curl --silent --show-error --insecure --http2 --output /dev/null --write-out '%{http_version}' "https://localhost:$port/bench")"; expected=2 ;;
    h3)
      if output="$(curl --silent --show-error --insecure --http3-only --output /dev/null --write-out '%{http_version}:%{size_download}' "https://localhost:$port/bench")"; then curl_status=0; else curl_status=$?; fi
      actual="${output%%:*}"
      if [[ "$actual" == 3 && "$output" == *:1024 && "$curl_status" == 18 ]]; then
        printf 'protocol_check server=%s mode=h3 version=3 payload_bytes=1024 warning=curl_stream_reset_after_full_payload exit=%s\n' "$server" "$curl_status" | tee -a "$RESULTS"
        return 0
      fi
      expected=3
      ;;
  esac
  if [[ "$actual" != "$expected" ]]; then
    printf 'protocol_check server=%s mode=%s expected=%s observed=%s\n' "$server" "$mode" "$expected" "$actual" | tee -a "$RESULTS"
    return 1
  fi
  printf 'protocol_check server=%s mode=%s version=%s payload_bytes=1024\n' "$server" "$mode" "$actual" | tee -a "$RESULTS"
}

benchmark_mode() {
  local server="$1" mode="$2" port="$3" connections="$CLIENTS" streams=1 repetition output
  if [[ "$mode" != h1 ]]; then
    connections=$(( (CLIENTS + STREAMS - 1) / STREAMS ))
    streams="$STREAMS"
  fi
  check_protocol "$server" "$mode" "$port"
  for repetition in $(seq 1 "$REPETITIONS"); do
    {
      echo "===== server=$server protocol=$mode repetition=$repetition of $REPETITIONS ====="
      if [[ "$mode" == h3 ]]; then
        echo "curl --parallel --parallel-max $CLIENTS --http3-only requests=$H3_REQUESTS"
      else
        echo "h2load -D $DURATION --warm-up-time $WARM_UP -c $connections -t $THREADS -m $streams"
      fi
    } | tee -a "$RESULTS"
    case "$mode" in
      h1) h2load --h1 -D "$DURATION" --warm-up-time "$WARM_UP" -c "$connections" -t "$THREADS" -m "$streams" "https://localhost:$port/bench" >"$TMP/h2load.out" 2>&1 ;;
      h2) h2load --alpn-list=h2 -D "$DURATION" --warm-up-time "$WARM_UP" -c "$connections" -t "$THREADS" -m "$streams" "https://localhost:$port/bench" >"$TMP/h2load.out" 2>&1 ;;
      h3) benchmark_http3_curl "$server" "$port" "$repetition" || return 1; continue ;;
    esac || { cat "$TMP/h2load.out" | tee -a "$RESULTS"; return 1; }
    cat "$TMP/h2load.out" >>"$RESULTS"
    if ! grep -Eq 'requests: [0-9]+ total, [0-9]+ started, [0-9]+ done, [1-9][0-9]* succeeded, 0 failed, 0 errored, 0 timeout' "$TMP/h2load.out"; then
      echo "benchmark_error server=$server protocol=$mode repetition=$repetition reason=no_successful_requests_or_client_errors" | tee -a "$RESULTS" >&2
      return 1
    fi
  done
}

curl_h3_transfers() {
  local port="$1" count="$2" output_file="$3" index
  local -a args=(--parallel --parallel-immediate --parallel-max "$CLIENTS")
  for ((index = 1; index <= count; index++)); do
    if ((index > 1)); then args+=(--next); fi
    args+=(--http3-only --insecure --silent --show-error --output /dev/null
      --write-out '%{http_code},%{time_total},%{size_download}\n'
      --url "https://localhost:$port/bench")
  done
  local status=0
  curl "${args[@]}" >"$output_file" || status=$?
  return "$status"
}

benchmark_http3_curl() {
  local server="$1" port="$2" repetition="$3" start_ns end_ns elapsed curl_status=0
  local warmup_file="$TMP/h3-warmup.csv" results_file="$TMP/h3-results.csv"
  if ((H3_WARM_UP_REQUESTS > 0)); then
    curl_h3_transfers "$port" "$H3_WARM_UP_REQUESTS" "$warmup_file" || {
      echo "benchmark_error server=$server protocol=h3 reason=warmup_failed" | tee -a "$RESULTS" >&2
      return 1
    }
  fi
  echo "h3_curl requests=$H3_REQUESTS max_parallel=$CLIENTS warm_up_requests=$H3_WARM_UP_REQUESTS" | tee -a "$RESULTS"
  start_ns="$(date +%s%N)"
  curl_h3_transfers "$port" "$H3_REQUESTS" "$results_file" || curl_status=$?
  end_ns="$(date +%s%N)"
  elapsed="$(awk -v start="$start_ns" -v end="$end_ns" 'BEGIN { printf "%.9f", (end - start) / 1000000000 }')"
  python3 - "$server" "$repetition" "$H3_REQUESTS" "$elapsed" "$curl_status" "$results_file" <<'PY' | tee -a "$RESULTS"
import csv
import math
import sys

server, repetition, expected, elapsed, curl_status, result_path = sys.argv[1:]
expected = int(expected)
elapsed = float(elapsed)
curl_status = int(curl_status)
rows = []
with open(result_path, newline="", encoding="utf-8") as results:
    for row in csv.reader(results):
        if len(row) == 3:
            try:
                rows.append((int(row[0]), float(row[1]), int(row[2])))
            except ValueError:
                pass
successes = [row for row in rows if row[0] == 200 and row[2] == 1024]
latencies = sorted(row[1] for row in successes)
def percentile(fraction):
    if not latencies:
        return 0.0
    return latencies[max(0, math.ceil(fraction * len(latencies)) - 1)]
print(
    f"h3_curl_result server={server} repetition={repetition} expected={expected} "
    f"completed={len(rows)} succeeded={len(successes)} curl_exit={curl_status} "
    f"wall_seconds={elapsed} req_per_sec={len(successes) / elapsed:.2f} "
    f"latency_p50_ms={percentile(0.50) * 1000:.3f} "
    f"latency_p95_ms={percentile(0.95) * 1000:.3f}"
)
if curl_status != 0 or len(rows) != expected or len(successes) != expected:
    raise SystemExit(1)
PY
  cat "$results_file" >>"$RESULTS"
}

for entry in "relay:$RELAY_PORT" "caddy:$CADDY_PORT" "nginx:$NGINX_PORT" "apache:$APACHE_PORT"; do
  server="${entry%%:*}"
  port="${entry##*:}"
  for protocol in $PROTOCOLS; do
    if [[ "$protocol" == h3 && "$server" == apache ]]; then
      echo "protocol_unavailable server=apache mode=h3 reason=Apache httpd 2.4 build has no HTTP/3 module" | tee -a "$RESULTS"
    else
      benchmark_mode "$server" "$protocol" "$port"
    fi
  done
done

echo "benchmark_results=$RESULTS"
