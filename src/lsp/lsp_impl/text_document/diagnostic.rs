//! Diagnostic method for Kakehashi.
//!
//! Implements ADR-0020 Phase 1: Pull-first diagnostic forwarding.
//! Sprint 17: Multi-region diagnostic aggregation with parallel fan-out.
//!
//! # Cancel Handling
//!
//! This module supports immediate cancellation of diagnostic requests:
//! - When `$/cancelRequest` is received, the handler aborts and returns `RequestCancelled`
//! - The JoinSet is dropped, aborting all spawned downstream tasks
//! - Best-effort cancel forwarding to downstream servers (fire-and-forget via middleware)
//!
//! This is achieved using `tokio::select!` to race between:
//! 1. Cancel notification (via `CancelForwarder::subscribe()`)
//! 2. Result aggregation (collecting from all downstream tasks)

use std::sync::Arc;
use std::time::Duration;

use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, MessageType, RelatedFullDocumentDiagnosticReport,
};
use url::Url;

use crate::config::settings::BridgeServerConfig;
use crate::language::InjectionResolver;
use crate::lsp::bridge::{LanguageServerPool, UpstreamId};
use crate::lsp::get_current_request_id;

use super::super::{Kakehashi, uri_to_url};

/// Per-request timeout for diagnostic fan-out (ADR-0020)
const DIAGNOSTIC_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Information needed to send a diagnostic request for one injection region.
///
/// This struct captures all the data required to make a single diagnostic request
/// to a downstream language server. It's used during parallel fan-out (Sprint 17)
/// where multiple injection regions are processed concurrently.
///
/// Uses Arc for config to avoid cloning large structs for each region - multiple
/// regions using the same language server share the same config Arc.
struct DiagnosticRequestInfo {
    /// Name of the downstream language server (e.g., "lua-language-server")
    server_name: String,
    /// Shared reference to the bridge server configuration
    config: Arc<BridgeServerConfig>,
    /// Language of the injection region (e.g., "lua", "python")
    injection_language: String,
    /// Unique identifier for this injection region within the host document
    region_id: String,
    /// Starting line of the injection region in the host document (0-indexed)
    /// Used to transform diagnostic positions back to host coordinates
    region_start_line: u32,
    /// The extracted content of the injection region, formatted as a virtual document
    /// This is what gets sent to the downstream language server for analysis
    virtual_content: String,
}

impl Kakehashi {
    pub(crate) async fn diagnostic_impl(
        &self,
        params: DocumentDiagnosticParams,
    ) -> Result<DocumentDiagnosticReportResult> {
        let lsp_uri = params.text_document.uri;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in diagnostic: {}", lsp_uri.as_str());
            return Ok(empty_diagnostic_report());
        };

        // Use LOG level (lowest severity) for per-request logging in hot path
        // to avoid flooding client with INFO messages on frequent diagnostic requests
        self.client
            .log_message(
                MessageType::LOG,
                format!("textDocument/diagnostic called for {}", uri),
            )
            .await;

