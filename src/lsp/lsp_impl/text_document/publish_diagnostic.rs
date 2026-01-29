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
use std::time::Duration;

use tower_lsp_server::ls_types::{Diagnostic, Uri};
use url::Url;

use crate::config::settings::BridgeServerConfig;
use crate::language::InjectionResolver;
use crate::lsp::bridge::{LanguageServerPool, UpstreamId};

use super::super::Kakehashi;

/// Per-request timeout for diagnostic fan-out (same as pull diagnostics)
const DIAGNOSTIC_REQUEST_TIMEOUT: Duration = Duration::from_secs(5);

/// Owned diagnostic request info for background tasks.
///
/// This struct owns all its data, allowing it to be moved to a spawned
/// tokio task without lifetime complications. It mirrors the structure
/// of the private `DiagnosticRequestInfo` in `diagnostic.rs`.
///
/// Uses Arc for config to share across multiple regions using the same server.
pub(crate) struct DiagnosticRequestInfoOwned {
    pub(crate) server_name: String,
    pub(crate) config: Arc<BridgeServerConfig>,
    pub(crate) injection_language: String,
    pub(crate) region_id: String,
    pub(crate) region_start_line: u32,
    pub(crate) virtual_content: String,
}

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
                    target: "kakehashi::synthetic_diag",
                    "No diagnostics to collect for {} (no snapshot data)",
                    uri_clone
                );
                return;
            };

            if request_infos.is_empty() {
                log::debug!(
                    target: "kakehashi::synthetic_diag",
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
                target: "kakehashi::synthetic_diag",
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
    fn prepare_diagnostic_snapshot(&self, uri: &Url) -> Option<Vec<DiagnosticRequestInfoOwned>> {
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

        // Build request infos (owned version for background task)
        Some(self.build_diagnostic_request_infos_owned(&language_name, &all_regions))
    }

    /// Build owned DiagnosticRequestInfo for background tasks.
    ///
    /// Similar to `build_diagnostic_request_infos` in `diagnostic.rs` but returns
    /// owned data that can be moved to a background task.
    fn build_diagnostic_request_infos_owned(
        &self,
        language_name: &str,
        all_regions: &[crate::language::injection::ResolvedInjection],
    ) -> Vec<DiagnosticRequestInfoOwned> {
        let mut config_cache: std::collections::HashMap<String, Arc<BridgeServerConfig>> =
            std::collections::HashMap::with_capacity(2);
        let mut request_infos = Vec::with_capacity(all_regions.len());

        for resolved in all_regions {
            if let Some(resolved_config) =
                self.get_bridge_config_for_language(language_name, &resolved.injection_language)
            {
                let server_name = resolved_config.server_name.clone();
                let config_arc = config_cache
                    .entry(server_name.clone())
                    .or_insert_with(|| Arc::new(resolved_config.config.clone()))
                    .clone();

                request_infos.push(DiagnosticRequestInfoOwned {
                    server_name,
                    config: config_arc,
                    injection_language: resolved.injection_language.clone(),
                    region_id: resolved.region.region_id.clone(),
                    region_start_line: resolved.region.line_range.start,
                    virtual_content: resolved.virtual_content.clone(),
                });
            }
        }

        request_infos
    }
}

/// Fan-out diagnostic requests to downstream servers.
///
/// Spawns parallel requests to all injection regions and aggregates results.
/// Uses JoinSet for structured concurrency with automatic cleanup on drop.
async fn fan_out_diagnostic_requests(
    pool: &Arc<LanguageServerPool>,
    uri: &Url,
    request_infos: Vec<DiagnosticRequestInfoOwned>,
) -> Vec<Diagnostic> {
    let mut join_set = tokio::task::JoinSet::new();

    for info in request_infos {
        let pool = Arc::clone(pool);
        let uri = uri.clone();

        join_set.spawn(async move {
            send_diagnostic_with_timeout(&pool, &info, &uri, DIAGNOSTIC_REQUEST_TIMEOUT).await
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
                    target: "kakehashi::publish_diag",
                    "Diagnostic task panicked: {}",
                    e
                );
            }
        }
    }

    all_diagnostics
}

/// Send a diagnostic request with timeout.
///
/// Returns parsed diagnostics or None on failure/timeout.
async fn send_diagnostic_with_timeout(
    pool: &LanguageServerPool,
    info: &DiagnosticRequestInfoOwned,
    uri: &Url,
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
        UpstreamId::Null, // No upstream request for background tasks
        None,             // No previous_result_id
    );

    // Apply timeout per-request (ADR-0020: return partial results on timeout)
    let response = match tokio::time::timeout(timeout, request_future).await {
        Ok(Ok(response)) => response,
        Ok(Err(e)) => {
            log::warn!(
                target: "kakehashi::publish_diag",
                "Diagnostic request failed for region {}: {}",
                info.region_id,
                e
            );
            return None;
        }
        Err(_) => {
            log::warn!(
                target: "kakehashi::publish_diag",
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

    // Check if it's an "unchanged" report - treat as empty for synthetic push
    if result.get("kind").and_then(|k| k.as_str()) == Some("unchanged") {
        return Some(Vec::new());
    }

    // Parse as full report with diagnostics
    let items = result.get("items")?;
    serde_json::from_value::<Vec<Diagnostic>>(items.clone()).ok()
}
