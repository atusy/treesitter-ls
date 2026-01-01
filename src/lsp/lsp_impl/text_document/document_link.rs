//! Document link method for TreeSitterLs.

use tower_lsp::jsonrpc::Result;
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

            // Get shared connection from async pool
            let pool_key = server_config.cmd.first().cloned().unwrap_or_default();
            let conn = match self
                .async_language_server_pool
                .get_connection(&pool_key, &server_config)
                .await
            {
                Some(c) => c,
                None => {
                    self.client
                        .log_message(MessageType::ERROR, format!("Failed to spawn {}", pool_key))
                        .await;
                    continue;
                }
            };

            // Send didOpen and wait for indexing
            if conn
                .did_open_and_wait("rust", &virtual_content)
                .await
                .is_err()
            {
                continue;
            }

            // Send document_link request and await response asynchronously
            let links = conn.document_link().await;

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
