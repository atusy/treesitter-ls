//! Goto definition method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn goto_definition_impl(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "goto_definition called for {} at line {} col {}",
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
            log::debug!(target: "treesitter_ls::definition", "No language detected");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::definition", "Language: {}", language_name);

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
            log::debug!(target: "treesitter_ls::definition", "No parse tree");
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
            log::debug!(target: "treesitter_ls::definition", "Failed to convert position to byte");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::definition", "Byte offset: {}", byte_offset);

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
        // TODO(ADR-0012): Re-implement async bridge goto_definition
        // When LanguageServerPool is implemented:
        // 1. Extract virtual content: let virtual_content = cacheable.extract_content(text).to_owned();
        // 2. Translate position: let virtual_position = cacheable.translate_host_to_virtual(position);
        // 3. Call language_server_pool.goto_definition(...)
        // 4. Translate response positions back to host document
        //
        // Bridge functionality is currently disabled
        Ok(None)
    }
}
