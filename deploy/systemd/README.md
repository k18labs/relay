# Linux systemd deployment example

This is a starting point for a single Relay instance on a systemd Linux host. Review the addresses, limits, filesystem layout, and firewall policy for your environment before enabling it. Relay does not drop privileges itself; systemd starts it directly as the dedicated `relay` account.

## Install files and account

Install the release binary at `/usr/local/bin/relay` and create a system account with no login shell or home directory. Put the configuration and PEM files under `/etc/relay`; make the directory root-owned and readable by the `relay` group. The private key should be readable only by root and the Relay service group.

For example, on Debian-like systems:

```sh
sudo adduser --system --group --no-create-home --disabled-login relay
sudo install -d -o root -g relay -m 0750 /etc/relay
sudo install -o root -g relay -m 0640 relay.toml /etc/relay/relay.toml
sudo install -o root -g relay -m 0640 relay.pem /etc/relay/relay.pem
sudo install -o root -g relay -m 0640 relay-key.pem /etc/relay/relay-key.pem
```

Adjust TLS file names in the configuration to match installed files. Keep upstream destinations explicit in `[[routes]]`; never populate them from request values. Validate the actual installed configuration before starting the service:

```sh
sudo -u relay /usr/local/bin/relay check --config /etc/relay/relay.toml
```

Relative certificate and key paths are resolved from the process working directory, which the unit sets to `/etc/relay`.

## Install and start the unit

Copy `relay.service` to `/etc/systemd/system/relay.service`, then run:

```sh
sudo systemctl daemon-reload
sudo systemctl enable --now relay.service
sudo systemctl status relay.service
sudo journalctl -u relay.service
```

The unit checks configuration and TLS files before each start, restarts on failure, and gives Relay up to ten seconds to stop. Relay's configured shutdown deadline should be shorter than `TimeoutStopSec` so the process can finish its own cleanup.

## Network and host limits

- Allow inbound TCP on the configured listener port for HTTP/1.1 and HTTP/2 over TLS. Allow inbound UDP on the same numeric port for HTTP/3 over QUIC when TLS is enabled.
- Restrict outbound connections to the configured upstream addresses and ports where host or network firewall policy allows it.
- Do not expose a plaintext listener to untrusted networks. `tls = false` is intended for local development or a separately protected network path.
- Check the host's file-descriptor limit against `max_connections`, upstream sockets, and other service needs. The example sets a systemd limit of 65,536; tune it to measured workload and host capacity.
- Keep configuration and key material root-owned and non-writable by the service. Rotate certificates by replacing files during a controlled restart; live reload is not implemented.
- Inspect logs with `journalctl`; Relay logs operational events and must not log request headers, bodies, credentials, or cookies.

This example does not establish a production security boundary by itself. Validate the deployment on the target distribution and complete the release gates in `docs/RELEASE_CHECKLIST.md` before production use.

## Validation status

The unit passed `systemd-analyze verify` with systemd 261 under WSL2 using a
temporary root containing the unit, standard target units, and a local debug
Relay binary at the configured executable path. This checks unit syntax and
referenced executable presence on that systemd version only. It does not test
service startup, account or file permissions, firewall behavior, or a target
Linux distribution. Perform those checks on the actual deployment distribution
before enabling the service. `scripts/release/verify-systemd-unit.sh` repeats
the staged check against the release binary; CI now runs it on both supported
Linux architectures after building. The workflow has not yet run for this
uncommitted change.
