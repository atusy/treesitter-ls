//! Code action handler for textDocument/codeAction requests.
//!
//! This module contains the implementation of the code_action method for TreeSitterLs,
//! which provides code actions for both the host document and injection regions via bridge
//! language servers.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;

use crate::analysis::handle_code_actions;
use crate::language::injection::CacheableInjectionRegion;
use crate::lsp::TreeSitterLs;
use crate::text::PositionMapper;

impl TreeSitterLs {
    pub(crate) async fn code_action(
        &self,
        params: CodeActionParams,
    ) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let range = params.range;

        // Get document and tree
        let Some(doc) = self.documents.get(&uri) else {
            return Ok(None);
        };
        let text = doc.text();
        let Some(tree) = doc.tree() else {
            return Ok(None);
        };

        let domain_range = range;

        // Get language for the document
        let language_name = self.get_language_for_document(&uri);

        // Try to get bridged actions from injection region (child language)
        let bridged_actions = if let Some(ref lang) = language_name {
            self.try_bridge_code_action(&uri, text, tree, lang, range)
                .await
        } else {
            None
        };

        // Get capture mappings
        let capture_mappings = self.language.get_capture_mappings();
        let capture_context = language_name.as_deref().map(|ft| (ft, &capture_mappings));

        // Get treesitter-ls actions (parent language)
        let parent_actions = if let Some(lang) = language_name.clone() {
            let highlight_query = self.language.get_highlight_query(&lang);
            let locals_query = self.language.get_locals_query(&lang);
            let injection_query = self.language.get_injection_query(&lang);

            let queries = highlight_query
                .as_ref()
                .map(|hq| (hq.as_ref(), locals_query.as_ref().map(|lq| lq.as_ref())));

            // Build code action options with injection support
            let mut options =
                crate::analysis::refactor::CodeActionOptions::new(&uri, text, tree, domain_range)
                    .with_queries(queries)
                    .with_capture_context(capture_context);

            // Add injection query if available
            if let Some(inj_q) = injection_query.as_ref() {
                options = options.with_injection(inj_q.as_ref());
            }

            // Use coordinator if we can get parser pool lock
            if let Ok(mut pool) = self.parser_pool.lock() {
                options = options.with_coordinator(&self.language, &mut pool);
                handle_code_actions(options)
            } else {
                // Fallback without coordinator
                handle_code_actions(options)
            }
        } else {
            handle_code_actions(crate::analysis::refactor::CodeActionOptions::new(
                &uri,
                text,
                tree,
                domain_range,
            ))
        };

        // Merge actions: child (bridged) first, then parent (treesitter-ls)
        let lsp_response = match (bridged_actions, parent_actions) {
            (Some(mut child), Some(parent)) => {
                // Child actions first, then parent actions
                child.extend(parent);
                Some(child)
            }
            (Some(child), None) => Some(child),
            (None, Some(parent)) => Some(parent),
            (None, None) => None,
        };

