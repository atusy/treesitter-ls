//! Completion method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn completion_impl(
        &self,
        params: CompletionParams,
    ) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "completion called for {} at line {} col {}",
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
            log::debug!(target: "treesitter_ls::completion", "No language detected");
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
        let Some(_server_config) =
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

        // Extract virtual document content for didOpen
        let virtual_content = cacheable.extract_content(text).to_string();

        // Translate position from host to virtual coordinates
        let virtual_position = cacheable.translate_host_to_virtual(position);

        // Create virtual document URI
        // Format: treesitter-ls://virtual/<language>/<hash>.lua
        let virtual_uri = format!(
            "file:///virtual/{}/{}.{}",
            region.language, cacheable.content_hash, region.language
        )
        .parse()
        .map_err(|e| {
            tower_lsp::jsonrpc::Error::invalid_params(format!("Invalid virtual URI: {}", e))
        })?;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Translated position from host {}:{} to virtual {}:{} for URI {}",
                    position.line,
                    position.character,
                    virtual_position.line,
                    virtual_position.character,
                    virtual_uri
                ),
            )
            .await;

        // Create completion params with virtual document URI and translated position
        let virtual_params = CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier { uri: virtual_uri },
                position: virtual_position,
            },
            work_done_progress_params: params.work_done_progress_params,
            partial_result_params: params.partial_result_params,
            context: params.context,
        };

        // Call language_server_pool.completion() for the injection region, passing virtual content
        let completion_response = self
            .language_server_pool
            .completion(virtual_params, virtual_content)
            .await?;

        // TODO(PBI-180a Subtask 4): Translate response ranges from virtual to host coordinates
        // For now, return response as-is

        Ok(completion_response)
    }
}
