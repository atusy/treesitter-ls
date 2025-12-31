//! Semantic token methods for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;

use crate::analysis::{
    IncrementalDecision, compute_incremental_tokens, decide_tokenization_strategy,
    decode_semantic_tokens, encode_semantic_tokens, handle_semantic_tokens_full_delta,
    next_result_id,
};
use crate::error::LockResultExt;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn semantic_tokens_full_impl(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let Some(language_name) = self.get_language_for_document(&uri) else {
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
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        }

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get document data and compute tokens, then drop the reference
        let result = {
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                    result_id: None,
                    data: vec![],
                })));
            };
            let text = doc.text().to_string();
            let tree = match doc.tree() {
                Some(t) => t.clone(),
                None => {
                    // Document has no tree yet - parse it now.
                    // This handles the race condition where semantic tokens are
                    // requested before didOpen finishes parsing.
                    drop(doc); // Release lock before acquiring parser pool
                    let sync_parse_result = {
                        let mut pool = self
                            .parser_pool
                            .lock()
                            .recover_poison("semantic_tokens_full sync_parse")
                            .unwrap();
                        if let Some(mut parser) = pool.acquire(&language_name) {
                            let result = parser.parse(&text, None);
                            pool.release(language_name.clone(), parser);
                            result
                        } else {
                            None
                        }
                    }; // pool lock released here

                    match sync_parse_result {
                        Some(tree) => {
                            // Update document with parsed tree
                            self.documents.update_document(
                                uri.clone(),
                                text.clone(),
                                Some(tree.clone()),
                            );
                            tree
                        }
                        None => {
                            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                                result_id: None,
                                data: vec![],
                            })));
                        }
                    }
                }
            };

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self
                .parser_pool
                .lock()
                .recover_poison("semantic_tokens_full parser_pool")
                .unwrap();
            crate::analysis::handle_semantic_tokens_full(
                &text,
                &tree,
                &query,
                Some(&language_name),
                Some(&capture_mappings),
                Some(&self.language),
                Some(&mut pool),
            )
        }; // doc reference is dropped here

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
        Ok(Some(SemanticTokensResult::Tokens(lsp_tokens)))
    }

    pub(crate) async fn semantic_tokens_full_delta_impl(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let uri = params.text_document.uri;
        let previous_result_id = params.previous_result_id;

        let Some(language_name) = self.get_language_for_document(&uri) else {
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Get document data and compute delta, then drop the reference
        let result = {
            let Some(doc) = self.documents.get(&uri) else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            let text = doc.text();
            let Some(tree) = doc.tree() else {
                return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                    SemanticTokens {
                        result_id: None,
                        data: vec![],
                    },
                )));
            };

            // Get previous tokens from cache with result_id validation
            let previous_tokens = self.semantic_cache.get_if_valid(&uri, &previous_result_id);

            // Get previous text for incremental tokenization
            let previous_text = doc.previous_text().map(|s| s.to_string());

            // Decide tokenization strategy based on change size
            let strategy = decide_tokenization_strategy(doc.previous_tree(), tree, text.len());

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Use injection-aware handler (works with or without injection support)
            let mut pool = self
                .parser_pool
                .lock()
                .recover_poison("semantic_tokens_full_delta parser_pool")
                .unwrap();

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

            if use_incremental {
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
                    text,
                    tree,
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
                        tree,
                        prev_text,
                        text,
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
                    text,
                    tree,
                    &query,
                    &previous_result_id,
                    previous_tokens.as_ref(),
                    Some(&language_name),
                    Some(&capture_mappings),
                    Some(&self.language),
                    Some(&mut pool),
                )
            }
        }; // doc reference is dropped here

        let domain_result = result.unwrap_or_else(|| {
            tower_lsp::lsp_types::SemanticTokensFullDeltaResult::Tokens(
                tower_lsp::lsp_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        });

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
        let mut pool = self
            .parser_pool
            .lock()
            .recover_poison("semantic_tokens_range parser_pool")
            .unwrap();
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
