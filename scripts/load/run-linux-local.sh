#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 RELAY_BINARY NGINX_BINARY" >&2
  exit 2
fi
relay_bin=$(realpath "$1")
nginx_bin=$(realpath "$2")
command -v wrk >/dev/null
command -v pidstat >/dev/null
command -v python3 >/dev/null

duration=${DURATION:-15s}
concurrency=${CONCURRENCY:-50}
threads=${THREADS:-4}
tmp=$(mktemp -d)
relay_pid=
nginx_pid=
upstream_pid=
load_pid=
shutdown_watchdog=
monitor_pid=
cleanup() {
  [[ -z "$relay_pid" ]] || kill "$relay_pid" 2>/dev/null || true
  [[ -z "$nginx_pid" ]] || kill "$nginx_pid" 2>/dev/null || true
  [[ -z "$upstream_pid" ]] || kill "$upstream_pid" 2>/dev/null || true
  [[ -z "$load_pid" ]] || kill "$load_pid" 2>/dev/null || true
  [[ -z "$shutdown_watchdog" ]] || kill "$shutdown_watchdog" 2>/dev/null || true
  [[ -z "$monitor_pid" ]] || kill "$monitor_pid" 2>/dev/null || true
  wait 2>/dev/null || true
  if [[ "${KEEP_TMP:-0}" == 1 ]]; then
    echo "retained benchmark files: $tmp" >&2
  else
    rm -rf "$tmp"
  fi
}
trap cleanup EXIT

mkdir -p "$tmp/www" "$tmp/logs" "$tmp/client_body" "$tmp/proxy_temp" \
  "$tmp/fastcgi_temp" "$tmp/uwsgi_temp" "$tmp/scgi_temp"
python3 -c 'import pathlib,sys; pathlib.Path(sys.argv[1]).write_bytes(b"relay-load-test\n" * 64)' "$tmp/www/bench.txt"
cat >"$tmp/server.py" <<'PY'
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer
import sys

class Handler(SimpleHTTPRequestHandler):
    protocol_version = "HTTP/1.1"
    def log_message(self, *_args):
        pass

class Server(ThreadingHTTPServer):
    request_queue_size = 1024
    daemon_threads = True

Server(("127.0.0.1", 18081), lambda *args, **kwargs: Handler(*args, directory=sys.argv[1], **kwargs)).serve_forever()
PY

cat >"$tmp/relay.toml" <<EOF
[server]
listen = "127.0.0.1:18082"
max_connections = 256
tls = false

[[routes]]
host = "example.org"
path = "/"
upstream = "http://127.0.0.1:18081"

[limits]
max_concurrent_requests = 1024
EOF

