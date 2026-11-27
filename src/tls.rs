use std::{
    fs::File,
    io::{self, Read},
    path::Path,
    sync::Arc,
};

use rustls::{
    ClientConfig, RootCertStore, ServerConfig,
    pki_types::{
        CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer, PrivateSec1KeyDer,
        pem::{PemObject, SectionKind},
    },
};
use thiserror::Error;

use crate::config::TlsConfig;

#[derive(Debug, Error)]
pub enum TlsConfigError {
    #[error("failed to read TLS material: {0}")]
    Io(#[from] io::Error),
    #[error("invalid PEM encoding: {0}")]
    Pem(String),
    #[error("TLS certificate file has no certificates")]
    EmptyCertificateFile,
    #[error("TLS private key file must contain exactly one private key and no other PEM items")]
    InvalidPrivateKeyFile,
    #[error("TLS certificate file may contain only X.509 certificates")]
    InvalidCertificateFile,
    #[error("server certificate and private key do not match or are not supported")]
    CertificateKeyMismatch,
    #[error("upstream CA file contains no certificates")]
    EmptyTrustFile,
    #[error("upstream CA certificate is not a valid trust anchor")]
    InvalidTrustAnchor,
    #[error("TLS PEM file exceeds the 1 MiB limit")]
    PemFileTooLarge,
    #[error("could not construct a TLS 1.3 configuration for the selected crypto provider")]
    UnsupportedTlsConfiguration,
}

pub fn load_server_config(config: &TlsConfig) -> Result<Arc<ServerConfig>, TlsConfigError> {
    let certificate_path = config
        .certificate
        .as_deref()
        .ok_or(TlsConfigError::EmptyCertificateFile)?;
    let key_path = config
        .private_key
        .as_deref()
        .ok_or(TlsConfigError::InvalidPrivateKeyFile)?;
    let certificates = load_certificates(certificate_path)?;
    let key = load_private_key(key_path)?;
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| TlsConfigError::UnsupportedTlsConfiguration)?;
    let mut server = builder
        .with_no_client_auth()
        .with_single_cert(certificates, key)
        .map_err(|_| TlsConfigError::CertificateKeyMismatch)?;
    server.alpn_protocols = vec![b"http/1.1".to_vec()];
    server.max_early_data_size = 0;
    Ok(Arc::new(server))
}

pub fn build_upstream_client_config(
    config: &TlsConfig,
) -> Result<Arc<ClientConfig>, TlsConfigError> {
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    for path in &config.upstream_ca_files {
        let certificates = load_certificates(path)?;
        for certificate in certificates {
            roots
                .add(certificate)
                .map_err(|_| TlsConfigError::InvalidTrustAnchor)?;
        }
    }
    let provider = Arc::new(rustls::crypto::aws_lc_rs::default_provider());
    let builder = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(&[&rustls::version::TLS13])
        .map_err(|_| TlsConfigError::UnsupportedTlsConfiguration)?;
    let mut client = builder.with_root_certificates(roots).with_no_client_auth();
    client.enable_early_data = false;
    Ok(Arc::new(client))
}

fn load_certificates(path: &Path) -> Result<Vec<CertificateDer<'static>>, TlsConfigError> {
    let pem = read_pem_file(path)?;
    let mut certificates = Vec::new();
    for item in <(SectionKind, Vec<u8>) as PemObject>::pem_reader_iter(pem.as_slice()) {
        let (kind, der) = item.map_err(|error| TlsConfigError::Pem(error.to_string()))?;
        match kind {
            SectionKind::Certificate => certificates.push(CertificateDer::from(der)),
            _ => return Err(TlsConfigError::InvalidCertificateFile),
        }
    }
    if certificates.is_empty() {
        return Err(TlsConfigError::EmptyCertificateFile);
    }
    Ok(certificates)
}

fn load_private_key(path: &Path) -> Result<PrivateKeyDer<'static>, TlsConfigError> {
    let pem = read_pem_file(path)?;
    let mut key = None;
    for item in <(SectionKind, Vec<u8>) as PemObject>::pem_reader_iter(pem.as_slice()) {
        let (kind, der) = item.map_err(|error| TlsConfigError::Pem(error.to_string()))?;
        let parsed = match kind {
            SectionKind::RsaPrivateKey => PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(der)),
            SectionKind::PrivateKey => PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(der)),
            SectionKind::EcPrivateKey => PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(der)),
            _ => return Err(TlsConfigError::InvalidPrivateKeyFile),
        };
        if key.replace(parsed).is_some() {
            return Err(TlsConfigError::InvalidPrivateKeyFile);
        }
    }
    key.ok_or(TlsConfigError::InvalidPrivateKeyFile)
}

fn read_pem_file(path: &Path) -> Result<Vec<u8>, TlsConfigError> {
    const MAX_PEM_BYTES: u64 = 1024 * 1024;
    let mut reader = File::open(path)?.take(MAX_PEM_BYTES + 1);
    let mut pem = Vec::new();
    reader.read_to_end(&mut pem)?;
    if pem.len() as u64 > MAX_PEM_BYTES {
        return Err(TlsConfigError::PemFileTooLarge);
    }
    Ok(pem)
}
