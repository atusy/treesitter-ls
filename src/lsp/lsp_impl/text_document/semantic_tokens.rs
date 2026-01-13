//! Semantic token methods for TreeSitterLs.

use std::panic::{AssertUnwindSafe, catch_unwind};
use tokio::time::Duration;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tree_sitter::Tree;

use crate::analysis::{
    IncrementalDecision, compute_incremental_tokens, decide_tokenization_strategy,
    decode_semantic_tokens, encode_semantic_tokens, handle_semantic_tokens_full_delta,
    next_result_id,
};

use super::super::TreeSitterLs;

/// Reason why a semantic token request was cancelled.
#[derive(Debug, Clone, Copy)]
enum CancellationReason {
    StaleText,
    DocumentMissing,
}

impl TreeSitterLs {
    /// Check if the document text matches the expected text, returning the cancellation reason if not.
    fn check_text_staleness(&self, uri: &Url, expected_text: &str) -> Option<CancellationReason> {
        match self.documents.get(uri) {
            Some(doc) if doc.text() == expected_text => None,
            Some(_) => Some(CancellationReason::StaleText),
            None => Some(CancellationReason::DocumentMissing),
        }
    }

    async fn parse_on_demand(&self, uri: &Url, language_name: &str) -> Option<Tree> {
        let doc = self.documents.get(uri)?;
        let text = doc.text().to_string();
        drop(doc);

        let parser = {
            let mut pool = self.parser_pool.lock().await;
            pool.acquire(language_name)
        };

        let parse_result = if let Some(mut parser) = parser {
            let text_clone = text.clone();
            let language_name_clone = language_name.to_string();

            let result = tokio::task::spawn_blocking(move || {
                let parse_result =
                    catch_unwind(AssertUnwindSafe(|| parser.parse(&text_clone, None)))
                        .ok()
                        .flatten();
                (parser, parse_result)
            })
            .await
            .ok();

            if let Some((parser, parse_result)) = result {
                let mut pool = self.parser_pool.lock().await;
                pool.release(language_name_clone, parser);
                parse_result
            } else {
                None
            }
        } else {
            None
        };

        if let Some(tree) = parse_result {
            let mut doc_is_current = false;
            let mut should_update = false;
            if let Some(current_doc) = self.documents.get(uri)
                && current_doc.text() == text
            {
                doc_is_current = true;
                should_update = current_doc.tree().is_none();
            }

            if should_update {
                self.documents
                    .update_document(uri.clone(), text.clone(), Some(tree.clone()));
            }

            if self.failed_parsers.is_failed(language_name)
                && let Err(error) = self.failed_parsers.clear_failed(language_name)
            {
                log::warn!(
                    target: "treesitter_ls::crash_recovery",
                    "Failed to clear failed parser state for '{}': {}",
                    language_name,
                    error
                );
            }

            if doc_is_current {
                return Some(tree);
            }
        }

        None
    }

    pub(crate) async fn semantic_tokens_full_impl(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        // Start tracking this request - supersedes any previous request for this URI
        let request_id = self.semantic_request_tracker.start_request(&uri);

        self.client
            .log_message(
                MessageType::LOG,
                format!("[SEMANTIC_TOKENS] START uri={} req={}", uri, request_id),
            )
            .await;

        // Early exit if request was superseded
        if !self.semantic_request_tracker.is_active(&uri, request_id) {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!("[SEMANTIC_TOKENS] CANCELLED uri={} req={}", uri, request_id),
                )
                .await;
            return Ok(None);
        }

