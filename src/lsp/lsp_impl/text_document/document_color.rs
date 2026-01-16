//! Document color method for TreeSitterLs.

use tower_lsp::jsonrpc::{Id, Result};
use tower_lsp::lsp_types::*;

use crate::language::InjectionResolver;
use crate::lsp::get_current_request_id;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn document_color_impl(
        &self,
        params: DocumentColorParams,
    ) -> Result<Vec<ColorInformation>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(
                MessageType::INFO,
                format!("documentColor called for {}", uri),
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
            return Ok(Vec::new());
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "tree_sitter_ls::document_color", "No language detected");
            return Ok(Vec::new());
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(Vec::new());
        };

        // Collect all injection regions
        let all_regions = InjectionResolver::resolve_all(
            &self.region_id_tracker,
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
        );

        if all_regions.is_empty() {
            return Ok(Vec::new());
        }

        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => n,
            // For string IDs or no ID, use 0 as fallback
            _ => 0,
        };

        // Collect colors from all injection regions
        let mut all_colors: Vec<ColorInformation> = Vec::new();

        for resolved in all_regions {
            // Get bridge server config for this language
            // The bridge filter is checked inside get_bridge_config_for_language
            let Some(server_config) =
                self.get_bridge_config_for_language(&language_name, &resolved.injection_language)
            else {
                continue; // No bridge configured for this language
            };

            // Send document color request via language server pool
            let response = self
                .language_server_pool
                .send_document_color_request(
                    &server_config,
                    &uri,
                    &resolved.injection_language,
                    &resolved.region.result_id,
                    resolved.region.line_range.start,
                    &resolved.virtual_content,
                    upstream_request_id,
                )
                .await;

            match response {
                Ok(json_response) => {
                    // Parse the document color response
                    if let Some(result) = json_response.get("result") {
                        if result.is_null() {
                            continue;
                        }

                        // Parse the result into Vec<ColorInformation>
                        if let Ok(colors) =
                            serde_json::from_value::<Vec<ColorInformation>>(result.clone())
                        {
                            all_colors.extend(colors);
                        }
                    }
                }
                Err(e) => {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("Bridge document color request failed: {}", e),
                        )
                        .await;
                }
            }
        }

        Ok(all_colors)
    }
}
