//! `schwitzer-server`: hosts schwitzer rooms. Prints `server url: <url>` once started.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::Parser;
use schwitzer_server::{Options, RELEASES_URL, start};
use tracing_subscriber::EnvFilter;

#[derive(Parser)]
#[command(name = "schwitzer-server", version, about = "schwitzer server")]
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
    let opts = Options {
        listen: cli.listen,
        public_host: cli.public_host,
        data_dir: cli.data_dir,
        releases_url: Some(cli.releases_url),
    };
    let mut server = match start(opts).await {
        Ok(server) => server,
        Err(e) => {
            eprintln!("error: {e:#}");
            return ExitCode::FAILURE;
        }
    };
    println!("server url: {}", server.addr);
    // None of the endpoint tasks is meant to end: a server that lost one would
    // keep answering the rest, so it exits and is restarted.
    let task = server.ended().await;
    tracing::error!("server task {task} ended");
    ExitCode::FAILURE
}
