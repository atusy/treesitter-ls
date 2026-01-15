//! Response transformers for LSP bridge communication.
//!
//! This module provides functions to transform JSON-RPC responses from downstream
//! language servers back to host document coordinates by adding the region_start_line
//! offset to line numbers.

/// Transform a hover response from virtual to host document coordinates.
///
/// If the response contains a range, translates the line numbers from virtual
/// document coordinates back to host document coordinates by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_hover_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Check if response has a result with a range
    if let Some(result) = response.get_mut("result")
        && result.is_object()
        && let Some(range) = result.get_mut("range")
        && range.is_object()
    {
        // Transform start position
        if let Some(start) = range.get_mut("start")
            && let Some(line) = start.get_mut("line")
            && let Some(line_num) = line.as_u64()
        {
            *line = serde_json::json!(line_num + region_start_line as u64);
        }

        // Transform end position
        if let Some(end) = range.get_mut("end")
            && let Some(line) = end.get_mut("line")
            && let Some(line_num) = line.as_u64()
        {
            *line = serde_json::json!(line_num + region_start_line as u64);
        }
    }

    response
}

/// Transform a completion response from virtual to host document coordinates.
///
/// If completion items contain textEdit ranges, translates the line numbers from virtual
/// document coordinates back to host document coordinates by adding region_start_line.
/// Handles both CompletionList format (with items array) and direct array format.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_completion_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // Determine the items to transform
    // CompletionList: { items: [...] } or direct array: [...]
    let items = if result.is_array() {
        result.as_array_mut()
    } else if result.is_object() {
        result.get_mut("items").and_then(|i| i.as_array_mut())
    } else {
        None
    };

    if let Some(items) = items {
        for item in items.iter_mut() {
            transform_completion_item_range(item, region_start_line);
        }
    }

    response
}

/// Transform the textEdit range in a single completion item to host coordinates.
fn transform_completion_item_range(item: &mut serde_json::Value, region_start_line: u32) {
    // Check for textEdit field
    if let Some(text_edit) = item.get_mut("textEdit")
        && let Some(range) = text_edit.get_mut("range")
        && range.is_object()
    {
        transform_range(range, region_start_line);
    }

    // Also check for additionalTextEdits (array of TextEdit)
    if let Some(additional) = item.get_mut("additionalTextEdits")
        && let Some(additional_arr) = additional.as_array_mut()
    {
        for edit in additional_arr.iter_mut() {
            if let Some(range) = edit.get_mut("range")
                && range.is_object()
            {
                transform_range(range, region_start_line);
            }
        }
    }
}

/// Transform a range's line numbers from virtual to host coordinates.
fn transform_range(range: &mut serde_json::Value, region_start_line: u32) {
    // Transform start position
    if let Some(start) = range.get_mut("start")
        && let Some(line) = start.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num + region_start_line as u64);
    }

    // Transform end position
    if let Some(end) = range.get_mut("end")
        && let Some(line) = end.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num + region_start_line as u64);
    }
}

/// Transform a signature help response from virtual to host document coordinates.
///
/// SignatureHelp responses don't contain ranges that need transformation.
/// This function passes through the response unchanged, preserving:
/// - signatures array with label, documentation, and parameters
/// - activeSignature index
/// - activeParameter index
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `_region_start_line` - The starting line (unused for signature help, kept for API consistency)
pub(crate) fn transform_signature_help_response_to_host(
    response: serde_json::Value,
    _region_start_line: u32,
) -> serde_json::Value {
    // SignatureHelp doesn't have ranges that need transformation.
    // activeSignature and activeParameter are indices, not coordinates.
    // Pass through unchanged.
    response
}

/// Transform a document highlight response from virtual to host document coordinates.
///
/// DocumentHighlight responses are arrays of items with range and optional kind.
/// This function transforms each range's line numbers by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_document_highlight_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // DocumentHighlight[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            // Transform range in each DocumentHighlight
            if let Some(range) = item.get_mut("range")
                && range.is_object()
            {
                transform_range(range, region_start_line);
            }
        }
    }

    response
}

/// Transform a document link response from virtual to host document coordinates.
///
/// DocumentLink responses are arrays of items with range, target, tooltip, and data fields.
/// Only the range needs transformation - target, tooltip, and data are preserved unchanged.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_document_link_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // DocumentLink[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            // Transform range in each DocumentLink
            if let Some(range) = item.get_mut("range")
                && range.is_object()
            {
                transform_range(range, region_start_line);
            }
            // target, tooltip, and data are preserved unchanged
        }
    }

    response
}

