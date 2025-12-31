//! Document link method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    pub(crate) async fn document_link_impl(
        &self,
        params: DocumentLinkParams,
    ) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        self.client
            .log_message(
                MessageType::INFO,
                format!("document_link called for {}", uri),
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
            log::debug!(target: "treesitter_ls::document_link", "No language detected");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::document_link", "Language: {}", language_name);

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
            log::debug!(target: "treesitter_ls::document_link", "No parse tree");
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

        // Collect document links from all injection regions
        let mut all_links: Vec<DocumentLink> = Vec::new();

        for region in &injections {
            // Get bridge server config for this language
            // The bridge filter is checked inside get_bridge_config_for_language
            let Some(server_config) =
                self.get_bridge_config_for_language(&language_name, &region.language)
            else {
                continue;
            };

            // Create cacheable region for position translation
            let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);

            // Extract virtual document content (own it for the blocking task)
            let virtual_content = cacheable.extract_content(text).to_owned();

            // Create a virtual URI for the injection
            let virtual_uri = format!(
                "file:///tmp/treesitter-ls-virtual-{}.rs",
                std::process::id()
            );

            let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

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
                    continue;
                }
            };

            let virtual_uri_clone = virtual_uri.clone();
            let result = tokio::task::spawn_blocking(move || {
                let mut conn = conn;
                // Open the virtual document
                conn.did_open(&virtual_uri_clone, "rust", &virtual_content);

                // Request document links with notifications capture
                let result = conn.document_link_with_notifications(&virtual_uri_clone);

                // Return both result and connection for pool return
                (result, conn)
            })
            .await;

            // Handle spawn_blocking result and return connection to pool
            let (links, notifications) = match result {
                Ok((result, conn)) => {
                    self.language_server_pool.return_connection(&pool_key, conn);
                    (result.response, result.notifications)
                }
                Err(e) => {
                    self.client
                        .log_message(MessageType::ERROR, format!("spawn_blocking failed: {}", e))
                        .await;
                    continue;
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

            // Translate response positions back to host document
            if let Some(link_response) = links {
                for link in link_response {
                    // Map the range back to host document coordinates
                    let mapped_start = cacheable.translate_virtual_to_host(link.range.start);
                    let mapped_end = cacheable.translate_virtual_to_host(link.range.end);

                    all_links.push(DocumentLink {
                        range: Range {
                            start: mapped_start,
                            end: mapped_end,
                        },
                        target: link.target,
                        tooltip: link.tooltip,
                        data: link.data,
                    });
                }
            }
        }

        self.client
            .log_message(
                MessageType::INFO,
                format!("Returning {} document links", all_links.len()),
            )
            .await;

        if all_links.is_empty() {
            Ok(None)
        } else {
            Ok(Some(all_links))
        }
    }
}