        // Get document snapshot (minimizes lock duration)
        let (snapshot, missing_message) = match self.documents.get(&uri) {
            None => (None, Some("No document found")),
            Some(doc) => match doc.snapshot() {
                None => (None, Some("Document not fully initialized")),
                Some(snapshot) => (Some(snapshot), None),
            },
            // doc automatically dropped here, lock released
        };
        if let Some(message) = missing_message {
            self.client.log_message(MessageType::INFO, message).await;
            return Ok(empty_diagnostic_report());
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "kakehashi::diagnostic", "No language detected");
            return Ok(empty_diagnostic_report());
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(empty_diagnostic_report());
        };

        // Collect all injection regions
        let all_regions = InjectionResolver::resolve_all(
            &self.language,
            self.bridge.region_id_tracker(),
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
        );

        if all_regions.is_empty() {
            return Ok(empty_diagnostic_report());
        }

        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        //
        // Note: All parallel diagnostic requests for different injection regions share the
        // same upstream_request_id. This is intentional - when the client cancels a diagnostic
        // request, the handler returns RequestCancelled immediately and drops the JoinSet,
        // which aborts all spawned downstream tasks.
        //
        // Cancel handling flow:
        // 1. Client sends $/cancelRequest with request ID
        // 2. RequestIdCapture middleware notifies our cancel_rx via CancelForwarder
        // 3. tokio::select! triggers cancel branch, drops JoinSet (aborts all tasks)
        // 4. Handler returns RequestCancelled error to client
        // 5. Middleware forwards cancel to downstream servers (best-effort, fire-and-forget)
        //
        // LIMITATION (downstream cancel propagation):
        // The current upstream_request_registry maps each upstream ID to a single server_name.
        // When multiple regions use different downstream servers, only the last registered
        // server receives the cancel notification. This is acceptable because:
        // - Upstream cancel (this handler aborting) works correctly for ALL servers
        // - Downstream cancel forwarding is best-effort per LSP spec
        // - The JoinSet drop aborts tasks regardless of downstream cancel delivery
        // TODO: For full multi-server downstream cancel, refactor registry to HashMap<Id, Vec<String>>
        let upstream_request_id = match get_current_request_id() {
            Some(tower_lsp_server::jsonrpc::Id::Number(n)) => UpstreamId::Number(n),
            Some(tower_lsp_server::jsonrpc::Id::String(s)) => UpstreamId::String(s),
            // For notifications without ID or null ID, use Null to avoid collision with ID 0
            None | Some(tower_lsp_server::jsonrpc::Id::Null) => UpstreamId::Null,
        };

        // Subscribe to cancel notifications for this request
        // The receiver completes when $/cancelRequest arrives for this ID
        let cancel_rx = self
            .bridge
            .cancel_forwarder()
            .subscribe(upstream_request_id.clone());

        // Sprint 17: Process ALL injection regions with parallel fan-out
        // Collect request info for regions that have bridge configs
        // Group by server_name to share Arc<BridgeServerConfig> across regions using the same server
        // Pre-allocate with small capacity (typically 1-2 unique servers per document)
        let mut config_cache: std::collections::HashMap<String, Arc<BridgeServerConfig>> =
            std::collections::HashMap::with_capacity(2);
        let mut request_infos: Vec<DiagnosticRequestInfo> = Vec::with_capacity(all_regions.len());

        for resolved in &all_regions {
            // Get bridge server config for this language
            // The bridge filter is checked inside get_bridge_config_for_language
            if let Some(resolved_config) =
                self.get_bridge_config_for_language(&language_name, &resolved.injection_language)
            {
                // Clone server_name once, use for both HashMap lookup and DiagnosticRequestInfo
                let server_name = resolved_config.server_name.clone();

                // Reuse Arc if we've already seen this server, otherwise create new Arc
                let config_arc = config_cache
                    .entry(server_name.clone())
                    .or_insert_with(|| Arc::new(resolved_config.config.clone()))
                    .clone();

                request_infos.push(DiagnosticRequestInfo {
                    server_name,
                    config: config_arc,
                    injection_language: resolved.injection_language.clone(),
                    region_id: resolved.region.region_id.clone(),
                    region_start_line: resolved.region.line_range.start,
                    virtual_content: resolved.virtual_content.clone(),
                });
            } else {
                log::debug!(
                    target: "kakehashi::diagnostic",
                    "No bridge config for language {}",
                    resolved.injection_language
                );
            }
        }

        if request_infos.is_empty() {
            return Ok(empty_diagnostic_report());
        }

        // Get previous_result_id if provided (for incremental updates)
        // Note: For multi-region, we don't use previous_result_id since each region
        // would need its own tracking. Always request full diagnostics.
        let previous_result_id: Option<&str> = None;

        // Fan-out diagnostic requests to all regions in parallel using JoinSet
        let pool = self.bridge.pool_arc();
        let mut join_set = tokio::task::JoinSet::new();

        for info in request_infos {
            let pool = Arc::clone(&pool);
            let uri = uri.clone();
            let upstream_id = upstream_request_id.clone();

            join_set.spawn(async move {
                send_diagnostic_with_timeout(
                    &pool,
                    &info,
                    &uri,
                    upstream_id,
                    previous_result_id,
                    DIAGNOSTIC_REQUEST_TIMEOUT,
                )
                .await
            });
        }

        // Collect results from all regions, aggregating diagnostics
        // Note: Deduplication by (range, message, severity) is not implemented yet.
        // Per ADR-0020, this would be needed if downstream servers report overlapping
        // diagnostics. Currently each region is isolated so duplicates are unlikely.
        // TODO: Add deduplication when overlapping diagnostics are observed in practice.
        let result = collect_diagnostics_with_cancel(join_set, cancel_rx).await;

        // Clean up cancel subscription (idempotent - no-op if already cancelled)
        self.bridge
            .cancel_forwarder()
            .unsubscribe(&upstream_request_id);

        result
    }
}

