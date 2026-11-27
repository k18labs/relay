use relay::config::Config;

const VALID: &str = r#"
[server]
listen = "127.0.0.1:0"
max_connections = 32
tls = false

[[routes]]
host = "example.org"
path = "/"
upstream = "http://127.0.0.1:3000"
"#;

#[test]
fn accepts_a_valid_configuration() {
    let config = Config::parse(VALID).expect("valid config");
    assert_eq!(config.routes.len(), 1);
    assert_eq!(config.server.workers, 2);
}

#[test]
fn accepts_an_explicit_worker_count_within_bounds() {
    let input = VALID.replace("max_connections = 32", "max_connections = 32\nworkers = 4");
    let config = Config::parse(&input).expect("valid worker count");
    assert_eq!(config.server.workers, 4);
}

#[test]
fn rejects_worker_counts_outside_supported_bounds() {
    for workers in [0, 65] {
        let input = VALID.replace(
            "max_connections = 32",
            &format!("max_connections = 32\nworkers = {workers}"),
        );
        assert!(Config::parse(&input).is_err(), "workers={workers}");
    }
}

#[test]
fn rejects_unknown_fields() {
    let input = format!("{VALID}\n[server]\nadmin = true\n");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_equal_host_and_path_routes() {
    let input = format!(
        "{VALID}\n[[routes]]\nhost = \"example.org\"\npath = \"/\"\nupstream = \"http://127.0.0.1:3001\"\n"
    );
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_unsupported_upstream_schemes() {
    let input = VALID.replace("http://", "ftp://");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_an_upstream_port_outside_the_tcp_range() {
    let input = VALID.replace("127.0.0.1:3000", "127.0.0.1:65536");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_an_upstream_with_an_empty_explicit_port() {
    let input = VALID.replace("127.0.0.1:3000", "127.0.0.1:");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_invalid_route_host_syntax() {
    let input = VALID.replace("example.org", "example..org");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_more_than_one_trailing_dns_dot() {
    let input = VALID.replace("example.org", "example.org..");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn rejects_a_zero_request_body_limit() {
    let input = format!("{VALID}\n[limits]\nmax_body_bytes = 0\n");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn accepts_verified_https_upstream_origins() {
    let input = VALID.replace("http://", "https://");
    assert!(Config::parse(&input).is_ok());
}

#[test]
fn requires_server_certificate_and_private_key_together() {
    let input = format!(
        "{}\n[tls]\ncertificate = \"server.pem\"\n",
        VALID.replace("tls = false", "tls = true")
    );
    assert!(Config::parse(&input).is_err());
}

#[test]
fn tls_is_enabled_by_default_and_requires_credentials() {
    let input = VALID.replace("tls = false\n", "");
    assert!(Config::parse(&input).is_err());
}

#[test]
fn selects_the_longest_path_prefix_on_a_segment_boundary() {
    let input = format!(
        "{VALID}\n[[routes]]\nhost = \"example.org\"\npath = \"/api\"\nupstream = \"http://127.0.0.1:3001\"\n"
    );
    let config = Config::parse(&input).expect("valid routes");
    assert_eq!(
        config.route("example.org", "/api/users").unwrap().path,
        "/api"
    );
    assert_eq!(config.route("example.org", "/apix").unwrap().path, "/");
}
