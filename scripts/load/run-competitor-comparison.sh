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
PROTOCOLS="${PROTOCOLS:-h1 h2 h3}"
RESULTS="${RESULTS:-$ROOT/competitor-benchmark.txt}"

[[ "$DURATION" =~ ^([1-9]|[1-5][0-9]|60)s$ ]]
[[ "$WARM_UP" =~ ^([0-9]|[1-5][0-9]|60)s$ ]]
[[ "$REPETITIONS" =~ ^[1-5]$ ]]
[[ "$THREADS" =~ ^([1-9]|1[0-6])$ ]]
[[ "$CLIENTS" =~ ^([1-9][0-9]?|100)$ ]]
[[ "$STREAMS" =~ ^([1-9][0-9]?|100)$ ]]

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

cat >"$TMP/upstream.py" <<'PY'
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

ThreadingHTTPServer(("127.0.0.1", 18000), Handler).serve_forever()
PY

cat >"$TMP/relay.toml" <<EOF
[server]
listen = "127.0.0.1:18443"
max_connections = 512
workers = 2

[tls]
certificate = "$CERT"
private_key = "$KEY"

[[routes]]
host = "localhost"
path = "/"
upstream = "http://127.0.0.1:18000"
EOF

cat >"$TMP/Caddyfile" <<EOF
{
	admin off
}
https://localhost:18444 {
	tls $CERT $KEY
	reverse_proxy 127.0.0.1:18000
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
        listen 127.0.0.1:18445 ssl;
        listen 127.0.0.1:18445 quic reuseport;
        http2 on;
        http3 on;
        ssl_certificate $CERT;
        ssl_certificate_key $KEY;
        ssl_protocols TLSv1.2 TLSv1.3;
        add_header Alt-Svc 'h3=":18445"; ma=86400' always;
        location / {
            proxy_pass http://127.0.0.1:18000;
            proxy_set_header Host localhost;
        }
    }
}
EOF

cat >"$TMP/apache.conf" <<EOF
ServerRoot /etc/apache2
PidFile $TMP/apache.pid
Listen 127.0.0.1:18446
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
<VirtualHost 127.0.0.1:18446>
    ServerName localhost
    SSLEngine on
    SSLCertificateFile $CERT
    SSLCertificateKeyFile $KEY
    SSLProtocol TLSv1.2 TLSv1.3
    ProxyPreserveHost On
    ProxyPass / http://127.0.0.1:18000/
    ProxyPassReverse / http://127.0.0.1:18000/
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
  echo "duration=$DURATION warm_up=$WARM_UP repetitions=$REPETITIONS threads=$THREADS total_inflight=$CLIENTS h2_h3_connections=$(( (CLIENTS + STREAMS - 1) / STREAMS )) streams_per_connection=$STREAMS"
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

for port in 18443 18444 18445 18446; do wait_http1 "$port"; done

check_protocol() {
  local server="$1" mode="$2" port="$3" actual expected
  case "$mode" in
    h1) actual="$(curl --silent --show-error --insecure --http1.1 --output /dev/null --write-out '%{http_version}' "https://localhost:$port/bench")"; expected=1.1 ;;
    h2) actual="$(curl --silent --show-error --insecure --http2 --output /dev/null --write-out '%{http_version}' "https://localhost:$port/bench")"; expected=2 ;;
    h3) actual="$(curl --silent --show-error --insecure --http3-only --output /dev/null --write-out '%{http_version}' "https://localhost:$port/bench")"; expected=3 ;;
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
      echo "h2load -D $DURATION --warm-up-time $WARM_UP -c $connections -t $THREADS -m $streams"
    } | tee -a "$RESULTS"
    case "$mode" in
      h1) h2load --h1 -D "$DURATION" --warm-up-time "$WARM_UP" -c "$connections" -t "$THREADS" -m "$streams" "https://localhost:$port/bench" ;;
      h2) h2load --alpn-list=h2 -D "$DURATION" --warm-up-time "$WARM_UP" -c "$connections" -t "$THREADS" -m "$streams" "https://localhost:$port/bench" ;;
      h3) h2load --h3 -D "$DURATION" --warm-up-time "$WARM_UP" -c "$connections" -t "$THREADS" -m "$streams" "https://localhost:$port/bench" ;;
    esac >>"$RESULTS" 2>&1
  done
}

for entry in relay:18443 caddy:18444 nginx:18445 apache:18446; do
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
