use anyhow::{Context, Result};
use reemote_protocol::{read_message, write_message, Message};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::split;
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;
use tracing::info;

use crate::capture::{list_displays, CaptureSession};
use crate::config::HostConfig;
use crate::input::InputInjector;

pub async fn handle_connection(
    stream: tokio::net::TcpStream,
    acceptor: TlsAcceptor,
    config: Arc<Mutex<HostConfig>>,
) -> Result<()> {
    let tls_stream = acceptor
        .accept(stream)
        .await
        .context("TLS handshake failed")?;
    let (mut reader, mut writer) = split(tls_stream);

    match read_message(&mut reader).await? {
        Message::ClientHello { protocol_version }
            if protocol_version == reemote_protocol::PROTOCOL_VERSION => {}
        _ => anyhow::bail!("unexpected or incompatible client hello"),
    }
    write_message(
        &mut writer,
        &Message::ServerHello {
            protocol_version: reemote_protocol::PROTOCOL_VERSION,
            host_name: hostname_string(),
        },
    )
    .await?;

    let authed = loop {
        match read_message(&mut reader).await? {
            Message::AuthRequest { password } => {
                let ok = config.lock().await.verify_password(&password);
                write_message(
                    &mut writer,
                    &Message::AuthResult {
                        ok,
                        reason: if ok {
                            None
                        } else {
                            Some("invalid password".into())
                        },
                    },
                )
                .await?;
                if ok {
                    break true;
                }
            }
            Message::Disconnect { .. } => return Ok(()),
            _ => anyhow::bail!("expected AuthRequest"),
        }
    };
    if !authed {
        return Ok(());
    }
    info!("client authenticated");

    let displays = list_displays()?;
    write_message(
        &mut writer,
        &Message::Displays {
            displays: displays.clone(),
        },
    )
    .await?;

    let display_id = match read_message(&mut reader).await? {
        Message::SelectDisplay { display_id } => display_id,
        _ => displays.first().map(|d| d.id).unwrap_or(0),
    };

    let mut capture = CaptureSession::new(display_id)?;
    let mut injector = InputInjector::new()?;
    let mut ticker = tokio::time::interval(Duration::from_millis(33));

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                if let Some(chunk) = capture.capture_chunk()? {
                    if write_message(&mut writer, &Message::Frame(chunk)).await.is_err() {
                        break;
                    }
                }
            }
            msg = read_message(&mut reader) => {
                match msg {
                    Ok(Message::Input(event)) => {
                        let _ = injector.apply(event);
                    }
                    Ok(Message::Ping) => {
                        if write_message(&mut writer, &Message::Pong).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Disconnect { .. }) | Err(_) => break,
                    _ => {}
                }
            }
        }
    }

    info!("session ended");
    Ok(())
}

fn hostname_string() -> String {
    hostname::get()
        .ok()
        .and_then(|s| s.into_string().ok())
        .unwrap_or_else(|| "reemote-host".to_string())
}
