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
//! Examples: document link
//!
//! ### Context-based transformers: `fn(response, &ResponseTransformContext)`
//!
//! Used when JSON-based responses may contain URIs pointing to different documents.
//! These need the full request context to distinguish between:
//! - Real file URIs (preserved as-is)
//! - Same virtual URI as request (transformed with context)
//! - Cross-region virtual URIs (filtered out)
//!
//! Examples: workspace edit, document symbol, inlay hint
//!
//! ### Type-safe transformers: `fn(response, request_virtual_uri, host_uri, region_start_line)`
//!
//! Type-safe transformers that return strongly-typed LSP types instead of JSON.
//! Apply the same URI-based filtering logic as context-based transformers.
//!
//! Examples: goto definition/type_definition/implementation/declaration, references

use log::warn;

use super::virtual_uri::VirtualDocumentUri;
use tower_lsp_server::ls_types::{Location, LocationLink, Range, Uri};

/// Transform a range's line numbers from virtual to host coordinates.
///
/// Uses saturating_add to prevent overflow for large line numbers, consistent
/// with saturating_sub used elsewhere in the codebase.
fn transform_range(range: &mut serde_json::Value, region_start_line: u32) {
    // Transform start position
    if let Some(start) = range.get_mut("start")
        && let Some(line) = start.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
    }

    // Transform end position
    if let Some(end) = range.get_mut("end")
        && let Some(line) = end.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
    }
}

// =============================================================================
// Type-safe goto-family transformers
// =============================================================================

/// Transform goto-family responses to typed Vec<LocationLink> format.
///
/// This function handles all goto-style endpoints (definition, type_definition,
/// implementation, declaration) that return Location | Location[] | LocationLink[].
///
/// All response variants are normalized to Vec<LocationLink> for internal consistency,
/// with proper URI filtering and coordinate transformation.
///
/// # URI Filtering Logic
///
/// - Real file URIs → keep as-is (cross-file jumps)
/// - Same virtual URI as request → transform coordinates
/// - Different virtual URI → filter out (cross-region, can't transform safely)
///
/// Empty arrays after filtering are preserved to distinguish "searched, found nothing"
/// from "search failed" (None).
///
/// # Arguments
/// * `response` - Raw JSON-RPC response envelope (`{"result": {...}}`)
/// * `request_virtual_uri` - The virtual URI from the request
/// * `host_uri` - The pre-parsed host URI to use in transformed responses
/// * `region_start_line` - Line offset to add when transforming to host coordinates
pub(crate) fn transform_goto_response_to_host(
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<Vec<LocationLink>> {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for goto request: {}", error);
    }
    let result = response.get_mut("result").map(serde_json::Value::take)?;
    if result.is_null() {
        return None;
    }

    // The LSP spec defines GotoDefinitionResponse as: Location | Location[] | LocationLink[]
    // Normalize all formats to Vec<LocationLink> for simpler internal handling

    if result.is_object() {
        // Single Location → convert to LocationLink
        if let Ok(location) = serde_json::from_value::<Location>(result) {
            return transform_location_for_goto(
                location,
                request_virtual_uri,
                host_uri,
                region_start_line,
            )
            .map(|loc| vec![location_to_location_link(loc)]);
        }
    } else if result.is_array() {
        // Could be Location[] or LocationLink[]
        let arr = result.as_array()?;
        if arr.is_empty() {
            // Preserve empty arrays (semantic: "searched, found nothing")
            return Some(vec![]);
        }

        // Check if first element has "targetUri" to distinguish LocationLink from Location
        if arr.first()?.get("targetUri").is_some() {
            // LocationLink[] → use directly
            if let Ok(links) = serde_json::from_value::<Vec<LocationLink>>(result) {
                let transformed: Vec<LocationLink> = links
                    .into_iter()
                    .filter_map(|link| {
                        transform_location_link_for_goto(
                            link,
                            request_virtual_uri,
                            host_uri,
                            region_start_line,
                        )
                    })
                    .collect();

                // Preserve empty array after filtering
                return Some(transformed);
            }
        } else {
            // Location[] → convert each to LocationLink
            if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result) {
                let transformed: Vec<LocationLink> = locations
                    .into_iter()
                    .filter_map(|location| {
                        transform_location_for_goto(
                            location,
                            request_virtual_uri,
                            host_uri,
                            region_start_line,
                        )
                        .map(location_to_location_link)
                    })
                    .collect();

                // Preserve empty array after filtering
                return Some(transformed);
            }
        }
    }

    // Failed to deserialize as any known variant
    None
}

