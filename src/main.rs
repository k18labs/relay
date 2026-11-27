#![forbid(unsafe_code)]

use std::{
    fs::File,
    io::{self, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

use clap::{Parser, Subcommand};
use relay::{config::Config, proxy, tls};
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(name = "relay", version, about = "A lightweight HTTP reverse proxy")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Run {
        #[arg(long)]
        config: PathBuf,
    },
    Check {
        #[arg(long)]
        config: PathBuf,
    },
    Version,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cli = Cli::parse();
    match cli.command {
        Command::Check { config } => {
            let config = load_config(&config)?;
            let _upstream_tls = tls::build_upstream_client_config(&config.tls)?;
            if config.server.tls {
                let _server_tls = tls::load_server_config(&config.tls)?;
            }
            println!("configuration is valid");
        }
        Command::Version => println!(
            "relay {} ({})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::ARCH
        ),
        Command::Run { config: path } => {
            tracing_subscriber::fmt()
                .with_env_filter(
                    tracing_subscriber::EnvFilter::try_from_default_env()
                        .unwrap_or_else(|_| "info".into()),
                )
                .with_target(false)
                .init();
            let config = Arc::new(load_config(&path)?);
            let upstream_tls = tls::build_upstream_client_config(&config.tls)?;
            let server_tls = if config.server.tls {
                Some(tls::load_server_config(&config.tls)?)
            } else {
                None
            };
            let workers = config.server.workers;
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(workers)
                .enable_all()
                .build()?
                .block_on(async move {
                    let listener = TcpListener::bind(config.server.listen).await?;
                    tracing::info!(address = %listener.local_addr()?, tls = config.server.tls, workers, "Relay listening");
                    proxy::serve_with_tls(
                        listener,
                        config,
                        server_tls,
                        upstream_tls,
                        shutdown_signal(),
                    )
                    .await
                })?;
        }
    }
    Ok(())
}

fn load_config(path: &Path) -> Result<Config, Box<dyn std::error::Error>> {
    const MAX_CONFIG_BYTES: u64 = 1024 * 1024;
    let file = File::open(path)?;
    let mut reader = file.take(MAX_CONFIG_BYTES + 1);
    let mut text = String::new();
    reader.read_to_string(&mut text)?;
    if text.len() as u64 > MAX_CONFIG_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "configuration file exceeds 1 MiB",
        )
        .into());
    }
    Ok(Config::parse(&text)?)
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut terminate = signal(SignalKind::terminate()).expect("install SIGTERM handler");
        tokio::select! { _ = tokio::signal::ctrl_c() => {}, _ = terminate.recv() => {} }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}
