//! Downstream message handler for notification forwarding.
//!
//! This module implements the handler task that receives notifications from
//! downstream language servers (via mpsc channel from Reader Tasks) and
//! forwards them to the client with appropriate transformations.
//!
//! # Architecture (ADR-0016)
//!
//! ```text
//! Reader Task 1 ─┐
//! Reader Task 2 ─┼──► mpsc::Receiver<DownstreamMessage> ──► Handler ──► Client
//! Reader Task N ─┘
//! ```
//!
//! # Current Scope (Phase 0)
//!
//! - `$/progress`: Forward with `server_name:` prefix on token
//!
//! # Future Phases
//!
//! - Phase 1: `textDocument/publishDiagnostics` with URI/position transformation
//! - Phase 5: `window/showMessage`, `window/logMessage`

use log::{debug, error, warn};
use serde_json::Value;
use tokio::sync::mpsc;
use tokio::task::JoinHandle;
use tower_lsp_server::Client;
use tower_lsp_server::ls_types::notification::Progress;
use tower_lsp_server::ls_types::ProgressParams;

use super::{DownstreamMessage, DownstreamNotification};

/// Handle to a running DownstreamMessageHandler task.
///
/// The handle wraps a supervisor task that monitors the handler for panics.
/// If the handler panics, the supervisor logs the error at ERROR level.
///
/// Dropping this handle will cause the channel sender to be orphaned,
/// which will eventually terminate the handler loop when the last
/// sender is dropped.
pub(crate) struct DownstreamHandlerHandle {
    /// Join handle for the supervisor task (which monitors the handler task).
    _supervisor_handle: JoinHandle<()>,
}

/// Spawn a downstream message handler task with supervisor.
///
/// The handler receives notifications from the channel and forwards them
/// to the client with appropriate transformations. A supervisor task monitors
/// the handler and logs any panics at ERROR level.
///
/// # Arguments
/// * `rx` - The receiving end of the downstream message channel
/// * `client` - The LSP client for sending notifications
///
/// # Returns
/// A handle to the spawned supervisor task.
pub(crate) fn spawn_downstream_handler(
    rx: mpsc::Receiver<DownstreamMessage>,
    client: Client,
) -> DownstreamHandlerHandle {
    // Spawn the actual handler task
    let handler_handle = tokio::spawn(handler_loop(rx, client));

    // Spawn a supervisor that awaits the handler and logs panics
    let supervisor_handle = tokio::spawn(async move {
        match handler_handle.await {
            Ok(()) => {
                debug!(
                    target: "kakehashi::bridge::downstream_handler",
                    "Downstream handler exited normally"
                );
            }
            Err(e) if e.is_panic() => {
                // Extract panic payload for logging
                let panic_info = e.into_panic();
                let panic_msg = if let Some(s) = panic_info.downcast_ref::<&str>() {
                    (*s).to_string()
                } else if let Some(s) = panic_info.downcast_ref::<String>() {
                    s.clone()
                } else {
                    "unknown panic payload".to_string()
                };
                error!(
                    target: "kakehashi::bridge::downstream_handler",
                    "Downstream handler PANICKED: {}",
                    panic_msg
                );
            }
            Err(e) => {
                // Task was cancelled (e.g., via abort())
                warn!(
                    target: "kakehashi::bridge::downstream_handler",
                    "Downstream handler was cancelled: {}",
                    e
                );
            }
        }
    });

    DownstreamHandlerHandle {
        _supervisor_handle: supervisor_handle,
    }
}

/// The main handler loop - receives and processes downstream messages.
async fn handler_loop(mut rx: mpsc::Receiver<DownstreamMessage>, client: Client) {
    while let Some(message) = rx.recv().await {
        match message {
            DownstreamMessage::Notification(notification) => {
                handle_notification(notification, &client).await;
            }
        }
    }

    debug!(
        target: "kakehashi::bridge::downstream_handler",
        "Downstream handler loop exited (channel closed)"
    );
}

/// Handle a notification from a downstream language server.
async fn handle_notification(notification: DownstreamNotification, client: &Client) {
    let method = notification
        .notification
        .get("method")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    match method {
        "$/progress" => {
            handle_progress(notification, client).await;
        }
        // Phase 1 will add: "textDocument/publishDiagnostics"
        // Phase 5 will add: "window/showMessage", "window/logMessage"
        _ => {
            debug!(
                target: "kakehashi::bridge::downstream_handler",
                "Ignoring notification from {}: {}",
                notification.server_name,
                method
            );
        }
    }
}

