//! Hover method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

/// Message shown when hover returns no result (e.g., during indexing or no info available).
pub(crate) const NO_RESULT_MESSAGE: &str = "No result or indexing";

/// Create an informative hover message for when no result is available.
///
/// This is shown to users when the bridged language server returns no hover
/// result, which can happen during indexing or when no information is available
/// for the current position.
pub(crate) fn create_no_result_hover() -> Hover {
    Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::PlainText,
            value: NO_RESULT_MESSAGE.to_string(),
        }),
        range: None,
    }
}

impl TreeSitterLs {
    pub(crate) async fn hover_impl(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "hover called for {} at line {} col {}",
                    uri, position.line, position.character
                ),
            )
            .await;

        // Get document
        let Some(doc) = self.documents.get(&uri) else {
            self.client
                .log_message(MessageType::INFO, "No document found")
                .await;
            return Ok(None);
        };
        let text = doc.text();

        // Get the language for this document
        let Some(language_name) = self.get_language_for_document(&uri) else {
            log::debug!(target: "treesitter_ls::hover", "No language detected");
            return Ok(None);
        };

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            return Ok(None);
        };

        // Get the parse tree
        let Some(tree) = doc.tree() else {
            return Ok(None);
        };

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            text,
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            return Ok(None);
        };

        // Convert Position to byte offset
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            return Ok(None);
        };

        // Find which injection region (if any) contains this position
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            // Not in an injection region - return None
            return Ok(None);
        };

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &region.language)
        else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "No bridge server configured for language: {} (host: {})",
                        region.language, language_name
                    ),
                )
                .await;
            return Ok(None);
        };

        // Create cacheable region for position translation
        let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);

        // Extract virtual document content
        let virtual_content = cacheable.extract_content(text).to_owned();

        // Translate host position to virtual position
        let virtual_position = cacheable.translate_host_to_virtual(position);

        // Get pool key from config
        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        self.client
            .log_message(
                MessageType::LOG,
                format!("[HOVER] async bridge START pool_key={}", pool_key),
            )
            .await;

        // Use fully async hover via TokioAsyncLanguageServerPool
        // Pass the host document URI for tracking host-to-bridge mapping
        let hover = self
            .tokio_async_pool
            .hover(
                &pool_key,
                &server_config,
                uri.as_str(),
                &region.language,
                &virtual_content,
                virtual_position,
            )
            .await;

        self.client
            .log_message(
                MessageType::LOG,
                format!("[HOVER] async bridge DONE has_hover={}", hover.is_some()),
            )
            .await;

        // Translate hover response range back to host document (if present)
        // If no hover result, return informative message instead of None (PBI-147)
        let Some(mut hover_response) = hover else {
            return Ok(Some(create_no_result_hover()));
        };

        // Translate the range if present
        if let Some(range) = hover_response.range {
            let mapped_start = cacheable.translate_virtual_to_host(range.start);
            let mapped_end = cacheable.translate_virtual_to_host(range.end);
            hover_response.range = Some(Range {
                start: mapped_start,
                end: mapped_end,
            });
        }

        Ok(Some(hover_response))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that create_no_result_hover returns Hover with informative message.
    ///
    /// PBI-147 Subtask 1: When bridged LSP returns None for hover,
    /// hover_impl should return an informative message instead of None.
    #[test]
    fn create_no_result_hover_returns_informative_message() {
        let hover = create_no_result_hover();

        // Verify the message contains "No result or indexing"
        match hover.contents {
            HoverContents::Markup(markup) => {
                assert!(
                    markup.value.contains("No result or indexing"),
                    "Hover message should contain 'No result or indexing', got: {}",
                    markup.value
                );
                assert_eq!(markup.kind, MarkupKind::PlainText);
            }
            _ => panic!("Expected MarkupContent, got different variant"),
        }

        // Verify no range is set
        assert!(
            hover.range.is_none(),
            "No range should be set for fallback hover"
        );
    }

    /// Test that NO_RESULT_MESSAGE constant has the expected value.
    #[test]
    fn no_result_message_constant_is_correct() {
        assert_eq!(NO_RESULT_MESSAGE, "No result or indexing");
    }
}
