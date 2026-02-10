//! Rename request handling for bridge connections.
//!
//! This module provides rename request functionality for downstream language servers,
//! handling the coordinate transformation between host and virtual documents.
//!
//! # Single-Writer Loop (ADR-0015)
//!
//! This handler uses `send_request()` to queue requests via the channel-based
//! writer task, ensuring FIFO ordering with other messages.

use std::io;

use std::collections::HashMap;

use crate::config::settings::BridgeServerConfig;
use tower_lsp_server::ls_types::{
    DocumentChangeOperation, DocumentChanges, OneOf, Position, TextDocumentEdit, TextEdit, Uri,
    WorkspaceEdit,
};
use url::Url;

use super::super::pool::{ConnectionHandleSender, LanguageServerPool, UpstreamId};
use super::super::protocol::{RequestId, VirtualDocumentUri, build_position_based_request};

impl LanguageServerPool {
    /// Send a rename request and wait for the response.
    ///
    /// This is a convenience method that handles the full request/response cycle:
    /// 1. Get or create a connection (state check is atomic with lookup - ADR-0015)
    /// 2. Send a textDocument/didOpen notification if needed
    /// 3. Send the rename request
    /// 4. Wait for and return the response
    ///
    /// The `upstream_request_id` enables $/cancelRequest forwarding.
    /// See [`LanguageServerPool::register_upstream_request()`] for the full flow.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn send_rename_request(
        &self,
        server_name: &str,
        server_config: &BridgeServerConfig,
        host_uri: &Url,
        host_position: Position,
        injection_language: &str,
        region_id: &str,
        region_start_line: u32,
        virtual_content: &str,
        new_name: &str,
        upstream_request_id: UpstreamId,
    ) -> io::Result<Option<WorkspaceEdit>> {
        // Get or create connection - state check is atomic with lookup (ADR-0015)
        let handle = self
            .get_or_create_connection(server_name, server_config)
            .await?;

        // Convert host_uri to lsp_types::Uri for bridge protocol functions
        let host_uri_lsp = crate::lsp::lsp_impl::url_to_uri(host_uri)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        // Build virtual document URI
        let virtual_uri = VirtualDocumentUri::new(&host_uri_lsp, injection_language, region_id);
        let virtual_uri_string = virtual_uri.to_uri_string();

        // Register in the upstream request registry FIRST for cancel lookup.
        // This order matters: if a cancel arrives between pool and router registration,
        // the cancel will fail at the router lookup (which is acceptable for best-effort
        // cancel semantics) rather than finding the server but no downstream ID.
        self.register_upstream_request(upstream_request_id.clone(), server_name);

        // Register request with upstream ID mapping for cancel forwarding
        let (request_id, response_rx) =
            match handle.register_request_with_upstream(Some(upstream_request_id.clone())) {
                Ok(result) => result,
                Err(e) => {
                    // Clean up the pool registration on failure
                    self.unregister_upstream_request(&upstream_request_id, server_name);
                    return Err(e);
                }
            };

        // Build rename request
        let rename_request = build_rename_request(
            &host_uri_lsp,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            new_name,
            request_id,
        );

        // Use a closure for cleanup on any failure path
        let cleanup = || {
            handle.router().remove(request_id);
            self.unregister_upstream_request(&upstream_request_id, server_name);
        };

        // Send didOpen notification only if document hasn't been opened yet
        if let Err(e) = self
            .ensure_document_opened(
                &mut ConnectionHandleSender(&handle),
                host_uri,
                &virtual_uri,
                virtual_content,
                server_name,
            )
            .await
        {
            cleanup();
            return Err(e);
        }

        // Queue the rename request via single-writer loop (ADR-0015)
        if let Err(e) = handle.send_request(rename_request, request_id) {
            cleanup();
            return Err(e.into());
        }

        // Wait for response via oneshot channel (no Mutex held) with timeout
        let response = handle.wait_for_response(request_id, response_rx).await;

        // Unregister from the upstream request registry regardless of result
        self.unregister_upstream_request(&upstream_request_id, server_name);

        // Transform WorkspaceEdit response to host coordinates and URI
        // Cross-region virtual URIs are filtered out
        Ok(transform_workspace_edit_response_to_host(
            response?,
            &virtual_uri_string,
            &host_uri_lsp,
            region_start_line,
        ))
    }
}

/// Build a JSON-RPC rename request for a downstream language server.
///
/// Rename extends the position-based request pattern with an additional
/// `newName` parameter that specifies the new name for the symbol.
///
/// # Coordinate Translation
///
/// Uses `build_position_based_request` to handle host→virtual position
/// translation, then adds the `newName` field to params.
fn build_rename_request(
    host_uri: &tower_lsp_server::ls_types::Uri,
    host_position: tower_lsp_server::ls_types::Position,
    injection_language: &str,
    region_id: &str,
    region_start_line: u32,
    new_name: &str,
    request_id: RequestId,
) -> serde_json::Value {
    let mut request = build_position_based_request(
        host_uri,
        host_position,
        injection_language,
        region_id,
        region_start_line,
        request_id,
        "textDocument/rename",
    );

    // Add the newName parameter required by rename request
    if let Some(params) = request.get_mut("params") {
        params["newName"] = serde_json::json!(new_name);
    }

    request
}

