//! Pull diagnostics for Kakehashi (textDocument/diagnostic).
//!
//! Implements ADR-0020 Phase 1: Pull-first diagnostic forwarding.
//! Sprint 17: Multi-region diagnostic aggregation with parallel fan-out.
//!
//! For synthetic push diagnostics (publishDiagnostics), see `publish_diagnostic.rs`.
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

use tower_lsp_server::jsonrpc::{Error, Result};
use tower_lsp_server::ls_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, MessageType, RelatedFullDocumentDiagnosticReport,
};

use crate::config::settings::BridgeServerConfig;
use crate::language::InjectionResolver;
use crate::lsp::bridge::UpstreamId;
use crate::lsp::get_current_request_id;
use crate::lsp::request_id::CancelForwarder;

use super::super::{Kakehashi, uri_to_url};
use super::diagnostic_common::{send_diagnostic_with_timeout, DiagnosticRequestInfo, DIAGNOSTIC_REQUEST_TIMEOUT};

/// RAII guard that ensures cancel subscription is cleaned up on drop.
///
/// This guard automatically calls `unsubscribe()` when dropped, preventing
/// subscription leaks on early return paths. The unsubscribe is idempotent,
/// so it's safe to call even after the subscription was already cleaned up
/// by cancel notification.
struct CancelSubscriptionGuard<'a> {
    cancel_forwarder: &'a CancelForwarder,
    upstream_id: UpstreamId,
}

impl<'a> CancelSubscriptionGuard<'a> {
    fn new(cancel_forwarder: &'a CancelForwarder, upstream_id: UpstreamId) -> Self {
        Self {
            cancel_forwarder,
            upstream_id,
        }
    }
}

impl Drop for CancelSubscriptionGuard<'_> {
    fn drop(&mut self) {
        self.cancel_forwarder.unsubscribe(&self.upstream_id);
    }
}

/// Logging target for pull diagnostics.
const LOG_TARGET: &str = "kakehashi::diagnostic";

impl Kakehashi {
    /// Build DiagnosticRequestInfo for all injection regions that have bridge configs.
    ///
    /// Groups by server_name to share Arc<BridgeServerConfig> across regions using the same server.
    ///
    /// This is `pub(super)` for use by both pull diagnostics and synthetic push diagnostics.
    pub(super) fn build_diagnostic_request_infos(
        &self,
        language_name: &str,
        all_regions: &[crate::language::injection::ResolvedInjection],
    ) -> Vec<DiagnosticRequestInfo> {
        // Pre-allocate with small capacity (typically 1-2 unique servers per document)
        let mut config_cache: std::collections::HashMap<String, Arc<BridgeServerConfig>> =
            std::collections::HashMap::with_capacity(2);
        let mut request_infos: Vec<DiagnosticRequestInfo> = Vec::with_capacity(all_regions.len());

        for resolved in all_regions {
            // Get bridge server config for this language
            // The bridge filter is checked inside get_bridge_config_for_language
            if let Some(resolved_config) =
                self.get_bridge_config_for_language(language_name, &resolved.injection_language)
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

        request_infos
    }

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
        // AlreadySubscribedError indicates a bug (same request ID subscribed twice)
        // - proceed without cancel support rather than failing the request
        //
        // The guard ensures unsubscribe is called on all return paths (including early returns).
        let (cancel_rx, _subscription_guard) = match self
            .bridge
            .cancel_forwarder()
            .subscribe(upstream_request_id.clone())
        {
            Ok(rx) => {
                let guard = CancelSubscriptionGuard::new(
                    self.bridge.cancel_forwarder(),
                    upstream_request_id.clone(),
                );
                (Some(rx), Some(guard))
            }
            Err(e) => {
                log::error!(
                    target: "kakehashi::diagnostic",
                    "Failed to subscribe to cancel notifications for {}: already subscribed. \
                     This is a bug - proceeding without cancel support.",
                    e.0
                );
                (None, None)
            }
        };

        // Build request infos using the factored-out method
        let request_infos = self.build_diagnostic_request_infos(&language_name, &all_regions);

        if request_infos.is_empty() {
            return Ok(empty_diagnostic_report());
        }

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
                    None, // No previous_result_id for multi-region
                    DIAGNOSTIC_REQUEST_TIMEOUT,
                    LOG_TARGET,
                )
                .await
            });
        }

        // Collect results from all regions, aggregating diagnostics
        // Note: Deduplication by (range, message, severity) is not implemented yet.
        // Per ADR-0020, this would be needed if downstream servers report overlapping
        // diagnostics. Currently each region is isolated so duplicates are unlikely.
        // TODO: Add deduplication when overlapping diagnostics are observed in practice.
        //
        // Cleanup: _subscription_guard is dropped here, calling unsubscribe automatically.
        // This ensures cleanup happens on all paths including early returns and panics.
        collect_diagnostics_with_cancel(join_set, cancel_rx).await
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
///
/// If `cancel_rx` is `None`, cancel handling is disabled (graceful degradation
/// when subscription failed due to `AlreadySubscribedError`).
async fn collect_diagnostics_with_cancel(
    mut join_set: tokio::task::JoinSet<Option<Vec<Diagnostic>>>,
    cancel_rx: Option<crate::lsp::request_id::CancelReceiver>,
) -> Result<DocumentDiagnosticReportResult> {
    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();

    // Handle None case: no cancel support, just collect results
    let Some(cancel_rx) = cancel_rx else {
        while let Some(result) = join_set.join_next().await {
            match result {
                Ok(Some(diagnostics)) => all_diagnostics.extend(diagnostics),
                Ok(None) => {}
                Err(e) => {
                    log::error!(
                        target: "kakehashi::diagnostic",
                        "Diagnostic task panicked: {}",
                        e
                    );
                }
            }
        }
        return Ok(make_diagnostic_report(all_diagnostics));
    };

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

    Ok(make_diagnostic_report(all_diagnostics))
}

/// Create a full diagnostic report from aggregated diagnostics.
fn make_diagnostic_report(diagnostics: Vec<Diagnostic>) -> DocumentDiagnosticReportResult {
    DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        RelatedFullDocumentDiagnosticReport {
            full_document_diagnostic_report: FullDocumentDiagnosticReport {
                result_id: None, // No result_id for aggregated multi-region response
                items: diagnostics,
            },
            related_documents: None,
        },
    ))
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
