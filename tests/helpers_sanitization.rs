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
}
