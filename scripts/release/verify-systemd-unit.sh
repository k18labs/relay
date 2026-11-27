#!/usr/bin/env bash
set -euo pipefail

command -v systemd-analyze >/dev/null || {
  echo "systemd-analyze is required to verify deploy/systemd/relay.service" >&2
  exit 1
}
test -x target/release/relay || {
  echo "build target/release/relay before verifying the systemd unit" >&2
  exit 1
}

stage="$(mktemp -d "${TMPDIR:-/tmp}/relay-systemd-verify.XXXXXX")"
trap 'rm -rf -- "$stage"' EXIT

install -D -m 0755 target/release/relay "$stage/usr/local/bin/relay"
install -D -m 0644 deploy/systemd/relay.service "$stage/etc/systemd/system/relay.service"
install -d -m 0755 "$stage/etc/relay" "$stage/usr/lib/systemd/system"

for unit in sysinit.target network-online.target network.target basic.target multi-user.target; do
  source_path="/usr/lib/systemd/system/$unit"
  if [[ ! -f "$source_path" ]]; then
    source_path="/lib/systemd/system/$unit"
  fi
  if [[ ! -f "$source_path" ]]; then
    echo "required systemd target is missing: $unit" >&2
    exit 1
  fi
  install -m 0644 "$source_path" "$stage/usr/lib/systemd/system/$unit"
done

systemd-analyze verify --root="$stage" /etc/systemd/system/relay.service