        Ok(lsp_response)
    }

    /// Try to bridge code action request to an external language server.
    ///
    /// Returns Some(CodeActionResponse) if the request was successfully bridged,
    /// None if the position is not in an injection region or bridging failed.
    async fn try_bridge_code_action(
        &self,
        uri: &Url,
        text: &str,
        tree: &tree_sitter::Tree,
        language_name: &str,
        range: Range,
    ) -> Option<CodeActionResponse> {
        // Get injection query to detect injection regions
        let injection_query = self.language.get_injection_query(language_name)?;

        // Collect all injection regions
        let injections = crate::language::injection::collect_all_injections(
            &tree.root_node(),
            text,
            Some(injection_query.as_ref()),
        )?;

        // Convert Position to byte offset
        let mapper = PositionMapper::new(text);
        let byte_offset = mapper.position_to_byte(range.start)?;

        // Find which injection region (if any) contains this position
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        })?;

        // Get bridge server config for this language
        let server_config =
            self.get_bridge_config_for_language(language_name, &matching_region.language)?;

        // Create cacheable region for position translation
        let cacheable = CacheableInjectionRegion::from_region_info(matching_region, "temp", text);

        // Extract virtual document content
        let virtual_content = cacheable.extract_content(text).to_owned();

        // Translate host range to virtual range
        let virtual_range = Range {
            start: cacheable.translate_host_to_virtual(range.start),
            end: cacheable.translate_host_to_virtual(range.end),
        };

        // Create a virtual URI for the injection
        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );

        // Get language server connection from pool
        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        // Take connection from pool (will spawn if none exists)
        let conn = self
            .language_server_pool
            .take_connection(&pool_key, &server_config)?;

        let virtual_uri_clone = virtual_uri.clone();
        let uri_clone = uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            // Open the virtual document
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

            // Request code action with notifications capture
            let result = conn.code_action_with_notifications(&virtual_uri_clone, virtual_range);

            // Return both result and connection for pool return
            (result, conn, uri_clone, cacheable)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let (code_action_result, notifications, uri_for_translate, cacheable_for_translate) =
            match result {
                Ok((result, conn, uri, cacheable)) => {
                    self.language_server_pool.return_connection(&pool_key, conn);
                    (result.response, result.notifications, uri, cacheable)
                }
                Err(e) => {
                    self.client
                        .log_message(MessageType::ERROR, format!("spawn_blocking failed: {}", e))
                        .await;
                    return None;
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

        // Translate CodeActionResponse ranges back to host document coordinates
        code_action_result.map(|actions| {
            actions
                .into_iter()
                .map(|action_or_cmd| {
                    Self::translate_code_action_or_command(
                        action_or_cmd,
                        &uri_for_translate,
                        &cacheable_for_translate,
                    )
                })
                .collect()
        })
    }

    /// Translate a CodeActionOrCommand from virtual to host coordinates.
    fn translate_code_action_or_command(
        action_or_cmd: CodeActionOrCommand,
        uri: &Url,
        cacheable: &CacheableInjectionRegion,
    ) -> CodeActionOrCommand {
        match action_or_cmd {
            CodeActionOrCommand::Command(cmd) => {
                // Commands don't have ranges, no translation needed
                CodeActionOrCommand::Command(cmd)
            }
            CodeActionOrCommand::CodeAction(action) => {
                let translated_action = CodeAction {
                    title: action.title,
                    kind: action.kind,
                    diagnostics: action.diagnostics.map(|diags| {
                        diags
                            .into_iter()
                            .map(|diag| Diagnostic {
                                range: Range {
                                    start: cacheable.translate_virtual_to_host(diag.range.start),
                                    end: cacheable.translate_virtual_to_host(diag.range.end),
                                },
                                ..diag
                            })
                            .collect()
                    }),
                    edit: action
                        .edit
                        .map(|edit| Self::translate_workspace_edit(edit, uri, cacheable)),
                    command: action.command,
                    is_preferred: action.is_preferred,
                    disabled: action.disabled,
                    data: action.data,
                };
                CodeActionOrCommand::CodeAction(translated_action)
            }
        }
    }

    /// Translate a WorkspaceEdit from virtual to host coordinates.
    fn translate_workspace_edit(
        edit: WorkspaceEdit,
        uri: &Url,
        cacheable: &CacheableInjectionRegion,
    ) -> WorkspaceEdit {
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
                                start: cacheable.translate_virtual_to_host(text_edit.range.start),
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
                                                end: cacheable
                                                    .translate_virtual_to_host(text_edit.range.end),
                                            },
                                            new_text: text_edit.new_text,
                                        }),
                                        OneOf::Right(annotated_edit) => {
                                            OneOf::Right(AnnotatedTextEdit {
                                                text_edit: TextEdit {
                                                    range: Range {
                                                        start: cacheable.translate_virtual_to_host(
                                                            annotated_edit.text_edit.range.start,
                                                        ),
                                                        end: cacheable.translate_virtual_to_host(
                                                            annotated_edit.text_edit.range.end,
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
    }
}