/// Transform a WorkspaceEdit response from virtual to host document coordinates.
///
/// WorkspaceEdit can have two formats per LSP spec:
/// 1. `changes: { [uri: string]: TextEdit[] }` - A map from URI to text edits
/// 2. `documentChanges: (TextDocumentEdit | CreateFile | RenameFile | DeleteFile)[]`
///
/// This function handles three cases for each URI in the response:
/// 1. **Real file URI** (not a virtual URI): Preserved as-is with original coordinates
/// 2. **Same virtual URI as request**: Transformed using request's context
/// 3. **Different virtual URI** (cross-region): Filtered out from results
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `request_virtual_uri` - The virtual URI from the request
/// * `host_uri` - The pre-parsed host URI to use in transformed responses
/// * `region_start_line` - Line offset to add when transforming to host coordinates
fn transform_workspace_edit_response_to_host(
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<WorkspaceEdit> {
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
    let result = response.get_mut("result").map(serde_json::Value::take)?;

    if result.is_null() {
        return None;
    }

    // Parse into typed WorkspaceEdit
    let mut edit: WorkspaceEdit = serde_json::from_value(result).ok()?;

    // Transform changes map: { [uri: string]: TextEdit[] }
    if let Some(changes) = &mut edit.changes {
        transform_changes_map(changes, request_virtual_uri, host_uri, region_start_line);
    }

    // Transform documentChanges array
    if let Some(doc_changes) = &mut edit.document_changes {
        transform_document_changes(
            doc_changes,
            request_virtual_uri,
            host_uri,
            region_start_line,
        );
    }

    Some(edit)
}

/// Transform the `changes` map in a WorkspaceEdit.
///
/// Re-keys virtual URIs to host URI and transforms TextEdit ranges.
/// Cross-region virtual URIs are removed entirely.
fn transform_changes_map(
    changes: &mut HashMap<Uri, Vec<TextEdit>>,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) {
    // Collect keys to process (can't modify HashMap keys in-place)
    let keys: Vec<Uri> = changes.keys().cloned().collect();

    for key in keys {
        let uri_str = key.as_str();

        // Case 1: Real file URI → keep as-is
        if !VirtualDocumentUri::is_virtual_uri(uri_str) {
            continue;
        }

        // Case 2: Same virtual URI → transform ranges, re-key to host URI
        if uri_str == request_virtual_uri {
            if let Some(mut edits) = changes.remove(&key) {
                for edit in &mut edits {
                    edit.range.start.line = edit.range.start.line.saturating_add(region_start_line);
                    edit.range.end.line = edit.range.end.line.saturating_add(region_start_line);
                }
                changes.insert(host_uri.clone(), edits);
            }
            continue;
        }

        // Case 3: Different virtual URI (cross-region) → filter out
        changes.remove(&key);
    }
}

/// Transform the `documentChanges` array in a WorkspaceEdit.
///
/// Handles both `Edits(Vec<TextDocumentEdit>)` and
/// `Operations(Vec<DocumentChangeOperation>)` variants.
/// File operations (CreateFile, RenameFile, DeleteFile) are preserved as-is.
fn transform_document_changes(
    doc_changes: &mut DocumentChanges,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) {
    match doc_changes {
        DocumentChanges::Edits(edits) => {
            edits.retain_mut(|edit| {
                transform_text_document_edit(edit, request_virtual_uri, host_uri, region_start_line)
            });
        }
        DocumentChanges::Operations(ops) => {
            ops.retain_mut(|op| match op {
                DocumentChangeOperation::Edit(edit) => transform_text_document_edit(
                    edit,
                    request_virtual_uri,
                    host_uri,
                    region_start_line,
                ),
                DocumentChangeOperation::Op(_) => true, // File operations preserved
            });
        }
    }
}

/// Transform a single TextDocumentEdit's URI and edit ranges.
///
/// Returns `true` if the edit should be kept, `false` if it should be filtered out.
fn transform_text_document_edit(
    edit: &mut TextDocumentEdit,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> bool {
    let uri_str = edit.text_document.uri.as_str();

    // Case 1: Real file URI → keep as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return true;
    }

    // Case 2: Same virtual URI → transform
    if uri_str == request_virtual_uri {
        edit.text_document.uri = host_uri.clone();
        for one_of in &mut edit.edits {
            match one_of {
                OneOf::Left(text_edit) => {
                    text_edit.range.start.line =
                        text_edit.range.start.line.saturating_add(region_start_line);
                    text_edit.range.end.line =
                        text_edit.range.end.line.saturating_add(region_start_line);
                }
                OneOf::Right(annotated_edit) => {
                    annotated_edit.text_edit.range.start.line = annotated_edit
                        .text_edit
                        .range
                        .start
                        .line
                        .saturating_add(region_start_line);
                    annotated_edit.text_edit.range.end.line = annotated_edit
                        .text_edit
                        .range
                        .end
                        .line
                        .saturating_add(region_start_line);
                }
            }
        }
        return true;
    }

    // Case 3: Cross-region → filter out
    false
}
