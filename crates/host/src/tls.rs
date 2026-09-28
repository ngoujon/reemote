use anyhow::{Context, Result};
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::sync::Arc;

/// A self-signed identity for this host, persisted across restarts so the
/// client's trust-on-first-use pin keeps working after a reboot.
pub struct HostIdentity {
    pub cert_der: Vec<u8>,
    pub key_der: Vec<u8>,
}

fn identity_dir() -> Result<PathBuf> {
    let dirs = directories::ProjectDirs::from("dev", "reemote", "reemote-host")
        .context("could not determine config directory")?;
    let dir = dirs.config_dir().to_path_buf();
    std::fs::create_dir_all(&dir)?;
    Ok(dir)
}

impl HostIdentity {
    /// Loads the persisted self-signed cert/key, generating and saving a
    /// fresh one on first run.
    pub fn load_or_generate() -> Result<Self> {
        let dir = identity_dir()?;
        let cert_path = dir.join("host_cert.der");
        let key_path = dir.join("host_key.der");

        if cert_path.exists() && key_path.exists() {
            let cert_der = std::fs::read(&cert_path)?;
            let key_der = std::fs::read(&key_path)?;
            return Ok(Self { cert_der, key_der });
        }

        let params = rcgen::CertificateParams::new(vec!["reemote-host".to_string()])
            .context("failed to build certificate params")?;
        let key_pair = rcgen::KeyPair::generate().context("failed to generate key pair")?;
        let cert = params
            .self_signed(&key_pair)
            .context("failed to self-sign certificate")?;

        let cert_der = cert.der().to_vec();
        let key_der = key_pair.serialize_der();

        std::fs::write(&cert_path, &cert_der)?;
        std::fs::write(&key_path, &key_der)?;

        Ok(Self { cert_der, key_der })
    }

    /// SHA-256 fingerprint of the DER certificate, formatted as uppercase
    /// hex pairs separated by colons (e.g. `AB:CD:...`), for the operator
    /// to read out-of-band to whoever is setting up the client's pin.
    pub fn fingerprint(&self) -> String {
        let digest = Sha256::digest(&self.cert_der);
        digest
            .iter()
            .map(|b| format!("{b:02X}"))
            .collect::<Vec<_>>()
            .join(":")
    }

    pub fn server_config(&self) -> Result<Arc<rustls::ServerConfig>> {
        let cert = CertificateDer::from(self.cert_der.clone());
        let key = PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone()));

        let config = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(vec![cert], key)
            .context("failed to build TLS server config")?;

        Ok(Arc::new(config))
    }
}