cat >"$tmp/nginx.conf" <<EOF
worker_processes 1;
pid $tmp/nginx.pid;
error_log stderr warn;
events { worker_connections 512; }
http {
  access_log off;
  client_body_temp_path $tmp/client_body;
  proxy_temp_path $tmp/proxy_temp;
  fastcgi_temp_path $tmp/fastcgi_temp;
  uwsgi_temp_path $tmp/uwsgi_temp;
  scgi_temp_path $tmp/scgi_temp;
  server {
    listen 127.0.0.1:18083;
    location / { proxy_http_version 1.1; proxy_set_header Connection ""; proxy_pass http://127.0.0.1:18081; }
  }
}
EOF

python3 "$tmp/server.py" "$tmp/www" >"$tmp/logs/upstream.log" 2>&1 &
upstream_pid=$!
"$nginx_bin" -p "$tmp/" -c nginx.conf -g 'daemon off;' >"$tmp/logs/nginx.log" 2>&1 &
nginx_pid=$!
relay_started_at_ns=$(date +%s%N)
RUST_LOG=${RUST_LOG:-warn} "$relay_bin" run --config "$tmp/relay.toml" >"$tmp/logs/relay.log" 2>&1 &
relay_pid=$!

wait_for() {
  local service=$1 url=$2 pid=$3
  for _ in $(seq 1 100); do
    if curl --silent --fail -H 'Host: example.org' "$url" >/dev/null; then return 0; fi
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 0.01
  done
  echo "$service did not become ready; log follows:" >&2
  cat "$tmp/logs/$service.log" >&2
  return 1
}
wait_for upstream http://127.0.0.1:18081/bench.txt "$upstream_pid"
wait_for relay http://127.0.0.1:18082/bench.txt "$relay_pid"
wait_for nginx http://127.0.0.1:18083/bench.txt "$nginx_pid"
relay_ready_at_ns=$(date +%s%N)
relay_startup_to_http_ready_ms=$(((relay_ready_at_ns - relay_started_at_ns) / 1000000))
relay_binary_bytes=$(stat --printf='%s' "$relay_bin")
relay_idle_rss_kib=$(awk '/^VmRSS:/ {print $2}' "/proc/$relay_pid/status")

rustc_bin=$(command -v rustc || true)
if [[ -z "$rustc_bin" ]] && command -v rustup >/dev/null; then
  rustc_bin=$(rustup which rustc --toolchain 1.96.0 2>/dev/null || true)
fi

if [[ "${SHUTDOWN_STRESS:-0}" == 1 ]]; then
  load_duration=${SHUTDOWN_LOAD_DURATION:-30s}
  echo "host=$(uname -a)"
  echo "cpu=$(lscpu | awk -F: '/Model name/ {sub(/^ +/, "", $2); print $2; exit}')"
  echo "logical_cpus=$(nproc)"
  echo "memory_kib=$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
  echo "relay=$($relay_bin version)"
  echo "relay_binary_bytes=$relay_binary_bytes"
  echo "relay_startup_to_http_ready_ms=$relay_startup_to_http_ready_ms"
  echo "relay_idle_rss_kib=$relay_idle_rss_kib"
  echo "wrk=$(wrk -v 2>&1 | head -1)"
  echo "python=$(python3 --version 2>&1)"
  if [[ -z "$rustc_bin" ]]; then
    echo "rustc=unavailable"
  else
    echo "rustc=$("$rustc_bin" --version)"
  fi
  echo "relay_limits=max_connections=256 max_concurrent_requests=1024"
  echo "shutdown_load_duration=$load_duration threads=$threads concurrency=$concurrency"
  echo "Starting $threads-thread, $concurrency-client load for graceful-shutdown check ($load_duration)."
  wrk -t "$threads" -c "$concurrency" -d "$load_duration" --latency \
    -H 'Host: example.org' http://127.0.0.1:18082/bench.txt \
    >"$tmp/logs/shutdown-wrk.log" 2>&1 &
  load_pid=$!
  sleep 2
  (
    sleep 7
    if kill -0 "$relay_pid" 2>/dev/null; then
      : >"$tmp/shutdown-timeout"
      kill -KILL "$relay_pid" 2>/dev/null || true
    fi
  ) &
  shutdown_watchdog=$!
  kill -TERM "$relay_pid"
  set +e
  wait "$relay_pid"
  relay_status=$?
  set -e
  relay_pid=
  kill "$shutdown_watchdog" 2>/dev/null || true
  wait "$shutdown_watchdog" 2>/dev/null || true
  if [[ -e "$tmp/shutdown-timeout" ]]; then
    echo "Relay did not exit within 7 seconds during active load." >&2
    cat "$tmp/logs/relay.log" >&2
    exit 1
  fi
  if [[ "$relay_status" -ne 0 ]]; then
    echo "Relay exited with status $relay_status during graceful shutdown." >&2
    cat "$tmp/logs/relay.log" >&2
    exit 1
  fi
  kill -INT "$load_pid" 2>/dev/null || true
  wait "$load_pid" 2>/dev/null || true
  load_pid=
  echo "graceful_shutdown_under_load=passed"
  cat "$tmp/logs/shutdown-wrk.log"
  exit 0
fi

echo "host=$(uname -a)"
echo "cpu=$(lscpu | awk -F: '/Model name/ {sub(/^ +/, "", $2); print $2; exit}')"
echo "logical_cpus=$(nproc)"
echo "memory_kib=$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
echo "relay=$($relay_bin version)"
echo "relay_binary_bytes=$relay_binary_bytes"
echo "relay_startup_to_http_ready_ms=$relay_startup_to_http_ready_ms"
echo "relay_idle_rss_kib=$relay_idle_rss_kib"
echo "nginx=$($nginx_bin -v 2>&1)"
echo "wrk=$(wrk -v 2>&1 | head -1)"
echo "python=$(python3 --version 2>&1)"
echo "rustc=$([[ -z "$rustc_bin" ]] && echo unavailable || "$rustc_bin" --version)"
echo "duration=$duration threads=$threads concurrency=$concurrency"
echo "relay_limits=max_connections=256 max_concurrent_requests=1024"
echo "upstream=Python ThreadingHTTPServer HTTP/1.1; fixed 1 KiB response; loopback only"
echo "Upstream sanity run:"
wrk -t "$threads" -c "$concurrency" -d "$duration" --latency -H 'Host: example.org' http://127.0.0.1:18081/bench.txt
echo "Relay reverse-proxy run:"
duration_seconds=${duration%s}
if [[ ! "$duration_seconds" =~ ^[1-9][0-9]*$ ]]; then
  echo "pidstat sampling requires a duration specified in whole seconds, such as 30s." >&2
  exit 2
fi
pidstat -h -u -r -p "$relay_pid" 1 "$duration_seconds" >"$tmp/logs/relay-pidstat.log" 2>&1 &
monitor_pid=$!
wrk -t "$threads" -c "$concurrency" -d "$duration" --latency \
  -H 'Host: example.org' http://127.0.0.1:18082/bench.txt \
  >"$tmp/logs/relay-wrk.log" 2>&1 &
load_pid=$!
set +e
wait "$load_pid"
wrk_status=$?
wait "$monitor_pid"
monitor_status=$?
set -e
load_pid=
monitor_pid=
cat "$tmp/logs/relay-wrk.log"
echo "Relay CPU and RSS samples from pidstat:"
cat "$tmp/logs/relay-pidstat.log"
if [[ "$monitor_status" -ne 0 ]]; then
  echo "pidstat failed with status $monitor_status." >&2
  exit "$monitor_status"
fi
if [[ "$wrk_status" -ne 0 ]]; then
  exit "$wrk_status"
fi
echo "NGINX reverse-proxy run:"
wrk -t "$threads" -c "$concurrency" -d "$duration" --latency http://127.0.0.1:18083/bench.txt
echo "Redirect stdout to preserve raw wrk output. This is a smoke run, not release performance evidence."
