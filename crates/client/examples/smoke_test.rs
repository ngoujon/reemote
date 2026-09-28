//! Throwaway end-to-end smoke test: connects to a local reemote-host,
//! completes the handshake/auth/display flow, reads a couple of frames,
//! and sends one input event. Not part of the shipped product — used only
//! to validate the wire protocol during development. Accepts any TLS cert
//! (no pinning) since it targets a known-local test host.
use anyhow::{Context, Result};
use reemote_protocol::{read_message, write_message, InputEvent, Message, MouseButton};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::{ClientConfig, DigitallySignedStruct, SignatureScheme};
use std::sync::Arc;
use tokio::io::split;
use tokio::net::TcpStream;
use tokio_rustls::TlsConnector;

#[derive(Debug)]
struct AcceptAny;
impl ServerCertVerifier for AcceptAny {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        Ok(ServerCertVerified::assertion())
    }
    fn verify_tls12_signature(
        &self,
        _m: &[u8],
        _c: &CertificateDer<'_>,
        _d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn verify_tls13_signature(
        &self,
        _m: &[u8],
        _c: &CertificateDer<'_>,
        _d: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        Ok(HandshakeSignatureValid::assertion())
    }
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ED25519,
            SignatureScheme::RSA_PSS_SHA256,
        ]
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install crypto provider");

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(AcceptAny))
        .with_no_client_auth();
    let connector = TlsConnector::from(Arc::new(config));

    let tcp = TcpStream::connect(("127.0.0.1", 17723)).await?;
    let tls = connector
        .connect(ServerName::IpAddress("127.0.0.1".parse::<std::net::IpAddr>()?.into()), tcp)
        .await
        .context("tls handshake")?;
    println!("TLS handshake OK");

    let (mut r, mut w) = split(tls);

    write_message(&mut w, &Message::ClientHello { protocol_version: reemote_protocol::PROTOCOL_VERSION }).await?;
    match read_message(&mut r).await? {
        Message::ServerHello { host_name, .. } => println!("ServerHello from {host_name}"),
        m => anyhow::bail!("unexpected: {m:?}"),
    }

    write_message(&mut w, &Message::AuthRequest { password: "testpass123".into() }).await?;
    match read_message(&mut r).await? {
        Message::AuthResult { ok, reason } => println!("AuthResult ok={ok} reason={reason:?}"),
        m => anyhow::bail!("unexpected: {m:?}"),
    }

    let displays = match read_message(&mut r).await? {
        Message::Displays { displays } => displays,
        m => anyhow::bail!("unexpected: {m:?}"),
    };
    println!("Displays: {displays:?}");
    let first = displays.first().map(|d| d.id).unwrap_or(0);
    write_message(&mut w, &Message::SelectDisplay { display_id: first }).await?;

    for i in 0..3 {
        match read_message(&mut r).await? {
            Message::Frame(chunk) => println!(
                "Frame {i}: {}x{} region ({},{} {}x{}), {} jpeg bytes",
                chunk.full_width, chunk.full_height, chunk.x, chunk.y, chunk.width, chunk.height, chunk.jpeg.len()
            ),
            m => println!("unexpected during frame wait: {m:?}"),
        }
    }

    write_message(&mut w, &Message::Input(InputEvent::MouseMove { x: 100.0, y: 100.0 })).await?;
    write_message(&mut w, &Message::Input(InputEvent::MouseButton { button: MouseButton::Left, down: true })).await?;
    write_message(&mut w, &Message::Input(InputEvent::MouseButton { button: MouseButton::Left, down: false })).await?;
    println!("Sent input events");

    write_message(&mut w, &Message::Disconnect { reason: "smoke test done".into() }).await?;
    println!("Smoke test complete");
    Ok(())
}
