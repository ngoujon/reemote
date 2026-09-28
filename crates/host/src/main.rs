mod capture;
mod config;
mod input;
mod session;
mod tls;

use anyhow::Result;
use clap::{Parser, Subcommand};
use config::HostConfig;
use std::sync::Arc;
use tls::HostIdentity;
use tokio::sync::Mutex;
use tokio_rustls::TlsAcceptor;
use tracing::{error, info};

#[derive(Parser)]
#[command(
    name = "reemote-host",
    about = "Reemote host agent: runs on the machine you want to control remotely."
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Set (or change) the access password required to connect.
    SetPassword,
    /// Start the host agent and listen for incoming connections.
    Run {
        #[arg(long)]
        port: Option<u16>,
    },
    /// Print this host's TLS certificate fingerprint for pinning on the client.
    Fingerprint,
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("failed to install rustls crypto provider");
    let cli = Cli::parse();

    match cli.command {
        Command::SetPassword => {
            let mut config = HostConfig::load_or_default()?;
            // REEMOTE_PASSWORD allows non-interactive provisioning (e.g. a
            // first-boot setup script) where there is no TTY to prompt on.
            let password = match std::env::var("REEMOTE_PASSWORD") {
                Ok(p) => p,
                Err(_) => {
                    let password = rpassword::prompt_password("New access password: ")?;
                    let confirm = rpassword::prompt_password("Confirm password: ")?;
                    if password != confirm {
                        anyhow::bail!("passwords do not match");
                    }
                    password
                }
            };
            if password.len() < 8 {
                anyhow::bail!("password must be at least 8 characters");
            }
            config.set_password(&password)?;
            config.save()?;
            println!("Password updated.");
        }
        Command::Fingerprint => {
            let identity = HostIdentity::load_or_generate()?;
            println!("{}", identity.fingerprint());
        }
        Command::Run { port } => {
            let mut config = HostConfig::load_or_default()?;
            if config.password_hash.is_none() {
                anyhow::bail!("no access password set yet; run `reemote-host set-password` first");
            }
            if let Some(p) = port {
                config.port = p;
            }

            let identity = HostIdentity::load_or_generate()?;
            let fingerprint = identity.fingerprint();
            let server_config = identity.server_config()?;
            let acceptor = TlsAcceptor::from(server_config);

            let listener = tokio::net::TcpListener::bind(("0.0.0.0", config.port)).await?;
            info!("reemote-host listening on port {}", config.port);
            println!("Listening on port {}", config.port);
            println!("TLS fingerprint (share with the client for pinning): {fingerprint}");

            let config = Arc::new(Mutex::new(config));
            let local = tokio::task::LocalSet::new();

            local
                .run_until(async move {
                    loop {
                        let (stream, peer) = listener.accept().await?;
                        info!("incoming connection from {peer}");
                        let acceptor = acceptor.clone();
                        let config = config.clone();
                        tokio::task::spawn_local(async move {
                            if let Err(e) = session::handle_connection(stream, acceptor, config).await {
                                error!("session with {peer} ended with error: {e:#}");
                            }
                        });
                    }
                    #[allow(unreachable_code)]
                    Ok::<(), anyhow::Error>(())
                })
                .await?;
        }
    }

    Ok(())
}
