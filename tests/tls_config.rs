use std::path::PathBuf;

use relay::{config::TlsConfig, tls};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("tls")
        .join(name)
}

#[test]
fn loads_a_pem_certificate_and_matching_private_key() {
    let config = TlsConfig {
        certificate: Some(fixture("localhost-cert.pem")),
        private_key: Some(fixture("localhost-key.pem")),
        upstream_ca_files: Vec::new(),
    };
    assert!(tls::load_server_config(&config).is_ok());
}

#[test]
fn rejects_a_certificate_and_private_key_that_do_not_match() {
    let config = TlsConfig {
        certificate: Some(fixture("localhost-cert.pem")),
        private_key: Some(fixture("wrong-host-key.pem")),
        upstream_ca_files: Vec::new(),
    };
    assert!(tls::load_server_config(&config).is_err());
}

#[test]
fn accepts_an_explicit_test_ca_for_verified_upstream_tls() {
    let config = TlsConfig {
        certificate: None,
        private_key: None,
        upstream_ca_files: vec![fixture("test-ca.pem")],
    };
    assert!(tls::build_upstream_client_config(&config).is_ok());
}

#[test]
fn rejects_a_missing_configured_trust_certificate() {
    let config = TlsConfig {
        certificate: None,
        private_key: None,
        upstream_ca_files: vec![fixture("missing-ca.pem")],
    };
    assert!(tls::build_upstream_client_config(&config).is_err());
}