        let Some(language_name) = self.get_language_for_document(&uri) else {
            self.semantic_request_tracker
                .finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Ensure language is loaded before trying to get queries.
        // This handles the race condition where semanticTokens/full arrives
        // before didOpen finishes loading the language.
        let load_result = self.language.ensure_language_loaded(&language_name);
        if !load_result.success {
            self.semantic_request_tracker
                .finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        }

        // Early exit check after loading language
        if !self.semantic_request_tracker.is_active(&uri, request_id) {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!(
                        "[SEMANTIC_TOKENS] CANCELLED uri={} req={} (after language load)",
                        uri, request_id
                    ),
                )
                .await;
            return Ok(None);
        }

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            self.semantic_request_tracker
                .finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Early exit check before expensive computation
        if !self.semantic_request_tracker.is_active(&uri, request_id) {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!(
                        "[SEMANTIC_TOKENS] CANCELLED uri={} req={} (before compute)",
                        uri, request_id
                    ),
                )
                .await;
            return Ok(None);
        }

        // Get document data and compute tokens, then drop the reference
        let (result, text_used) = {
            let Some(doc) = self.documents.get(&uri) else {
                self.semantic_request_tracker
                    .finish_request(&uri, request_id);
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };
            let mut text = doc.text().to_string();
            let tree = match doc.tree() {
                Some(t) => t.clone(),
                None => {
                    drop(doc);
                    self.documents
                        .wait_for_parse_completion(&uri, Duration::from_millis(200))
                        .await;
                    let Some(doc) = self.documents.get(&uri) else {
                        self.semantic_request_tracker
                            .finish_request(&uri, request_id);
                        return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                            result_id: None,
                            data: vec![],
                        })));
                    };
                    text = doc.text().to_string();
                    if let Some(tree) = doc.tree().cloned() {
                        tree
                    } else {
                        drop(doc);
                        let Some(tree) = self.parse_on_demand(&uri, &language_name).await else {
                            self.semantic_request_tracker
                                .finish_request(&uri, request_id);
                            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                                result_id: None,
                                data: vec![],
                            })));
                        };
                        tree
                    }
                }
            };

            if let Some(reason) = self.check_text_staleness(&uri, &text) {
                self.semantic_request_tracker
                    .finish_request(&uri, request_id);
                log::debug!(
                    target: "treesitter_ls::semantic",
                    "[SEMANTIC_TOKENS] CANCELLED uri={} req={} ({:?})",
                    uri, request_id, reason
                );
                return Ok(None);
            }

            // Early exit check after waiting for parse completion
            if !self.semantic_request_tracker.is_active(&uri, request_id) {
                self.client
                    .log_message(
                        MessageType::LOG,
                        format!("[SEMANTIC_TOKENS] CANCELLED uri={} req={}", uri, request_id),
                    )
                    .await;
                return Ok(None);
            }

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self.parser_pool.lock().await;
            let result = crate::analysis::handle_semantic_tokens_full(
                &text,
                &tree,
                &query,
                Some(&language_name),
                Some(&capture_mappings),
                Some(&self.language),
                Some(&mut pool),
            );
            (result, text)
        }; // doc reference is dropped here

        if let Some(reason) = self.check_text_staleness(&uri, &text_used) {
            self.semantic_request_tracker
                .finish_request(&uri, request_id);
            log::debug!(
                target: "treesitter_ls::semantic",
                "[SEMANTIC_TOKENS] CANCELLED uri={} req={} ({:?})",
                uri, request_id, reason
            );
            return Ok(None);
        }

        let mut tokens_with_id = match result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        }) {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(tokens) => tokens,
            tower_lsp::lsp_types::SemanticTokensResult::Partial(_) => {
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                }
            }
        };
        // Use atomic sequential ID for efficient cache validation
        tokens_with_id.result_id = Some(next_result_id());
        let stored_tokens = tokens_with_id.clone();
        let lsp_tokens = tokens_with_id;
        // Store in dedicated cache for delta requests with result_id validation
        self.semantic_cache.store(uri.clone(), stored_tokens);

        // Finish tracking this request
        self.semantic_request_tracker
            .finish_request(&uri, request_id);

        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "[SEMANTIC_TOKENS] DONE uri={} req={} tokens={}",
                    uri,
                    request_id,
                    lsp_tokens.data.len()
                ),
            )
            .await;

        Ok(Some(SemanticTokensResult::Tokens(lsp_tokens)))
    }

    pub(crate) async fn semantic_tokens_full_delta_impl(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = params.text_document.uri;
        let previous_result_id = params.previous_result_id;

        // Start tracking this request - supersedes any previous request for this URI
        let request_id = self.semantic_request_tracker.start_request(&uri);

        self.client
            .log_message(
                MessageType::LOG,
                format!(
                    "[SEMANTIC_TOKENS_DELTA] START uri={} req={}",
                    uri, request_id
                ),
            )
            .await;

        // Early exit if request was superseded
        if !self.semantic_request_tracker.is_active(&uri, request_id) {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!(
                        "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={}",
                        uri, request_id
                    ),
                )
                .await;
            return Ok(None);
        }

        let Some(language_name) = self.get_language_for_document(&uri) else {
            self.semantic_request_tracker
                .finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            self.semantic_request_tracker
                .finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Early exit check before expensive computation
        if !self.semantic_request_tracker.is_active(&uri, request_id) {
            self.client
                .log_message(
                    MessageType::LOG,
                    format!(
                        "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={} (before compute)",
                        uri, request_id
                    ),
                )
                .await;
            return Ok(None);
        }

        // Get document data and compute delta, then drop the reference
        let (result, text_used) = {
            let Some(doc) = self.documents.get(&uri) else {
                self.semantic_request_tracker
                    .finish_request(&uri, request_id);
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            let mut text = doc.text().to_string();
            let tree = match doc.tree() {
                Some(t) => t.clone(),
                None => {
                    drop(doc);
                    self.documents
                        .wait_for_parse_completion(&uri, Duration::from_millis(200))
                        .await;
                    let Some(doc) = self.documents.get(&uri) else {
                        self.semantic_request_tracker
                            .finish_request(&uri, request_id);
                        return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                            SemanticTokens {
                                result_id: None,
                                data: vec![],
                            },
                        )));
                    };
                    text = doc.text().to_string();
                    if let Some(tree) = doc.tree().cloned() {
                        tree
                    } else {
                        drop(doc);
                        let Some(tree) = self.parse_on_demand(&uri, &language_name).await else {
                            self.semantic_request_tracker
                                .finish_request(&uri, request_id);
                            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                                SemanticTokens {
                                    result_id: None,
                                    data: vec![],
                                },
                            )));
                        };
                        tree
                    }
                }
            };

            if let Some(reason) = self.check_text_staleness(&uri, &text) {
                self.semantic_request_tracker
                    .finish_request(&uri, request_id);
                log::debug!(
                    target: "treesitter_ls::semantic",
                    "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={} ({:?})",
                    uri, request_id, reason
                );
                return Ok(None);
            }

            // Early exit check after waiting for parse completion
            if !self.semantic_request_tracker.is_active(&uri, request_id) {
                self.client
                    .log_message(
                        MessageType::LOG,
                        format!(
                            "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={}",
                            uri, request_id
                        ),
                    )
                    .await;
                return Ok(None);
            }

            // Re-acquire document reference after potential waiting
            let doc = match self.documents.get(&uri) {
                Some(d) => d,
                None => {
                    self.semantic_request_tracker
                        .finish_request(&uri, request_id);
                    return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                        SemanticTokens {
                            result_id: None,
                            data: vec![],
                        },
                    )));
                }
            };

            // Get previous tokens from cache with result_id validation
            let previous_tokens = self.semantic_cache.get_if_valid(&uri, &previous_result_id);

            // Get previous text for incremental tokenization
            let previous_text = doc.previous_text().map(|s| s.to_string());

            // Decide tokenization strategy based on change size
            let strategy = decide_tokenization_strategy(doc.previous_tree(), &tree, text.len());

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self.parser_pool.lock().await;

            // Incremental Tokenization Path
            // ==============================
            // When UseIncremental strategy is selected AND we have all required state:
            // 1. Decode previous tokens to absolute (line, column) format
            // 2. Compute new tokens for the ENTIRE document (needed for changed regions)
            // 3. Use Tree-sitter's changed_ranges() to find what lines changed
            // 4. Merge: preserve old tokens outside changed lines, use new for changed lines
            // 5. Encode back to delta format and compute LSP delta
            //
            // This preserves cached tokens for unchanged regions, reducing redundant work.
            // Falls back to full path if any required state is missing.
            let use_incremental = matches!(strategy, IncrementalDecision::UseIncremental)
                && previous_tokens.is_some()
                && doc.previous_tree().is_some()
                && previous_text.is_some();

            let result = if use_incremental {
                log::debug!(
                    target: "treesitter_ls::semantic",
                    "Using incremental tokenization path"
                );

                // Safe to unwrap because we checked above
                let prev_tokens = previous_tokens.as_ref().unwrap();
                let prev_tree = doc.previous_tree().unwrap();
                let prev_text = previous_text.as_ref().unwrap();

                // Decode previous tokens to AbsoluteToken format
                let old_absolute = decode_semantic_tokens(prev_tokens);

                // Get new tokens via full computation (still needed for changed region)
                let new_tokens_result = handle_semantic_tokens_full_delta(
                    &text,
                    &tree,
                    &query,
                    &previous_result_id,
                    None, // Don't pass previous - we'll merge ourselves
                    Some(&language_name),
                    Some(&capture_mappings),
                    Some(&self.language),
                    Some(&mut pool),
                );

                // Extract current tokens from the result
                if let Some(result) = new_tokens_result {
                    let current_tokens = match &result {
                        SemanticTokensFullDeltaResult::Tokens(tokens) => tokens.clone(),
                        SemanticTokensFullDeltaResult::TokensDelta(_)
                        | SemanticTokensFullDeltaResult::PartialTokensDelta { .. } => {
                            // If we got a delta, we need the full tokens
                            // This shouldn't happen since we passed None for previous
                            log::warn!(
                                target: "treesitter_ls::semantic",
                                "Unexpected delta result when computing full tokens"
                            );
                            return Ok(Some(result));
                        }
                    };

                    // Decode new tokens to AbsoluteToken format
                    let new_absolute = decode_semantic_tokens(&current_tokens);

                    // Use incremental merge
                    let merge_result = compute_incremental_tokens(
                        &old_absolute,
                        prev_tree,
                        &tree,
                        prev_text,
                        &text,
                        &new_absolute,
                    );

                    log::debug!(
                        target: "treesitter_ls::semantic",
                        "Incremental merge: {} changed lines, line_delta={}",
                        merge_result.changed_lines.len(),
                        merge_result.line_delta
                    );

                    // Encode merged tokens back to SemanticTokens
                    let merged_tokens = encode_semantic_tokens(
                        &merge_result.tokens,
                        current_tokens.result_id.clone(),
                    );

                    // Calculate delta against original previous tokens
                    Some(crate::analysis::semantic::calculate_delta_or_full(
                        prev_tokens,
                        &merged_tokens,
                        &previous_result_id,
                    ))
                } else {
                    None
                }
            } else {
                log::debug!(
                    target: "treesitter_ls::semantic",
                    "Using full tokenization path (strategy={:?}, has_prev_tokens={}, has_prev_tree={}, has_prev_text={})",
                    strategy,
                    previous_tokens.is_some(),
                    doc.previous_tree().is_some(),
                    previous_text.is_some()
                );

                // Delegate to handler with injection support
                handle_semantic_tokens_full_delta(
                    &text,
                    &tree,
                    &query,
                    &previous_result_id,
                    previous_tokens.as_ref(),
                    Some(&language_name),
                    Some(&capture_mappings),
                    Some(&self.language),
                    Some(&mut pool),
                )
            };
            (result, text)
        }; // doc reference is dropped here

        let domain_result = result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensFullDeltaResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        });

        // Finish tracking this request
        self.semantic_request_tracker
            .finish_request(&uri, request_id);

        if let Some(reason) = self.check_text_staleness(&uri, &text_used) {
            log::debug!(
                target: "treesitter_ls::semantic",
                "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={} ({:?})",
                uri, request_id, reason
            );
            return Ok(None);
        }

        log::debug!(
            target: "treesitter_ls::semantic",
            "[SEMANTIC_TOKENS_DELTA] DONE uri={} req={}",
            uri, request_id
        );

        match domain_result {
            tower_lsp::lsp_types::SemanticTokensFullDeltaResult::Tokens(tokens) => {
                let mut tokens_with_id = tokens;
                // Use atomic sequential ID for efficient cache validation
                tokens_with_id.result_id = Some(next_result_id());
                let stored_tokens = tokens_with_id.clone();
                let lsp_tokens = tokens_with_id;
                // Store in dedicated cache for next delta request
                self.semantic_cache.store(uri.clone(), stored_tokens);
                Ok(Some(SemanticTokensFullDeltaResult::Tokens(lsp_tokens)))
            }
            other => Ok(Some(other)),
        }
    }

    pub(crate) async fn semantic_tokens_range_impl(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let uri = params.text_document.uri;
        let range = params.range;
        let domain_range = range;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let Some(doc) = self.documents.get(&uri) else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        let text = doc.text();
        let Some(tree) = doc.tree() else {
            return Ok(Some(SemanticTokensRangeResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get capture mappings
        let capture_mappings = self.language.get_capture_mappings();

        // Use injection-aware handler (works with or without injection support)
        let mut pool = self.parser_pool.lock().await;
        let result = crate::analysis::handle_semantic_tokens_range(
            text,
            tree,
            &query,
            &domain_range,
            Some(&language_name),
            Some(&capture_mappings),
            Some(&self.language),
            Some(&mut pool),
        );

        // Convert to RangeResult, treating partial responses as empty for now
        let domain_range_result = match result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        }) {
            tower_lsp::lsp_types::SemanticTokensResult::Tokens(tokens) => {
                tower_lsp::lsp_types::SemanticTokensRangeResult::from(tokens)
            }
            tower_lsp::lsp_types::SemanticTokensResult::Partial(partial) => {
                tower_lsp::lsp_types::SemanticTokensRangeResult::from(partial)
            }
        };

        Ok(Some(domain_range_result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, sleep, timeout};
    use tower_lsp::LspService;

    #[tokio::test]
    async fn semantic_tokens_delta_does_not_overwrite_newer_text() {
        let (service, _socket) = LspService::new(TreeSitterLs::new);
        let server = service.inner();
        let uri = Url::parse("file:///semantic_delta_race.lua").expect("should construct test uri");

        let mut initial_text = String::from("local M = {}\n");
        for _ in 0..2000 {
            initial_text.push_str("local x = 1\n");
        }
        initial_text.push_str("return M\n");

        server
            .documents
            .insert(uri.clone(), initial_text, Some("lua".to_string()), None);

        let load_result = server.language.ensure_language_loaded("lua");
        if !load_result.success {
            eprintln!("Skipping: lua language parser not available for semantic tokens test");
            return;
        }

        let new_text = "local LONG_NAME = {}\nreturn LONG_NAME\n".to_string();
        let new_text_clone = new_text.clone();

        let update_future = async {
            sleep(Duration::from_millis(10)).await;
            server
                .documents
                .insert(uri.clone(), new_text_clone, Some("lua".to_string()), None);
        };

        let params = SemanticTokensDeltaParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            previous_result_id: "0".to_string(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let (result, _) = tokio::join!(
            server.semantic_tokens_full_delta_impl(params),
            update_future
        );

        assert!(
            result.is_ok(),
            "semantic tokens delta request should complete without error"
        );

        let doc = server
            .documents
            .get(&uri)
            .expect("document should still exist after delta request");

        assert_eq!(
            doc.text(),
            new_text,
            "delta path should not overwrite newer document text"
        );
    }

    #[tokio::test]
    async fn semantic_tokens_full_times_out_but_parses_on_demand() {
        let (service, _socket) = LspService::new(TreeSitterLs::new);
        let server = service.inner();
        let uri = Url::parse("file:///semantic_timeout.rs").expect("should construct test uri");

        server.documents.insert(
            uri.clone(),
            "fn main() {}".to_string(),
            Some("rust".to_string()),
            None,
        );

        let load_result = server.language.ensure_language_loaded("rust");
        if !load_result.success || server.language.get_highlight_query("rust").is_none() {
            eprintln!("Skipping: rust highlight query not available");
            return;
        }

        server.documents.mark_parse_started(&uri);

        let params = SemanticTokensParams {
            text_document: TextDocumentIdentifier { uri: uri.clone() },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = timeout(
            Duration::from_secs(2),
            server.semantic_tokens_full_impl(params),
        )
        .await;

        assert!(
            result.is_ok(),
            "semantic tokens full should complete after waiting timeout"
        );
        let result = result.unwrap();
        assert!(
            result.is_ok(),
            "semantic tokens full should return without error"
        );

        let doc = server
            .documents
            .get(&uri)
            .expect("document should exist after on-demand parse");
        assert!(
            doc.tree().is_some(),
            "on-demand parse should populate a syntax tree"
        );
    }
}
