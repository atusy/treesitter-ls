//! Goto definition method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
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

        // Get pool key from config
        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        self.client
            .log_message(
                MessageType::LOG,
                format!("[DEFINITION] async bridge START pool_key={}", pool_key),
            )
            .await;

        // Use fully async goto_definition via TokioAsyncLanguageServerPool
        // Pass the host document URI for tracking host-to-bridge mapping
        let definition = self
            .tokio_async_pool
            .goto_definition(
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
                format!(
                    "[DEFINITION] async bridge DONE has_definition={}",
                    definition.is_some()
                ),
            )
            .await;

        // Translate response positions back to host document
        let Some(def_response) = definition else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "No definition response from language server",
                )
                .await;
            return Ok(None);
        };
        self.client
            .log_message(
                MessageType::INFO,
                format!("Got definition response: {:?}", def_response),
            )
            .await;

        // Map the response locations back to host document
        let mapped_response = match def_response {
            GotoDefinitionResponse::Scalar(loc) => {
                let mapped_pos = cacheable.translate_virtual_to_host(loc.range.start);
                let mapped_end = cacheable.translate_virtual_to_host(loc.range.end);
                GotoDefinitionResponse::Scalar(Location {
                    uri: uri.clone(),
                    range: Range {
                        start: mapped_pos,
                        end: mapped_end,
                    },
                })
            }
            GotoDefinitionResponse::Array(locs) => {
                let mapped: Vec<Location> = locs
                    .into_iter()
                    .map(|loc| {
                        let mapped_pos = cacheable.translate_virtual_to_host(loc.range.start);
                        let mapped_end = cacheable.translate_virtual_to_host(loc.range.end);
                        Location {
                            uri: uri.clone(),
                            range: Range {
                                start: mapped_pos,
                                end: mapped_end,
                            },
                        }
                    })
                    .collect();
                GotoDefinitionResponse::Array(mapped)
            }
            GotoDefinitionResponse::Link(links) => {
                let mapped: Vec<LocationLink> = links
                    .into_iter()
                    .map(|link| {
                        let mapped_start =
                            cacheable.translate_virtual_to_host(link.target_range.start);
                        let mapped_end = cacheable.translate_virtual_to_host(link.target_range.end);
                        let sel_start =
                            cacheable.translate_virtual_to_host(link.target_selection_range.start);
                        let sel_end =
                            cacheable.translate_virtual_to_host(link.target_selection_range.end);
                        LocationLink {
                            origin_selection_range: link.origin_selection_range,
                            target_uri: uri.clone(),
                            target_range: Range {
                                start: mapped_start,
                                end: mapped_end,
                            },
                            target_selection_range: Range {
                                start: sel_start,
                                end: sel_end,
                            },
                        }
                    })
                    .collect();
                GotoDefinitionResponse::Link(mapped)
            }
        };

        Ok(Some(mapped_response))
    }
}