/// Handle a `$/progress` notification by prefixing the token with server name.
///
/// The token is prefixed with `{server_name}:` to allow clients to distinguish
/// progress notifications from different downstream servers.
///
/// For example, token `"abc123"` from `lua-language-server` becomes
/// `"lua-language-server:abc123"`.
async fn handle_progress(notification: DownstreamNotification, client: &Client) {
    let params = match notification.notification.get("params") {
        Some(params) => params.clone(),
        None => {
            warn!(
                target: "kakehashi::bridge::downstream_handler",
                "Progress notification from {} missing params",
                notification.server_name
            );
            return;
        }
    };

    // Extract and prefix the token
    let prefixed_params = match prefix_progress_token(params, &notification.server_name) {
        Some(p) => p,
        None => {
            warn!(
                target: "kakehashi::bridge::downstream_handler",
                "Progress notification from {} has invalid token format",
                notification.server_name
            );
            return;
        }
    };

    // Parse into ProgressParams and send
    match serde_json::from_value::<ProgressParams>(prefixed_params) {
        Ok(progress_params) => {
            client.send_notification::<Progress>(progress_params).await;
            debug!(
                target: "kakehashi::bridge::downstream_handler",
                "Forwarded $/progress from {}",
                notification.server_name
            );
        }
        Err(e) => {
            warn!(
                target: "kakehashi::bridge::downstream_handler",
                "Failed to parse progress params from {}: {}",
                notification.server_name,
                e
            );
        }
    }
}

/// Prefix the token in a progress params JSON value with the server name.
///
/// Returns `None` if the token field is missing or has an unexpected format.
fn prefix_progress_token(mut params: Value, server_name: &str) -> Option<Value> {
    let token = params.get("token")?;

    let prefixed_token: Value = match token {
        Value::String(s) => Value::String(format!("{}:{}", server_name, s)),
        Value::Number(n) => Value::String(format!("{}:{}", server_name, n)),
        _ => return None,
    };

    params
        .as_object_mut()?
        .insert("token".to_string(), prefixed_token);

    Some(params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn prefix_progress_token_with_string_token() {
        let params = json!({
            "token": "abc123",
            "value": { "kind": "begin", "title": "Indexing" }
        });

        let result = prefix_progress_token(params, "lua-ls").expect("should prefix");

        assert_eq!(result["token"], "lua-ls:abc123");
        // Other fields should be preserved
        assert_eq!(result["value"]["kind"], "begin");
    }

    #[test]
    fn prefix_progress_token_with_number_token() {
        let params = json!({
            "token": 42,
            "value": { "kind": "end" }
        });

        let result = prefix_progress_token(params, "pyright").expect("should prefix");

        assert_eq!(result["token"], "pyright:42");
    }

    #[test]
    fn prefix_progress_token_missing_token() {
        let params = json!({
            "value": { "kind": "begin" }
        });

        let result = prefix_progress_token(params, "server");
        assert!(result.is_none());
    }

    #[test]
    fn prefix_progress_token_invalid_token_type() {
        let params = json!({
            "token": { "nested": "object" },
            "value": { "kind": "begin" }
        });

        let result = prefix_progress_token(params, "server");
        assert!(result.is_none());
    }

    #[tokio::test]
    async fn handler_processes_notifications_from_channel() {
        let (tx, rx) = mpsc::channel(10);

        // Create a notification
        let notification = DownstreamNotification {
            server_name: "test-server".to_string(),
            notification: json!({
                "jsonrpc": "2.0",
                "method": "$/progress",
                "params": {
                    "token": "test-token",
                    "value": { "kind": "begin", "title": "Test" }
                }
            }),
        };

        // Send the notification
        tx.send(DownstreamMessage::Notification(notification))
            .await
            .expect("should send");

        // Drop the sender to close the channel
        drop(tx);

        // We can't easily test the full handler loop without a real Client,
        // but we can verify the channel mechanics work.
        // The actual notification forwarding is tested via integration tests.

        // Receive the message manually to verify it was sent correctly
        let mut test_rx = rx;
        let received = test_rx.recv().await.expect("should receive");

        match received {
            DownstreamMessage::Notification(n) => {
                assert_eq!(n.server_name, "test-server");
                assert_eq!(n.notification["method"], "$/progress");
            }
        }
    }
}
