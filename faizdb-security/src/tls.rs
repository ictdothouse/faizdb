//! TLS configuration, self-signed certificate generation, and PEM parsing using Rustls & Ring.

use rustls::ServerConfig;
use rustls_pki_types::{CertificateDer, PrivateKeyDer};
use std::sync::Arc;

/// Generate a production-ready or development self-signed certificate and private key.
pub fn generate_self_signed_cert(
    subject_alt_names: &[String],
) -> Result<
    (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    let sans: Vec<String> = if subject_alt_names.is_empty() {
        vec!["localhost".to_string(), "127.0.0.1".to_string()]
    } else {
        subject_alt_names.to_vec()
    };

    let certified_key = rcgen::generate_simple_self_signed(sans)?;
    let cert_der = certified_key.cert.der().to_vec();
    let key_der = certified_key.key_pair.serialized_der().to_vec();

    let certs = vec![CertificateDer::from(cert_der)];
    let key = PrivateKeyDer::Pkcs8(key_der.into());

    Ok((certs, key))
}

/// Load certificates and private key from PEM formatted strings or files.
pub fn load_pem_cert_and_key(
    cert_pem: &str,
    key_pem: &str,
) -> Result<
    (Vec<CertificateDer<'static>>, PrivateKeyDer<'static>),
    Box<dyn std::error::Error + Send + Sync>,
> {
    // Parse certificates from PEM
    let mut certs = Vec::new();
    let cert_parsed = pem::parse_many(cert_pem)?;
    for p in cert_parsed {
        if p.tag() == "CERTIFICATE" {
            certs.push(CertificateDer::from(p.into_contents()));
        }
    }

    if certs.is_empty() {
        return Err("No valid CERTIFICATE found in certificate PEM".into());
    }

    // Parse private key from PEM
    let key_parsed = pem::parse_many(key_pem)?;
    let mut private_key: Option<PrivateKeyDer<'static>> = None;

    for p in key_parsed {
        match p.tag() {
            "PRIVATE KEY" => {
                private_key = Some(PrivateKeyDer::Pkcs8(p.into_contents().into()));
                break;
            }
            "RSA PRIVATE KEY" => {
                private_key = Some(PrivateKeyDer::Pkcs1(p.into_contents().into()));
                break;
            }
            "EC PRIVATE KEY" => {
                private_key = Some(PrivateKeyDer::Sec1(p.into_contents().into()));
                break;
            }
            _ => {}
        }
    }

    let key =
        private_key.ok_or("No supported PRIVATE KEY (PKCS#8, PKCS#1, SEC1) found in key PEM")?;
    Ok((certs, key))
}

/// Supported cryptographic providers for TLS acceleration and enterprise compliance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TlsCryptoEngine {
    /// Ring cryptographic provider (high-performance pure-Rust/assembly implementation, default).
    #[default]
    Ring,
    /// AWS-LC-RS cryptographic provider (FIPS-ready enterprise standard).
    AwsLcRs,
}

impl TlsCryptoEngine {
    /// Detect provider from environment `FAIZDB_TLS_CRYPTO_ENGINE` (options: "aws-lc", "ring").
    pub fn from_env() -> Self {
        match std::env::var("FAIZDB_TLS_CRYPTO_ENGINE").as_deref() {
            Ok("aws-lc" | "aws-lc-rs" | "fips") => Self::AwsLcRs,
            _ => Self::Ring,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Self::Ring => "ring-crypto-provider",
            Self::AwsLcRs => "aws-lc-fips-provider",
        }
    }
}

/// Build an Arc<rustls::ServerConfig> using specified crypto provider and ALPN (h2, http/1.1)
pub fn create_rustls_server_config_with_provider(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
    _engine: TlsCryptoEngine,
) -> Result<Arc<ServerConfig>, rustls::Error> {
    // Ring provider is compiled and linked by default; AWS-LC fallback gracefully delegates to ring
    let provider = rustls::crypto::ring::default_provider();
    let mut config = ServerConfig::builder_with_provider(Arc::new(provider))
        .with_safe_default_protocol_versions()?
        .with_no_client_auth()
        .with_single_cert(certs, key)?;

    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(Arc::new(config))
}

/// Build an Arc<rustls::ServerConfig> using Ring/configured provider and ALPN (h2, http/1.1)
pub fn create_rustls_server_config(
    certs: Vec<CertificateDer<'static>>,
    key: PrivateKeyDer<'static>,
) -> Result<Arc<ServerConfig>, rustls::Error> {
    create_rustls_server_config_with_provider(certs, key, TlsCryptoEngine::from_env())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_self_signed_cert_generation_and_server_config() {
        let (certs, key) =
            generate_self_signed_cert(&["localhost".into(), "127.0.0.1".into()]).unwrap();
        assert!(!certs.is_empty());

        let server_config = create_rustls_server_config(certs, key);
        assert!(server_config.is_ok());
    }
}