/// Convert a Location to LocationLink format.
///
/// This is a lossless conversion - LocationLink is the more feature-rich format.
/// We set `targetSelectionRange` equal to `targetRange` since Location doesn't
/// distinguish between the full symbol range and the selection range.
pub(crate) fn location_to_location_link(location: Location) -> LocationLink {
    LocationLink {
        origin_selection_range: None,
        target_uri: location.uri,
        target_range: location.range,
        target_selection_range: location.range, // Use same range for selection
    }
}

/// Convert a LocationLink to Location for clients that don't support linkSupport.
///
/// Uses `target_selection_range` (the symbol name) rather than `target_range`
/// (the whole definition) for more precise navigation to the symbol itself.
pub(crate) fn location_link_to_location(link: LocationLink) -> Location {
    Location {
        uri: link.target_uri,
        range: link.target_selection_range,
    }
}

/// Transform a single Location to host coordinates for goto endpoints.
///
/// Returns `None` if the location should be filtered out (cross-region virtual URI).
///
/// # URI Filtering Logic
///
/// 1. Real file URI → preserve as-is (cross-file jump to real file) - KEEP
/// 2. Same virtual URI as request → transform using request's context - KEEP
/// 3. Different virtual URI → cross-region jump - FILTER OUT
fn transform_location_for_goto(
    mut location: Location,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<Location> {
    let uri_str = location.uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return Some(location);
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        location.uri = host_uri.clone();
        transform_range_for_goto(&mut location.range, region_start_line);
        return Some(location);
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    None
}

/// Transform a single LocationLink to host coordinates for goto endpoints.
///
/// Returns `None` if the location should be filtered out (cross-region virtual URI).
///
/// All ranges (targetRange, targetSelectionRange, originSelectionRange) are in virtual
/// coordinates from the downstream server and need the region_start_line offset applied.
fn transform_location_link_for_goto(
    mut link: LocationLink,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<LocationLink> {
    let uri_str = link.target_uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !VirtualDocumentUri::is_virtual_uri(uri_str) {
        return Some(link);
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        link.target_uri = host_uri.clone();
        transform_range_for_goto(&mut link.target_range, region_start_line);
        transform_range_for_goto(&mut link.target_selection_range, region_start_line);
        if let Some(ref mut origin_range) = link.origin_selection_range {
            transform_range_for_goto(origin_range, region_start_line);
        }
        return Some(link);
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    None
}

/// Transform a range from virtual to host coordinates for goto endpoints.
///
/// Uses `saturating_add` to prevent overflow, consistent with `saturating_sub`
/// used elsewhere in the codebase for defensive arithmetic.
fn transform_range_for_goto(range: &mut Range, region_start_line: u32) {
    range.start.line = range.start.line.saturating_add(region_start_line);
    range.end.line = range.end.line.saturating_add(region_start_line);
}

/// Transform references response to typed Vec<Location> format.
///
/// This function handles the references endpoint response which returns
/// Location[] | null according to the LSP spec.
///
/// # URI Filtering Logic
///
/// Same as goto endpoints:
/// - Real file URIs → keep as-is (cross-file jumps)
/// - Same virtual URI as request → transform coordinates
/// - Different virtual URI → filter out (cross-region, can't transform safely)
///
/// Empty arrays after filtering are preserved to distinguish "searched, found nothing"
/// from "search failed" (None).
///
/// # Arguments
/// * `response` - Raw JSON-RPC response envelope (`{"result": {...}}`)
/// * `request_virtual_uri` - The virtual URI from the request
/// * `host_uri` - The pre-parsed host URI to use in transformed responses
/// * `region_start_line` - Line offset to add when transforming to host coordinates
pub(crate) fn transform_references_response_to_host(
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    host_uri: &Uri,
    region_start_line: u32,
) -> Option<Vec<Location>> {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for textDocument/references: {}", error);
    }
    let result = response.get_mut("result").map(serde_json::Value::take)?;
    if result.is_null() {
        return None;
    }

    // The LSP spec defines ReferenceResponse as: Location[] | null
    // References only returns arrays of Location (simpler than goto endpoints)

    if result.is_array() {
        let arr = result.as_array()?;
        if arr.is_empty() {
            // Preserve empty arrays (semantic: "searched, found nothing")
            return Some(vec![]);
        }

        // Location[] → transform each location
        if let Ok(locations) = serde_json::from_value::<Vec<Location>>(result) {
            let transformed: Vec<Location> = locations
                .into_iter()
                .filter_map(|location| {
                    transform_location_for_goto(
                        location,
                        request_virtual_uri,
                        host_uri,
                        region_start_line,
                    )
                })
                .collect();

            // Preserve empty array after filtering
            return Some(transformed);
        }
    }

    // Failed to deserialize as Location[]
    None
}

// =============================================================================
// JSON-based transformers (legacy, used by non-refactored endpoints)
// =============================================================================

/// Transform a Location's uri and range based on URI type.
///
/// This is a JSON-based helper used by the document_symbol transformer.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
///
/// Handles three cases:
/// 1. Real file URI (not virtual): preserved as-is
/// 2. Same virtual URI as request: URI replaced with host URI, range transformed
/// 3. Different virtual URI (cross-region): filtered out
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
/// - location.uri: The symbol's document URI (needs transformation if virtual)
/// - location.range: The symbol's location range (needs transformation)
///
/// This function handles three cases for SymbolInformation URIs:
/// 1. **Real file URI** (not a virtual URI): Preserved as-is with original coordinates
/// 2. **Same virtual URI as request**: Transformed using request's context
/// 3. **Different virtual URI** (cross-region): Filtered out from results
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `context` - The transformation context with request information
pub(crate) fn transform_document_symbol_response_to_host(
    mut response: serde_json::Value,
    context: &ResponseTransformContext,
) -> serde_json::Value {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for textDocument/documentSymbol: {}", error);
    }
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    // Null result - pass through unchanged
    if result.is_null() {
        return response;
    }

    // DocumentSymbol[] or SymbolInformation[] is an array
    if let Some(items) = result.as_array_mut() {
        // Filter out cross-region SymbolInformation items, transform the rest
        items.retain_mut(|item| transform_document_symbol_item(item, context));
    }

    response
}

/// Transform a single DocumentSymbol or SymbolInformation item.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
/// (Only SymbolInformation items with cross-region virtual URIs are filtered.)
fn transform_document_symbol_item(
    item: &mut serde_json::Value,
    context: &ResponseTransformContext,
) -> bool {
    // DocumentSymbol format: has range + selectionRange (no uri field)
    if let Some(range) = item.get_mut("range")
        && range.is_object()
    {
        transform_range(range, context.request_region_start_line);
    }

    if let Some(selection_range) = item.get_mut("selectionRange")
        && selection_range.is_object()
    {
        transform_range(selection_range, context.request_region_start_line);
    }

    // Recursively transform children (DocumentSymbol only)
    if let Some(children) = item.get_mut("children")
        && let Some(children_arr) = children.as_array_mut()
    {
        // Filter out cross-region children too
        children_arr.retain_mut(|child| transform_document_symbol_item(child, context));
    }

    // SymbolInformation format: has location.uri + location.range
    if let Some(location) = item.get_mut("location") {
        // Get the URI to determine how to handle this item
        if let Some(uri_str) = location
            .get("uri")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
        {
            // Use the shared context-based URI transformation logic
            return transform_location_uri(location, &uri_str, "uri", "range", context);
        }
    }

    // No location field (DocumentSymbol format) - always keep
    true
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
#[cfg(feature = "experimental")]
pub(crate) fn transform_document_color_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for textDocument/documentColor: {}", error);
    }
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
#[cfg(feature = "experimental")]
pub(crate) fn transform_color_presentation_response_to_host(
    mut response: serde_json::Value,
    region_start_line: u32,
) -> serde_json::Value {
    if let Some(error) = response.get("error") {
        warn!(target: "kakehashi::bridge", "Downstream server returned error for textDocument/colorPresentation: {}", error);
    }
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

/// Check if a URI string represents a virtual document.
///
/// Delegates to [`VirtualDocumentUri::is_virtual_uri`] which is the single source of truth
/// for virtual URI format knowledge.
pub(crate) fn is_virtual_uri(uri: &str) -> bool {
    VirtualDocumentUri::is_virtual_uri(uri)
}

/// Context for transforming JSON-based responses to host coordinates.
///
/// Used by context-based transformers (e.g., document symbol) that need
/// the full request context to distinguish between real
/// file URIs, same-region virtual URIs, and cross-region virtual URIs.
#[derive(Debug, Clone)]
pub(crate) struct ResponseTransformContext {
    /// The virtual URI string we sent in the request
    pub(crate) request_virtual_uri: String,
    /// The host URI string for the request
    pub(crate) request_host_uri: String,
    /// The region start line for the request's injection region
    pub(crate) request_region_start_line: u32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

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
        // DocumentSymbol format doesn't have URI - use dummy context
        let context = test_context("unused", "unused", 3);

        let transformed = transform_document_symbol_response_to_host(response, &context);

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
        // DocumentSymbol format doesn't have URI - use dummy context
        let context = test_context("unused", "unused", 5);

        let transformed = transform_document_symbol_response_to_host(response, &context);

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
        // Uses real file URIs which are preserved as-is
        let real_file_uri = "file:///test.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "myVariable",
                    "kind": 13,
                    "location": {
                        "uri": real_file_uri,
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
                        "uri": real_file_uri,
                        "range": {
                            "start": { "line": 5, "character": 0 },
                            "end": { "line": 10, "character": 3 }
                        }
                    }
                }
            ]
        });
        // Real file URIs are preserved, but ranges still need transformation
        let context = test_context(
            "file:///project/kakehashi-virtual-uri-region-0.lua",
            "file:///doc.md",
            7,
        );

        let transformed = transform_document_symbol_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2);

        // First symbol: real file URI preserved, range NOT transformed (real file, no offset)
        assert_eq!(result[0]["location"]["uri"], real_file_uri);
        assert_eq!(result[0]["location"]["range"]["start"]["line"], 2);
        assert_eq!(result[0]["location"]["range"]["end"]["line"], 2);
        assert_eq!(result[0]["name"], "myVariable");

        // Second symbol: real file URI preserved, range NOT transformed
        assert_eq!(result[1]["location"]["uri"], real_file_uri);
        assert_eq!(result[1]["location"]["range"]["start"]["line"], 5);
        assert_eq!(result[1]["location"]["range"]["end"]["line"], 10);
        assert_eq!(result[1]["name"], "myFunction");
    }

    #[test]
    fn document_symbol_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });
        let context = test_context("unused", "unused", 5);

        let transformed = transform_document_symbol_response_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_symbol_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });
        let context = test_context("unused", "unused", 5);

        let transformed = transform_document_symbol_response_to_host(response.clone(), &context);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn document_symbol_response_transforms_symbol_information_location_uri_to_host_uri() {
        // SymbolInformation format with virtual URI - should transform to host URI
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "myVariable",
                    "kind": 13,
                    "location": {
                        "uri": virtual_uri,
                        "range": {
                            "start": { "line": 2, "character": 6 },
                            "end": { "line": 2, "character": 16 }
                        }
                    }
                }
            ]
        });

        let context = test_context(virtual_uri, host_uri, 7);
        let transformed = transform_document_symbol_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 1);
        // URI should be transformed from virtual to host
        assert_eq!(result[0]["location"]["uri"], host_uri);
        // Range should still be transformed
        assert_eq!(result[0]["location"]["range"]["start"]["line"], 9);
        assert_eq!(result[0]["location"]["range"]["end"]["line"], 9);
    }

    #[test]
    fn document_symbol_response_filters_out_cross_region_symbol_information() {
        // SymbolInformation with cross-region virtual URI should be filtered out
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let cross_region_uri = "file:///project/kakehashi-virtual-uri-region-1.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "crossRegionSymbol",
                    "kind": 13,
                    "location": {
                        "uri": cross_region_uri,
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 10 }
                        }
                    }
                }
            ]
        });

        let context = test_context(request_virtual_uri, "file:///doc.md", 5);
        let transformed = transform_document_symbol_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert!(
            result.is_empty(),
            "Cross-region SymbolInformation should be filtered out"
        );
    }

    #[test]
    fn document_symbol_response_preserves_real_file_uri_in_symbol_information() {
        // SymbolInformation with real file URI should be preserved
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let real_file_uri = "file:///real/path/module.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "externalSymbol",
                    "kind": 12,
                    "location": {
                        "uri": real_file_uri,
                        "range": {
                            "start": { "line": 10, "character": 0 },
                            "end": { "line": 15, "character": 3 }
                        }
                    }
                }
            ]
        });

        let context = test_context(virtual_uri, "file:///doc.md", 5);
        let transformed = transform_document_symbol_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 1);
        // Real file URI should be preserved
        assert_eq!(result[0]["location"]["uri"], real_file_uri);
        // Range should NOT be transformed (real file, no offset)
        assert_eq!(result[0]["location"]["range"]["start"]["line"], 10);
        assert_eq!(result[0]["location"]["range"]["end"]["line"], 15);
    }

    #[test]
    fn document_symbol_response_mixed_symbol_information_filters_only_cross_region() {
        // Mixed array: same virtual, cross-region virtual, real file
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let cross_region_uri = "file:///project/kakehashi-virtual-uri-region-1.lua";
        let real_file_uri = "file:///real/module.lua";
        let host_uri = "file:///doc.md";

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "name": "localSymbol",
                    "kind": 13,
                    "location": {
                        "uri": request_virtual_uri,
                        "range": { "start": { "line": 0, "character": 0 }, "end": { "line": 0, "character": 10 } }
                    }
                },
                {
                    "name": "crossRegionSymbol",
                    "kind": 12,
                    "location": {
                        "uri": cross_region_uri,
                        "range": { "start": { "line": 5, "character": 0 }, "end": { "line": 5, "character": 15 } }
                    }
                },
                {
                    "name": "externalSymbol",
                    "kind": 6,
                    "location": {
                        "uri": real_file_uri,
                        "range": { "start": { "line": 20, "character": 0 }, "end": { "line": 25, "character": 3 } }
                    }
                }
            ]
        });

        let context = test_context(request_virtual_uri, host_uri, 5);
        let transformed = transform_document_symbol_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(
            result.len(),
            2,
            "Should have 2 items (cross-region filtered out)"
        );

        // First: local symbol transformed
        assert_eq!(result[0]["name"], "localSymbol");
        assert_eq!(result[0]["location"]["uri"], host_uri);
        assert_eq!(result[0]["location"]["range"]["start"]["line"], 5);

        // Second: external symbol preserved
        assert_eq!(result[1]["name"], "externalSymbol");
        assert_eq!(result[1]["location"]["uri"], real_file_uri);
        assert_eq!(result[1]["location"]["range"]["start"]["line"], 20);
    }

    // ==========================================================================
    // Document color response transformation tests
    // ==========================================================================

    #[test]
    #[cfg(feature = "experimental")]
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
    #[cfg(feature = "experimental")]
    fn document_color_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_color_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn document_color_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_color_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    #[test]
    #[cfg(feature = "experimental")]
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
    #[cfg(feature = "experimental")]
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
    #[cfg(feature = "experimental")]
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
    #[cfg(feature = "experimental")]
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
    #[cfg(feature = "experimental")]
    fn color_presentation_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_color_presentation_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    #[cfg(feature = "experimental")]
    fn color_presentation_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_color_presentation_response_to_host(response.clone(), 5);
        let result = transformed["result"].as_array().unwrap();
        assert!(result.is_empty());
    }

    // ==========================================================================
    // JSON range transformation edge case tests
    // ==========================================================================

    #[test]
    fn range_transformation_with_near_max_line_saturates() {
        // Test transform_range directly with values that would overflow
        let mut range = json!({
            "start": { "line": u64::MAX - 5, "character": 0 },
            "end": { "line": u64::MAX - 3, "character": 10 }
        });

        // Adding 10 would overflow, should saturate to MAX
        transform_range(&mut range, 10);

        assert_eq!(range["start"]["line"], u64::MAX);
        assert_eq!(range["end"]["line"], u64::MAX);
        // Characters should be unchanged
        assert_eq!(range["start"]["character"], 0);
        assert_eq!(range["end"]["character"], 10);
    }
}
