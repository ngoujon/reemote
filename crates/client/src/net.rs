use anyhow::{Context, Result};
use reemote_protocol::{read_message, write_message, DisplayInfo, FrameChunk, InputEvent, Message};
use rustls::pki_types::ServerName;
use rustls::ClientConfig;
use std::net::IpAddr;
use std::sync::mpsc as std_mpsc;
use std::sync::Arc;
use tokio::io::split;
use tokio::net::TcpStream;
use tokio::sync::mpsc as tokio_mpsc;
use tokio_rustls::TlsConnector;

use crate::pin_store::PinStore;
use crate::verifier::PinningVerifier;

pub enum UiToNet {
    Connect {
        host: String,
        port: u16,
        password: String,
    },
    Input(InputEvent),
    Disconnect,
}

pub enum NetToUi {
    Status(String),
    Connected { host_name: String, fingerprint: String, new_pin: bool },
    AuthFailed(String),
    Displays(Vec<DisplayInfo>),
    Frame(FrameChunk),
    Disconnected(String),
    Error(String),
}

pub fn spawn_network_thread(
    mut cmd_rx: tokio_mpsc::UnboundedReceiver<UiToNet>,
    event_tx: std_mpsc::Sender<NetToUi>,
    ctx: eframe::egui::Context,
) {
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build network runtime");
        rt.block_on(async move {
            loop {
                match cmd_rx.recv().await {
                    Some(UiToNet::Connect { host, port, password }) => {
                        let result =
                            run_session(&host, port, &password, &mut cmd_rx, &event_tx, &ctx).await;
                        if let Err(e) = result {
                            let _ = event_tx.send(NetToUi::Error(format!("{e:#}")));
                            ctx.request_repaint();
                        }
                    }
                    Some(UiToNet::Disconnect) | Some(UiToNet::Input(_)) => {
                        // No active session; nothing to do with these yet.
                    }
                    None => break,
                }
            }
        });
    });
}

async fn run_session(
    host: &str,
    port: u16,
    password: &str,
    cmd_rx: &mut tokio_mpsc::UnboundedReceiver<UiToNet>,
    event_tx: &std_mpsc::Sender<NetToUi>,
    ctx: &eframe::egui::Context,
) -> Result<()> {
    let target = format!("{host}:{port}");
    let _ = event_tx.send(NetToUi::Status(format!("Resolving {target}...")));
    ctx.request_repaint();

    let mut pin_store = PinStore::load().unwrap_or_default();
    let expected = pin_store.get(&target);
    let verifier = PinningVerifier::new(expected.clone());

    let mut config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier.clone())
        .with_no_client_auth();
    config.enable_sni = false;

    let connector = TlsConnector::from(Arc::new(config));
    let server_name = parse_server_name(host)?;

    let _ = event_tx.send(NetToUi::Status(format!("Connecting to {target}...")));
    ctx.request_repaint();

    let tcp = TcpStream::connect((host, port))
        .await
        .with_context(|| format!("failed to reach {target}"))?;
    let tls_stream = connector
        .connect(server_name, tcp)
        .await
        .context("TLS handshake failed")?;

    let seen_fingerprint = verifier
        .seen_fingerprint
        .lock()
        .unwrap()
        .clone()
        .context("no certificate was presented by the host")?;
    let new_pin = expected.is_none();
    if new_pin {
        pin_store.set(&target, &seen_fingerprint);
        pin_store.save().ok();
    }

    let (mut reader, mut writer) = split(tls_stream);

    write_message(
        &mut writer,
        &Message::ClientHello {
            protocol_version: reemote_protocol::PROTOCOL_VERSION,
        },
    )
    .await?;

    let host_name = match read_message(&mut reader).await? {
        Message::ServerHello { host_name, .. } => host_name,
        _ => anyhow::bail!("unexpected reply to ClientHello"),
    };

    write_message(
        &mut writer,
        &Message::AuthRequest {
            password: password.to_string(),
        },
    )
    .await?;

    match read_message(&mut reader).await? {
        Message::AuthResult { ok: true, .. } => {}
        Message::AuthResult { ok: false, reason } => {
            let _ = event_tx.send(NetToUi::AuthFailed(
                reason.unwrap_or_else(|| "invalid password".to_string()),
            ));
            ctx.request_repaint();
            return Ok(());
        }
        _ => anyhow::bail!("unexpected reply to AuthRequest"),
    }

    let _ = event_tx.send(NetToUi::Connected {
        host_name,
        fingerprint: seen_fingerprint,
        new_pin,
    });
    ctx.request_repaint();

    let displays = match read_message(&mut reader).await? {
        Message::Displays { displays } => displays,
        _ => anyhow::bail!("expected display list"),
    };
    let first_display = displays.first().map(|d| d.id).unwrap_or(0);
    let _ = event_tx.send(NetToUi::Displays(displays));
    ctx.request_repaint();

    write_message(
        &mut writer,
        &Message::SelectDisplay {
            display_id: first_display,
        },
    )
    .await?;

    loop {
        tokio::select! {
            incoming = read_message(&mut reader) => {
                match incoming {
                    Ok(Message::Frame(chunk)) => {
                        let _ = event_tx.send(NetToUi::Frame(chunk));
                        ctx.request_repaint();
                    }
                    Ok(Message::Disconnect { reason }) => {
                        let _ = event_tx.send(NetToUi::Disconnected(reason));
                        ctx.request_repaint();
                        return Ok(());
                    }
                    Ok(Message::Pong) | Ok(_) => {}
                    Err(e) => {
                        let _ = event_tx.send(NetToUi::Disconnected(format!("connection lost: {e}")));
                        ctx.request_repaint();
                        return Ok(());
                    }
                }
            }
            cmd = cmd_rx.recv() => {
                match cmd {
                    Some(UiToNet::Input(event)) => {
                        if write_message(&mut writer, &Message::Input(event)).await.is_err() {
                            break;
                        }
                    }
                    Some(UiToNet::Disconnect) => {
                        let _ = write_message(&mut writer, &Message::Disconnect { reason: "client disconnected".into() }).await;
                        let _ = event_tx.send(NetToUi::Disconnected("disconnected".to_string()));
                        ctx.request_repaint();
                        return Ok(());
                    }
                    Some(UiToNet::Connect { .. }) => {
                        // Already connected; ignore until this session ends.
                    }
                    None => return Ok(()),
                }
            }
        }
    }

    Ok(())
}

fn parse_server_name(host: &str) -> Result<ServerName<'static>> {
    if let Ok(ip) = host.parse::<IpAddr>() {
        Ok(ServerName::IpAddress(ip.into()))
    } else {
        Ok(ServerName::try_from(host.to_string()).context("invalid host name")?)
    }
}
