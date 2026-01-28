//! Semantic token methods for Kakehashi.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

use tower_lsp_server::jsonrpc::Result;
use tower_lsp_server::ls_types::{
    SemanticTokens, SemanticTokensDeltaParams, SemanticTokensFullDeltaResult, SemanticTokensParams,
    SemanticTokensRangeParams, SemanticTokensRangeResult, SemanticTokensResult,
};
use tree_sitter::Tree;
use url::Url;

#[cfg(test)]
use tower_lsp_server::ls_types::{
    PartialResultParams, TextDocumentIdentifier, WorkDoneProgressParams,
};

use crate::analysis::{
    IncrementalDecision, collect_injection_languages, compute_incremental_tokens,
    decide_tokenization_strategy, decode_semantic_tokens, encode_semantic_tokens,
    handle_semantic_tokens_full_delta, handle_semantic_tokens_full_with_local_parsers,
    next_result_id,
};

use super::super::{Kakehashi, uri_to_url};

/// Timeout for spawn_blocking parse operations to prevent hangs on pathological inputs.
const PARSE_TIMEOUT: Duration = Duration::from_secs(10);

/// Reason why a semantic token request was cancelled.
#[derive(Debug, Clone, Copy)]
enum CancellationReason {
    StaleText,
    DocumentMissing,
}

impl Kakehashi {
    /// Check if the document text matches the expected text, returning the cancellation reason if not.
    fn check_text_staleness(&self, uri: &Url, expected_text: &str) -> Option<CancellationReason> {
        match self.documents.get(uri) {
            Some(doc) if doc.text() == expected_text => None,
            Some(_) => Some(CancellationReason::StaleText),
            None => Some(CancellationReason::DocumentMissing),
        }
    }

    /// Get the syntax tree for a document, waiting for parse completion or parsing on-demand.
    ///
    /// This handles the race condition where semantic tokens are requested before
    /// `didOpen`/`didChange` finishes parsing. Strategy:
    /// 1. Wait up to 200ms for any in-flight parse to complete
    /// 2. Try to use the tree from the document store (preferred for incremental tokenization)
    /// 3. Parse on-demand as fallback if tree is missing or stale
    ///
    /// Returns `(tree, text)` tuple where tree was verified to be parsed from text,
    /// or `None` if the document is missing or parsing failed.
    async fn get_tree_with_wait(&self, uri: &Url, language_name: &str) -> Option<(Tree, String)> {
        // Wait for any in-flight parse to complete
        self.documents
            .wait_for_parse_completion(uri, Duration::from_millis(200))
            .await;

        // First, try to use the tree from the document store.
        // This is preferred because:
        // 1. The tree was parsed with the old tree reference (via tree.edit())
        // 2. Tree-sitter can accurately compute changed_ranges() for incremental tokenization
        // 3. Avoids redundant parsing
        if let Some(doc) = self.documents.get(uri) {
            let text = doc.text().to_string();
            if let Some(tree) = doc.tree().cloned() {
                log::debug!(
                    target: "kakehashi::semantic",
                    "Using existing tree from document store for {}",
                    uri.path()
                );
                return Some((tree, text));
            }
        }

        // Fallback: parse on-demand if no tree is available.
        // This handles race conditions where semantic tokens are requested before
        // didOpen/didChange finishes parsing.
        log::debug!(
            target: "kakehashi::semantic",
            "Parsing on-demand for {} (no tree in store)",
            uri.path()
        );
        self.try_parse_and_update_document(uri, language_name).await
    }

