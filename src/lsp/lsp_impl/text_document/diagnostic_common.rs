//! Shared utilities for diagnostic handling (ADR-0020).
//!
//! This module contains constants and helper functions shared between
//! pull diagnostics (`diagnostic.rs`) and synthetic push diagnostics
//! (`publish_diagnostic.rs`).

use std::sync::Arc;
use std::time::Duration;

use tower_lsp_server::ls_types::Diagnostic;
use url::Url;

use crate::config::settings::BridgeServerConfig;
use crate::lsp::bridge::{LanguageServerPool, UpstreamId};

/// Per-request timeout for diagnostic fan-out (ADR-0020).
///
/// Used by both pull diagnostics (textDocument/diagnostic) and
/// synthetic push diagnostics (didSave/didOpen triggered).
pub(super) const DIAGNOSTIC_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Information needed to send a diagnostic request for one injection region.
///
/// This struct captures all the data required to make a single diagnostic request
/// to a downstream language server. It's used during parallel fan-out where
/// multiple injection regions are processed concurrently.
///
/// Uses Arc for config to avoid cloning large structs for each region - multiple
/// regions using the same language server share the same config Arc.
pub(super) struct DiagnosticRequestInfo {
    /// Name of the downstream language server (e.g., "lua-language-server")
    pub(super) server_name: String,
    /// Shared reference to the bridge server configuration
    pub(super) config: Arc<BridgeServerConfig>,
    /// Language of the injection region (e.g., "lua", "python")
    pub(super) injection_language: String,
    /// Unique identifier for this injection region within the host document
    pub(super) region_id: String,
    /// Starting line of the injection region in the host document (0-indexed)
    /// Used to transform diagnostic positions back to host coordinates
    pub(super) region_start_line: u32,
    /// The extracted content of the injection region, formatted as a virtual document
    /// This is what gets sent to the downstream language server for analysis
    pub(super) virtual_content: String,
}

/// Send a diagnostic request with timeout, returning parsed diagnostics or None on failure.
///
/// This is the shared implementation used by both pull and push diagnostics.
/// It handles timeout, error logging, and response parsing.
///
/// # Arguments
/// * `pool` - The language server pool for sending requests
/// * `info` - Request info containing server details and region data
/// * `uri` - The host document URI
/// * `upstream_request_id` - The upstream request ID for cancel forwarding
/// * `previous_result_id` - Optional result ID for unchanged detection
/// * `timeout` - Request timeout duration
/// * `log_target` - Logging target (e.g., "kakehashi::diagnostic")
pub(super) async fn send_diagnostic_with_timeout(
    pool: &LanguageServerPool,
    info: &DiagnosticRequestInfo,
    uri: &Url,
    upstream_request_id: UpstreamId,
    previous_result_id: Option<&str>,
    timeout: Duration,
    log_target: &str,
) -> Option<Vec<Diagnostic>> {
    let request_future = pool.send_diagnostic_request(
        &info.server_name,
        &info.config,
        uri,
        &info.injection_language,
        &info.region_id,
        info.region_start_line,
        &info.virtual_content,
        upstream_request_id,
        previous_result_id,
    );

    // Apply timeout per-request (ADR-0020: return partial results on timeout)
    let response = match tokio::time::timeout(timeout, request_future).await {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => {
            log::warn!(
                target: log_target,
                "Diagnostic request failed for region {}: {}",
                info.region_id,
                e
            );
            return None;
        }
        Err(_) => {
            log::warn!(
                target: log_target,
                "Diagnostic request timed out for region {} after {:?}",
                info.region_id,
                timeout
            );
            return None;
        }
    };

    // Parse the diagnostic response
    let result = response.get("result")?;
    if result.is_null() {
        return Some(Vec::new());
    }

    // Check if it's an "unchanged" report - treat as empty for aggregation
    // since we can't meaningfully aggregate unchanged reports
    if result.get("kind").and_then(|k| k.as_str()) == Some("unchanged") {
        return Some(Vec::new());
    }

    // Parse as full report with diagnostics
    // The positions have already been transformed by transform_diagnostic_response_to_host
    let items = result.get("items")?;
    serde_json::from_value::<Vec<Diagnostic>>(items.clone()).ok()
}
