//! Synthetic push diagnostics for ADR-0020 Phase 2.
//!
//! This module handles proactive diagnostic publishing triggered by
//! `didSave` and `didOpen` events. Unlike pull diagnostics (`diagnostic.rs`),
//! these are pushed to the client via `textDocument/publishDiagnostics`.
//!
//! # Architecture
//!
//! ```text
//! didSave/didOpen
//!       │
//!       ▼
//! spawn_synthetic_diagnostic_task()
//!       │
//!       ├─► prepare_diagnostic_snapshot() [sync: extract data]
//!       │
//!       └─► tokio::spawn [async: background task]
//!               │
//!               ▼
//!           fan_out_diagnostic_requests()
//!               │
//!               ▼
//!           client.publish_diagnostics()
//! ```
//!
//! # Superseding Pattern
//!
//! When multiple saves occur rapidly, earlier tasks are aborted via
//! `SyntheticDiagnosticsManager` to prevent stale diagnostics from
//! being published. Only the latest task completes.

use std::sync::Arc;

use tower_lsp_server::ls_types::{Diagnostic, Uri};
use url::Url;

use crate::language::InjectionResolver;
use crate::lsp::bridge::{LanguageServerPool, UpstreamId};

use super::super::Kakehashi;
use super::diagnostic_common::{send_diagnostic_with_timeout, DiagnosticRequestInfo, DIAGNOSTIC_REQUEST_TIMEOUT};

/// Logging target for synthetic push diagnostics.
const LOG_TARGET: &str = "kakehashi::synthetic_diag";

impl Kakehashi {
    /// Spawn a background task to collect and publish diagnostics.
    ///
    /// ADR-0020 Phase 2: Synthetic push on didSave/didOpen.
    ///
    /// The task:
    /// 1. Registers itself with `SyntheticDiagnosticsManager` (superseding any previous task)
    /// 2. Collects diagnostics via fan-out to downstream servers
    /// 3. Publishes diagnostics via `textDocument/publishDiagnostics`
    ///
    /// # Arguments
    /// * `uri` - The document URI (url::Url for internal use)
    /// * `lsp_uri` - The document URI (ls_types::Uri for LSP notification)
    pub(crate) fn spawn_synthetic_diagnostic_task(&self, uri: Url, lsp_uri: Uri) {
        // Clone what we need for the background task
        let client = self.client.clone();

        // Get snapshot data before spawning (extracts all necessary data synchronously)
        let snapshot_data = self.prepare_diagnostic_snapshot(&uri);
        let bridge_pool = self.bridge.pool_arc();
        let uri_clone = uri.clone();

        // Spawn the background task
        let task = tokio::spawn(async move {
            // Collect diagnostics
            let Some(request_infos) = snapshot_data else {
                log::debug!(
                    target: LOG_TARGET,
                    "No diagnostics to collect for {} (no snapshot data)",
                    uri_clone
                );
                return;
            };

            if request_infos.is_empty() {
                log::debug!(
                    target: LOG_TARGET,
                    "No bridge configs for any injection regions in {}",
                    uri_clone
                );
                // Publish empty diagnostics to clear any previous
                client.publish_diagnostics(lsp_uri, Vec::new(), None).await;
                return;
            }

            // Fan-out diagnostic requests
            let diagnostics =
                fan_out_diagnostic_requests(&bridge_pool, &uri_clone, request_infos).await;

            log::debug!(
                target: LOG_TARGET,
                "Collected {} diagnostics for {}",
                diagnostics.len(),
                uri_clone
            );

            // Publish diagnostics
            client.publish_diagnostics(lsp_uri, diagnostics, None).await;
        });

        // Register the task (superseding any previous task for this document)
        self.synthetic_diagnostics
            .register_task(uri, task.abort_handle());
    }

    /// Prepare diagnostic snapshot data for a background task.
    ///
    /// This extracts all necessary data synchronously before spawning,
    /// avoiding lifetime issues with `self` references in async tasks.
    ///
    /// Returns `None` if the document doesn't exist or has no injection regions.
    fn prepare_diagnostic_snapshot(&self, uri: &Url) -> Option<Vec<DiagnosticRequestInfo>> {
        // Get document snapshot
        let snapshot = {
            let doc = self.documents.get(uri)?;
            doc.snapshot()?
        };

        // Get language for document
        let language_name = self.get_language_for_document(uri)?;

        // Get injection query
        let injection_query = self.language.get_injection_query(&language_name)?;

        // Collect all injection regions
        let all_regions = InjectionResolver::resolve_all(
            &self.language,
            self.bridge.region_id_tracker(),
            uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
        );

        if all_regions.is_empty() {
            return Some(Vec::new());
        }

        // Build request infos for background task
        Some(self.build_diagnostic_request_infos(&language_name, &all_regions))
    }
}

/// Fan-out diagnostic requests to downstream servers.
///
/// Spawns parallel requests to all injection regions and aggregates results.
/// Uses JoinSet for structured concurrency with automatic cleanup on drop.
async fn fan_out_diagnostic_requests(
    pool: &Arc<LanguageServerPool>,
    uri: &Url,
    request_infos: Vec<DiagnosticRequestInfo>,
) -> Vec<Diagnostic> {
    let mut join_set = tokio::task::JoinSet::new();

    for info in request_infos {
        let pool = Arc::clone(pool);
        let uri = uri.clone();

        join_set.spawn(async move {
            send_diagnostic_with_timeout(
                &pool,
                &info,
                &uri,
                UpstreamId::Null, // No upstream request for background tasks
                None,             // No previous_result_id
                DIAGNOSTIC_REQUEST_TIMEOUT,
                LOG_TARGET,
            )
            .await
        });
    }

    // Collect results from all regions
    let mut all_diagnostics: Vec<Diagnostic> = Vec::new();
    while let Some(result) = join_set.join_next().await {
        match result {
            Ok(Some(diagnostics)) => all_diagnostics.extend(diagnostics),
            Ok(None) => {}
            Err(e) => {
                log::error!(
                    target: LOG_TARGET,
                    "Diagnostic task panicked: {}",
                    e
                );
            }
        }
    }

    all_diagnostics
}
