//! Response transformers for LSP bridge communication.
//!
//! This module provides functions to transform JSON-RPC responses from downstream
//! language servers back to host document coordinates by adding the region_start_line
//! offset to line numbers.
//!
//! ## Function Signature Patterns
//!
//! Transform functions use two different signatures based on their transformation needs:
//!
//! ### Simple transformers: `fn(response, region_start_line: u32)`
//!
//! Used when transformation only requires adding a line offset. The response contains
//! ranges that reference the same virtual document as the request.
//!
//! - [`transform_hover_response_to_host`] - Range in hover result
//! - [`transform_completion_response_to_host`] - TextEdit ranges in completion items
//! - [`transform_signature_help_response_to_host`] - No ranges (passthrough)
//! - [`transform_document_highlight_response_to_host`] - Ranges in highlight array
//! - [`transform_document_link_response_to_host`] - Ranges in document links
//!
//! ### Context-based transformers: `fn(response, &ResponseTransformContext)`
//!
//! Used when responses may contain URIs pointing to different documents. These need
//! the full request context to distinguish between:
//! - Real file URIs (preserved as-is)
//! - Same virtual URI as request (transformed with context)
//! - Cross-region virtual URIs (filtered out)
//!
//! - [`transform_definition_response_to_host`] - Location/LocationLink with URIs
//! - [`transform_workspace_edit_to_host`] - TextDocumentEdit with URIs

use super::virtual_uri::VirtualDocumentUri;

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
        transform_range(range, region_start_line);
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
///
/// Handles both TextEdit format (has `range`) and InsertReplaceEdit format (has `insert` + `replace`).
/// InsertReplaceEdit is used by some language servers (e.g., rust-analyzer, tsserver) to provide
/// both insert and replace options for the same completion.
fn transform_completion_item_range(item: &mut serde_json::Value, region_start_line: u32) {
    // Check for textEdit field
    if let Some(text_edit) = item.get_mut("textEdit") {
        // TextEdit format: { range, newText }
        if let Some(range) = text_edit.get_mut("range")
            && range.is_object()
        {
            transform_range(range, region_start_line);
        }

        // InsertReplaceEdit format: { insert, replace, newText }
        // Used by some language servers to offer both insert and replace behaviors
        if let Some(insert) = text_edit.get_mut("insert")
            && insert.is_object()
        {
            transform_range(insert, region_start_line);
        }
        if let Some(replace) = text_edit.get_mut("replace")
            && replace.is_object()
        {
            transform_range(replace, region_start_line);
        }
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

/// Transform a document symbol response from virtual to host document coordinates.
///
/// DocumentSymbol responses can be in two formats per LSP spec:
/// - DocumentSymbol[] (hierarchical with range, selectionRange, and optional children)
/// - SymbolInformation[] (flat with location.uri + location.range)
///
/// For DocumentSymbol format:
/// - range: The full scope of the symbol (e.g., entire function body)
/// - selectionRange: The identifier/name of the symbol (e.g., function name)
/// - children: Optional nested symbols (recursively processed)
///
/// For SymbolInformation format:
/// - location.range: The symbol's location range
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_document_symbol_response_to_host(
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

    // DocumentSymbol[] or SymbolInformation[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            transform_document_symbol_item(item, region_start_line);
        }
    }

    response
}

/// Transform a single DocumentSymbol or SymbolInformation item.
fn transform_document_symbol_item(item: &mut serde_json::Value, region_start_line: u32) {
    // DocumentSymbol format: has range + selectionRange (no uri field)
    if let Some(range) = item.get_mut("range")
        && range.is_object()
    {
        transform_range(range, region_start_line);
    }

    if let Some(selection_range) = item.get_mut("selectionRange")
        && selection_range.is_object()
    {
        transform_range(selection_range, region_start_line);
    }

    // Recursively transform children (DocumentSymbol only)
    if let Some(children) = item.get_mut("children")
        && let Some(children_arr) = children.as_array_mut()
    {
        for child in children_arr.iter_mut() {
            transform_document_symbol_item(child, region_start_line);
        }
    }

    // SymbolInformation format: has location.uri + location.range
    if let Some(location) = item.get_mut("location")
        && let Some(range) = location.get_mut("range")
        && range.is_object()
    {
        transform_range(range, region_start_line);
    }
}

/// Transform an inlay hint response from virtual to host document coordinates.
///
/// InlayHint responses are arrays of items where each hint has:
/// - position: The position where the hint should appear (needs transformation)
/// - label: The hint text (string or label parts)
/// - kind, tooltip, paddingLeft, paddingRight, data: Optional fields (preserved unchanged)
/// - textEdits: Optional array of TextEdit (needs transformation, handled separately)
///
/// This function transforms each hint's position by adding region_start_line to the line number.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_inlay_hint_response_to_host(
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

    // InlayHint[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            transform_inlay_hint_item(item, region_start_line);
        }
    }

    response
}

/// Transform a single InlayHint item's position and textEdits to host coordinates.
fn transform_inlay_hint_item(item: &mut serde_json::Value, region_start_line: u32) {
    // Transform position
    if let Some(position) = item.get_mut("position") {
        transform_position(position, region_start_line);
    }

    // Transform textEdits ranges (optional field)
    if let Some(text_edits) = item.get_mut("textEdits")
        && let Some(text_edits_arr) = text_edits.as_array_mut()
    {
        for edit in text_edits_arr.iter_mut() {
            if let Some(range) = edit.get_mut("range")
                && range.is_object()
            {
                transform_range(range, region_start_line);
            }
        }
    }
}