/// Collect diagnostics from all regions, aborting immediately if cancelled.
///
/// Uses `tokio::select!` with biased mode to prioritize cancel handling.
/// When cancelled:
/// - Returns `RequestCancelled` error immediately
/// - Drops the JoinSet, which aborts all spawned tasks
///
/// When all regions complete:
/// - Returns aggregated diagnostics from all successful regions
async fn collect_diagnostics_with_cancel(
    mut join_set: tokio::task::JoinSet<Option<Vec<Diagnostic>>>,
    cancel_rx: crate::lsp::request_id::CancelReceiver,
) -> Result<DocumentDiagnosticReportResult> {
    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();

    // Pin the cancel receiver for use in select!
    tokio::pin!(cancel_rx);

    loop {
        tokio::select! {
            // Biased: check cancel first to ensure immediate abort on cancellation
            biased;

            // Cancel notification received - abort immediately
            _ = &mut cancel_rx => {
                log::debug!(
                    target: "kakehashi::diagnostic",
                    "Diagnostic request cancelled, aborting {} remaining tasks",
                    join_set.len()
                );
                // JoinSet dropped here, aborting all spawned tasks
                return Err(Error::request_cancelled());
            }

            // Next task completed - collect result
            result = join_set.join_next() => {
                match result {
                    Some(Ok(Some(diagnostics))) => {
                        all_diagnostics.extend(diagnostics);
                    }
                    Some(Ok(None)) => {
                        // Region returned no diagnostics or failed - continue with others
                    }
                    Some(Err(e)) => {
                        // Task panicked - log and continue
                        log::error!(
                            target: "kakehashi::diagnostic",
                            "Diagnostic task panicked: {}",
                            e
                        );
                    }
                    None => {
                        // All tasks completed - return aggregated results
                        break;
                    }
                }
            }
        }
    }

    // Return aggregated diagnostics
    Ok(DocumentDiagnosticReportResult::Report(
        DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None, // No result_id for aggregated multi-region response
                items: all_diagnostics,
            },
            related_documents: None,
        }),
    ))
}

/// Send a diagnostic request with timeout, returning parsed diagnostics or None on failure.
async fn send_diagnostic_with_timeout(
    pool: &LanguageServerPool,
    info: &DiagnosticRequestInfo,
    uri: &Url,
    upstream_request_id: UpstreamId,
    previous_result_id: Option<&str>,
    timeout: Duration,
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
                target: "kakehashi::diagnostic",
                "Diagnostic request failed for region {}: {}",
                info.region_id,
                e
            );
            return None;
        }
        Err(_) => {
            log::warn!(
                target: "kakehashi::diagnostic",
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

    // Check if it's an "unchanged" report - for multi-region we treat as empty
    // since we can't meaningfully aggregate unchanged reports
    if result.get("kind").and_then(|k| k.as_str()) == Some("unchanged") {
        return Some(Vec::new());
    }

    // Parse as full report with diagnostics
    // The positions have already been transformed by transform_diagnostic_response_to_host
    let items = result.get("items")?;
    serde_json::from_value::<Vec<Diagnostic>>(items.clone()).ok()
}

/// Create an empty diagnostic report (full report with no items).
fn empty_diagnostic_report() -> DocumentDiagnosticReportResult {
    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport {
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None,
                items: Vec::new(),
            },
            related_documents: None,
        },
    ))
}
