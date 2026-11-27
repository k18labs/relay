#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 2 ]]; then
  echo "usage: $0 RELAY_BINARY NGINX_BINARY" >&2
  exit 2
fi
relay_bin=$(realpath "$1")
nginx_bin=$(realpath "$2")
command -v wrk >/dev/null
command -v python3 >/dev/null

duration=${DURATION:-15s}
concurrency=${CONCURRENCY:-50}
threads=${THREADS:-4}
tmp=$(mktemp -d)
relay_pid=
nginx_pid=
upstream_pid=
cleanup() {
  [[ -z "$relay_pid" ]] || kill "$relay_pid" 2>/dev/null || true
  [[ -z "$nginx_pid" ]] || kill "$nginx_pid" 2>/dev/null || true
  [[ -z "$upstream_pid" ]] || kill "$upstream_pid" 2>/dev/null || true
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
RUST_LOG=${RUST_LOG:-warn} "$relay_bin" run --config "$tmp/relay.toml" >"$tmp/logs/relay.log" 2>&1 &
relay_pid=$!

wait_for() {
  local service=$1 url=$2 pid=$3
  for _ in $(seq 1 100); do
    if curl --silent --fail -H 'Host: example.org' "$url" >/dev/null; then return 0; fi
    if ! kill -0 "$pid" 2>/dev/null; then break; fi
    sleep 0.1
  done
  echo "$service did not become ready; log follows:" >&2
  cat "$tmp/logs/$service.log" >&2
  return 1
}
wait_for upstream http://127.0.0.1:18081/bench.txt "$upstream_pid"
wait_for relay http://127.0.0.1:18082/bench.txt "$relay_pid"
wait_for nginx http://127.0.0.1:18083/bench.txt "$nginx_pid"

echo "host=$(uname -a)"
echo "cpu=$(lscpu | awk -F: '/Model name/ {sub(/^ +/, "", $2); print $2; exit}')"
echo "logical_cpus=$(nproc)"
echo "memory_kib=$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
echo "relay=$($relay_bin version)"
echo "nginx=$($nginx_bin -v 2>&1)"
echo "wrk=$(wrk -v 2>&1 | head -1)"
echo "python=$(python3 --version 2>&1)"
rustc_bin=$(command -v rustc || true)
if [[ -z "$rustc_bin" ]] && command -v rustup >/dev/null; then
  rustc_bin=$(rustup which rustc --toolchain 1.96.0 2>/dev/null || true)
fi
echo "rustc=$([[ -z "$rustc_bin" ]] && echo unavailable || "$rustc_bin" --version)"
echo "duration=$duration threads=$threads concurrency=$concurrency"
echo "upstream=Python ThreadingHTTPServer HTTP/1.1; fixed 1 KiB response; loopback only"
echo "Upstream sanity run:"
wrk -t "$threads" -c "$concurrency" -d "$duration" --latency -H 'Host: example.org' http://127.0.0.1:18081/bench.txt
echo "Relay reverse-proxy run:"
wrk -t "$threads" -c "$concurrency" -d "$duration" --latency -H 'Host: example.org' http://127.0.0.1:18082/bench.txt
echo "NGINX reverse-proxy run:"
wrk -t "$threads" -c "$concurrency" -d "$duration" --latency http://127.0.0.1:18083/bench.txt
echo "Redirect stdout to preserve raw wrk output. This is a smoke run, not release performance evidence."