/// Transform a position's line number from virtual to host coordinates.
fn transform_position(position: &mut serde_json::Value, region_start_line: u32) {
    if let Some(line) = position.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num + region_start_line as u64);
    }
}

/// Transform a document color response from virtual to host document coordinates.
///
/// DocumentColor responses are arrays of ColorInformation items, each containing:
/// - range: The range where the color was found (needs transformation)
/// - color: The color value with RGBA components (preserved unchanged)
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_document_color_response_to_host(
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

    // ColorInformation[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            // Transform range in each ColorInformation
            // Color values are preserved unchanged
            if let Some(range) = item.get_mut("range")
                && range.is_object()
            {
                transform_range(range, region_start_line);
            }
        }
    }

    response
}

/// Transform a color presentation response from virtual to host document coordinates.
///
/// ColorPresentation responses are arrays of ColorPresentation items, each containing:
/// - label: The presentation label (preserved unchanged)
/// - textEdit: Optional TextEdit with range (needs transformation)
/// - additionalTextEdits: Optional array of TextEdits (ranges need transformation)
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_color_presentation_response_to_host(
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

    // ColorPresentation[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            // Transform textEdit range if present
            if let Some(text_edit) = item.get_mut("textEdit")
                && let Some(range) = text_edit.get_mut("range")
            {
                transform_range(range, region_start_line);
            }

            // Transform additionalTextEdits ranges if present
            if let Some(additional_edits) = item.get_mut("additionalTextEdits")
                && let Some(edits_arr) = additional_edits.as_array_mut()
            {
                for edit in edits_arr.iter_mut() {
                    if let Some(range) = edit.get_mut("range") {
                        transform_range(range, region_start_line);
                    }
                }
            }
        }
    }

    response
}

/// Transform a moniker response from virtual to host document coordinates.
///
/// Moniker responses don't contain ranges or positions that need transformation.
/// This function passes through the response unchanged, preserving:
/// - scheme: The moniker scheme (e.g., "tsc", "npm")
/// - identifier: The unique identifier for the symbol
/// - unique: Uniqueness level ("document", "project", "scheme", "global")
/// - kind: Optional moniker kind ("import", "export", "local")
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `_region_start_line` - The starting line (unused for moniker, kept for API consistency)
pub(crate) fn transform_moniker_response_to_host(
    response: serde_json::Value,
    _region_start_line: u32,
) -> serde_json::Value {
    // Moniker doesn't have ranges that need transformation.
    // scheme, identifier, unique, and kind are all non-coordinate data.
    // Pass through unchanged.
    response
}