/// Check if a URI string represents a virtual document.
///
/// Virtual document URIs have the pattern `file:///.treesitter-ls/{hash}/{region_id}.{ext}`.
/// This is used to distinguish virtual URIs from real file URIs in definition responses.
pub(crate) fn is_virtual_uri(uri: &str) -> bool {
    uri.contains("/.treesitter-ls/")
}

/// Context for transforming definition responses to host coordinates.
///
/// Contains information about the original request to enable proper coordinate
/// transformation for responses that may reference different virtual documents.
#[derive(Debug, Clone)]
pub(crate) struct ResponseTransformContext {
    /// The virtual URI string we sent in the request
    pub request_virtual_uri: String,
    /// The host URI string for the request
    pub request_host_uri: String,
    /// The region start line for the request's injection region
    pub request_region_start_line: u32,
}

/// Transform a definition response from virtual to host document coordinates.
///
/// Definition responses can be in multiple formats per LSP spec:
/// - null (no definition found)
/// - Location (single location with uri + range)
/// - Location[] (array of locations)
/// - LocationLink[] (array of location links with target ranges)
///
/// This function handles three cases for each URI in the response:
/// 1. **Real file URI** (not a virtual URI): Preserved as-is with original coordinates
/// 2. **Same virtual URI as request**: Transformed using request's context
/// 3. **Different virtual URI** (cross-region): Filtered out from results
///
/// Cross-region virtual URIs are filtered because we cannot reliably map their
/// coordinates back to the host document (the region_start_line may be stale
/// after host document edits).
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `context` - The transformation context with request information
pub(crate) fn transform_definition_response_to_host(
    mut response: serde_json::Value,
    context: &ResponseTransformContext,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // Array format: Location[] or LocationLink[]
    if let Some(arr) = result.as_array_mut() {
        // Filter out cross-region virtual URIs, transform the rest
        arr.retain_mut(|item| transform_definition_item(item, context));
    } else if result.is_object() {
        // Single Location or LocationLink
        if !transform_definition_item(result, context) {
            // Item was filtered - return null result
            response["result"] = serde_json::Value::Null;
        }
    }

    response
}

/// Transform a single Location or LocationLink item to host coordinates.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
///
/// Handles three cases:
/// 1. Real file URI → preserve as-is (cross-file jump to real file) - KEEP
/// 2. Same virtual URI as request → transform using request's context - KEEP
/// 3. Different virtual URI → cross-region jump - FILTER OUT
fn transform_definition_item(
    item: &mut serde_json::Value,
    context: &ResponseTransformContext,
) -> bool {
    // Handle Location format (has uri + range)
    if let Some(uri_str) = item
        .get("uri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        return transform_location_uri(item, &uri_str, "uri", "range", context);
    }

    // Handle LocationLink format (has targetUri + targetRange + targetSelectionRange)
    if let Some(target_uri_str) = item
        .get("targetUri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        return transform_location_link_target(item, &target_uri_str, context);
    }
    // Note: originSelectionRange stays in host coordinates (it's already correct)

    // Unknown format - keep it
    true
}

