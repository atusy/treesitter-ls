//! Document highlight method for TreeSitterLs.

use tower_lsp::jsonrpc::{Id, Result};
use tower_lsp::lsp_types::*;

use crate::language::InjectionResolver;
use crate::lsp::get_current_request_id;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn document_highlight_impl(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "documentHighlight called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
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
            return Ok(None);
        }
        let snapshot = snapshot.expect("snapshot set when missing_message is None");

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "kakehashi::document_highlight", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Resolve injection region at position (centralizes 29-86 lines of duplication)
        let mapper = PositionMapper::new(snapshot.text());
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            return Ok(None);
        };

        let Some(resolved) = InjectionResolver::resolve_at_byte_offset(
            &self.region_id_tracker,
            &uri,
            snapshot.tree(),
            snapshot.text(),
            injection_query.as_ref(),
            byte_offset,
        ) else {
            // Not in an injection region - return None
            return Ok(None);
        };

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &resolved.injection_language)
        else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "No bridge server configured for language: {} (host: {})",
                        resolved.injection_language, language_name
                    ),
                )
                .await;
            return Ok(None);
        };

        // Send document highlight request via language server pool
        // Get upstream request ID from task-local storage (set by RequestIdCapture middleware)
        let upstream_request_id = match get_current_request_id() {
            Some(Id::Number(n)) => n,
            // For string IDs or no ID, use 0 as fallback
            _ => 0,
        };
        let response = self
            .language_server_pool
            .send_document_highlight_request(
                &server_config,
                &uri,
                position,
                &resolved.injection_language,
                &resolved.region.result_id,
                resolved.region.line_range.start,
                &resolved.virtual_content,
                upstream_request_id,
            )
            .await;

        match response {
            Ok(json_response) => {
                // Parse the document highlight response
                if let Some(result) = json_response.get("result") {
                    if result.is_null() {
                        return Ok(None);
                    }

                    // Parse the result into Vec<DocumentHighlight>
                    if let Ok(highlights) =
                        serde_json::from_value::<Vec<DocumentHighlight>>(result.clone())
                    {
                        return Ok(Some(highlights));
                    }
                }
                Ok(None)
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Bridge document highlight request failed: {}", e),
                    )
                    .await;
                Ok(None)
            }
        }
    }
}