/// Check if a URI string represents a virtual document.
///
/// Delegates to [`VirtualDocumentUri::is_virtual_uri`] which is the single source of truth
/// for virtual URI format knowledge.
pub(crate) fn is_virtual_uri(uri: &str) -> bool {
    VirtualDocumentUri::is_virtual_uri(uri)
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

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // ==========================================================================
    // Hover response transformation tests
    // ==========================================================================

    #[test]
    fn response_transformation_with_zero_region_start() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": { "kind": "markdown", "value": "docs" },
                "range": {
                    "start": { "line": 2, "character": 0 },
                    "end": { "line": 2, "character": 10 }
                }
            }
        });
        let region_start_line = 0;

        let transformed = transform_hover_response_to_host(response, region_start_line);

        assert_eq!(
            transformed["result"]["range"]["start"]["line"], 2,
            "With region_start_line=0, host line equals virtual line"
        );
    }

    #[test]
    fn response_transformation_at_line_zero() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": { "kind": "markdown", "value": "docs" },
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 5 }
                }
            }
        });
        let region_start_line = 10;

        let transformed = transform_hover_response_to_host(response, region_start_line);

        assert_eq!(
            transformed["result"]["range"]["start"]["line"], 10,
            "Virtual line 0 should map to region_start_line"
        );
        assert_eq!(
            transformed["result"]["range"]["end"]["line"], 10,
            "Virtual line 0 should map to region_start_line"
        );
    }

    #[test]
    fn hover_response_transforms_range_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": { "kind": "markdown", "value": "docs" },
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }
        });
        let region_start_line = 3;

        let transformed = transform_hover_response_to_host(response, region_start_line);

        assert_eq!(transformed["result"]["range"]["start"]["line"], 3);
        assert_eq!(transformed["result"]["range"]["end"]["line"], 3);
        assert_eq!(transformed["result"]["range"]["start"]["character"], 9);
        assert_eq!(transformed["result"]["range"]["end"]["character"], 14);
    }

    #[test]
    fn hover_response_without_range_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": { "contents": "Simple hover text" }
        });

        let transformed = transform_hover_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn hover_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_hover_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    // ==========================================================================
    // Completion response transformation tests
    // ==========================================================================

    #[test]
    fn completion_response_transforms_textedit_ranges() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [
                    {
                        "label": "print",
                        "kind": 3,
                        "textEdit": {
                            "range": {
                                "start": { "line": 1, "character": 0 },
                                "end": { "line": 1, "character": 3 }
                            },
                            "newText": "print"
                        }
                    },
                    { "label": "pairs", "kind": 3 }
                ]
            }
        });
        let region_start_line = 3;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        let items = transformed["result"]["items"].as_array().unwrap();
        assert_eq!(items[0]["textEdit"]["range"]["start"]["line"], 4);
        assert_eq!(items[0]["textEdit"]["range"]["end"]["line"], 4);
        assert_eq!(items[1]["label"], "pairs");
        assert!(items[1].get("textEdit").is_none());
    }

    #[test]
    fn completion_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_completion_response_to_host(response.clone(), 3);
        assert_eq!(transformed, response);
    }

    #[test]
    fn completion_response_handles_array_format() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "label": "print",
                "textEdit": {
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 2 }
                    },
                    "newText": "print"
                }
            }]
        });
        let region_start_line = 5;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        let items = transformed["result"].as_array().unwrap();
        assert_eq!(items[0]["textEdit"]["range"]["start"]["line"], 5);
        assert_eq!(items[0]["textEdit"]["range"]["end"]["line"], 5);
    }

    #[test]
    fn completion_response_transforms_insert_replace_edit() {
        // InsertReplaceEdit format used by rust-analyzer, tsserver, etc.
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [{
                    "label": "println!",
                    "textEdit": {
                        "insert": {
                            "start": { "line": 2, "character": 0 },
                            "end": { "line": 2, "character": 3 }
                        },
                        "replace": {
                            "start": { "line": 2, "character": 0 },
                            "end": { "line": 2, "character": 8 }
                        },
                        "newText": "println!"
                    }
                }]
            }
        });
        let region_start_line = 10;

        let transformed = transform_completion_response_to_host(response, region_start_line);

        let item = &transformed["result"]["items"][0]["textEdit"];
        // Insert range transformed: line 2 + 10 = 12
        assert_eq!(item["insert"]["start"]["line"], 12);
        assert_eq!(item["insert"]["end"]["line"], 12);
        // Replace range transformed: line 2 + 10 = 12
        assert_eq!(item["replace"]["start"]["line"], 12);
        assert_eq!(item["replace"]["end"]["line"], 12);
    }

    // ==========================================================================
    // SignatureHelp response transformation tests
    // ==========================================================================

    #[test]
    fn signature_help_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_signature_help_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn signature_help_response_preserves_active_parameter_and_signature() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "signatures": [
                    {
                        "label": "string.format(formatstring, ...)",
                        "documentation": "Formats a string",
                        "parameters": [
                            { "label": "formatstring" },
                            { "label": "..." }
                        ]
                    }
                ],
                "activeSignature": 0,
                "activeParameter": 1
            }
        });
        let region_start_line = 3;

        let transformed =
            transform_signature_help_response_to_host(response.clone(), region_start_line);

        assert_eq!(transformed["result"]["activeSignature"], 0);
        assert_eq!(transformed["result"]["activeParameter"], 1);
        assert_eq!(
            transformed["result"]["signatures"][0]["label"],
            "string.format(formatstring, ...)"
        );
    }

    #[test]
    fn signature_help_response_without_metadata_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "signatures": [
                    { "label": "print(...)" }
                ]
            }
        });

        let transformed = transform_signature_help_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    // ==========================================================================
    // Definition response transformation tests
    // ==========================================================================

    fn test_context(
        virtual_uri: &str,
        host_uri: &str,
        region_start_line: u32,
    ) -> ResponseTransformContext {
        ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: region_start_line,
        }
    }

    #[test]
    fn definition_response_transforms_location_array_ranges() {
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 14 }
                    }
                },
                {
                    "uri": virtual_uri,
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 2, "character": 10 }
                    }
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["range"]["end"]["line"], 3);
        assert_eq!(result[1]["range"]["start"]["line"], 5);
        assert_eq!(result[1]["range"]["end"]["line"], 5);
        assert_eq!(result[0]["uri"], host_uri);
        assert_eq!(result[1]["uri"], host_uri);
    }

    #[test]
    fn definition_response_transforms_single_location() {
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": virtual_uri,
                "range": {
                    "start": { "line": 1, "character": 5 },
                    "end": { "line": 1, "character": 15 }
                }
            }
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        assert_eq!(transformed["result"]["range"]["start"]["line"], 4);
        assert_eq!(transformed["result"]["range"]["end"]["line"], 4);
        assert_eq!(transformed["result"]["uri"], host_uri);
    }

    #[test]
    fn definition_response_transforms_location_link_array() {
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "originSelectionRange": {
                    "start": { "line": 5, "character": 0 },
                    "end": { "line": 5, "character": 10 }
                },
                "targetUri": virtual_uri,
                "targetRange": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 2, "character": 3 }
                },
                "targetSelectionRange": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        // originSelectionRange NOT transformed (in host coordinates)
        assert_eq!(result[0]["originSelectionRange"]["start"]["line"], 5);
        // targetRange transformed
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetRange"]["end"]["line"], 5);
        // targetSelectionRange transformed
        assert_eq!(result[0]["targetSelectionRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetUri"], host_uri);
    }

    #[test]
    fn definition_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });
        let context = test_context(
            "file:///.treesitter-ls/abc123/region-0.lua",
            "file:///project/doc.md",
            3,
        );

        let transformed = transform_definition_response_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    #[test]
    fn definition_response_transforms_location_uri_to_host_uri() {
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "uri": virtual_uri,
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result[0]["uri"], host_uri);
        assert_eq!(result[0]["range"]["start"]["line"], 3);
    }

    #[test]
    fn definition_response_transforms_location_link_target_uri_to_host_uri() {
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "originSelectionRange": {
                    "start": { "line": 5, "character": 0 },
                    "end": { "line": 5, "character": 10 }
                },
                "targetUri": virtual_uri,
                "targetRange": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 2, "character": 3 }
                },
                "targetSelectionRange": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result[0]["targetUri"], host_uri);
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetSelectionRange"]["start"]["line"], 3);
    }

    // ==========================================================================
    // Cross-document transformation tests (is_virtual_uri tests are in virtual_uri.rs)
    // ==========================================================================

    #[test]
    fn definition_response_preserves_real_file_uri() {
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let real_file_uri = "file:///real/path/utils.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "uri": real_file_uri,
                "range": { "start": { "line": 10, "character": 0 }, "end": { "line": 10, "character": 5 } }
            }]
        });

        let host_uri = "file:///doc.md";
        let context = test_context(virtual_uri, host_uri, 5);
        let transformed = transform_definition_response_to_host(response, &context);

        assert_eq!(transformed["result"][0]["uri"], real_file_uri);
        assert_eq!(transformed["result"][0]["range"]["start"]["line"], 10);
    }

    #[test]
    fn definition_response_filters_out_different_region_virtual_uri() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let different_virtual_uri = "file:///.treesitter-ls/abc/region-1.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "uri": different_virtual_uri,
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } }
            }]
        });

        let host_uri = "file:///doc.md";
        let context = test_context(request_virtual_uri, host_uri, 5);

        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert!(
            result.is_empty(),
            "Cross-region virtual URI should be filtered out"
        );
    }

    #[test]
    fn definition_response_mixed_array_filters_only_cross_region() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let cross_region_uri = "file:///.treesitter-ls/abc/region-1.lua";
        let real_file_uri = "file:///real/utils.lua";

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": real_file_uri,
                    "range": { "start": { "line": 10, "character": 0 }, "end": { "line": 10, "character": 5 } }
                },
                {
                    "uri": request_virtual_uri,
                    "range": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 8 } }
                },
                {
                    "uri": cross_region_uri,
                    "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } }
                }
            ]
        });

        let host_uri = "file:///doc.md";
        let context = test_context(request_virtual_uri, host_uri, 5);

        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(
            result.len(),
            2,
            "Should have 2 items (cross-region filtered out)"
        );
        assert_eq!(result[0]["uri"], real_file_uri);
        assert_eq!(result[0]["range"]["start"]["line"], 10);
        assert_eq!(result[1]["uri"], host_uri);
        assert_eq!(result[1]["range"]["start"]["line"], 7);
    }

    #[test]
    fn definition_response_single_location_filtered_becomes_null() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let cross_region_uri = "file:///.treesitter-ls/abc/region-1.lua";

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": cross_region_uri,
                "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } }
            }
        });

        let host_uri = "file:///doc.md";
        let context = test_context(request_virtual_uri, host_uri, 5);

        let transformed = transform_definition_response_to_host(response, &context);

        assert!(transformed["result"].is_null());
    }

    #[test]
    fn definition_response_single_location_link_filtered_becomes_null() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let cross_region_uri = "file:///.treesitter-ls/abc/region-1.lua";

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "originSelectionRange": {
                    "start": { "line": 5, "character": 0 },
                    "end": { "line": 5, "character": 10 }
                },
                "targetUri": cross_region_uri,
                "targetRange": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 2, "character": 0 }
                },
                "targetSelectionRange": {
                    "start": { "line": 0, "character": 6 },
                    "end": { "line": 0, "character": 12 }
                }
            }
        });

        let host_uri = "file:///doc.md";
        let context = test_context(request_virtual_uri, host_uri, 5);

        let transformed = transform_definition_response_to_host(response, &context);

        assert!(transformed["result"].is_null());
    }

    #[test]
    fn definition_response_single_location_link_same_region_transforms() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "originSelectionRange": {
                    "start": { "line": 5, "character": 0 },
                    "end": { "line": 5, "character": 10 }
                },
                "targetUri": virtual_uri,
                "targetRange": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 2, "character": 0 }
                },
                "targetSelectionRange": {
                    "start": { "line": 0, "character": 6 },
                    "end": { "line": 0, "character": 12 }
                }
            }
        });

        let host_uri = "file:///doc.md";
        let context = test_context(virtual_uri, host_uri, 10);

        let transformed = transform_definition_response_to_host(response, &context);

        let result = &transformed["result"];
        assert!(result.is_object());
        assert_eq!(result["targetUri"], host_uri);
        assert_eq!(result["targetRange"]["start"]["line"], 10);
        assert_eq!(result["targetSelectionRange"]["start"]["line"], 10);
        assert_eq!(result["originSelectionRange"]["start"]["line"], 5);
    }

    #[test]
    fn definition_response_location_link_array_filters_cross_region() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let cross_region_uri = "file:///.treesitter-ls/abc/region-1.lua";

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "originSelectionRange": { "start": { "line": 1, "character": 0 }, "end": { "line": 1, "character": 5 } },
                    "targetUri": request_virtual_uri,
                    "targetRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 1, "character": 0 } },
                    "targetSelectionRange": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } }
                },
                {
                    "originSelectionRange": { "start": { "line": 2, "character": 0 }, "end": { "line": 2, "character": 5 } },
                    "targetUri": cross_region_uri,
                    "targetRange": { "start": { "line": 5, "character": 0 }, "end": { "line": 6, "character": 0 } },
                    "targetSelectionRange": { "start": { "line": 5, "character": 0 }, "end": { "line": 5, "character": 3 } }
                }
            ]
        });

        let host_uri = "file:///doc.md";
        let context = test_context(request_virtual_uri, host_uri, 3);

        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0]["targetUri"], host_uri);
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
    }

    // ==========================================================================
    // Document highlight response transformation tests
    // ==========================================================================

    #[test]
    fn document_highlight_response_transforms_ranges_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 6 },
                        "end": { "line": 0, "character": 11 }
                    },
                    "kind": 1
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 2, "character": 5 }
                    },
                    "kind": 2
                },
                {
                    "range": {
                        "start": { "line": 4, "character": 0 },
                        "end": { "line": 4, "character": 5 }
                    }
                }
            ]
        });
        let region_start_line = 3;

        let transformed =
            transform_document_highlight_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["range"]["end"]["line"], 3);
        assert_eq!(result[0]["kind"], 1);
        assert_eq!(result[1]["range"]["start"]["line"], 5);
        assert_eq!(result[1]["range"]["end"]["line"], 5);
        assert_eq!(result[2]["range"]["start"]["line"], 7);
    }

    #[test]
    fn document_highlight_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_highlight_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_highlight_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_highlight_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    // ==========================================================================
    // Document link response transformation tests
    // ==========================================================================

    #[test]
    fn document_link_response_transforms_ranges_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 10 },
                        "end": { "line": 0, "character": 25 }
                    },
                    "target": "file:///some/module.lua"
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 5 },
                        "end": { "line": 2, "character": 15 }
                    }
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0]["range"]["start"]["line"], 5);
        assert_eq!(result[0]["range"]["end"]["line"], 5);
        assert_eq!(result[0]["target"], "file:///some/module.lua");
        assert_eq!(result[1]["range"]["start"]["line"], 7);
        assert_eq!(result[1]["range"]["end"]["line"], 7);
    }

    #[test]
    fn document_link_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_link_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_link_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_link_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn document_link_response_preserves_target_tooltip_data_unchanged() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 10 }
                },
                "target": "file:///target.lua",
                "tooltip": "Go to definition",
                "data": { "custom": "data" }
            }]
        });
        let region_start_line = 3;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["target"], "file:///target.lua");
        assert_eq!(result[0]["tooltip"], "Go to definition");
        assert_eq!(result[0]["data"]["custom"], "data");
    }

    #[test]
    fn document_link_response_without_target_transforms_range() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "range": {
                    "start": { "line": 1, "character": 5 },
                    "end": { "line": 1, "character": 20 }
                }
            }]
        });
        let region_start_line = 10;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result[0]["range"]["start"]["line"], 11);
        assert_eq!(result[0]["range"]["end"]["line"], 11);
        assert!(result[0].get("target").is_none());
    }

    // ==========================================================================
    // WorkspaceEdit response transformation tests
    // ==========================================================================

    #[test]
    fn workspace_edit_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });
        let context = test_context(
            "file:///.treesitter-ls/abc/region-0.lua",
            "file:///doc.md",
            5,
        );

        let transformed = transform_workspace_edit_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    #[test]
    fn workspace_edit_transforms_textedit_ranges_in_changes_map() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let host_uri = "file:///doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    (virtual_uri): [
                        {
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 5 }
                            },
                            "newText": "renamed"
                        },
                        {
                            "range": {
                                "start": { "line": 2, "character": 10 },
                                "end": { "line": 2, "character": 15 }
                            },
                            "newText": "renamed"
                        }
                    ]
                }
            }
        });

        let context = test_context(virtual_uri, host_uri, 5);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let changes = &transformed["result"]["changes"];
        assert!(changes.get(virtual_uri).is_none());
        let host_edits = changes[host_uri].as_array().unwrap();
        assert_eq!(host_edits.len(), 2);
        assert_eq!(host_edits[0]["range"]["start"]["line"], 5);
        assert_eq!(host_edits[1]["range"]["start"]["line"], 7);
    }

    #[test]
    fn workspace_edit_replaces_virtual_uri_key_with_host_uri_in_changes() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let host_uri = "file:///doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    (virtual_uri): [{
                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
                        "newText": "new"
                    }]
                }
            }
        });

        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let changes = &transformed["result"]["changes"];
        assert!(changes.get(virtual_uri).is_none());
        assert!(changes.get(host_uri).is_some());
    }

    #[test]
    fn workspace_edit_preserves_real_file_uris_in_changes() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let real_file_uri = "file:///other/file.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    (real_file_uri): [{
                        "range": { "start": { "line": 10, "character": 0 }, "end": { "line": 10, "character": 5 } },
                        "newText": "updated"
                    }]
                }
            }
        });

        let context = test_context(virtual_uri, "file:///doc.md", 5);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let changes = &transformed["result"]["changes"];
        assert!(changes.get(real_file_uri).is_some());
        assert_eq!(changes[real_file_uri][0]["range"]["start"]["line"], 10);
    }

    #[test]
    fn workspace_edit_filters_out_cross_region_virtual_uris_in_changes() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let cross_region_uri = "file:///.treesitter-ls/abc/region-1.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    (cross_region_uri): [{
                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } },
                        "newText": "should_be_filtered"
                    }]
                }
            }
        });

        let context = test_context(request_virtual_uri, "file:///doc.md", 5);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let changes = &transformed["result"]["changes"];
        assert!(changes.get(cross_region_uri).is_none());
    }

    #[test]
    fn workspace_edit_transforms_textedit_ranges_in_document_changes() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let host_uri = "file:///doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [{
                    "textDocument": { "uri": virtual_uri, "version": 1 },
                    "edits": [
                        {
                            "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } },
                            "newText": "renamed"
                        },
                        {
                            "range": { "start": { "line": 2, "character": 10 }, "end": { "line": 2, "character": 15 } },
                            "newText": "renamed"
                        }
                    ]
                }]
            }
        });

        let context = test_context(virtual_uri, host_uri, 5);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert_eq!(document_changes.len(), 1);
        assert_eq!(document_changes[0]["textDocument"]["uri"], host_uri);
        let edits = document_changes[0]["edits"].as_array().unwrap();
        assert_eq!(edits[0]["range"]["start"]["line"], 5);
        assert_eq!(edits[1]["range"]["start"]["line"], 7);
    }

    #[test]
    fn workspace_edit_replaces_virtual_uri_with_host_uri_in_document_changes() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let host_uri = "file:///doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [{
                    "textDocument": { "uri": virtual_uri, "version": 1 },
                    "edits": [{
                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 3 } },
                        "newText": "new"
                    }]
                }]
            }
        });

        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert_eq!(document_changes[0]["textDocument"]["uri"], host_uri);
    }

    #[test]
    fn workspace_edit_preserves_real_file_uris_in_document_changes() {
        let virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let real_file_uri = "file:///other/file.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [{
                    "textDocument": { "uri": real_file_uri, "version": 1 },
                    "edits": [{
                        "range": { "start": { "line": 10, "character": 0 }, "end": { "line": 10, "character": 5 } },
                        "newText": "updated"
                    }]
                }]
            }
        });

        let context = test_context(virtual_uri, "file:///doc.md", 5);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert_eq!(document_changes[0]["textDocument"]["uri"], real_file_uri);
        assert_eq!(
            document_changes[0]["edits"][0]["range"]["start"]["line"],
            10
        );
    }

    #[test]
    fn workspace_edit_filters_out_cross_region_virtual_uris_in_document_changes() {
        let request_virtual_uri = "file:///.treesitter-ls/abc/region-0.lua";
        let cross_region_uri = "file:///.treesitter-ls/abc/region-1.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [{
                    "textDocument": { "uri": cross_region_uri, "version": 1 },
                    "edits": [{
                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 5 } },
                        "newText": "should_be_filtered"
                    }]
                }]
            }
        });

        let context = test_context(request_virtual_uri, "file:///doc.md", 5);
        let transformed = transform_workspace_edit_to_host(response, &context);

        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert!(document_changes.is_empty());
    }

    // ==========================================================================
    // Document symbol response transformation tests
    // ==========================================================================

    #[test]
    fn document_symbol_response_transforms_range_and_selection_range_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "myFunction",
                    "kind": 12,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 5, "character": 3 }
                    },
                    "selectionRange": {
                        "start": { "line": 0, "character": 9 },
                        "end": { "line": 0, "character": 19 }
                    }
                }
            ]
        });
        let region_start_line = 3;

        let transformed = transform_document_symbol_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 1);
        // range transformed: line 0 + 3 = 3, line 5 + 3 = 8
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["range"]["end"]["line"], 8);
        // selectionRange transformed: line 0 + 3 = 3
        assert_eq!(result[0]["selectionRange"]["start"]["line"], 3);
        assert_eq!(result[0]["selectionRange"]["end"]["line"], 3);
        // name and kind preserved
        assert_eq!(result[0]["name"], "myFunction");
        assert_eq!(result[0]["kind"], 12);
    }

    #[test]
    fn document_symbol_response_recursively_transforms_nested_children() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "myModule",
                    "kind": 2,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 10, "character": 3 }
                    },
                    "selectionRange": {
                        "start": { "line": 0, "character": 7 },
                        "end": { "line": 0, "character": 15 }
                    },
                    "children": [
                        {
                            "name": "innerFunc",
                            "kind": 12,
                            "range": {
                                "start": { "line": 2, "character": 2 },
                                "end": { "line": 5, "character": 5 }
                            },
                            "selectionRange": {
                                "start": { "line": 2, "character": 11 },
                                "end": { "line": 2, "character": 20 }
                            },
                            "children": [
                                {
                                    "name": "deeplyNested",
                                    "kind": 13,
                                    "range": {
                                        "start": { "line": 3, "character": 4 },
                                        "end": { "line": 4, "character": 7 }
                                    },
                                    "selectionRange": {
                                        "start": { "line": 3, "character": 10 },
                                        "end": { "line": 3, "character": 22 }
                                    }
                                }
                            ]
                        }
                    ]
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_document_symbol_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        // Top-level module transformed
        assert_eq!(result[0]["range"]["start"]["line"], 5);
        assert_eq!(result[0]["range"]["end"]["line"], 15);

        // First-level child transformed
        let child = &result[0]["children"][0];
        assert_eq!(child["range"]["start"]["line"], 7);
        assert_eq!(child["range"]["end"]["line"], 10);
        assert_eq!(child["selectionRange"]["start"]["line"], 7);

        // Deeply nested child transformed
        let deep_child = &child["children"][0];
        assert_eq!(deep_child["range"]["start"]["line"], 8);
        assert_eq!(deep_child["range"]["end"]["line"], 9);
        assert_eq!(deep_child["selectionRange"]["start"]["line"], 8);
        assert_eq!(deep_child["name"], "deeplyNested");
    }

    #[test]
    fn document_symbol_response_transforms_symbol_information_location_range() {
        // SymbolInformation format (flat, with location instead of range/selectionRange)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "myVariable",
                    "kind": 13,
                    "location": {
                        "uri": "file:///test.lua",
                        "range": {
                            "start": { "line": 2, "character": 6 },
                            "end": { "line": 2, "character": 16 }
                        }
                    }
                },
                {
                    "name": "myFunction",
                    "kind": 12,
                    "location": {
                        "uri": "file:///test.lua",
                        "range": {
                            "start": { "line": 5, "character": 0 },
                            "end": { "line": 10, "character": 3 }
                        }
                    }
                }
            ]
        });
        let region_start_line = 7;

        let transformed = transform_document_symbol_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2);

        // First symbol's location.range transformed: line 2 + 7 = 9
        assert_eq!(result[0]["location"]["range"]["start"]["line"], 9);
        assert_eq!(result[0]["location"]["range"]["end"]["line"], 9);
        assert_eq!(result[0]["name"], "myVariable");

        // Second symbol's location.range transformed: line 5 + 7 = 12, line 10 + 7 = 17
        assert_eq!(result[1]["location"]["range"]["start"]["line"], 12);
        assert_eq!(result[1]["location"]["range"]["end"]["line"], 17);
        assert_eq!(result[1]["name"], "myFunction");
    }

    #[test]
    fn document_symbol_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_symbol_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_symbol_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_symbol_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    // ==========================================================================
    // Inlay hint response transformation tests
    // ==========================================================================

    #[test]
    fn inlay_hint_response_transforms_positions_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "position": { "line": 0, "character": 10 },
                    "label": "string"
                },
                {
                    "position": { "line": 2, "character": 15 },
                    "label": "number",
                    "kind": 1
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_inlay_hint_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2);
        // First hint: line 0 + 5 = 5
        assert_eq!(result[0]["position"]["line"], 5);
        assert_eq!(result[0]["position"]["character"], 10);
        // Second hint: line 2 + 5 = 7
        assert_eq!(result[1]["position"]["line"], 7);
        assert_eq!(result[1]["position"]["character"], 15);
    }

    #[test]
    fn inlay_hint_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_inlay_hint_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn inlay_hint_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_inlay_hint_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn inlay_hint_response_preserves_non_position_fields() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 1, "character": 5 },
                "label": "string",
                "kind": 1,
                "paddingLeft": true,
                "paddingRight": false,
                "tooltip": "Type hint"
            }]
        });
        let region_start_line = 3;

        let transformed = transform_inlay_hint_response_to_host(response, region_start_line);

        let hint = &transformed["result"][0];
        assert_eq!(hint["position"]["line"], 4); // 1 + 3
        assert_eq!(hint["label"], "string");
        assert_eq!(hint["kind"], 1);
        assert_eq!(hint["paddingLeft"], true);
        assert_eq!(hint["paddingRight"], false);
        assert_eq!(hint["tooltip"], "Type hint");
    }

    #[test]
    fn inlay_hint_response_transforms_text_edits_ranges() {
        // InlayHint with textEdits field - ranges need transformation
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 10 },
                "label": ": string",
                "textEdits": [
                    {
                        "range": {
                            "start": { "line": 0, "character": 10 },
                            "end": { "line": 0, "character": 10 }
                        },
                        "newText": ": string"
                    }
                ]
            }]
        });
        let region_start_line = 5;

        let transformed = transform_inlay_hint_response_to_host(response, region_start_line);

        let hint = &transformed["result"][0];
        // Position transformed
        assert_eq!(hint["position"]["line"], 5);
        // textEdits range transformed
        let text_edit = &hint["textEdits"][0];
        assert_eq!(text_edit["range"]["start"]["line"], 5);
        assert_eq!(text_edit["range"]["end"]["line"], 5);
        assert_eq!(text_edit["newText"], ": string");
    }

    #[test]
    fn inlay_hint_response_transforms_multiple_text_edits() {
        // InlayHint with multiple textEdits
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 2, "character": 5 },
                "label": "hint",
                "textEdits": [
                    {
                        "range": {
                            "start": { "line": 1, "character": 0 },
                            "end": { "line": 1, "character": 5 }
                        },
                        "newText": "first"
                    },
                    {
                        "range": {
                            "start": { "line": 3, "character": 0 },
                            "end": { "line": 4, "character": 10 }
                        },
                        "newText": "second"
                    }
                ]
            }]
        });
        let region_start_line = 10;

        let transformed = transform_inlay_hint_response_to_host(response, region_start_line);

        let hint = &transformed["result"][0];
        // Position transformed: 2 + 10 = 12
        assert_eq!(hint["position"]["line"], 12);

        let edits = hint["textEdits"].as_array().unwrap();
        assert_eq!(edits.len(), 2);

        // First edit: 1 + 10 = 11
        assert_eq!(edits[0]["range"]["start"]["line"], 11);
        assert_eq!(edits[0]["range"]["end"]["line"], 11);

        // Second edit: 3 + 10 = 13, 4 + 10 = 14
        assert_eq!(edits[1]["range"]["start"]["line"], 13);
        assert_eq!(edits[1]["range"]["end"]["line"], 14);
    }

    #[test]
    fn inlay_hint_response_without_text_edits_is_valid() {
        // Most inlay hints don't have textEdits - ensure they work fine
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 5 },
                "label": "hint without edits"
            }]
        });
        let region_start_line = 3;

        let transformed = transform_inlay_hint_response_to_host(response, region_start_line);

        let hint = &transformed["result"][0];
        assert_eq!(hint["position"]["line"], 3);
        assert!(hint.get("textEdits").is_none());
    }

    // ==========================================================================
    // Document color response transformation tests
    // ==========================================================================

    #[test]
    fn document_color_response_transforms_ranges_to_host_coordinates() {
        // ColorInformation[] contains range + color for each color found
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 10 },
                        "end": { "line": 0, "character": 17 }
                    },
                    "color": {
                        "red": 1.0,
                        "green": 0.0,
                        "blue": 0.0,
                        "alpha": 1.0
                    }
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 5 },
                        "end": { "line": 2, "character": 12 }
                    },
                    "color": {
                        "red": 0.0,
                        "green": 1.0,
                        "blue": 0.0,
                        "alpha": 1.0
                    }
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_document_color_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2);
        // First color: line 0 + 5 = 5
        assert_eq!(result[0]["range"]["start"]["line"], 5);
        assert_eq!(result[0]["range"]["end"]["line"], 5);
        // Verify color is preserved unchanged
        assert_eq!(result[0]["color"]["red"], 1.0);
        assert_eq!(result[0]["color"]["green"], 0.0);
        // Second color: line 2 + 5 = 7
        assert_eq!(result[1]["range"]["start"]["line"], 7);
        assert_eq!(result[1]["range"]["end"]["line"], 7);
    }

    #[test]
    fn document_color_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_color_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_color_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_color_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn document_color_response_preserves_color_values() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 7 }
                },
                "color": {
                    "red": 0.5,
                    "green": 0.25,
                    "blue": 0.75,
                    "alpha": 0.9
                }
            }]
        });
        let region_start_line = 3;

        let transformed = transform_document_color_response_to_host(response, region_start_line);

        let result = &transformed["result"][0];
        assert_eq!(result["range"]["start"]["line"], 3);
        assert_eq!(result["color"]["red"], 0.5);
        assert_eq!(result["color"]["green"], 0.25);
        assert_eq!(result["color"]["blue"], 0.75);
        assert_eq!(result["color"]["alpha"], 0.9);
    }

    // ==========================================================================
    // Color presentation response transformation tests
    // ==========================================================================

    #[test]
    fn color_presentation_response_transforms_text_edit_range_to_host_coordinates() {
        // ColorPresentation[] contains label + optional textEdit + optional additionalTextEdits
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "label": "#ff0000",
                    "textEdit": {
                        "range": {
                            "start": { "line": 0, "character": 10 },
                            "end": { "line": 0, "character": 17 }
                        },
                        "newText": "#ff0000"
                    }
                }
            ]
        });
        let region_start_line = 5;

        let transformed =
            transform_color_presentation_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 1);
        // textEdit range transformed: line 0 + 5 = 5
        assert_eq!(result[0]["textEdit"]["range"]["start"]["line"], 5);
        assert_eq!(result[0]["textEdit"]["range"]["end"]["line"], 5);
        // label preserved unchanged
        assert_eq!(result[0]["label"], "#ff0000");
        assert_eq!(result[0]["textEdit"]["newText"], "#ff0000");
    }

    #[test]
    fn color_presentation_response_transforms_additional_text_edits_to_host_coordinates() {
        // ColorPresentation with additionalTextEdits (multiple edits beyond the main one)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "label": "rgb(255, 0, 0)",
                "textEdit": {
                    "range": {
                        "start": { "line": 2, "character": 5 },
                        "end": { "line": 2, "character": 12 }
                    },
                    "newText": "rgb(255, 0, 0)"
                },
                "additionalTextEdits": [
                    {
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 0 }
                        },
                        "newText": "import { rgb } from 'colors';\n"
                    },
                    {
                        "range": {
                            "start": { "line": 4, "character": 0 },
                            "end": { "line": 4, "character": 10 }
                        },
                        "newText": "cleanup()"
                    }
                ]
            }]
        });
        let region_start_line = 3;

        let transformed =
            transform_color_presentation_response_to_host(response, region_start_line);

        let result = &transformed["result"][0];
        // textEdit range transformed: line 2 + 3 = 5
        assert_eq!(result["textEdit"]["range"]["start"]["line"], 5);
        assert_eq!(result["textEdit"]["range"]["end"]["line"], 5);

        // additionalTextEdits ranges transformed
        let additional = result["additionalTextEdits"].as_array().unwrap();
        assert_eq!(additional.len(), 2);
        // First additional: line 0 + 3 = 3
        assert_eq!(additional[0]["range"]["start"]["line"], 3);
        assert_eq!(additional[0]["range"]["end"]["line"], 3);
        // Second additional: line 4 + 3 = 7
        assert_eq!(additional[1]["range"]["start"]["line"], 7);
        assert_eq!(additional[1]["range"]["end"]["line"], 7);
    }

    #[test]
    fn color_presentation_response_without_text_edit_passes_through() {
        // ColorPresentation with only label (no textEdit or additionalTextEdits)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                { "label": "#ff0000" },
                { "label": "rgb(255, 0, 0)" },
                { "label": "hsl(0, 100%, 50%)" }
            ]
        });
        let region_start_line = 5;

        let transformed =
            transform_color_presentation_response_to_host(response.clone(), region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 3);
        assert_eq!(result[0]["label"], "#ff0000");
        assert_eq!(result[1]["label"], "rgb(255, 0, 0)");
        assert_eq!(result[2]["label"], "hsl(0, 100%, 50%)");
    }

    #[test]
    fn color_presentation_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_color_presentation_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn color_presentation_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_color_presentation_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    // ==========================================================================
    // Moniker response tests
    // ==========================================================================

    #[test]
    fn moniker_response_passes_through_unchanged() {
        // Moniker[] has scheme/identifier/unique/kind - no position/range data
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "scheme": "tsc",
                    "identifier": "typescript:foo:bar:Baz",
                    "unique": "document",
                    "kind": "export"
                },
                {
                    "scheme": "npm",
                    "identifier": "package:module:Class.method",
                    "unique": "scheme",
                    "kind": "local"
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_moniker_response_to_host(response.clone(), region_start_line);

        // Response should be unchanged - no coordinates to transform
        assert_eq!(transformed, response);
    }

    #[test]
    fn moniker_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_moniker_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn moniker_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_moniker_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }
}
