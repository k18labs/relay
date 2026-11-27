# ADR 0001: Hyper and Tokio for the proxy core

- Status: accepted for initial implementation
- Decision: use Tokio for async networking and Hyper for HTTP parsing, serialization, and client connection reuse.
- Context: Relay needs a small trusted codebase without a custom HTTP parser.
- Alternatives: a web framework adds behavior not needed by a reverse proxy; a custom HTTP stack would duplicate mature protocol code and increase risk.
- Consequences: the implementation owns route policy and resource limits while Hyper owns wire parsing. Hyper defaults and client pooling still require explicit review and configuration before release.
