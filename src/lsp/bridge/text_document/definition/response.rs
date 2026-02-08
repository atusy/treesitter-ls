//! Response transformation for definition requests.
//!
//! Transforms definition responses from virtual to host document coordinates.
//! Handles Location and LocationLink formats, and filters out cross-region references.

use std::str::FromStr;
use tower_lsp_server::ls_types::{GotoDefinitionResponse, Location, LocationLink, Position, Range};

/// Transform a single position from virtual to host coordinates.
///
/// Uses saturating_add to prevent overflow for large line numbers.
fn transform_position(position: Position, region_start_line: u32) -> Position {
    Position {
        line: position.line.saturating_add(region_start_line),
        character: position.character,
    }
}

/// Transform a range from virtual to host coordinates.
///
/// Uses saturating_add to prevent overflow for large line numbers.
fn transform_range(range: Range, region_start_line: u32) -> Range {
    Range {
        start: transform_position(range.start, region_start_line),
        end: transform_position(range.end, region_start_line),
    }
}

/// Check if a URI is a virtual URI (contains the virtual URI marker).
fn is_virtual_uri(uri_str: &str) -> bool {
    uri_str.contains("kakehashi-virtual-uri-")
}

/// Transform a Location from virtual to host coordinates.
///
/// Returns `None` if the location references a different virtual region (cross-region).
fn transform_location(
    location: Location,
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> Option<Location> {
    let uri_str = location.uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(uri_str) {
        return Some(location);
    }

    // Case 2: Same virtual URI as request → transform to host coordinates
    if uri_str == request_virtual_uri {
        let host_uri = tower_lsp_server::ls_types::Uri::from_str(request_host_uri).ok()?;
        return Some(Location {
            uri: host_uri,
            range: transform_range(location.range, request_region_start_line),
        });
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    None
}

/// Transform a LocationLink from virtual to host coordinates.
///
/// Returns `None` if the link references a different virtual region (cross-region).
fn transform_location_link(
    link: LocationLink,
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> Option<LocationLink> {
    let target_uri_str = link.target_uri.as_str();

    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(target_uri_str) {
        return Some(link);
    }

    // Case 2: Same virtual URI as request → transform to host coordinates
    if target_uri_str == request_virtual_uri {
        let host_uri = tower_lsp_server::ls_types::Uri::from_str(request_host_uri).ok()?;
        return Some(LocationLink {
            origin_selection_range: link.origin_selection_range,
            target_uri: host_uri,
            target_range: transform_range(link.target_range, request_region_start_line),
            target_selection_range: transform_range(
                link.target_selection_range,
                request_region_start_line,
            ),
        });
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    None
}

/// Transform a definition response from virtual to host document coordinates.
///
/// Handles three cases for each URI in the response:
/// 1. Real file URI → preserve as-is (cross-file jump to real file)
/// 2. Same virtual URI as request → transform using request's context
/// 3. Different virtual URI → cross-region jump - filter out
///
/// # Arguments
/// * `response` - The JSON-RPC response from the downstream language server
/// * `request_virtual_uri` - The virtual URI used in the request
/// * `request_host_uri` - The host URI to map back to
/// * `request_region_start_line` - The starting line of the injection region
pub(super) fn transform_definition_response_to_host(
    response: serde_json::Value,
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> Option<GotoDefinitionResponse> {
    // Extract the result field
    let result = response.get("result")?;

    // Handle null result
    if result.is_null() {
        return None;
    }

    // Try to deserialize as GotoDefinitionResponse
    let definition_response: GotoDefinitionResponse =
        serde_json::from_value(result.clone()).ok()?;

    // Transform based on the variant
    match definition_response {
        GotoDefinitionResponse::Scalar(location) => {
            // Single Location
            transform_location(
                location,
                request_virtual_uri,
                request_host_uri,
                request_region_start_line,
            )
            .map(GotoDefinitionResponse::Scalar)
        }
        GotoDefinitionResponse::Array(locations) => {
            // Vec<Location>
            let transformed: Vec<Location> = locations
                .into_iter()
                .filter_map(|loc| {
                    transform_location(
                        loc,
                        request_virtual_uri,
                        request_host_uri,
                        request_region_start_line,
                    )
                })
                .collect();

            if transformed.is_empty() {
                None
            } else {
                Some(GotoDefinitionResponse::Array(transformed))
            }
        }
        GotoDefinitionResponse::Link(links) => {
            // Vec<LocationLink>
            let transformed: Vec<LocationLink> = links
                .into_iter()
                .filter_map(|link| {
                    transform_location_link(
                        link,
                        request_virtual_uri,
                        request_host_uri,
                        request_region_start_line,
                    )
                })
                .collect();

            if transformed.is_empty() {
                None
            } else {
                Some(GotoDefinitionResponse::Link(transformed))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const TEST_VIRTUAL_URI: &str = "file:///lua/kakehashi-virtual-uri-region-0.lua";
    const TEST_HOST_URI: &str = "file:///test.md";
    const TEST_REGION_START_LINE: u32 = 5;

    #[test]
    fn transforms_single_location_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": TEST_VIRTUAL_URI,
                "range": {
                    "start": { "line": 2, "character": 5 },
                    "end": { "line": 2, "character": 15 }
                }
            }
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri.as_str(), TEST_HOST_URI);
                assert_eq!(location.range.start.line, 7); // 2 + 5
                assert_eq!(location.range.end.line, 7);
                assert_eq!(location.range.start.character, 5); // unchanged
            }
            _ => panic!("Expected Scalar variant"),
        }
    }

    #[test]
    fn transforms_location_array_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": TEST_VIRTUAL_URI,
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 10 }
                    }
                },
                {
                    "uri": TEST_VIRTUAL_URI,
                    "range": {
                        "start": { "line": 3, "character": 5 },
                        "end": { "line": 4, "character": 0 }
                    }
                }
            ]
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 2);

                // First location: line 0 + 5 = 5
                assert_eq!(locations[0].uri.as_str(), TEST_HOST_URI);
                assert_eq!(locations[0].range.start.line, 5);
                assert_eq!(locations[0].range.end.line, 5);

                // Second location: lines 3,4 + 5 = 8,9
                assert_eq!(locations[1].uri.as_str(), TEST_HOST_URI);
                assert_eq!(locations[1].range.start.line, 8);
                assert_eq!(locations[1].range.end.line, 9);
            }
            _ => panic!("Expected Array variant"),
        }
    }

    #[test]
    fn transforms_location_link_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "targetUri": TEST_VIRTUAL_URI,
                "targetRange": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 3, "character": 5 }
                },
                "targetSelectionRange": {
                    "start": { "line": 1, "character": 9 },
                    "end": { "line": 1, "character": 14 }
                }
            }]
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Link(links) => {
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].target_uri.as_str(), TEST_HOST_URI);
                assert_eq!(links[0].target_range.start.line, 6); // 1 + 5
                assert_eq!(links[0].target_range.end.line, 8); // 3 + 5
                assert_eq!(links[0].target_selection_range.start.line, 6);
                assert_eq!(links[0].target_selection_range.end.line, 6);
            }
            _ => panic!("Expected Link variant"),
        }
    }

    #[test]
    fn preserves_real_file_location() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": "file:///other_module.lua",
                "range": {
                    "start": { "line": 10, "character": 0 },
                    "end": { "line": 10, "character": 5 }
                }
            }
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Scalar(location) => {
                assert_eq!(location.uri.as_str(), "file:///other_module.lua");
                assert_eq!(location.range.start.line, 10); // unchanged
                assert_eq!(location.range.end.line, 10);
            }
            _ => panic!("Expected Scalar variant"),
        }
    }

    #[test]
    fn preserves_real_file_location_link() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [{
                "targetUri": "file:///stdlib/builtin.lua",
                "targetRange": {
                    "start": { "line": 25, "character": 0 },
                    "end": { "line": 28, "character": 3 }
                },
                "targetSelectionRange": {
                    "start": { "line": 25, "character": 4 },
                    "end": { "line": 25, "character": 9 }
                }
            }]
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Link(links) => {
                assert_eq!(links.len(), 1);
                assert_eq!(links[0].target_uri.as_str(), "file:///stdlib/builtin.lua");
                assert_eq!(links[0].target_range.start.line, 25); // unchanged
                assert_eq!(links[0].target_range.end.line, 28);
            }
            _ => panic!("Expected Link variant"),
        }
    }

    #[test]
    fn filters_cross_region_location() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": "file:///lua/kakehashi-virtual-uri-region-1.lua", // Different region
                "range": {
                    "start": { "line": 0, "character": 0 },
                    "end": { "line": 0, "character": 5 }
                }
            }
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        // Should be filtered to None
        assert!(result.is_none());
    }

    #[test]
    fn filters_cross_region_locations_from_array() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "uri": TEST_VIRTUAL_URI, // Same region - keep
                    "range": {
                        "start": { "line": 1, "character": 0 },
                        "end": { "line": 1, "character": 5 }
                    }
                },
                {
                    "uri": "file:///lua/kakehashi-virtual-uri-region-99.lua", // Different region - filter
                    "range": {
                        "start": { "line": 0, "character": 0 },
                        "end": { "line": 0, "character": 5 }
                    }
                },
                {
                    "uri": "file:///real_file.lua", // Real file - keep
                    "range": {
                        "start": { "line": 10, "character": 0 },
                        "end": { "line": 10, "character": 5 }
                    }
                }
            ]
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Array(locations) => {
                assert_eq!(locations.len(), 2); // Only 2 items kept

                assert_eq!(locations[0].uri.as_str(), TEST_HOST_URI);
                assert_eq!(locations[0].range.start.line, 6); // Transformed

                assert_eq!(locations[1].uri.as_str(), "file:///real_file.lua");
                assert_eq!(locations[1].range.start.line, 10); // Unchanged
            }
            _ => panic!("Expected Array variant"),
        }
    }

    #[test]
    fn handles_null_result() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_none());
    }

    #[test]
    fn handles_missing_result() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": { "code": -32603, "message": "internal error" }
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_none());
    }

    #[test]
    fn handles_empty_array() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": []
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        // Empty array should return None (no results)
        assert!(result.is_none());
    }

    #[test]
    fn uses_saturating_add_for_line_numbers() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": TEST_VIRTUAL_URI,
                "range": {
                    "start": { "line": u32::MAX - 2, "character": 0 },
                    "end": { "line": u32::MAX - 2, "character": 5 }
                }
            }
        });

        let result = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert!(result.is_some());
        match result.unwrap() {
            GotoDefinitionResponse::Scalar(location) => {
                // Should saturate at u32::MAX instead of overflowing
                assert_eq!(location.range.start.line, u32::MAX);
                assert_eq!(location.range.end.line, u32::MAX);
            }
            _ => panic!("Expected Scalar variant"),
        }
    }
}
