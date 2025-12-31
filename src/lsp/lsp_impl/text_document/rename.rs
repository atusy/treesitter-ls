//! Rename handler for textDocument/rename requests.
//!
//! This module contains the implementation of the rename method for TreeSitterLs,
//! which provides rename functionality for injection regions via bridge language servers.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::lsp::TreeSitterLs;
use crate::text::PositionMapper;

impl TreeSitterLs {
    pub(crate) async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let new_name = params.new_name;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "rename called for {} at line {} col {} -> {}",
                    uri, position.line, position.character, new_name
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
            log::debug!(target: "treesitter_ls::rename", "No language detected");
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
        let new_name_clone = new_name.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            // Open the virtual document
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

            // Request rename with notifications capture
            let result = conn.rename_with_notifications(
                &virtual_uri_clone,
                virtual_position,
                &new_name_clone,
            );

            // Return both result and connection for pool return
            (result, conn)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let (rename_result, notifications) = match result {
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

        // Translate WorkspaceEdit ranges back to host document coordinates
        let translated_edit = rename_result.map(|edit| {
            // Translate changes (HashMap<Uri, Vec<TextEdit>>)
            let changes = edit.changes.map(|changes| {
                changes
                    .into_values()
                    .map(|edits| {
                        // Remap URI to host document and translate ranges
                        let translated_edits: Vec<TextEdit> = edits
                            .into_iter()
                            .map(|text_edit| TextEdit {
                                range: Range {
                                    start: cacheable
                                        .translate_virtual_to_host(text_edit.range.start),
                                    end: cacheable.translate_virtual_to_host(text_edit.range.end),
                                },
                                new_text: text_edit.new_text,
                            })
                            .collect();
                        (uri.clone(), translated_edits)
                    })
                    .collect()
            });

            // Translate document_changes (Vec<DocumentChangeOperation>)
            let document_changes = edit.document_changes.map(|doc_changes| {
                match doc_changes {
                    DocumentChanges::Edits(edits) => {
                        let translated_edits: Vec<TextDocumentEdit> = edits
                            .into_iter()
                            .map(|text_doc_edit| {
                                let translated_text_edits: Vec<OneOf<TextEdit, AnnotatedTextEdit>> =
                                    text_doc_edit
                                        .edits
                                        .into_iter()
                                        .map(|edit| match edit {
                                            OneOf::Left(text_edit) => OneOf::Left(TextEdit {
                                                range: Range {
                                                    start: cacheable.translate_virtual_to_host(
                                                        text_edit.range.start,
                                                    ),
                                                    end: cacheable.translate_virtual_to_host(
                                                        text_edit.range.end,
                                                    ),
                                                },
                                                new_text: text_edit.new_text,
                                            }),
                                            OneOf::Right(annotated_edit) => {
                                                OneOf::Right(AnnotatedTextEdit {
                                                    text_edit: TextEdit {
                                                        range: Range {
                                                            start: cacheable
                                                                .translate_virtual_to_host(
                                                                    annotated_edit
                                                                        .text_edit
                                                                        .range
                                                                        .start,
                                                                ),
                                                            end: cacheable
                                                                .translate_virtual_to_host(
                                                                    annotated_edit
                                                                        .text_edit
                                                                        .range
                                                                        .end,
                                                                ),
                                                        },
                                                        new_text: annotated_edit.text_edit.new_text,
                                                    },
                                                    annotation_id: annotated_edit.annotation_id,
                                                })
                                            }
                                        })
                                        .collect();
                                TextDocumentEdit {
                                    text_document: OptionalVersionedTextDocumentIdentifier {
                                        uri: uri.clone(),
                                        version: text_doc_edit.text_document.version,
                                    },
                                    edits: translated_text_edits,
                                }
                            })
                            .collect();
                        DocumentChanges::Edits(translated_edits)
                    }
                    DocumentChanges::Operations(ops) => {
                        // For operations (create/rename/delete), we don't translate
                        // as they operate on whole files, not ranges within the injection
                        DocumentChanges::Operations(ops)
                    }
                }
            });

            WorkspaceEdit {
                changes,
                document_changes,
                change_annotations: edit.change_annotations,
            }
        });

        Ok(translated_edit)
    }
}
