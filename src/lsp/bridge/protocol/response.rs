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
//! Examples: moniker (passthrough), document highlight, document link
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
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
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
    // Extract result from JSON-RPC envelope, taking ownership to avoid clones
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
/// This is a JSON-based helper used by document_symbol and inlay_hint transformers.
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

/// Transform a document highlight response from virtual to host document coordinates.
///
/// DocumentHighlight responses are arrays of items with range and optional kind.
/// This function transforms each range's line numbers by adding region_start_line.
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `region_start_line` - The starting line of the injection region in the host document
pub(crate) fn transform_document_highlight_response_to_host(
    response: serde_json::Value,
    region_start_line: u32,
) -> Option<Vec<tower_lsp_server::ls_types::DocumentHighlight>> {
    use tower_lsp_server::ls_types::DocumentHighlight;

    // Get result field
    let result = response.get("result")?;

    // Null result - return None
    if result.is_null() {
        return None;
    }

    // Parse into typed Vec<DocumentHighlight>
    let mut highlights: Vec<DocumentHighlight> = serde_json::from_value(result.clone()).ok()?;

    // Transform ranges to host coordinates
    for highlight in &mut highlights {
        highlight.range.start.line = highlight.range.start.line.saturating_add(region_start_line);
        highlight.range.end.line = highlight.range.end.line.saturating_add(region_start_line);
    }

    Some(highlights)
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

/// Transform an inlay hint response from virtual to host document coordinates.
///
/// InlayHint responses are arrays of items where each hint has:
/// - position: The position where the hint should appear (needs transformation)
/// - label: The hint text (string or InlayHintLabelPart[] with optional location)
/// - kind, tooltip, paddingLeft, paddingRight, data: Optional fields (preserved unchanged)
/// - textEdits: Optional array of TextEdit (needs transformation, handled separately)
///
/// When label is an array of InlayHintLabelPart, each part may have a location field
/// that needs URI and range transformation following the same context-based pattern.
///
/// This function handles three cases for each location URI:
/// 1. **Real file URI** (not a virtual URI): Preserved as-is with original coordinates
/// 2. **Same virtual URI as request**: Transformed using request's context
/// 3. **Different virtual URI** (cross-region): Label part filtered out
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `context` - The transformation context with request information
pub(crate) fn transform_inlay_hint_response_to_host(
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

    // InlayHint[] is an array
    if let Some(items) = result.as_array_mut() {
        for item in items.iter_mut() {
            transform_inlay_hint_item(item, context);
        }
    }

    response
}

/// Transform a single InlayHint item's position, textEdits, and label parts to host coordinates.
fn transform_inlay_hint_item(item: &mut serde_json::Value, context: &ResponseTransformContext) {
    let region_start_line = context.request_region_start_line;

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

    // Transform label parts if label is an array (InlayHintLabelPart[])
    // Per LSP 3.17: label can be string | InlayHintLabelPart[]
    // InlayHintLabelPart has optional location: { uri, range }
    // Cross-region parts are filtered out, same as other context-based responses
    if let Some(label) = item.get_mut("label")
        && let Some(label_parts) = label.as_array_mut()
    {
        label_parts.retain_mut(|part| transform_inlay_hint_label_part(part, context));
    }
}

/// Transform a single InlayHintLabelPart's location to host coordinates.
///
/// Returns `true` if the part should be kept, `false` if it should be filtered out.
///
/// Handles three cases for the location URI:
/// 1. Real file URI (not virtual): preserved as-is, range NOT transformed
/// 2. Same virtual URI as request: URI replaced with host URI, range transformed
/// 3. Different virtual URI (cross-region): filtered out
fn transform_inlay_hint_label_part(
    part: &mut serde_json::Value,
    context: &ResponseTransformContext,
) -> bool {
    // Parts without location field are always kept
    let Some(location) = part.get_mut("location") else {
        return true;
    };

    // Get the URI to determine how to handle this part
    let Some(uri_str) = location
        .get("uri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    else {
        return true; // No URI, keep the part
    };

    // Use the standard URI transformation helper
    // This handles all three cases: real file, same virtual, cross-region
    transform_location_uri(location, &uri_str, "uri", "range", context)
}

/// Transform a position's line number from virtual to host coordinates.
fn transform_position(position: &mut serde_json::Value, region_start_line: u32) {
    if let Some(line) = position.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
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
#[cfg(feature = "experimental")]
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
#[cfg(feature = "experimental")]
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
/// This function parses the JSON response into typed Vec<Moniker>, preserving:
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
) -> Option<Vec<tower_lsp_server::ls_types::Moniker>> {
    // Get result field
    let result = response.get("result")?;

    // Null result - return None
    if result.is_null() {
        return None;
    }

    // Parse into typed Vec<Moniker>
    // Moniker doesn't have ranges that need transformation.
    // scheme, identifier, unique, and kind are all non-coordinate data.
    serde_json::from_value(result.clone()).ok()
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
/// Used by context-based transformers (e.g., workspace edit, document symbol,
/// inlay hint) that need the full request context to distinguish between real
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

        assert!(transformed.is_some());
        let highlights = transformed.unwrap();
        assert_eq!(highlights.len(), 3);
        assert_eq!(highlights[0].range.start.line, 3);
        assert_eq!(highlights[0].range.end.line, 3);
        assert!(highlights[0].kind.is_some());
        assert_eq!(highlights[1].range.start.line, 5);
        assert_eq!(highlights[1].range.end.line, 5);
        assert!(highlights[1].kind.is_some());
        assert_eq!(highlights[2].range.start.line, 7);
    }

    #[test]
    fn document_highlight_response_with_null_result_returns_none() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_document_highlight_response_to_host(response, 5);
        assert!(transformed.is_none());
    }

    #[test]
    fn document_highlight_response_with_empty_array_returns_empty_vec() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_document_highlight_response_to_host(response, 5);
        assert!(transformed.is_some());
        let highlights = transformed.unwrap();
        assert!(highlights.is_empty());
    }

    #[test]
    fn document_highlight_response_preserves_character_coordinates() {
        // Character coordinates should not be transformed, only line numbers
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 15 },
                        "end": { "line": 1, "character": 20 }
                    }
                }
            ]
        });
        let region_start_line = 10;

        let transformed =
            transform_document_highlight_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let highlights = transformed.unwrap();
        assert_eq!(highlights.len(), 1);
        assert_eq!(highlights[0].range.start.line, 10); // 0 + 10
        assert_eq!(highlights[0].range.start.character, 15); // Preserved
        assert_eq!(highlights[0].range.end.line, 11); // 1 + 10
        assert_eq!(highlights[0].range.end.character, 20); // Preserved
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
            "file:///project/kakehashi-virtual-uri-region-0.lua",
            "file:///doc.md",
            5,
        );

        let transformed = transform_workspace_edit_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    #[test]
    fn workspace_edit_transforms_textedit_ranges_in_changes_map() {
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
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
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
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
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
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
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let cross_region_uri = "file:///project/kakehashi-virtual-uri-region-1.lua";
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
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
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
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
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
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
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
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let cross_region_uri = "file:///project/kakehashi-virtual-uri-region-1.lua";
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
    // Inlay hint response transformation tests
    // ==========================================================================

    /// Helper to create a ResponseTransformContext for inlay hint tests.
    /// Uses dummy URIs since most tests only need region_start_line.
    fn inlay_hint_context(region_start_line: u32) -> ResponseTransformContext {
        ResponseTransformContext {
            request_virtual_uri: "file:///test.md.lua.region1".to_string(),
            request_host_uri: "file:///test.md".to_string(),
            request_region_start_line: region_start_line,
        }
    }

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
        let context = inlay_hint_context(5);

        let transformed = transform_inlay_hint_response_to_host(response, &context);

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

        let transformed =
            transform_inlay_hint_response_to_host(response.clone(), &inlay_hint_context(5));
        assert_eq!(transformed, response);
    }

    #[test]
    fn inlay_hint_response_with_empty_array_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed =
            transform_inlay_hint_response_to_host(response.clone(), &inlay_hint_context(5));
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
        let context = inlay_hint_context(3);

        let transformed = transform_inlay_hint_response_to_host(response, &context);

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
        let context = inlay_hint_context(5);

        let transformed = transform_inlay_hint_response_to_host(response, &context);

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
        let context = inlay_hint_context(10);

        let transformed = transform_inlay_hint_response_to_host(response, &context);

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
        let context = inlay_hint_context(3);

        let transformed = transform_inlay_hint_response_to_host(response, &context);

        let hint = &transformed["result"][0];
        assert_eq!(hint["position"]["line"], 3);
        assert!(hint.get("textEdits").is_none());
    }

    #[test]
    fn inlay_hint_label_part_location_range_transforms_to_host_coordinates() {
        // InlayHint with label as array of InlayHintLabelPart with location field
        // Per LSP 3.17: label can be string | InlayHintLabelPart[]
        // InlayHintLabelPart.location is { uri, range }
        // Use proper virtual URI format (kakehashi-virtual-uri-{id}.{ext}) so is_virtual_uri recognizes it
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let host_uri = "file:///test.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 10 },
                "label": [
                    {
                        "value": "SomeType",
                        "location": {
                            "uri": virtual_uri,
                            "range": {
                                "start": { "line": 5, "character": 0 },
                                "end": { "line": 5, "character": 8 }
                            }
                        }
                    }
                ]
            }]
        });
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 10,
        };

        let transformed = transform_inlay_hint_response_to_host(response, &context);

        let hint = &transformed["result"][0];
        // Position should be transformed
        assert_eq!(hint["position"]["line"], 10);
        // Label part location range should be transformed: line 5 + 10 = 15
        let label_part = &hint["label"][0];
        assert_eq!(label_part["value"], "SomeType");
        assert_eq!(label_part["location"]["range"]["start"]["line"], 15);
        assert_eq!(label_part["location"]["range"]["end"]["line"], 15);
    }

    #[test]
    fn inlay_hint_label_part_location_uri_transforms_to_host_uri() {
        // When location.uri matches the request's virtual URI, it should be replaced with host URI
        // Use proper virtual URI format (kakehashi-virtual-uri-{id}.{ext}) so is_virtual_uri recognizes it
        let virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let host_uri = "file:///test.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 10 },
                "label": [
                    {
                        "value": "MyType",
                        "location": {
                            "uri": virtual_uri,
                            "range": {
                                "start": { "line": 2, "character": 0 },
                                "end": { "line": 2, "character": 6 }
                            }
                        }
                    }
                ]
            }]
        });
        // Create context with matching URIs
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 5,
        };

        let transformed = transform_inlay_hint_response_to_host(response, &context);

        let label_part = &transformed["result"][0]["label"][0];
        // URI should be transformed from virtual to host
        assert_eq!(label_part["location"]["uri"], host_uri);
        // Range should also be transformed
        assert_eq!(label_part["location"]["range"]["start"]["line"], 7);
    }

    #[test]
    fn inlay_hint_label_part_cross_region_location_is_filtered_out() {
        // When location.uri is a DIFFERENT virtual URI (cross-region), the part should be removed
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let different_virtual_uri = "file:///project/kakehashi-virtual-uri-region-1.lua"; // Different region
        let host_uri = "file:///test.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 10 },
                "label": [
                    {
                        "value": "SameRegion",
                        "location": {
                            "uri": request_virtual_uri,
                            "range": {
                                "start": { "line": 2, "character": 0 },
                                "end": { "line": 2, "character": 10 }
                            }
                        }
                    },
                    {
                        "value": "CrossRegion",
                        "location": {
                            "uri": different_virtual_uri,
                            "range": {
                                "start": { "line": 5, "character": 0 },
                                "end": { "line": 5, "character": 11 }
                            }
                        }
                    }
                ]
            }]
        });
        let context = ResponseTransformContext {
            request_virtual_uri: request_virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 10,
        };

        let transformed = transform_inlay_hint_response_to_host(response, &context);

        // The label array should have only 1 part (CrossRegion filtered out)
        let label = transformed["result"][0]["label"].as_array().unwrap();
        assert_eq!(label.len(), 1, "Cross-region part should be filtered out");
        assert_eq!(label[0]["value"], "SameRegion");
        // Same-region part should have URI transformed and range offset applied
        assert_eq!(label[0]["location"]["uri"], host_uri);
        assert_eq!(label[0]["location"]["range"]["start"]["line"], 12);
    }

    #[test]
    fn inlay_hint_label_part_real_file_uri_preserved_unchanged() {
        // When location.uri is a real file (not virtual), it should be preserved as-is
        // Note: Real file URIs don't have ranges transformed (they reference real file positions)
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let real_file_uri = "file:///usr/local/lib/lua/5.4/types.lua"; // External library file
        let host_uri = "file:///test.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 10 },
                "label": [
                    {
                        "value": "ExternalType",
                        "location": {
                            "uri": real_file_uri,
                            "range": {
                                "start": { "line": 100, "character": 0 },
                                "end": { "line": 100, "character": 12 }
                            }
                        }
                    }
                ]
            }]
        });
        let context = ResponseTransformContext {
            request_virtual_uri: request_virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 10,
        };

        let transformed = transform_inlay_hint_response_to_host(response, &context);

        let label_part = &transformed["result"][0]["label"][0];
        // Real file URI should be preserved unchanged
        assert_eq!(label_part["location"]["uri"], real_file_uri);
        // Range should NOT be transformed (it's a real file, not a virtual document)
        assert_eq!(label_part["location"]["range"]["start"]["line"], 100);
    }

    #[test]
    fn inlay_hint_label_part_without_location_preserved_unchanged() {
        // Label parts with only value/tooltip/command (no location) should be preserved
        let request_virtual_uri = "file:///project/kakehashi-virtual-uri-region-0.lua";
        let host_uri = "file:///test.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "position": { "line": 0, "character": 10 },
                "label": [
                    {
                        "value": "SimpleHint",
                        "tooltip": "A simple tooltip"
                    },
                    {
                        "value": " -> ",
                        "command": { "title": "Do something", "command": "action" }
                    }
                ]
            }]
        });
        let context = ResponseTransformContext {
            request_virtual_uri: request_virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 10,
        };

        let transformed = transform_inlay_hint_response_to_host(response, &context);

        // Both label parts should be preserved
        let label = transformed["result"][0]["label"].as_array().unwrap();
        assert_eq!(label.len(), 2);
        assert_eq!(label[0]["value"], "SimpleHint");
        assert_eq!(label[0]["tooltip"], "A simple tooltip");
        assert!(label[0].get("location").is_none());
        assert_eq!(label[1]["value"], " -> ");
        assert_eq!(label[1]["command"]["title"], "Do something");
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
    // Moniker response tests
    // ==========================================================================

    #[test]
    fn moniker_response_returns_typed_vec() {
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

        let transformed = transform_moniker_response_to_host(response, region_start_line);

        assert!(transformed.is_some());
        let monikers = transformed.unwrap();
        assert_eq!(monikers.len(), 2);
        assert_eq!(monikers[0].scheme, "tsc");
        assert_eq!(monikers[0].identifier, "typescript:foo:bar:Baz");
        assert_eq!(monikers[1].scheme, "npm");
        assert_eq!(monikers[1].identifier, "package:module:Class.method");
    }

    #[test]
    fn moniker_response_with_null_result_returns_none() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });

        let transformed = transform_moniker_response_to_host(response, 5);
        assert!(transformed.is_none());
    }

    #[test]
    fn moniker_response_with_empty_array_returns_empty_vec() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": [] });

        let transformed = transform_moniker_response_to_host(response, 5);
        assert!(transformed.is_some());
        let monikers = transformed.unwrap();
        assert!(monikers.is_empty());
    }

    #[test]
    fn moniker_response_ignores_region_start_line() {
        // Verify that region_start_line parameter has no effect on moniker response
        // since moniker doesn't contain coordinate data
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "scheme": "test",
                    "identifier": "test:identifier",
                    "unique": "project"
                }
            ]
        });

        // Transform with different region_start_line values
        let transformed1 = transform_moniker_response_to_host(response.clone(), 0);
        let transformed2 = transform_moniker_response_to_host(response, 1000);

        // Both should produce identical results
        assert!(transformed1.is_some());
        assert!(transformed2.is_some());
        let monikers1 = transformed1.unwrap();
        let monikers2 = transformed2.unwrap();
        assert_eq!(monikers1.len(), monikers2.len());
        assert_eq!(monikers1[0].scheme, monikers2[0].scheme);
        assert_eq!(monikers1[0].identifier, monikers2[0].identifier);
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