/// Transform a Location's uri and range based on URI type.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
fn transform_location_uri(
    item: &mut serde_json::Value,
    uri_str: &str,
    uri_field: &str,
    range_field: &str,
    context: &ResponseTransformContext,
) -> bool {
    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(uri_str) {
        return true;
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == context.request_virtual_uri {
        item[uri_field] = serde_json::json!(&context.request_host_uri);
        if let Some(range) = item.get_mut(range_field) {
            transform_range(range, context.request_region_start_line);
        }
        return true;
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    // We cannot reliably transform these because region_start_line may be stale
    false
}

/// Transform a LocationLink's targetUri and associated ranges.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
fn transform_location_link_target(
    item: &mut serde_json::Value,
    target_uri_str: &str,
    context: &ResponseTransformContext,
) -> bool {
    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(target_uri_str) {
        return true;
    }

    // Case 2: Same virtual URI as request → use request's context
    if target_uri_str == context.request_virtual_uri {
        item["targetUri"] = serde_json::json!(&context.request_host_uri);
        if let Some(range) = item.get_mut("targetRange") {
            transform_range(range, context.request_region_start_line);
        }
        if let Some(range) = item.get_mut("targetSelectionRange") {
            transform_range(range, context.request_region_start_line);
        }
        return true;
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    // We cannot reliably transform these because region_start_line may be stale
    false
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
/// * `context` - The transformation context with request information
pub(crate) fn transform_workspace_edit_to_host(
    mut response: serde_json::Value,
    context: &ResponseTransformContext,
) -> serde_json::Value {
    // Get mutable reference to result
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // Handle changes format: { [uri: string]: TextEdit[] }
    if let Some(changes) = result.get_mut("changes")
        && let Some(changes_obj) = changes.as_object_mut()
    {
        transform_workspace_edit_changes(changes_obj, context);
    }

    // Handle documentChanges format
    if let Some(document_changes) = result.get_mut("documentChanges")
        && let Some(document_changes_arr) = document_changes.as_array_mut()
    {
        transform_workspace_edit_document_changes(document_changes_arr, context);
    }

    response
}

/// Transform the changes map in a WorkspaceEdit.
///
/// Processes each URI in the changes map:
/// - Real file URIs: Keep as-is
/// - Request's virtual URI: Replace key with host URI and transform ranges
/// - Other virtual URIs: Remove (cross-region filtering)
fn transform_workspace_edit_changes(
    changes: &mut serde_json::Map<String, serde_json::Value>,
    context: &ResponseTransformContext,
) {
    // Collect URIs to process (we need to modify the map while iterating)
    let uris_to_process: Vec<String> = changes.keys().cloned().collect();

    for uri in uris_to_process {
        let Some(edits) = changes.remove(&uri) else {
            continue;
        };

        // Case 1: NOT a virtual URI (real file reference) → preserve as-is
        if !is_virtual_uri(&uri) {
            changes.insert(uri, edits);
            continue;
        }

        // Case 2: Same virtual URI as request → transform
        if uri == context.request_virtual_uri {
            let mut edits = edits;
            // Transform ranges in each TextEdit
            if let Some(edits_arr) = edits.as_array_mut() {
                for edit in edits_arr.iter_mut() {
                    if let Some(range) = edit.get_mut("range") {
                        transform_range(range, context.request_region_start_line);
                    }
                }
            }
            // Insert with host URI as key
            changes.insert(context.request_host_uri.clone(), edits);
            continue;
        }

        // Case 3: Different virtual URI (cross-region) → filter out
        // Don't re-insert the edits
    }
}

/// Transform documentChanges array in a WorkspaceEdit.
///
/// Processes each item in the documentChanges array. Items can be:
/// - TextDocumentEdit: Has textDocument.uri and edits[]
/// - CreateFile, RenameFile, DeleteFile: File operations (preserved as-is)
///
/// For TextDocumentEdit items:
/// - Real file URIs: Keep as-is
/// - Request's virtual URI: Replace textDocument.uri with host URI and transform ranges
/// - Other virtual URIs: Remove (cross-region filtering)
fn transform_workspace_edit_document_changes(
    document_changes: &mut Vec<serde_json::Value>,
    context: &ResponseTransformContext,
) {
    document_changes.retain_mut(|item| {
        // Check if this is a TextDocumentEdit (has textDocument field)
        let Some(text_document) = item.get_mut("textDocument") else {
            // Not a TextDocumentEdit (could be CreateFile, RenameFile, DeleteFile)
            // Keep file operations as-is
            return true;
        };

        // Get the URI from textDocument
        let Some(uri_str) = text_document
            .get("uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        else {
            return true; // No URI, keep the item
        };

        // Case 1: NOT a virtual URI (real file reference) → preserve as-is
        if !is_virtual_uri(&uri_str) {
            return true;
        }

        // Case 2: Same virtual URI as request → transform
        if uri_str == context.request_virtual_uri {
            // Update textDocument.uri to host URI
            text_document["uri"] = serde_json::json!(&context.request_host_uri);

            // Transform ranges in each TextEdit
            if let Some(edits) = item.get_mut("edits")
                && let Some(edits_arr) = edits.as_array_mut()
            {
                for edit in edits_arr.iter_mut() {
                    if let Some(range) = edit.get_mut("range") {
                        transform_range(range, context.request_region_start_line);
                    }
                }
            }
            return true;
        }

        // Case 3: Different virtual URI (cross-region) → filter out
        false
    });
}
