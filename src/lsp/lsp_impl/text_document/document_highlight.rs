//! Document highlight method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
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
                    "document_highlight called for {} at line {} col {}",
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
            log::debug!(target: "treesitter_ls::document_highlight", "No language detected");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::document_highlight", "Language: {}", language_name);

        // Get injection query to detect injection regions
        let Some(injection_query) = self.language.get_injection_query(&language_name) else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!("No injection query for {}", language_name),
                )
                .await;
            return Ok(None);
        };

        // Get the parse tree
        let Some(tree) = doc.tree() else {
            log::debug!(target: "treesitter_ls::document_highlight", "No parse tree");
            return Ok(None);
        };

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            text,
            Some(injection_query.as_ref()),
        );

        let Some(injections) = injections else {
            self.client
                .log_message(MessageType::INFO, "No injections found")
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Found {} injection regions", injections.len()),
            )
            .await;

        // Convert Position to byte offset
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(position) else {
            log::debug!(target: "treesitter_ls::document_highlight", "Failed to convert position to byte");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::document_highlight", "Byte offset: {}", byte_offset);

        // Find which injection region (if any) contains this position
        // Log all regions for debugging
        for inj in &injections {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "Region {} bytes {}..{}, contains {}? {}",
                        inj.language,
                        start,
                        end,
                        byte_offset,
                        byte_offset >= start && byte_offset < end
                    ),
                )
                .await;
        }
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            // Not in an injection region
            self.client
                .log_message(MessageType::INFO, "Position not in any injection region")
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Found matching region: {}", region.language),
            )
            .await;

        // Create cacheable region for position translation
        self.client
            .log_message(MessageType::INFO, "Creating cacheable region...")
            .await;
        let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);

        // Extract virtual document content (own it for the blocking task)
        let virtual_content = cacheable.extract_content(text).to_owned();
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Virtual content ({} chars): {}",
                    virtual_content.len(),
                    &virtual_content[..virtual_content.len().min(50)]
                ),
            )
            .await;

        // Translate host position to virtual position
        let virtual_position = cacheable.translate_host_to_virtual(position);
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Translated position: host line {} -> virtual line {}",
                    position.line, virtual_position.line
                ),
            )
            .await;

        // Get bridge server config for this language
        // The bridge filter is checked inside get_bridge_config_for_language
        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &region.language)
        else {
            self.client
                .log_message(
                    MessageType::INFO,
                    format!(
                        "No bridge server configured for language '{}' (host: {})",
                        region.language, language_name
                    ),
                )
                .await;
            return Ok(None);
        };

        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();
        let has_existing = self.async_language_server_pool.has_connection(&pool_key);
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Getting {} from async pool (existing: {})...",
                    pool_key, has_existing
                ),
            )
            .await;

        // Get shared connection from async pool (multiple callers can use concurrently)
        let conn = match self
            .async_language_server_pool
            .get_connection(&pool_key, &server_config)
            .await
        {
            Some(c) => c,
            None => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to spawn {}", pool_key))
                    .await;
                return Ok(None);
            }
        };

        // Send didOpen with virtual content (non-blocking)
        if let Err(e) = conn.did_open("rust", &virtual_content) {
            self.client
                .log_message(MessageType::ERROR, format!("didOpen failed: {}", e))
                .await;
            return Ok(None);
        }

        // Send document highlight request and await response asynchronously
        let highlights = conn.document_highlight(virtual_position).await;

        self.client
            .log_message(
                MessageType::INFO,
                format!("Document highlight response: {:?}", highlights),
            )
            .await;

        // Translate response positions back to host document
        let Some(highlight_response) = highlights else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "No document highlight response from rust-analyzer",
                )
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Got document highlight response: {:?}", highlight_response),
            )
            .await;

        // Map each highlight's range back to host document, preserving the kind field
        let mapped_highlights: Vec<DocumentHighlight> = highlight_response
            .into_iter()
            .map(|highlight| {
                let mapped_start = cacheable.translate_virtual_to_host(highlight.range.start);
                let mapped_end = cacheable.translate_virtual_to_host(highlight.range.end);
                DocumentHighlight {
                    range: Range {
                        start: mapped_start,
                        end: mapped_end,
                    },
                    kind: highlight.kind, // Preserve Read/Write/Text kind
                }
            })
            .collect();

        Ok(Some(mapped_highlights))
    }
}
