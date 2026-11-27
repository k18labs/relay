# Deployment

This is an early development build and has not completed Linux x86-64/ARM64 verification, fuzzing, load testing, or independent security review. Do not treat it as a production release.

TLS is enabled by default. Provide protected PEM certificate and private-key files, and restrict configured upstreams to the services Relay should reach. For local-only development, `server.tls = false` allows plaintext HTTP. Run under a dedicated unprivileged account, restrict listener and upstream network reachability, protect the configuration and key files, and monitor the configured limits.

Certificate, key, and custom upstream CA paths are resolved from the process working directory. Startup validation happens before binding. Relay does not include automatic certificate issuance/renewal, privilege dropping, or a network administration interface.
