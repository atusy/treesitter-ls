//! Response transformation for definition requests.
//!
//! Transforms definition responses from virtual to host document coordinates.
//! Handles Location and LocationLink formats, and filters out cross-region references.

/// Transform a range's line numbers from virtual to host coordinates.
///
/// Uses saturating_add to prevent overflow for large line numbers.
fn transform_range(range: &mut serde_json::Value, region_start_line: u32) {
    if let Some(start) = range.get_mut("start")
        && let Some(line) = start.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
    }

    if let Some(end) = range.get_mut("end")
        && let Some(line) = end.get_mut("line")
        && let Some(line_num) = line.as_u64()
    {
        *line = serde_json::json!(line_num.saturating_add(region_start_line as u64));
    }
}

/// Check if a URI is a virtual URI (contains the virtual URI marker).
fn is_virtual_uri(uri: &str) -> bool {
    uri.contains("kakehashi-virtual-uri-")
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
    mut response: serde_json::Value,
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> serde_json::Value {
    let Some(result) = response.get_mut("result") else {
        return response;
    };

    if result.is_null() {
        return response;
    }

    // Array format: Location[] or LocationLink[]
    if let Some(arr) = result.as_array_mut() {
        // Filter out cross-region virtual URIs, transform the rest
        arr.retain_mut(|item| {
            transform_definition_item(
                item,
                request_virtual_uri,
                request_host_uri,
                request_region_start_line,
            )
        });
    } else if result.is_object() {
        // Single Location or LocationLink
        if !transform_definition_item(
            result,
            request_virtual_uri,
            request_host_uri,
            request_region_start_line,
        ) {
            // Item was filtered - return null result
            response["result"] = serde_json::Value::Null;
        }
    }

    response
}

/// Transform a single Location or LocationLink item to host coordinates.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
fn transform_definition_item(
    item: &mut serde_json::Value,
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> bool {
    // Handle Location format (has uri + range)
    if let Some(uri_str) = item
        .get("uri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        return transform_location_uri(
            item,
            &uri_str,
            "uri",
            "range",
            request_virtual_uri,
            request_host_uri,
            request_region_start_line,
        );
    }

    // Handle LocationLink format (has targetUri + targetRange + targetSelectionRange)
    if let Some(target_uri_str) = item
        .get("targetUri")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
    {
        return transform_location_link_target(
            item,
            &target_uri_str,
            request_virtual_uri,
            request_host_uri,
            request_region_start_line,
        );
    }

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
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> bool {
    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(uri_str) {
        return true;
    }

    // Case 2: Same virtual URI as request → use request's context
    if uri_str == request_virtual_uri {
        item[uri_field] = serde_json::json!(request_host_uri);
        if let Some(range) = item.get_mut(range_field) {
            transform_range(range, request_region_start_line);
        }
        return true;
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    false
}

/// Transform a LocationLink's targetUri and associated ranges.
///
/// Returns `true` if the item should be kept, `false` if it should be filtered out.
fn transform_location_link_target(
    item: &mut serde_json::Value,
    target_uri_str: &str,
    request_virtual_uri: &str,
    request_host_uri: &str,
    request_region_start_line: u32,
) -> bool {
    // Case 1: NOT a virtual URI (real file reference) → preserve as-is
    if !is_virtual_uri(target_uri_str) {
        return true;
    }

    // Case 2: Same virtual URI as request → use request's context
    if target_uri_str == request_virtual_uri {
        item["targetUri"] = serde_json::json!(request_host_uri);
        if let Some(range) = item.get_mut("targetRange") {
            transform_range(range, request_region_start_line);
        }
        if let Some(range) = item.get_mut("targetSelectionRange") {
            transform_range(range, request_region_start_line);
        }
        return true;
    }

    // Case 3: Different virtual URI (cross-region) → filter out
    false
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

        let transformed = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        let result = &transformed["result"];
        assert_eq!(result["uri"], TEST_HOST_URI);
        assert_eq!(result["range"]["start"]["line"], 7); // 2 + 5
        assert_eq!(result["range"]["end"]["line"], 7);
        assert_eq!(result["range"]["start"]["character"], 5); // unchanged
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

        let transformed = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2);

        // First location: line 0 + 5 = 5
        assert_eq!(result[0]["uri"], TEST_HOST_URI);
        assert_eq!(result[0]["range"]["start"]["line"], 5);
        assert_eq!(result[0]["range"]["end"]["line"], 5);

        // Second location: lines 3,4 + 5 = 8,9
        assert_eq!(result[1]["uri"], TEST_HOST_URI);
        assert_eq!(result[1]["range"]["start"]["line"], 8);
        assert_eq!(result[1]["range"]["end"]["line"], 9);
    }

    #[test]
    fn transforms_location_link_to_host_coordinates() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "targetUri": TEST_VIRTUAL_URI,
                "targetRange": {
                    "start": { "line": 1, "character": 0 },
                    "end": { "line": 3, "character": 5 }
                },
                "targetSelectionRange": {
                    "start": { "line": 1, "character": 9 },
                    "end": { "line": 1, "character": 14 }
                }
            }
        });

        let transformed = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        let result = &transformed["result"];
        assert_eq!(result["targetUri"], TEST_HOST_URI);
        assert_eq!(result["targetRange"]["start"]["line"], 6); // 1 + 5
        assert_eq!(result["targetRange"]["end"]["line"], 8); // 3 + 5
        assert_eq!(result["targetSelectionRange"]["start"]["line"], 6);
        assert_eq!(result["targetSelectionRange"]["end"]["line"], 6);
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

        let transformed = transform_definition_response_to_host(
            response.clone(),
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        // Should be unchanged
        assert_eq!(transformed, response);
    }

    #[test]
    fn preserves_real_file_location_link() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "targetUri": "file:///stdlib/builtin.lua",
                "targetRange": {
                    "start": { "line": 25, "character": 0 },
                    "end": { "line": 28, "character": 3 }
                },
                "targetSelectionRange": {
                    "start": { "line": 25, "character": 4 },
                    "end": { "line": 25, "character": 9 }
                }
            }
        });

        let transformed = transform_definition_response_to_host(
            response.clone(),
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        // Should be unchanged
        assert_eq!(transformed, response);
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

        let transformed = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        // Should be filtered to null
        assert!(transformed["result"].is_null());
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

        let transformed = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 2); // Only 2 items kept

        assert_eq!(result[0]["uri"], TEST_HOST_URI);
        assert_eq!(result[0]["range"]["start"]["line"], 6); // Transformed

        assert_eq!(result[1]["uri"], "file:///real_file.lua");
        assert_eq!(result[1]["range"]["start"]["line"], 10); // Unchanged
    }

    #[test]
    fn handles_null_result() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let transformed = transform_definition_response_to_host(
            response.clone(),
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert_eq!(transformed, response);
    }

    #[test]
    fn handles_missing_result() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "error": { "code": -32603, "message": "internal error" }
        });

        let transformed = transform_definition_response_to_host(
            response.clone(),
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert_eq!(transformed, response);
    }

    #[test]
    fn handles_empty_array() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": []
        });

        let transformed = transform_definition_response_to_host(
            response.clone(),
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        assert_eq!(transformed, response);
    }

    #[test]
    fn uses_saturating_add_for_line_numbers() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "uri": TEST_VIRTUAL_URI,
                "range": {
                    "start": { "line": u64::MAX - 2, "character": 0 },
                    "end": { "line": u64::MAX - 2, "character": 5 }
                }
            }
        });

        let transformed = transform_definition_response_to_host(
            response,
            TEST_VIRTUAL_URI,
            TEST_HOST_URI,
            TEST_REGION_START_LINE,
        );

        // Should saturate at u64::MAX instead of overflowing
        assert_eq!(transformed["result"]["range"]["start"]["line"], u64::MAX);
    }
}
