//! Sanitization utilities for E2E test snapshot testing.
//!
//! Provides helpers to normalize LSP responses by replacing non-deterministic
//! data (file URIs, timestamps, etc.) with stable placeholders.

use serde_json::Value;

/// Sanitize hover response for snapshot testing.
///
/// Replaces non-deterministic data:
/// - File URIs -> "<TEST_FILE_URI>"
/// - Temp file paths in hover content -> "<TEMP_PATH>"
///
/// # Arguments
/// * `hover` - The Hover response object from LSP
///
/// # Returns
/// * Sanitized Hover object suitable for snapshot comparison
pub fn sanitize_hover_response(hover: &Value) -> Value {
    let mut sanitized = hover.clone();

    // Sanitize range if present
    if let Some(range) = sanitized.get_mut("range") {
        if let Some(uri) = range.get_mut("uri") {
            *uri = Value::String("<TEST_FILE_URI>".to_string());
        }
    }

    // Sanitize contents
    if let Some(contents) = sanitized.get_mut("contents") {
        *contents = sanitize_hover_contents(contents);
    }

    sanitized
}

/// Sanitize hover contents (MarkedString, MarkupContent, or array).
fn sanitize_hover_contents(contents: &Value) -> Value {
    match contents {
        Value::String(s) => Value::String(sanitize_text(s)),
        Value::Object(obj) => {
            let mut sanitized = obj.clone();
            // MarkupContent has "value" field
            if let Some(value) = sanitized.get_mut("value") {
                if let Some(s) = value.as_str() {
                    *value = Value::String(sanitize_text(s));
                }
            }
            // MarkedString has "language" and "value" fields
            if sanitized.get("language").is_some() {
                if let Some(value) = sanitized.get_mut("value") {
                    if let Some(s) = value.as_str() {
                        *value = Value::String(sanitize_text(s));
                    }
                }
            }
            Value::Object(sanitized)
        }
        Value::Array(arr) => Value::Array(arr.iter().map(sanitize_hover_contents).collect()),
        _ => contents.clone(),
    }
}

/// Sanitize text content by replacing temp file paths.
fn sanitize_text(text: &str) -> String {
    // Replace /var/folders/... (macOS temp) and /tmp/... (Linux temp) paths
    // Match the path but stop before : (line number separator)
    let re_macos = regex::Regex::new(r"/var/folders/[^/]+/[^/]+/[TP]/[^\s\):]+").unwrap();
    let re_linux = regex::Regex::new(r"/tmp/[^\s\):]+").unwrap();

    let sanitized = re_macos.replace_all(text, "<TEMP_PATH>");
    let sanitized = re_linux.replace_all(&sanitized, "<TEMP_PATH>");
    sanitized.to_string()
}

/// Sanitize completion response for snapshot testing.
///
/// Removes volatile `data` fields and temp paths from completion responses before snapshotting.
pub fn sanitize_completion_response(completion: &Value) -> Value {
    sanitize_recursive(completion, true)
}

/// Sanitize references response for snapshot testing.
///
/// Normalizes URIs and any embedded temp paths for deterministic snapshots.
pub fn sanitize_references_response(references: &Value) -> Value {
    sanitize_recursive(references, false)
}

/// Sanitize definition response by normalizing URIs in Location/LocationLink objects.
pub fn sanitize_definition_response(result: &Value) -> Value {
    match result {
        Value::Array(locations) => Value::Array(
            locations
                .iter()
                .map(|loc| sanitize_definition_object(loc))
                .collect(),
        ),
        Value::Object(_) => sanitize_definition_object(result),
        _ => result.clone(),
    }
}

fn sanitize_definition_object(value: &Value) -> Value {
    let mut map = value.clone();
    if let Value::Object(obj) = &mut map {
        if let Some(uri) = obj.get_mut("targetUri") {
            *uri = Value::String("<TEST_FILE_URI>".to_string());
        }
        if let Some(uri) = obj.get_mut("uri") {
            *uri = Value::String("<TEST_FILE_URI>".to_string());
        }
        if let Some(range) = obj.get_mut("range") {
            *range = sanitize_definition_range(range);
        }
        if let Some(range) = obj.get_mut("targetSelectionRange") {
            *range = sanitize_definition_range(range);
        }
    }
    map
}

fn sanitize_definition_range(value: &Value) -> Value {
    match value {
        Value::Object(obj) => {
            let mut sanitized = obj.clone();
            for v in sanitized.values_mut() {
                *v = sanitize_recursive(v, false);
            }
            Value::Object(sanitized)
        }
        _ => sanitize_recursive(value, false),
    }
}

