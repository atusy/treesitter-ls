//! Signature help method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn signature_help_impl(
        &self,
        params: SignatureHelpParams,
    ) -> Result<Option<SignatureHelp>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "signature_help called for {} at line {} col {}",
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
            log::debug!(target: "treesitter_ls::signature_help", "No language detected");
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

        // Create a virtual URI for the injection
        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );

        // Get language server connection from pool
        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        // Take connection from pool (will spawn if none exists)
        let conn = match self
            .language_server_pool
            .take_connection(&pool_key, &server_config)
        {
            Some(c) => c,
            None => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to spawn language server: {}", pool_key),
                    )
                    .await;
                return Ok(None);
            }
        };

        let virtual_uri_clone = virtual_uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            // Open the virtual document
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

            // Request signature help with notifications capture
            let result =
                conn.signature_help_with_notifications(&virtual_uri_clone, virtual_position);

            // Return both result and connection for pool return
            (result, conn)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let (signature_help, notifications) = match result {
            Ok((result, conn)) => {
                self.language_server_pool.return_connection(&pool_key, conn);
                (result.response, result.notifications)
            }
            Err(e) => {
                self.client
                    .log_message(MessageType::ERROR, format!("spawn_blocking failed: {}", e))
                    .await;
                (None, vec![])
            }
        };

        // Forward captured progress notifications to the client
        for notification in notifications {
            if let Some(params) = notification.get("params")
                && let Ok(progress_params) =
                    serde_json::from_value::<ProgressParams>(params.clone())
            {
                self.client
                    .send_notification::<Progress>(progress_params)
                    .await;
            }
        }

        Ok(signature_help)
    }
}
