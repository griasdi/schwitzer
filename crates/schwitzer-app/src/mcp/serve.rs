//! The listener: binds the loopback port, checks the bearer token and serves the tools while
//! the subscription lives.

use std::hash::{Hash, Hasher};
use std::io::ErrorKind;
use std::net::SocketAddr;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::http::header::{AUTHORIZATION, ORIGIN, WWW_AUTHENTICATE};
use axum::response::IntoResponse;
use futures::{SinkExt, Stream};
use iced::Subscription;
use rmcp::transport::streamable_http_server::session::local::LocalSessionManager;
use rmcp::transport::streamable_http_server::{StreamableHttpServerConfig, StreamableHttpService};
use schwitzer_client::Client;
use tokio_util::sync::CancellationToken;

use super::McpStatus;
use super::tools::Tools;

/// Largest accepted POST body.
const MAX_BODY_BYTES: usize = 256 * 1024;

/// Pause between bind attempts while the port cannot be bound.
const BIND_RETRY: Duration = Duration::from_secs(10);

/// Identity of the server subscription: a new port or token restarts it. The client is not
/// part of the identity (one per process). No `Debug`, so the token never prints.
#[derive(Clone)]
struct McpKey {
    client: Client,
    port: u16,
    token: String,
}

impl Hash for McpKey {
    fn hash<H: Hasher>(&self, h: &mut H) {
        0x7363_6d70_u32.hash(h);
        self.port.hash(h);
        self.token.hash(h);
    }
}

/// The MCP server as an iced subscription; it reports its status and serves until dropped.
pub fn subscription(client: Client, port: u16, token: String) -> Subscription<McpStatus> {
    Subscription::run_with(
        McpKey {
            client,
            port,
            token,
        },
        serve,
    )
}

/// Runs the MCP server while the stream lives. Dropping it closes the listener and cancels
/// every session.
fn serve(key: &McpKey) -> impl Stream<Item = McpStatus> + use<> {
    let key = key.clone();
    iced::stream::channel(8, async move |mut out| {
        let addr = SocketAddr::from(([127, 0, 0, 1], key.port));
        // A port that stays taken is reported once per distinct status; the server keeps
        // trying and comes up by itself when the port is free.
        let mut reported = None;
        let listener = loop {
            match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => break listener,
                Err(e) => {
                    let status = bind_failure(&e, key.port);
                    if reported.as_ref() != Some(&status) {
                        let _ = out.send(status.clone()).await;
                        reported = Some(status);
                    }
                    tokio::time::sleep(BIND_RETRY).await;
                }
            }
        };
        let ct = CancellationToken::new();
        let _stop = ct.clone().drop_guard();
        let tools = Tools::new(key.client.clone());
        // The default config keeps rmcp's loopback Host allowlist.
        let service = StreamableHttpService::new(
            move || Ok(tools.clone()),
            LocalSessionManager::default().into(),
            StreamableHttpServerConfig::default()
                .with_cancellation_token(ct.child_token())
                .with_max_request_body_bytes(MAX_BODY_BYTES),
        );
        let router = axum::Router::new().route_service("/mcp", service).layer(
            axum::middleware::from_fn_with_state(blake3::hash(key.token.as_bytes()), guard),
        );
        // The port the listener really has: it differs from `key.port` only for port 0.
        let bound = listener.local_addr().map_or(key.port, |a| a.port());
        let _ = out.send(McpStatus::Listening { url: url(bound) }).await;
        let served = axum::serve(listener, router)
            .with_graceful_shutdown(ct.clone().cancelled_owned())
            .await;
        if let Err(e) = served {
            let _ = out
                .send(McpStatus::Failed(format!("port {}: {e}", key.port)))
                .await;
        }
        std::future::pending::<()>().await
    })
}

/// The status a failed bind of `port` shows.
fn bind_failure(e: &std::io::Error, port: u16) -> McpStatus {
    if e.kind() == ErrorKind::AddrInUse {
        McpStatus::PortInUse
    } else {
        McpStatus::Failed(format!("port {port}: {e}"))
    }
}

/// Refuses browsers (any `Origin`) and requests without the access token.
async fn guard(
    State(expected): State<blake3::Hash>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    if req.headers().contains_key(ORIGIN) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // blake3::Hash compares in constant time.
    let authorized = req
        .headers()
        .get(AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "))
        .is_some_and(|token| blake3::hash(token.as_bytes()) == expected);
    if !authorized {
        return (StatusCode::UNAUTHORIZED, [(WWW_AUTHENTICATE, "Bearer")]).into_response();
    }
    next.run(req).await
}

/// The address MCP clients connect to.
fn url(port: u16) -> String {
    format!("http://127.0.0.1:{port}/mcp")
}

/// The configuration snippet for an MCP client.
pub fn client_config(port: u16, token: &str) -> String {
    let config = serde_json::json!({
        "mcpServers": {
            "schwitzer": {
                "type": "http",
                "url": url(port),
                "headers": { "Authorization": format!("Bearer {token}") },
            }
        }
    });
    serde_json::to_string_pretty(&config).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_port_reads_as_in_use_and_other_errors_fail() {
        let taken = std::io::Error::from(ErrorKind::AddrInUse);
        assert_eq!(bind_failure(&taken, 1), McpStatus::PortInUse);
        let denied = std::io::Error::from(ErrorKind::PermissionDenied);
        assert!(
            matches!(bind_failure(&denied, 80), McpStatus::Failed(m) if m.starts_with("port 80: "))
        );
    }

    #[test]
    fn the_client_config_names_the_url_and_the_bearer() {
        let config: serde_json::Value = serde_json::from_str(&client_config(13400, "tok")).unwrap();
        let server = &config["mcpServers"]["schwitzer"];
        assert_eq!(server["url"], "http://127.0.0.1:13400/mcp");
        assert_eq!(server["headers"]["Authorization"], "Bearer tok");
    }
}
