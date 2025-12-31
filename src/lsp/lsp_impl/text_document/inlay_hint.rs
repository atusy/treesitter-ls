//! Inlay hint method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn inlay_hint_impl(
        &self,
        params: InlayHintParams,
    ) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;
        let range = params.range;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "inlay_hint called for {} range {}:{}-{}:{}",
                    uri,
                    range.start.line,
                    range.start.character,
                    range.end.line,
                    range.end.character
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
            log::debug!(target: "treesitter_ls::inlay_hint", "No language detected");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::inlay_hint", "Language: {}", language_name);

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
            log::debug!(target: "treesitter_ls::inlay_hint", "No parse tree");
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

        // Convert range start position to byte offset
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(range.start) else {
            log::debug!(target: "treesitter_ls::inlay_hint", "Failed to convert position to byte");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::inlay_hint", "Byte offset: {}", byte_offset);

        // Find which injection region (if any) contains this range
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

        // Translate host range to virtual range
        let virtual_range = Range {
            start: cacheable.translate_host_to_virtual(range.start),
            end: cacheable.translate_host_to_virtual(range.end),
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Translated range: host line {}..{} -> virtual line {}..{}",
                    range.start.line,
                    range.end.line,
                    virtual_range.start.line,
                    virtual_range.end.line
                ),
            )
            .await;

        // Create a virtual URI for the injection
        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );
        self.client
            .log_message(MessageType::INFO, format!("Virtual URI: {}", virtual_uri))
            .await;

        // Get bridge server config for this language
        // Use spawn_blocking because language server communication is synchronous blocking I/O
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
        let has_existing = self.language_server_pool.has_connection(&pool_key);
        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "Getting {} from pool (existing: {})...",
                    pool_key, has_existing
                ),
            )
            .await;

        // Take connection from pool (will spawn if none exists)
        let conn = match self
            .language_server_pool
            .take_connection(&pool_key, &server_config)
        {
            Some(c) => c,
            None => {
                self.client
                    .log_message(MessageType::ERROR, format!("Failed to spawn {}", pool_key))
                    .await;
                return Ok(None);
            }
        };

        let virtual_uri_clone = virtual_uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            // Open the virtual document
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

            // Request inlay hints with notifications capture
            let result = conn.inlay_hint_with_notifications(&virtual_uri_clone, virtual_range);

            // Return both result and connection for pool return
            (result, conn)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let (hints, notifications) = match result {
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

        self.client
            .log_message(
                MessageType::INFO,
                format!("Inlay hint response: {:?}", hints),
            )
            .await;

        // Translate response positions back to host document
        let Some(hint_response) = hints else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "No inlay hint response from rust-analyzer",
                )
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Got inlay hint response: {} hints", hint_response.len()),
            )
            .await;

        // Map each hint's position back to host document, preserving all other fields
        let mapped_hints: Vec<InlayHint> = hint_response
            .into_iter()
            .map(|hint| {
                let mapped_position = cacheable.translate_virtual_to_host(hint.position);
                InlayHint {
                    position: mapped_position,
                    label: hint.label,
                    kind: hint.kind,
                    text_edits: hint.text_edits.map(|edits| {
                        edits
                            .into_iter()
                            .map(|edit| TextEdit {
                                range: Range {
                                    start: cacheable.translate_virtual_to_host(edit.range.start),
                                    end: cacheable.translate_virtual_to_host(edit.range.end),
                                },
                                new_text: edit.new_text,
                            })
                            .collect()
                    }),
                    tooltip: hint.tooltip,
                    padding_left: hint.padding_left,
                    padding_right: hint.padding_right,
                    data: hint.data,
                }
            })
            .collect();

        Ok(Some(mapped_hints))
    }
}
