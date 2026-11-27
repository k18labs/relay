#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 RELAY_BINARY" >&2
  exit 2
fi

relay_bin=$(realpath "$1")
command -v openssl >/dev/null
duration=${TLS_DURATION:-15}
if [[ ! "$duration" =~ ^[1-9][0-9]?$ ]]; then
  echo "TLS_DURATION must be a whole number of seconds from 1 through 99." >&2
  exit 2
fi

tmp=$(mktemp -d)
relay_pid=
cleanup() {
  [[ -z "$relay_pid" ]] || kill "$relay_pid" 2>/dev/null || true
  wait 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT

openssl req -x509 -newkey rsa:2048 -nodes -sha256 -days 1 \
  -subj "/CN=localhost" -addext "subjectAltName=DNS:localhost" \
  -keyout "$tmp/key.pem" -out "$tmp/cert.pem" >/dev/null 2>&1

cat >"$tmp/relay.toml" <<EOF
[server]
listen = "127.0.0.1:18084"
max_connections = 256
tls = true

[tls]
certificate = "$tmp/cert.pem"
private_key = "$tmp/key.pem"

[[routes]]
host = "localhost"
path = "/"
upstream = "http://127.0.0.1:18085"
EOF

relay_started_at_ns=$(date +%s%N)
RUST_LOG=error "$relay_bin" run --config "$tmp/relay.toml" >"$tmp/relay.log" 2>&1 &
relay_pid=$!

ready=0
for _ in $(seq 1 100); do
  if openssl s_client -connect 127.0.0.1:18084 -servername localhost -tls1_3 -brief \
    </dev/null >"$tmp/health.log" 2>&1; then
    ready=1
    break
  fi
  if ! kill -0 "$relay_pid" 2>/dev/null; then break; fi
  sleep 0.01
done
if [[ "$ready" != 1 ]]; then
  echo "Relay TLS listener did not complete a TLS 1.3 handshake." >&2
  cat "$tmp/relay.log" "$tmp/health.log" >&2
  exit 1
fi
relay_ready_at_ns=$(date +%s%N)
relay_startup_ms=$(((relay_ready_at_ns - relay_started_at_ns) / 1000000))
relay_idle_rss_kib=$(awk '/^VmRSS:/ {print $2}' "/proc/$relay_pid/status")

echo "host=$(uname -a)"
echo "cpu=$(lscpu | awk -F: '/Model name/ {sub(/^ +/, "", $2); print $2; exit}')"
echo "logical_cpus=$(nproc)"
echo "memory_kib=$(awk '/MemTotal/ {print $2}' /proc/meminfo)"
echo "relay=$($relay_bin version)"
echo "relay_binary_bytes=$(stat --printf='%s' "$relay_bin")"
echo "relay_tls_startup_to_handshake_ms=$relay_startup_ms"
echo "relay_tls_idle_rss_kib=$relay_idle_rss_kib"
echo "openssl=$(openssl version)"
echo "tls_version=TLS1.3"
echo "duration_seconds=$duration repetitions=3 mode=new-connections"
for repetition in 1 2 3; do
  echo "===== TLS 1.3 new-connection repetition $repetition of 3 ====="
  openssl s_time -new -connect 127.0.0.1:18084 -tls1_3 -time "$duration" 2>&1
done