fn sanitize_recursive(value: &Value, drop_data_field: bool) -> Value {
    match value {
        Value::String(s) => Value::String(sanitize_text(s)),
        Value::Array(arr) => Value::Array(
            arr.iter()
                .map(|v| sanitize_recursive(v, drop_data_field))
                .collect(),
        ),
        Value::Object(obj) => {
            let mut map = serde_json::Map::new();
            for (key, val) in obj {
                if drop_data_field && key == "data" {
                    continue;
                }
                if key == "uri" {
                    if let Some(uri) = val.as_str() {
                        map.insert(key.clone(), Value::String(sanitize_uri(uri)));
                        continue;
                    }
                }
                map.insert(key.clone(), sanitize_recursive(val, drop_data_field));
            }
            Value::Object(map)
        }
        _ => value.clone(),
    }
}

fn sanitize_uri(uri: &str) -> String {
    if uri.starts_with("file://") {
        "file://<TEMP_PATH>/test.md".to_string()
    } else {
        sanitize_text(uri)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_sanitize_hover_with_markup_content() {
        let hover = json!({
            "contents": {
                "kind": "markdown",
                "value": "```rust\nfn main()\n```\nDefined at /tmp/rust_abc123.md:4"
            },
            "range": {
                "start": {"line": 3, "character": 3},
                "end": {"line": 3, "character": 7}
            }
        });

        let sanitized = sanitize_hover_response(&hover);
        let contents = sanitized["contents"]["value"].as_str().unwrap();

        assert!(contents.contains("<TEMP_PATH>"));
        assert!(!contents.contains("/tmp/"));
    }

    #[test]
    fn test_sanitize_hover_with_string_contents() {
        let hover = json!({
            "contents": "fn main() at /var/folders/xy/z123/T/temp.md:4"
        });

        let sanitized = sanitize_hover_response(&hover);
        let contents = sanitized["contents"].as_str().unwrap();

        assert!(contents.contains("<TEMP_PATH>"));
        assert!(!contents.contains("/var/folders/"));
    }

    #[test]
    fn test_sanitize_text_macos_temp() {
        let text = "Defined at /var/folders/ab/cd123456/T/rust_xyz.md:10";
        let sanitized = sanitize_text(text);

        assert_eq!(sanitized, "Defined at <TEMP_PATH>:10");
    }

    #[test]
    fn test_sanitize_text_linux_temp() {
        let text = "Defined at /tmp/rust_xyz123.md:10";
        let sanitized = sanitize_text(text);

        assert_eq!(sanitized, "Defined at <TEMP_PATH>:10");
    }

    #[test]
    fn test_sanitize_text_no_temp_paths() {
        let text = "fn main() -> ()";
        let sanitized = sanitize_text(text);

        assert_eq!(sanitized, text);
    }

    #[test]
    fn test_sanitize_completion_response_removes_temp_paths() {
        let completion = json!({
            "items": [{
                "label": "example",
                "detail": "Defined at /tmp/test.file:4"
            }]
        });

        let sanitized = sanitize_completion_response(&completion);
        let detail = sanitized["items"][0]["detail"].as_str().unwrap();

        assert!(detail.contains("<TEMP_PATH>"));
        assert!(!detail.contains("/tmp/"));
    }

    #[test]
    fn test_sanitize_completion_response_removes_data_field() {
        let completion = json!([
            {
                "label": "example",
                "data": {
                    "something": "secret"
                }
            }
        ]);

        let sanitized = sanitize_completion_response(&completion);

        assert!(sanitized[0].get("data").is_none());
    }

    #[test]
    fn test_sanitize_references_response_normalizes_uri() {
        let references = json!([
            {
                "uri": "file:///tmp/tmpfile.md",
                "range": {
                    "start": {"line": 1, "character": 2},
                    "end": {"line": 1, "character": 3}
                }
            }
        ]);

        let sanitized = sanitize_references_response(&references);
        let uri = sanitized[0]["uri"].as_str().unwrap();

        assert_eq!(uri, "file://<TEMP_PATH>/test.md");
        assert!(sanitized[0]["range"]["start"]["line"].is_number());
    }

    #[test]
    fn test_sanitize_definition_response_replaces_uris() {
        let response = json!([
            {
                "targetUri": "file:///tmp/tmpfile.md",
                "targetSelectionRange": {
                    "start": { "line": 1, "character": 2 },
                    "end": { "line": 1, "character": 10 }
                }
            }
        ]);

        let sanitized = sanitize_definition_response(&response);
        let uri = sanitized[0]["targetUri"].as_str().unwrap();

        assert_eq!(uri, "<TEST_FILE_URI>");
        assert!(sanitized[0]["targetSelectionRange"]["start"]["line"].is_number());
    }
}
