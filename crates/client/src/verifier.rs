use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::{verify_tls12_signature, verify_tls13_signature};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::{Arc, Mutex};

/// Trust-on-first-use certificate verifier: the host has no CA-issued cert
/// (it self-signs on first run), so instead of validating a chain we pin
/// the exact certificate fingerprint after the operator confirms it out of
/// band (analogous to SSH host key pinning). The handshake signature is
/// still cryptographically verified, so an attacker who merely observes or
/// replays the certificate bytes cannot pass the handshake without the
/// matching private key.
#[derive(Debug)]
pub struct PinningVerifier {
    expected_fingerprint: Option<String>,
    pub seen_fingerprint: Mutex<Option<String>>,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

pub fn fingerprint_of(cert: &CertificateDer<'_>) -> String {
    let digest = Sha256::digest(cert.as_ref());
    digest
        .iter()
        .map(|b| format!("{b:02X}"))
        .collect::<Vec<_>>()
        .join(":")
}

impl PinningVerifier {
    pub fn new(expected_fingerprint: Option<String>) -> Arc<Self> {
        Arc::new(Self {
            expected_fingerprint,
            seen_fingerprint: Mutex::new(None),
            provider: Arc::new(rustls::crypto::ring::default_provider()),
        })
    }
}

impl ServerCertVerifier for PinningVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        let fp = fingerprint_of(end_entity);
        *self.seen_fingerprint.lock().unwrap() = Some(fp.clone());

        if let Some(expected) = &self.expected_fingerprint {
            if &fp != expected {
                return Err(rustls::Error::General(format!(
                    "certificate fingerprint mismatch: expected {expected}, got {fp}. \
                     Refusing to connect — this could mean the host was reinstalled, \
                     or that the connection is being intercepted."
                )));
            }
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider.signature_verification_algorithms.supported_schemes()
    }
}
