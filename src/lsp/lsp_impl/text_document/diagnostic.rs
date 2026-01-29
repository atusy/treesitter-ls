//! Diagnostic method for Kakehashi.
//!
//! Implements ADR-0020 Phase 1: Pull-first diagnostic forwarding.
//! Sprint 16 scope: First injection region only.

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    Diagnostic, DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    FullDocumentDiagnosticReport, MessageType, RelatedFullDocumentDiagnosticReport,
};

use crate::language::InjectionResolver;
use crate::lsp::bridge::UpstreamId;
use crate::lsp::get_current_request_id;

use super::super::{Kakehashi, uri_to_url};

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

        self.client
            .log_message(
                MessageType::INFO,
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
        let upstream_request_id = match get_current_request_id() {
            Some(tower_lsp_server::jsonrpc::Id::Number(n)) => UpstreamId::Number(n),
            Some(tower_lsp_server::jsonrpc::Id::String(s)) => UpstreamId::String(s),
            // For notifications without ID or null ID, use Null to avoid collision with ID 0
            None | Some(tower_lsp_server::jsonrpc::Id::Null) => UpstreamId::Null,
        };

        // Sprint 16: Process only the FIRST injection region
        // Sprint 17 will iterate through all regions and aggregate
        let resolved = &all_regions[0];

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(resolved_config) =
            self.get_bridge_config_for_language(&language_name, &resolved.injection_language)
        else {
            log::debug!(
                target: "kakehashi::diagnostic",
                "No bridge config for language {}",
                resolved.injection_language
            );
            return Ok(empty_diagnostic_report());
        };

        // Get previous_result_id if provided (for incremental updates)
        let previous_result_id = params.previous_result_id.as_deref();

        // Send diagnostic request via language server pool
        let response = self
            .bridge
            .pool()
            .send_diagnostic_request(
                &resolved_config.server_name,
                &resolved_config.config,
                &uri,
                &resolved.injection_language,
                &resolved.region.region_id,
                resolved.region.line_range.start,
                &resolved.virtual_content,
                upstream_request_id,
                previous_result_id,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the diagnostic response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(empty_diagnostic_report());
                    }

                    // Check if it's an "unchanged" report
                    if result.get("kind").and_then(|k| k.as_str()) == Some("unchanged") {
                        // Return unchanged report - the positions don't need transformation
                        if let Some(result_id) = result.get("resultId").and_then(|r| r.as_str()) {
                            return Ok(DocumentDiagnosticReportResult::Report(
                                DocumentDiagnosticReport::Unchanged(
                                    tower_lsp_server::ls_types::RelatedUnchangedDocumentDiagnosticReport {
                                        unchanged_document_diagnostic_report: tower_lsp_server::ls_types::UnchangedDocumentDiagnosticReport {
                                            result_id: result_id.to_string(),
                                        },
                                        related_documents: None,
                                    },
                                ),
                            ));
                        }
                    }

                    // Parse as full report with diagnostics
                    // The positions have already been transformed by transform_diagnostic_response_to_host
                    // in send_diagnostic_request, so we can directly parse and return
                    if let Some(items) = result.get("items")
                        && let Ok(diagnostics) =
                            serde_json::from_value::<Vec<Diagnostic>>(items.clone())
                    {
                        let result_id = result
                            .get("resultId")
                            .and_then(|r| r.as_str())
                            .map(String::from);

                        return Ok(DocumentDiagnosticReportResult::Report(
                            DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                                    result_id,
                                    items: diagnostics,
                                },
                                related_documents: None,
                            }),
                        ));
                    }
                }
                Ok(empty_diagnostic_report())
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge diagnostic request failed: {}", e),
                    )
                    .await;
                Ok(empty_diagnostic_report())
            }
        }
    }
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
