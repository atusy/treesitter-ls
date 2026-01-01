//! Type hierarchy methods for TreeSitterLs.
//!
//! This module implements:
//! - textDocument/prepareTypeHierarchy
//! - typeHierarchy/supertypes
//! - typeHierarchy/subtypes

use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::notification::Progress;
use tower_lsp::lsp_types::*;

use crate::language::injection::CacheableInjectionRegion;
use crate::text::PositionMapper;

use super::super::TreeSitterLs;

impl TreeSitterLs {
    /// Translate a TypeHierarchyItem's ranges from virtual to host coordinates.
    fn translate_type_hierarchy_item_to_host(
        &self,
        item: TypeHierarchyItem,
        uri: &Url,
        cacheable: &CacheableInjectionRegion,
    ) -> TypeHierarchyItem {
        TypeHierarchyItem {
            name: item.name,
            kind: item.kind,
            tags: item.tags,
            detail: item.detail,
            uri: uri.clone(),
            range: Range {
                start: cacheable.translate_virtual_to_host(item.range.start),
                end: cacheable.translate_virtual_to_host(item.range.end),
            },
            selection_range: Range {
                start: cacheable.translate_virtual_to_host(item.selection_range.start),
                end: cacheable.translate_virtual_to_host(item.selection_range.end),
            },
            data: item.data,
        }
    }

    /// Translate a TypeHierarchyItem's ranges from host to virtual coordinates.
    fn translate_type_hierarchy_item_to_virtual(
        &self,
        item: TypeHierarchyItem,
        virtual_uri: &str,
        cacheable: &CacheableInjectionRegion,
    ) -> TypeHierarchyItem {
        TypeHierarchyItem {
            name: item.name,
            kind: item.kind,
            tags: item.tags,
            detail: item.detail,
            uri: Url::parse(virtual_uri).unwrap_or(item.uri),
            range: Range {
                start: cacheable.translate_host_to_virtual(item.range.start),
                end: cacheable.translate_host_to_virtual(item.range.end),
            },
            selection_range: Range {
                start: cacheable.translate_host_to_virtual(item.selection_range.start),
                end: cacheable.translate_host_to_virtual(item.selection_range.end),
            },
            data: item.data,
        }
    }

    pub(crate) async fn prepare_type_hierarchy_impl(
        &self,
        params: TypeHierarchyPrepareParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        self.client
            .log_message(
                MessageType::INFO,
                format!(
                    "prepare_type_hierarchy called for {} at line {} col {}",
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
            log::debug!(target: "treesitter_ls::type_hierarchy", "No language detected");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::type_hierarchy", "Language: {}", language_name);

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
            log::debug!(target: "treesitter_ls::type_hierarchy", "No parse tree");
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
            log::debug!(target: "treesitter_ls::type_hierarchy", "Failed to convert position to byte");
            return Ok(None);
        };
        log::debug!(target: "treesitter_ls::type_hierarchy", "Byte offset: {}", byte_offset);

        // Find which injection region (if any) contains this position
        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
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

        // Get bridge server config for this language
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

            // Request prepare type hierarchy
            let result = conn
                .prepare_type_hierarchy_with_notifications(&virtual_uri_clone, virtual_position);

            (result, conn)
        })
        .await;

        // Handle spawn_blocking result and return connection to pool
        let (items, notifications) = match result {
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

        // Translate response items back to host document
        let Some(item_list) = items else {
            self.client
                .log_message(
                    MessageType::INFO,
                    "No prepare_type_hierarchy response from language server",
                )
                .await;
            return Ok(None);
        };

        // Translate each item's ranges to host coordinates
        let mapped_items: Vec<TypeHierarchyItem> = item_list
            .into_iter()
            .map(|item| self.translate_type_hierarchy_item_to_host(item, &uri, &cacheable))
            .collect();

        Ok(Some(mapped_items))
    }

