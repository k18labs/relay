# Deployment

This is an early development build. ARM64 tests, lint, and bounded fuzz smoke runs pass locally; Linux x86-64 evidence for the current changes, sustained load and comparative performance results, an external independent security review, and verified release artifacts have not been completed. Do not treat it as a production release.

TLS is enabled by default. Provide protected PEM certificate and private-key files, and restrict configured upstreams to the services Relay should reach. For local-only development, `server.tls = false` allows plaintext HTTP. Run under a dedicated unprivileged account, restrict listener and upstream network reachability, protect the configuration and key files, and monitor the configured limits.

Certificate, key, and custom upstream CA paths are resolved from the process working directory. Startup validation happens before binding. Relay does not include automatic certificate issuance/renewal, privilege dropping, or a network administration interface.

A systemd unit and operator setup checklist are available in [`deploy/systemd/`](../deploy/systemd/). The unit starts Relay as a dedicated account, validates the configuration before startup, and bounds service stop time. Adapt its filesystem permissions, TCP/UDP firewall rules, and file-descriptor limit to the host.

The tagged release workflow creates a draft GitHub release after Linux x86-64 and ARM64 builds and an SPDX SBOM job complete. It attaches binary checksums and requests GitHub build provenance attestations. A draft is a review artifact, not authorization to publish: maintainers must verify the attestation and checksums, review the SBOM, and complete the release checklist before publishing.