    /// Parse the document on-demand and update the store if successful.
    ///
    /// This is a fallback path when the normal parse pipeline hasn't completed.
    /// Side effects:
    /// - Updates the document store with the parsed tree (if text unchanged)
    /// - Clears any failed parser state for recovery
    ///
    /// Returns `(tree, text)` tuple where `text` is the exact text the tree was
    /// parsed from (and verified unchanged). This prevents race conditions where
    /// the document changes after parsing but before the caller captures text.
    async fn try_parse_and_update_document(
        &self,
        uri: &Url,
        language_name: &str,
    ) -> Option<(Tree, String)> {
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
            let uri_clone = uri.clone();

            // Parse in spawn_blocking with timeout and panic protection
            let result = tokio::time::timeout(
                PARSE_TIMEOUT,
                tokio::task::spawn_blocking(move || {
                    let parse_result =
                        catch_unwind(AssertUnwindSafe(|| parser.parse(&text_clone, None)))
                            .ok()
                            .flatten();
                    (parser, parse_result)
                }),
            )
            .await;

            // Handle timeout vs successful completion
            let result = match result {
                Ok(join_result) => match join_result {
                    Ok(result) => Some(result),
                    Err(e) => {
                        log::error!(
                            "Parse task panicked for language '{}' on document {}: {}",
                            language_name_clone,
                            uri_clone,
                            e
                        );
                        None
                    }
                },
                Err(_timeout) => {
                    log::warn!(
                        "Parse timeout after {:?} for language '{}' on document {} ({} bytes)",
                        PARSE_TIMEOUT,
                        language_name_clone,
                        uri_clone,
                        text.len()
                    );
                    None
                }
            };

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

            if doc_is_current {
                if self.auto_install.is_parser_failed(language_name)
                    && let Err(error) = self.auto_install.clear_failed(language_name)
                {
                    log::warn!(
                        target: "kakehashi::crash_recovery",
                        "Failed to clear failed parser state for '{}': {}",
                        language_name,
                        error
                    );
                }
                // Return both tree and the validated text to prevent TOCTOU race
                return Some((tree, text));
            }
        }

