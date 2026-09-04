use faizdb_security::{create_rustls_server_config, generate_self_signed_cert};

#[tokio::test]
async fn test_tls_self_signed_cert_and_server_config() {
    let (certs, key) = generate_self_signed_cert(&["localhost".into(), "127.0.0.1".into()])
        .expect("Failed to generate self-signed cert for TLS");

    assert!(!certs.is_empty(), "Certificates should not be empty");

    let server_config =
        create_rustls_server_config(certs, key).expect("Failed to build Rustls ServerConfig");

    assert_eq!(
        server_config.alpn_protocols,
        vec![b"h2".to_vec(), b"http/1.1".to_vec()]
    );
}

#[tokio::test]
async fn test_tls_server_config_loader_from_env() {
    // Test that FAIZDB_ENABLE_TLS=true produces valid RustlsConfig
    unsafe {
        std::env::set_var("FAIZDB_ENABLE_TLS", "true");
    }

    let tls_config = faizdb_server::get_server_tls_config().await;
    assert!(
        tls_config.is_some(),
        "TLS config should be generated when FAIZDB_ENABLE_TLS=true"
    );

    unsafe {
        std::env::remove_var("FAIZDB_ENABLE_TLS");
    }
}
