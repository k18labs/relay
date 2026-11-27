use std::{collections::HashSet, net::SocketAddr, str::FromStr};

use http::{Uri, uri::Authority};
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("invalid TOML configuration: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("invalid configuration: {0}")]
    Invalid(String),
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub server: ServerConfig,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub tls: TlsConfig,
    pub routes: Vec<Route>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ServerConfig {
    pub listen: SocketAddr,
    pub max_connections: usize,
    #[serde(default = "default_worker_count")]
    pub workers: usize,
    #[serde(default = "tls_enabled_by_default")]
    pub tls: bool,
}

fn default_worker_count() -> usize {
    2
}

fn tls_enabled_by_default() -> bool {
    true
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TlsConfig {
    pub certificate: Option<std::path::PathBuf>,
    pub private_key: Option<std::path::PathBuf>,
    pub upstream_ca_files: Vec<std::path::PathBuf>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Limits {
    pub max_header_bytes: usize,
    pub max_header_count: usize,
    pub max_body_bytes: usize,
    pub max_concurrent_requests: usize,
    pub max_concurrent_streams_per_connection: usize,
    pub max_concurrent_tls_handshakes: usize,
    pub header_timeout_ms: u64,
    pub handshake_timeout_ms: u64,
    pub connect_timeout_ms: u64,
    pub response_timeout_ms: u64,
    pub body_idle_timeout_ms: u64,
    pub downstream_write_timeout_ms: u64,
    pub upstream_idle_timeout_ms: u64,
    pub shutdown_timeout_ms: u64,
    pub max_idle_upstream_per_host: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_header_bytes: 16 * 1024,
            max_header_count: 64,
            max_body_bytes: 10 * 1024 * 1024,
            max_concurrent_requests: 1024,
            max_concurrent_streams_per_connection: 64,
            max_concurrent_tls_handshakes: 128,
            header_timeout_ms: 5_000,
            handshake_timeout_ms: 5_000,
            connect_timeout_ms: 5_000,
            response_timeout_ms: 30_000,
            body_idle_timeout_ms: 30_000,
            downstream_write_timeout_ms: 30_000,
            upstream_idle_timeout_ms: 30_000,
            shutdown_timeout_ms: 5_000,
            max_idle_upstream_per_host: 8,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Route {
    pub host: String,
    pub path: String,
    pub upstream: String,
}

impl Config {
    pub fn parse(input: &str) -> Result<Self, ConfigError> {
        let config: Self = toml::from_str(input)?;
        config.validate()?;
        Ok(config)
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.server.max_connections == 0 {
            return Err(ConfigError::Invalid(
                "server.max_connections must be greater than zero".into(),
            ));
        }
        if self.server.max_connections > 65_536 {
            return Err(ConfigError::Invalid(
                "server.max_connections must not exceed 65536".into(),
            ));
        }
        if !(1..=64).contains(&self.server.workers) {
            return Err(ConfigError::Invalid(
                "server.workers must be between 1 and 64".into(),
            ));
        }
        if self.routes.is_empty() {
            return Err(ConfigError::Invalid(
                "at least one route is required".into(),
            ));
        }
        if self.routes.len() > 1024 {
            return Err(ConfigError::Invalid(
                "configuration must not contain more than 1024 routes".into(),
            ));
        }
        if self.server.tls && (self.tls.certificate.is_none() || self.tls.private_key.is_none()) {
            return Err(ConfigError::Invalid(
                "TLS is enabled by default; configure both tls.certificate and tls.private_key, or explicitly set server.tls = false for local plaintext use".into(),
            ));
        }
        if self.tls.certificate.is_some() != self.tls.private_key.is_some() {
            return Err(ConfigError::Invalid(
                "tls.certificate and tls.private_key must be configured together".into(),
            ));
        }
        if self.tls.upstream_ca_files.len() > 32 {
            return Err(ConfigError::Invalid(
                "tls.upstream_ca_files must not contain more than 32 files".into(),
            ));
        }
        if !self.server.tls && self.tls.certificate.is_some() {
            return Err(ConfigError::Invalid(
                "TLS credentials are set while server.tls is false".into(),
            ));
        }
        if self.limits.max_header_bytes < 8 * 1024
            || self.limits.max_header_bytes > 1024 * 1024
            || self.limits.max_header_count == 0
            || self.limits.max_header_count > 1024
            || self.limits.max_body_bytes == 0
            || self.limits.max_body_bytes > 1024 * 1024 * 1024
            || self.limits.max_concurrent_requests == 0
            || self.limits.max_concurrent_requests > 65_536
            || self.limits.max_concurrent_streams_per_connection == 0
            || self.limits.max_concurrent_streams_per_connection > 1024
            || self.limits.max_concurrent_tls_handshakes == 0
            || self.limits.max_concurrent_tls_handshakes > 65_536
        {
            return Err(ConfigError::Invalid(
                "limits are outside supported bounds (headers: 8192 bytes to 1 MiB and 1 to 1024 fields; body: 1 byte to 1 GiB; concurrent requests and TLS handshakes: 1 to 65536; concurrent streams per connection: 1 to 1024)".into(),
            ));
        }
        if self.limits.header_timeout_ms == 0
            || self.limits.handshake_timeout_ms == 0
            || self.limits.connect_timeout_ms == 0
            || self.limits.response_timeout_ms == 0
            || self.limits.body_idle_timeout_ms == 0
            || self.limits.downstream_write_timeout_ms == 0
            || self.limits.upstream_idle_timeout_ms == 0
            || self.limits.shutdown_timeout_ms == 0
        {
            return Err(ConfigError::Invalid(
                "configured timeouts must be greater than zero".into(),
            ));
        }
        if self.limits.max_idle_upstream_per_host > 256 {
            return Err(ConfigError::Invalid(
                "limits.max_idle_upstream_per_host must not exceed 256".into(),
            ));
        }
        let mut seen = HashSet::new();
        for route in &self.routes {
            let host = normalize_host(&route.host).ok_or_else(|| {
                ConfigError::Invalid(format!("invalid route host {:?}", route.host))
            })?;
            if route.path.is_empty()
                || route.path.len() > 4096
                || !route.path.starts_with('/')
                || route.path.contains(['?', '#'])
            {
                return Err(ConfigError::Invalid(format!(
                    "route path must be an absolute path without query or fragment: {:?}",
                    route.path
                )));
            }
            if !seen.insert((host, route.path.clone())) {
                return Err(ConfigError::Invalid(format!(
                    "duplicate route for host {:?} and path {:?}",
                    route.host, route.path
                )));
            }
            let uri = Uri::from_str(&route.upstream).map_err(|_| {
                ConfigError::Invalid(format!("invalid upstream URI for host {:?}", route.host))
            })?;
            if !matches!(uri.scheme_str(), Some("http" | "https"))
                || uri.authority().is_none()
                || uri.path() != "/"
                || uri.query().is_some()
                || uri.authority().is_some_and(|authority| {
                    authority.as_str().contains('@') || !authority_port_is_valid(authority)
                })
            {
                return Err(ConfigError::Invalid(format!(
                    "upstream for host {:?} must be an http:// origin with no credentials, path, or query",
                    route.host
                )));
            }
        }
        Ok(())
    }

    pub fn route(&self, host: &str, path: &str) -> Option<&Route> {
        let host = normalize_host(host)?;
        self.routes
            .iter()
            .filter(|route| normalize_host(&route.host).as_deref() == Some(host.as_str()))
            .filter(|route| path_matches(path, &route.path))
            .max_by_key(|route| route.path.len())
    }
}

pub(crate) fn authority_port_is_valid(authority: &Authority) -> bool {
    let raw = authority.as_str();
    let suffix = if raw.starts_with('[') {
        let Some(end) = raw.find(']') else {
            return false;
        };
        &raw[end + 1..]
    } else if let Some(colon) = raw.find(':') {
        &raw[colon..]
    } else {
        return true;
    };

    if suffix.is_empty() {
        return true;
    }
    let Some(port) = suffix.strip_prefix(':') else {
        return false;
    };
    !port.is_empty() && port.parse::<u16>().is_ok()
}

pub fn normalize_host(host: &str) -> Option<String> {
    let host = host.trim();
    let host = host.strip_suffix('.').unwrap_or(host);
    if host.is_empty() || host.len() > 253 {
        return None;
    }
    let normalized = host.to_ascii_lowercase();
    if normalized.parse::<std::net::Ipv4Addr>().is_ok()
        || normalized.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label.as_bytes()[0].is_ascii_alphanumeric()
                && label.as_bytes()[label.len() - 1].is_ascii_alphanumeric()
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
    {
        Some(normalized)
    } else {
        None
    }
}

fn path_matches(request_path: &str, route_path: &str) -> bool {
    if route_path == "/" || request_path == route_path {
        return true;
    }
    request_path.starts_with(route_path)
        && (route_path.ends_with('/')
            || request_path.as_bytes().get(route_path.len()) == Some(&b'/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_dns_host_case_and_trailing_dot() {
        assert_eq!(
            normalize_host("Example.ORG.").as_deref(),
            Some("example.org")
        );
    }
}