        None
    }

    pub(crate) async fn semantic_tokens_full_impl(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let lsp_uri = params.text_document.uri;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in semanticTokens/full: {}", lsp_uri.as_str());
            return Ok(None);
        };

        // Start tracking this request - supersedes any previous request for this URI
        let request_id = self.cache.start_request(&uri);

        log::debug!(
            target: "kakehashi::semantic",
            "[SEMANTIC_TOKENS] START uri={} req={}",
            uri, request_id
        );

        // Early exit if request was superseded
        if !self.cache.is_request_active(&uri, request_id) {
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS] CANCELLED uri={} req={}",
                uri, request_id
            );
            return Ok(None);
        }

        let Some(language_name) = self.get_language_for_document(&uri) else {
            self.cache.finish_request(&uri, request_id);
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
            self.cache.finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        }

        // Early exit check after loading language
        if !self.cache.is_request_active(&uri, request_id) {
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS] CANCELLED uri={} req={} (after language load)",
                uri, request_id
            );
            return Ok(None);
        }

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            self.cache.finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Early exit check before expensive computation
        if !self.cache.is_request_active(&uri, request_id) {
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS] CANCELLED uri={} req={} (before compute)",
                uri, request_id
            );
            return Ok(None);
        }

        // Get tree and text, waiting for parse completion or parsing on-demand
        let Some((tree, text)) = self.get_tree_with_wait(&uri, &language_name).await else {
            self.cache.finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
                result_id: None,
                data: vec![],
            })));
        };

        // Get document data and compute tokens
        let (result, text_used) = {
            if let Some(reason) = self.check_text_staleness(&uri, &text) {
                self.cache.finish_request(&uri, request_id);
                log::debug!(
                    target: "kakehashi::semantic",
                    "[SEMANTIC_TOKENS] CANCELLED uri={} req={} ({:?})",
                    uri, request_id, reason
                );
                return Ok(None);
            }

            // Early exit check after waiting for parse completion
            if !self.cache.is_request_active(&uri, request_id) {
                log::debug!(
                    target: "kakehashi::semantic",
                    "[SEMANTIC_TOKENS] CANCELLED uri={} req={}",
                    uri, request_id
                );
                return Ok(None);
            }

            // Get capture mappings
            let capture_mappings = self.language.get_capture_mappings();

            // Narrow lock scope pattern (Task 1.1):
            // 1. Acquire lock, collect injection languages recursively, pre-acquire parsers, release lock
            // 2. Process tokens without holding lock (100ms-1s)
            // 3. Acquire lock, return parsers (~10μs)
            //
            // Note: collect_injection_languages now recursively parses nested injections
            // to discover all required languages. This uses the pool during discovery,
            // but the overall lock time is still much shorter than holding it during
            // the entire token processing phase.
            let mut local_parsers = {
                let mut pool = self.parser_pool.lock().await;

                // Discover all injection languages recursively (requires pool for nested parsing)
                let injection_languages = collect_injection_languages(
                    &tree,
                    &text,
                    &language_name,
                    &self.language,
                    &mut pool,
                );

                // Pre-acquire parsers for all discovered languages
                let mut parsers = std::collections::HashMap::new();
                for lang_id in &injection_languages {
                    if let Some(parser) = pool.acquire(lang_id) {
                        parsers.insert(lang_id.clone(), parser);
                    }
                }
                parsers
            }; // Lock released here

            // Step 3: Process tokens (no lock held)
            let supports_multiline = self.supports_multiline_tokens();
            let result = handle_semantic_tokens_full_with_local_parsers(
                &text,
                &tree,
                &query,
                Some(&language_name),
                Some(&capture_mappings),
                Some(&self.language),
                &mut local_parsers,
                supports_multiline,
            );

            // Step 4: Return parsers with brief lock
            {
                let mut pool = self.parser_pool.lock().await;
                for (lang_id, parser) in local_parsers {
                    pool.release(lang_id, parser);
                }
            }

            (result, text)
        }; // doc reference is dropped here

        if let Some(reason) = self.check_text_staleness(&uri, &text_used) {
            self.cache.finish_request(&uri, request_id);
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS] CANCELLED uri={} req={} ({:?})",
                uri, request_id, reason
            );
            return Ok(None);
        }

        let mut tokens_with_id = match result.unwrap_or_else(|| {
            tower_lsp_server::ls_types::SemanticTokensResult::Tokens(
                tower_lsp_server::ls_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        }) {
            tower_lsp_server::ls_types::SemanticTokensResult::Tokens(tokens) => tokens,
            tower_lsp_server::ls_types::SemanticTokensResult::Partial(_) => {
                tower_lsp_server::ls_types::SemanticTokens {
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
        self.cache
            .store_tokens(uri.clone(), stored_tokens, &text_used);

        // Finish tracking this request
        self.cache.finish_request(&uri, request_id);

        log::debug!(
            target: "kakehashi::semantic",
            "[SEMANTIC_TOKENS] DONE uri={} req={} tokens={}",
            uri, request_id, lsp_tokens.data.len()
        );

        Ok(Some(SemanticTokensResult::Tokens(lsp_tokens)))
    }

    pub(crate) async fn semantic_tokens_full_delta_impl(
        &self,
        params: SemanticTokensDeltaParams,
    ) -> Result<Option<SemanticTokensFullDeltaResult>> {
        let lsp_uri = params.text_document.uri;
        let previous_result_id = params.previous_result_id;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!(
                "Invalid URI in semanticTokens/full/delta: {}",
                lsp_uri.as_str()
            );
            return Ok(None);
        };

        // Start tracking this request - supersedes any previous request for this URI
        let request_id = self.cache.start_request(&uri);

        log::debug!(
            target: "kakehashi::semantic",
            "[SEMANTIC_TOKENS_DELTA] START uri={} req={}",
            uri, request_id
        );

        // Early exit if request was superseded
        if !self.cache.is_request_active(&uri, request_id) {
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={}",
                uri, request_id
            );
            return Ok(None);
        }

        let Some(language_name) = self.get_language_for_document(&uri) else {
            self.cache.finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        let Some(query) = self.language.get_highlight_query(&language_name) else {
            self.cache.finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Early exit check before expensive computation
        if !self.cache.is_request_active(&uri, request_id) {
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={} (before compute)",
                uri, request_id
            );
            return Ok(None);
        }

        // Get tree and text, waiting for parse completion or parsing on-demand
        let Some((tree, text)) = self.get_tree_with_wait(&uri, &language_name).await else {
            self.cache.finish_request(&uri, request_id);
            return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                SemanticTokens {
                    result_id: None,
                    data: vec![],
                },
            )));
        };

        // Get document data and compute delta
        let (result, text_used) = {
            if let Some(reason) = self.check_text_staleness(&uri, &text) {
                self.cache.finish_request(&uri, request_id);
                log::debug!(
                    target: "kakehashi::semantic",
                    "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={} ({:?})",
                    uri, request_id, reason
                );
                return Ok(None);
            }

            // Early exit check after waiting for parse completion
            if !self.cache.is_request_active(&uri, request_id) {
                log::debug!(
                    target: "kakehashi::semantic",
                    "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={}",
                    uri, request_id
                );
                return Ok(None);
            }

            // Get document reference for delta computation
            let doc = match self.documents.get(&uri) {
                Some(d) => d,
                None => {
                    self.cache.finish_request(&uri, request_id);
                    return Ok(Some(SemanticTokensFullDeltaResult::Tokens(
                        SemanticTokens {
                            result_id: None,
                            data: vec![],
                        },
                    )));
                }
            };

            // Atomically extract all state required for incremental tokenization.
            // This eliminates TOCTOU race conditions by extracting previous_tree
            // and previous_text together in a single operation.
            let incremental_state = doc.incremental_tokenization_state();

            // Get previous tokens from cache with result_id validation for full delta path.
            let previous_tokens_for_delta =
                self.cache.get_tokens_if_valid(&uri, &previous_result_id);

            // Decide tokenization strategy based on change size
            let strategy = decide_tokenization_strategy(
                incremental_state.as_ref().map(|s| &s.previous_tree),
                &tree,
                text.len(),
            );

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
            //
            // Note: We use pattern matching to ensure all required state is available
            // atomically, eliminating TOCTOU (time-of-check-to-time-of-use) issues.
            let incremental_context = if matches!(strategy, IncrementalDecision::UseIncremental) {
                match &incremental_state {
                    Some(state) => {
                        let previous_text_hash =
                            crate::lsp::cache::calculate_text_hash(&state.previous_text);
                        let previous_tokens = self.cache.get_tokens_if_valid_with_text_hash(
                            &uri,
                            &previous_result_id,
                            previous_text_hash,
                        );
                        previous_tokens.map(|tokens| (tokens, state))
                    }
                    None => None,
                }
            } else {
                None
            };

            let result = if let Some((prev_tokens, inc_state)) = incremental_context {
                log::debug!(
                    target: "kakehashi::semantic",
                    "Using incremental tokenization path"
                );

                // Decode previous tokens to AbsoluteToken format
                let old_absolute = decode_semantic_tokens(&prev_tokens);

                // Get new tokens via full computation (still needed for changed region)
                let supports_multiline = self.supports_multiline_tokens();
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
                    supports_multiline,
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
                                target: "kakehashi::semantic",
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
                        &inc_state.previous_tree,
                        &tree,
                        &inc_state.previous_text,
                        &text,
                        &new_absolute,
                    );

                    log::debug!(
                        target: "kakehashi::semantic",
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
                        &prev_tokens,
                        &merged_tokens,
                        &previous_result_id,
                    ))
                } else {
                    None
                }
            } else {
                log::debug!(
                    target: "kakehashi::semantic",
                    "Using full tokenization path (strategy={:?}, has_prev_tokens={}, has_incremental_state={})",
                    strategy,
                    previous_tokens_for_delta.is_some(),
                    incremental_state.is_some()
                );

                // Delegate to handler with injection support
                let supports_multiline = self.supports_multiline_tokens();
                handle_semantic_tokens_full_delta(
                    &text,
                    &tree,
                    &query,
                    &previous_result_id,
                    previous_tokens_for_delta.as_ref(),
                    Some(&language_name),
                    Some(&capture_mappings),
                    Some(&self.language),
                    Some(&mut pool),
                    supports_multiline,
                )
            };
            (result, text)
        }; // doc reference is dropped here

        let domain_result = result.unwrap_or_else(|| {
            tower_lsp_server::ls_types::SemanticTokensFullDeltaResult::Tokens(
                tower_lsp_server::ls_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        });

        // Finish tracking this request
        self.cache.finish_request(&uri, request_id);

        if let Some(reason) = self.check_text_staleness(&uri, &text_used) {
            log::debug!(
                target: "kakehashi::semantic",
                "[SEMANTIC_TOKENS_DELTA] CANCELLED uri={} req={} ({:?})",
                uri, request_id, reason
            );
            return Ok(None);
        }

        log::debug!(
            target: "kakehashi::semantic",
            "[SEMANTIC_TOKENS_DELTA] DONE uri={} req={}",
            uri, request_id
        );

        match domain_result {
            tower_lsp_server::ls_types::SemanticTokensFullDeltaResult::Tokens(tokens) => {
                let mut tokens_with_id = tokens;
                // Use atomic sequential ID for efficient cache validation
                tokens_with_id.result_id = Some(next_result_id());
                let stored_tokens = tokens_with_id.clone();
                let lsp_tokens = tokens_with_id;
                // Store in dedicated cache for next delta request
                self.cache
                    .store_tokens(uri.clone(), stored_tokens, &text_used);
                Ok(Some(SemanticTokensFullDeltaResult::Tokens(lsp_tokens)))
            }
            other => Ok(Some(other)),
        }
    }

    pub(crate) async fn semantic_tokens_range_impl(
        &self,
        params: SemanticTokensRangeParams,
    ) -> Result<Option<SemanticTokensRangeResult>> {
        let lsp_uri = params.text_document.uri;
        let range = params.range;

        // Convert ls_types::Uri to url::Url for internal use
        let Ok(uri) = uri_to_url(&lsp_uri) else {
            log::warn!("Invalid URI in semanticTokens/range: {}", lsp_uri.as_str());
            return Ok(None);
        };

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
        let supports_multiline = self.supports_multiline_tokens();
        let result = crate::analysis::handle_semantic_tokens_range(
            text,
            tree,
            &query,
            &domain_range,
            Some(&language_name),
            Some(&capture_mappings),
            Some(&self.language),
            Some(&mut pool),
            supports_multiline,
        );

        // Convert to RangeResult, treating partial responses as empty for now
        let domain_range_result = match result.unwrap_or_else(|| {
            tower_lsp_server::ls_types::SemanticTokensResult::Tokens(
                tower_lsp_server::ls_types::SemanticTokens {
                    result_id: None,
                    data: Vec::new(),
                },
            )
        }) {
            tower_lsp_server::ls_types::SemanticTokensResult::Tokens(tokens) => {
                tower_lsp_server::ls_types::SemanticTokensRangeResult::from(tokens)
            }
            tower_lsp_server::ls_types::SemanticTokensResult::Partial(partial) => {
                tower_lsp_server::ls_types::SemanticTokensRangeResult::from(partial)
            }
        };

        Ok(Some(domain_range_result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::{Duration, sleep, timeout};
    use tower_lsp_server::LspService;
    use url::Url;

    #[tokio::test]
    async fn semantic_tokens_delta_does_not_overwrite_newer_text() {
        let (service, _socket) = LspService::new(Kakehashi::new);
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
            text_document: TextDocumentIdentifier {
                uri: crate::lsp::lsp_impl::url_to_uri(&uri).expect("test URI should convert"),
            },
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
        let (service, _socket) = LspService::new(Kakehashi::new);
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
            text_document: TextDocumentIdentifier {
                uri: crate::lsp::lsp_impl::url_to_uri(&uri).expect("test URI should convert"),
            },
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

    /// Test that semantic token cache is preserved for delta calculations.
    ///
    /// This verifies the fix for the issue where `invalidate_semantic()` was being
    /// called on every `didChange`, preventing delta calculations from ever working.
    #[tokio::test]
    async fn semantic_tokens_cache_preserved_for_delta() {
        let (service, _socket) = LspService::new(Kakehashi::new);
        let server = service.inner();
        let uri = Url::parse("file:///cache_test.lua").expect("should construct test uri");

        // Insert a document
        server.documents.insert(
            uri.clone(),
            "local x = 1".to_string(),
            Some("lua".to_string()),
            None,
        );

        let load_result = server.language.ensure_language_loaded("lua");
        if !load_result.success {
            eprintln!("Skipping: lua language parser not available");
            return;
        }

        // First request: semanticTokens/full to populate the cache
        let params = SemanticTokensParams {
            text_document: TextDocumentIdentifier {
                uri: crate::lsp::lsp_impl::url_to_uri(&uri).expect("test URI should convert"),
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        };

        let result = server.semantic_tokens_full_impl(params).await;
        assert!(result.is_ok(), "semantic_tokens_full should succeed");

        let tokens_result = result.unwrap();
        assert!(tokens_result.is_some(), "should return tokens");

        // Extract the result_id from the response
        let result_id = match tokens_result.unwrap() {
            SemanticTokensResult::Tokens(tokens) => tokens.result_id,
            _ => panic!("expected Tokens variant"),
        };
        assert!(result_id.is_some(), "should have result_id");
        let result_id = result_id.unwrap();

        // Verify the cache contains tokens with this result_id
        let cached = server.cache.get_tokens_if_valid(&uri, &result_id);
        assert!(
            cached.is_some(),
            "cache should contain tokens with result_id '{}'",
            result_id
        );

        // Simulate a document change (this would normally be done via didChange)
        // In production, didChange does NOT invalidate semantic cache anymore
        server.documents.update_document(
            uri.clone(),
            "local y = 2".to_string(),
            None, // tree will be None until next parse
        );

        // The cache should STILL contain the previous tokens
        // (This is the key assertion - previously this would fail because
        // didChange invalidated the cache)
        let still_cached = server.cache.get_tokens_if_valid(&uri, &result_id);
        assert!(
            still_cached.is_some(),
            "cache should STILL contain tokens after document update - needed for delta calculations"
        );
    }
}
