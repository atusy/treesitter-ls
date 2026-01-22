//! LSP initialize/initialized handshake for downstream servers.
//!
//! This module handles the LSP protocol handshake that establishes a connection
//! with a downstream language server. The handshake follows the LSP specification:
//! 1. Send `initialize` request
//! 2. Wait for `initialize` response
//! 3. Send `initialized` notification

use std::io;

use super::ConnectionHandle;
use crate::lsp::bridge::protocol::{
    RequestId, build_initialize_request, build_initialized_notification, validate_initialize_response,
};

/// Perform the LSP initialize/initialized handshake.
///
/// Sends the initialize request, waits for the response, and sends the
/// initialized notification. This function is called by `get_or_create_connection_with_timeout`
/// after the connection is spawned and the reader task is running.
///
/// # Arguments
/// * `handle` - The connection handle (in Initializing state)
/// * `init_request_id` - Pre-registered request ID (always 1)
/// * `init_response_rx` - Pre-registered receiver for initialize response
/// * `init_options` - Server-specific initialization options
///
/// # Returns
/// * `Ok(())` - Handshake completed successfully
/// * `Err(e)` - Handshake failed (server error, I/O error)
pub(super) async fn perform_lsp_handshake(
    handle: &ConnectionHandle,
    init_request_id: RequestId,
    init_response_rx: tokio::sync::oneshot::Receiver<serde_json::Value>,
    init_options: Option<serde_json::Value>,
) -> io::Result<()> {
    // 1. Build and send initialize request
    let init_request = build_initialize_request(init_request_id, init_options);
    {
        let mut writer = handle.writer().await;
        writer.write_message(&init_request).await?;
    }

    // 2. Wait for initialize response via pre-registered receiver
    let response = init_response_rx
        .await
        .map_err(|_| io::Error::other("bridge: initialize response channel closed"))?;

    // 3. Validate response
    validate_initialize_response(&response)?;

    // 4. Send initialized notification
    let initialized = build_initialized_notification();
    {
        let mut writer = handle.writer().await;
        writer.write_message(&initialized).await?;
    }

    Ok(())
}