    pub(crate) async fn supertypes_impl(
        &self,
        params: TypeHierarchySupertypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let item = params.item;

        self.client
            .log_message(
                MessageType::INFO,
                format!("supertypes called for {} at {}", item.name, item.uri),
            )
            .await;

        // The item's URI should be our host document URI
        let uri = item.uri.clone();

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
            log::debug!(target: "treesitter_ls::type_hierarchy", "No language detected");
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

        // Use the item's range start to find the injection region
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(item.range.start) else {
            return Ok(None);
        };

        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            return Ok(None);
        };

        let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);
        let virtual_content = cacheable.extract_content(text).to_owned();

        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );

        // Translate the item to virtual coordinates for the bridge server
        let virtual_item =
            self.translate_type_hierarchy_item_to_virtual(item.clone(), &virtual_uri, &cacheable);

        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &region.language)
        else {
            return Ok(None);
        };

        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        let conn = match self
            .language_server_pool
            .take_connection(&pool_key, &server_config)
        {
            Some(c) => c,
            None => return Ok(None),
        };

        let virtual_uri_clone = virtual_uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);
            let result = conn.supertypes_with_notifications(&virtual_item);
            (result, conn)
        })
        .await;

        let (types, notifications) = match result {
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

        // Forward progress notifications
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

        let Some(type_list) = types else {
            return Ok(None);
        };

        // Translate each supertype to host coordinates
        let mapped_types: Vec<TypeHierarchyItem> = type_list
            .into_iter()
            .map(|t| self.translate_type_hierarchy_item_to_host(t, &uri, &cacheable))
            .collect();

        Ok(Some(mapped_types))
    }

    pub(crate) async fn subtypes_impl(
        &self,
        params: TypeHierarchySubtypesParams,
    ) -> Result<Option<Vec<TypeHierarchyItem>>> {
        let item = params.item;

        self.client
            .log_message(
                MessageType::INFO,
                format!("subtypes called for {} at {}", item.name, item.uri),
            )
            .await;

        // The item's URI should be our host document URI
        let uri = item.uri.clone();

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
            log::debug!(target: "treesitter_ls::type_hierarchy", "No language detected");
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

        // Use the item's range start to find the injection region
        let mapper = PositionMapper::new(text);
        let Some(byte_offset) = mapper.position_to_byte(item.range.start) else {
            return Ok(None);
        };

        let matching_region = injections.iter().find(|inj| {
            let start = inj.content_node.start_byte();
            let end = inj.content_node.end_byte();
            byte_offset >= start && byte_offset < end
        });

        let Some(region) = matching_region else {
            return Ok(None);
        };

        let cacheable = CacheableInjectionRegion::from_region_info(region, "temp", text);
        let virtual_content = cacheable.extract_content(text).to_owned();

        let virtual_uri = format!(
            "file:///tmp/treesitter-ls-virtual-{}.rs",
            std::process::id()
        );

        // Translate the item to virtual coordinates for the bridge server
        let virtual_item =
            self.translate_type_hierarchy_item_to_virtual(item.clone(), &virtual_uri, &cacheable);

        let Some(server_config) =
            self.get_bridge_config_for_language(&language_name, &region.language)
        else {
            return Ok(None);
        };

        let pool_key = server_config.cmd.first().cloned().unwrap_or_default();

        let conn = match self
            .language_server_pool
            .take_connection(&pool_key, &server_config)
        {
            Some(c) => c,
            None => return Ok(None),
        };

        let virtual_uri_clone = virtual_uri.clone();
        let result = tokio::task::spawn_blocking(move || {
            let mut conn = conn;
            conn.did_open(&virtual_uri_clone, "rust", &virtual_content);
            let result = conn.subtypes_with_notifications(&virtual_item);
            (result, conn)
        })
        .await;

        let (types, notifications) = match result {
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

        // Forward progress notifications
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

        let Some(type_list) = types else {
            return Ok(None);
        };

        // Translate each subtype to host coordinates
        let mapped_types: Vec<TypeHierarchyItem> = type_list
            .into_iter()
            .map(|t| self.translate_type_hierarchy_item_to_host(t, &uri, &cacheable))
            .collect();

        Ok(Some(mapped_types))
    }
}
