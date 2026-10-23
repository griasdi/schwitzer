//! `schwitzer-server`: hosts schwitzer rooms. Prints `server url: <url>` once started.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use schwitzer_release::RELEASES_URL;
use schwitzer_server::{AutoUpdate, Options, Stop, start};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(
    name = "schwitzer-server",
    version = schwitzer_release::VERSION,
    about = "schwitzer server"
)]
struct Cli {
    /// UDP address to listen on.
    #[arg(long, default_value = "0.0.0.0:7444")]
    listen: SocketAddr,
    /// Host name or address clients use to reach this server.
    #[arg(long, default_value = "127.0.0.1")]
    public_host: String,
    /// Directory for the database and the server identity.
    #[arg(long, default_value = "./schwitzer-data")]
    data_dir: PathBuf,
    /// GitHub API URL of the release list the server checks for app updates.
    #[arg(long, default_value = RELEASES_URL)]
    releases_url: String,
    /// Install new releases from GitHub (see --update-channel) and restart into them in place
    /// (Linux x86_64 and aarch64; the program's folder must be writable).
    #[arg(long)]
    auto_update: bool,
    /// With --auto-update: `release` installs new stable releases, `dev` every build of the
    /// rolling `dev` prerelease. Switching installs the other channel's newest build, even an
    /// older one.
    #[arg(long, default_value = "release", requires = "auto_update")]
    update_channel: schwitzer_release::Channel,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    // stdout carries the server url and nothing else.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .init();
    let auto_update = if cli.auto_update {
        match AutoUpdate::prepare(cli.update_channel) {
            Ok(update) => Some(update),
            Err(e) => {
                tracing::error!("--auto-update: {e:#}");
                return ExitCode::FAILURE;
            }
        }
    } else {
        None
    };
    let opts = Options {
        listen: cli.listen,
        public_host: cli.public_host,
        data_dir: cli.data_dir,
        releases_url: Some(cli.releases_url),
        auto_update: auto_update.clone(),
    };
    let mut server = match start(opts).await {
        Ok(server) => server,
        Err(e) => {
            tracing::error!("{e:#}");
            return ExitCode::FAILURE;
        }
    };
    println!("server url: {}", server.addr);
    match server.stopped().await {
        // None of the endpoint tasks is meant to end: a server that lost one would
        // keep answering the rest, so it exits and is restarted.
        Stop::Failed(task) => {
            tracing::error!("server task {task} ended");
            ExitCode::FAILURE
        }
        Stop::Updated(version) => {
            tracing::info!("restarting into {version}");
            // Closes the connections, so clients reconnect at once, and frees the port.
            server.shutdown().await;
            let Some(update) = &auto_update else {
                unreachable!("only --auto-update installs releases");
            };
            let e = update.restart();
            // The new program is in place: a supervisor restarting on failure starts it.
            tracing::error!("cannot restart into {version}: {e}");
            ExitCode::FAILURE
        }
    }
}
