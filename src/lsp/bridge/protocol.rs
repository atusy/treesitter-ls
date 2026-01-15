//! LSP protocol types and transformations for bridge communication.
//!
//! This module provides types for virtual document URIs and message
//! transformation between host and virtual document coordinates.
//!
//! ## Module Structure
//!
//! - `virtual_uri` - VirtualDocumentUri type for encoding injection region references
//! - `request` - Request builders for downstream language servers
//! - `response` - Response transformers for coordinate translation

mod request;
mod response;
mod virtual_uri;

// Re-export all public items for external use
pub(crate) use request::*;
pub(crate) use response::*;
pub(crate) use virtual_uri::VirtualDocumentUri;

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tower_lsp::lsp_types::{Position, Url};

    // ==========================================================================
    // VirtualDocumentUri tests
    // ==========================================================================

    #[test]
    fn virtual_uri_uses_treesitter_ls_path_prefix() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.starts_with("file:///.treesitter-ls/"),
            "URI should use file:///.treesitter-ls/ path: {}",
            uri_string
        );
    }

    #[test]
    fn virtual_uri_includes_language_extension() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.ends_with(".lua"),
            "URI should have .lua extension: {}",
            uri_string
        );
    }

    #[test]
    fn region_id_accessor_returns_stored_value() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "01ARZ3NDEKTSV4RRFFQ69G5FAV");

        assert_eq!(virtual_uri.region_id(), "01ARZ3NDEKTSV4RRFFQ69G5FAV");
    }

    #[test]
    fn language_accessor_returns_stored_value() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "python", "region-0");

        assert_eq!(virtual_uri.language(), "python");
    }

    #[test]
    fn virtual_uri_percent_encodes_special_characters_in_region_id() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Test with characters that need encoding: space, slash, question mark
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region/0?test");

        let uri_string = virtual_uri.to_uri_string();
        // "/" should be encoded as %2F, "?" should be encoded as %3F
        assert!(
            uri_string.contains("region%2F0%3Ftest"),
            "Special characters should be percent-encoded: {}",
            uri_string
        );
    }

    #[test]
    fn virtual_uri_preserves_alphanumeric_and_safe_chars_in_region_id() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // RFC 3986 unreserved characters: A-Z a-z 0-9 - . _ ~
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "ABC-xyz_123.test~v2");

        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.contains("ABC-xyz_123.test~v2.lua"),
            "Unreserved characters should not be encoded: {}",
            uri_string
        );
    }

    #[test]
    fn virtual_uri_same_inputs_produce_same_output() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        assert_eq!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Same inputs should produce deterministic output"
        );
    }

    #[test]
    fn virtual_uri_different_region_ids_produce_different_uris() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host_uri, "lua", "region-1");

        assert_ne!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Different region_ids should produce different URIs"
        );
    }

    #[test]
    fn virtual_uri_different_languages_produce_different_extensions() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let lua_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");
        let python_uri = VirtualDocumentUri::new(&host_uri, "python", "region-0");

        assert!(lua_uri.to_uri_string().ends_with(".lua"));
        assert!(python_uri.to_uri_string().ends_with(".py"));
    }

    #[test]
    fn language_to_extension_maps_common_languages() {
        // Test a representative sample of the supported languages
        let test_cases = [
            ("lua", "lua"),
            ("python", "py"),
            ("rust", "rs"),
            ("javascript", "js"),
            ("typescript", "ts"),
            ("go", "go"),
            ("c", "c"),
            ("cpp", "cpp"),
            ("java", "java"),
            ("ruby", "rb"),
            ("bash", "sh"),
            ("sh", "sh"),
        ];

        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        for (language, expected_ext) in test_cases {
            let uri = VirtualDocumentUri::new(&host_uri, language, "region-0");
            let uri_string = uri.to_uri_string();
            assert!(
                uri_string.ends_with(&format!(".{}", expected_ext)),
                "Language '{}' should produce extension '{}', got: {}",
                language,
                expected_ext,
                uri_string
            );
        }
    }

    #[test]
    fn language_to_extension_falls_back_to_txt_for_unknown() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        // Unknown languages should default to .txt
        let unknown_cases = ["unknown-lang", "foobar", "notareallan"];

        for language in unknown_cases {
            let uri = VirtualDocumentUri::new(&host_uri, language, "region-0");
            let uri_string = uri.to_uri_string();
            assert!(
                uri_string.ends_with(".txt"),
                "Unknown language '{}' should produce .txt extension, got: {}",
                language,
                uri_string
            );
        }
    }

    #[test]
    fn virtual_uri_different_hosts_produce_different_hashes() {
        let host1 = Url::parse("file:///project/doc1.md").unwrap();
        let host2 = Url::parse("file:///project/doc2.md").unwrap();
        let uri1 = VirtualDocumentUri::new(&host1, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host2, "lua", "region-0");

        assert_ne!(
            uri1.to_uri_string(),
            uri2.to_uri_string(),
            "Different host URIs should produce different hashes"
        );
    }

    #[test]
    fn virtual_uri_equality_checks_all_fields() {
        let host1 = Url::parse("file:///project/doc1.md").unwrap();
        let host2 = Url::parse("file:///project/doc2.md").unwrap();

        let uri1 = VirtualDocumentUri::new(&host1, "lua", "region-0");
        let uri2 = VirtualDocumentUri::new(&host1, "lua", "region-0");
        let uri3 = VirtualDocumentUri::new(&host2, "lua", "region-0");
        let uri4 = VirtualDocumentUri::new(&host1, "python", "region-0");
        let uri5 = VirtualDocumentUri::new(&host1, "lua", "region-1");

        assert_eq!(uri1, uri2, "Same fields should be equal");
        assert_ne!(uri1, uri3, "Different host_uri should not be equal");
        assert_ne!(uri1, uri4, "Different language should not be equal");
        assert_ne!(uri1, uri5, "Different region_id should not be equal");
    }

    #[test]
    #[should_panic(expected = "language must not be empty")]
    #[cfg(debug_assertions)]
    fn virtual_uri_panics_on_empty_language_in_debug() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let _ = VirtualDocumentUri::new(&host_uri, "", "region-0");
    }

    #[test]
    #[should_panic(expected = "region_id must not be empty")]
    #[cfg(debug_assertions)]
    fn virtual_uri_panics_on_empty_region_id_in_debug() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let _ = VirtualDocumentUri::new(&host_uri, "lua", "");
    }

    #[test]
    fn percent_encode_preserves_unreserved_characters() {
        // RFC 3986 unreserved: ALPHA / DIGIT / "-" / "." / "_" / "~"
        let input = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-._~";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, input,
            "Unreserved characters should not be encoded"
        );
    }

    #[test]
    fn percent_encode_encodes_reserved_characters() {
        // Some reserved characters that need encoding in path segments
        let input = "test/path?query#fragment";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "test%2Fpath%3Fquery%23fragment",
            "Reserved characters should be percent-encoded"
        );
    }

    #[test]
    fn percent_encode_encodes_space() {
        let input = "hello world";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(encoded, "hello%20world", "Space should be encoded as %20");
    }

    #[test]
    fn percent_encode_handles_multibyte_utf8() {
        // UTF-8 multi-byte characters should have each byte percent-encoded
        // "日" (U+65E5) = E6 97 A5 in UTF-8
        let input = "日";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "%E6%97%A5",
            "Multi-byte UTF-8 should encode each byte"
        );
    }

    #[test]
    fn percent_encode_handles_mixed_ascii_and_utf8() {
        // Mix of ASCII alphanumerics (preserved) and UTF-8 (encoded)
        let input = "region-日本語-test";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        // "日" = E6 97 A5, "本" = E6 9C AC, "語" = E8 AA 9E
        assert_eq!(
            encoded, "region-%E6%97%A5%E6%9C%AC%E8%AA%9E-test",
            "Mixed content should preserve ASCII and encode UTF-8"
        );
    }

    #[test]
    fn percent_encode_handles_emoji() {
        // Emoji are 4-byte UTF-8 sequences
        // "🦀" (U+1F980) = F0 9F A6 80 in UTF-8
        let input = "rust🦀";
        let encoded = VirtualDocumentUri::percent_encode_path_segment(input);
        assert_eq!(
            encoded, "rust%F0%9F%A6%80",
            "4-byte UTF-8 (emoji) should encode all bytes"
        );
    }

    #[test]
    fn to_uri_string_contains_region_id_in_filename() {
        // Verify that the region_id appears in the URI (partial round-trip)
        // Note: Full round-trip isn't possible since host_uri is hashed
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let region_id = "01ARZ3NDEKTSV4RRFFQ69G5FAV";
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", region_id);

        let uri_string = virtual_uri.to_uri_string();

        // Extract filename from the URI path
        let filename = uri_string.rsplit('/').next().unwrap();
        // Remove extension to get the region_id
        let extracted_id = filename.rsplit_once('.').map(|(name, _)| name).unwrap();

        assert_eq!(
            extracted_id, region_id,
            "Region ID should be extractable from URI"
        );
    }

    #[test]
    fn to_uri_string_produces_valid_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "lua", "region-0");

        let uri_string = virtual_uri.to_uri_string();

        // Verify the output is a valid URL
        let parsed = Url::parse(&uri_string);
        assert!(
            parsed.is_ok(),
            "to_uri_string() should produce a valid URL: {}",
            uri_string
        );

        let parsed = parsed.unwrap();
        assert_eq!(parsed.scheme(), "file");
        assert!(parsed.path().starts_with("/.treesitter-ls/"));
    }

    // ==========================================================================
    // Hover request/response transformation tests
    // ==========================================================================

    #[test]
    fn hover_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_hover_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn hover_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/hover");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn position_translation_at_region_start_becomes_line_zero() {
        // When cursor is at the first line of the region, virtual line should be 0
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 3, // Same as region_start_line
            character: 5,
        };
        let region_start_line = 3;

        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(
            request["params"]["position"]["line"], 0,
            "Position at region start should translate to line 0"
        );
    }

    #[test]
    fn position_translation_with_zero_region_start() {
        // Region starting at line 0 (e.g., first line of document)
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 0,
        };
        let region_start_line = 0;

        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(
            request["params"]["position"]["line"], 5,
            "With region_start_line=0, virtual line equals host line"
        );
    }

    #[test]
    fn response_transformation_with_zero_region_start() {
        // Response transformation when region starts at line 0
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
        // Virtual document line 0 should map to region_start_line
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

        assert_eq!(
            transformed["result"]["range"]["start"]["line"], 3,
            "Start line should be translated (0 + 3 = 3)"
        );
        assert_eq!(
            transformed["result"]["range"]["end"]["line"], 3,
            "End line should be translated (0 + 3 = 3)"
        );
        // Characters unchanged
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
    // didChange notification tests
    // ==========================================================================

    #[test]
    fn didchange_notification_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let notification =
            build_bridge_didchange_notification(&host_uri, "lua", "region-0", "local x = 42", 2);

        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "textDocument/didChange");
        assert!(
            notification.get("id").is_none(),
            "Notification should not have id"
        );

        let uri_str = notification["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "didChange should use virtual URI: {}",
            uri_str
        );
        assert_eq!(notification["params"]["textDocument"]["version"], 2);
    }

    #[test]
    fn didchange_notification_contains_full_text() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let content = "local x = 42\nprint(x)";
        let notification =
            build_bridge_didchange_notification(&host_uri, "lua", "region-0", content, 1);

        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["text"], content);
    }

    // ==========================================================================
    // Completion request/response transformation tests
    // ==========================================================================

    #[test]
    fn completion_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 6,
        };

        let request =
            build_bridge_completion_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn completion_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 6,
        };
        let region_start_line = 3;

        let request = build_bridge_completion_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/completion");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 6,
            "Character should remain unchanged"
        );
    }

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
        // Item with textEdit has transformed range
        assert_eq!(items[0]["textEdit"]["range"]["start"]["line"], 4);
        assert_eq!(items[0]["textEdit"]["range"]["end"]["line"], 4);
        // Item without textEdit unchanged
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
        // Some servers return array directly instead of CompletionList
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

    // ==========================================================================
    // SignatureHelp request/response transformation tests
    // ==========================================================================

    #[test]
    fn signature_help_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_signature_help_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn signature_help_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_signature_help_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/signatureHelp");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

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

        // activeSignature and activeParameter must be preserved unchanged
        assert_eq!(
            transformed["result"]["activeSignature"], 0,
            "activeSignature must be preserved"
        );
        assert_eq!(
            transformed["result"]["activeParameter"], 1,
            "activeParameter must be preserved"
        );
        // signatures array must be preserved
        assert_eq!(
            transformed["result"]["signatures"][0]["label"],
            "string.format(formatstring, ...)"
        );
    }

    #[test]
    fn signature_help_response_without_metadata_passes_through() {
        // Some servers may return minimal response without activeSignature/activeParameter
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
    // Definition request/response transformation tests
    // ==========================================================================

    #[test]
    fn definition_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_definition_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn definition_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_definition_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/definition");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    /// Helper to create a ResponseTransformContext for tests.
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
        // Definition response as Location[] format
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
        // First location: line 0 -> 3
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["range"]["end"]["line"], 3);
        // Second location: line 2 -> 5
        assert_eq!(result[1]["range"]["start"]["line"], 5);
        assert_eq!(result[1]["range"]["end"]["line"], 5);
        // Characters unchanged
        assert_eq!(result[0]["range"]["start"]["character"], 9);
        assert_eq!(result[0]["range"]["end"]["character"], 14);
        // URI transformed to host
        assert_eq!(result[0]["uri"], host_uri);
        assert_eq!(result[1]["uri"], host_uri);
    }

    #[test]
    fn definition_response_transforms_single_location() {
        // Definition response as single Location (not array)
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

        // Single location: line 1 -> 4
        assert_eq!(transformed["result"]["range"]["start"]["line"], 4);
        assert_eq!(transformed["result"]["range"]["end"]["line"], 4);
        // Characters unchanged
        assert_eq!(transformed["result"]["range"]["start"]["character"], 5);
        assert_eq!(transformed["result"]["range"]["end"]["character"], 15);
        // URI transformed to host
        assert_eq!(transformed["result"]["uri"], host_uri);
    }

    #[test]
    fn definition_response_transforms_location_link_array() {
        // Definition response as LocationLink[] format
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
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
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        // originSelectionRange should NOT be transformed (it's in host coordinates)
        assert_eq!(result[0]["originSelectionRange"]["start"]["line"], 5);
        assert_eq!(result[0]["originSelectionRange"]["end"]["line"], 5);
        // targetRange should be transformed: line 0 -> 3, line 2 -> 5
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetRange"]["end"]["line"], 5);
        // targetSelectionRange should be transformed: line 0 -> 3
        assert_eq!(result[0]["targetSelectionRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetSelectionRange"]["end"]["line"], 3);
        // Characters unchanged
        assert_eq!(result[0]["targetSelectionRange"]["start"]["character"], 9);
        assert_eq!(result[0]["targetSelectionRange"]["end"]["character"], 14);
        // targetUri transformed to host
        assert_eq!(result[0]["targetUri"], host_uri);
    }

    #[test]
    fn definition_response_with_null_result_passes_through() {
        let response = json!({ "jsonrpc": "2.0", "id": 42, "result": null });
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);

        let transformed = transform_definition_response_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    #[test]
    fn definition_response_transforms_location_uri_to_host_uri() {
        // Definition response with virtual URI should be transformed to host URI
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
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        // URI should be transformed to host URI
        assert_eq!(
            result[0]["uri"], host_uri,
            "Location.uri should be transformed to host URI"
        );
        // Range transformation still works
        assert_eq!(result[0]["range"]["start"]["line"], 3);
    }

    #[test]
    fn definition_response_transforms_location_link_target_uri_to_host_uri() {
        // Definition response as LocationLink[] with virtual targetUri should be transformed
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
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
                }
            ]
        });
        let host_uri = "file:///project/doc.md";
        let context = test_context(virtual_uri, host_uri, 3);
        let transformed = transform_definition_response_to_host(response, &context);

        let result = transformed["result"].as_array().unwrap();
        // targetUri should be transformed to host URI
        assert_eq!(
            result[0]["targetUri"], host_uri,
            "LocationLink.targetUri should be transformed to host URI"
        );
        // Range transformations still work
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
        assert_eq!(result[0]["targetSelectionRange"]["start"]["line"], 3);
    }

    // ==========================================================================
    // New cross-document transformation tests
    // ==========================================================================

    #[test]
    fn is_virtual_uri_detects_virtual_uris() {
        assert!(is_virtual_uri("file:///.treesitter-ls/abc123/region-0.lua"));
        assert!(is_virtual_uri(
            "file:///.treesitter-ls/def456/01JPMQ8ZYYQA.py"
        ));
        assert!(is_virtual_uri("file:///.treesitter-ls/hash/test.txt"));
    }

    #[test]
    fn is_virtual_uri_rejects_real_uris() {
        assert!(!is_virtual_uri("file:///home/user/project/main.lua"));
        assert!(!is_virtual_uri("file:///C:/Users/dev/code.py"));
        assert!(!is_virtual_uri("untitled:Untitled-1"));
        assert!(!is_virtual_uri("file:///some/treesitter-ls/file.lua")); // No dot prefix
    }

    #[test]
    fn definition_response_preserves_real_file_uri() {
        // Response with a real file URI should be preserved as-is
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

        // Real file URI should be preserved
        assert_eq!(transformed["result"][0]["uri"], real_file_uri);
        // Range should be unchanged (real file coordinates)
        assert_eq!(transformed["result"][0]["range"]["start"]["line"], 10);
    }

    #[test]
    fn definition_response_filters_out_different_region_virtual_uri() {
        // Response with a different virtual URI should be filtered out
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

        // Cross-region virtual URI should be filtered out, leaving empty result
        let result = transformed["result"].as_array().unwrap();
        assert!(
            result.is_empty(),
            "Cross-region virtual URI should be filtered out"
        );
    }

    #[test]
    fn definition_response_mixed_array_filters_only_cross_region() {
        // CRITICAL TEST: Mixed array with real file, same virtual, and cross-region URIs
        // Only cross-region virtual URIs should be filtered; others preserved
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

        // First item: real file URI preserved as-is
        assert_eq!(result[0]["uri"], real_file_uri);
        assert_eq!(
            result[0]["range"]["start"]["line"], 10,
            "Real file coordinates unchanged"
        );

        // Second item: same virtual URI transformed to host
        assert_eq!(result[1]["uri"], host_uri);
        assert_eq!(
            result[1]["range"]["start"]["line"], 7,
            "Virtual coordinates offset by 5"
        );
    }

    #[test]
    fn definition_response_single_location_filtered_becomes_null() {
        // When a single Location (not array) is filtered, result should become null
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

        assert!(
            transformed["result"].is_null(),
            "Single filtered Location should become null result"
        );
    }

    #[test]
    fn definition_response_single_location_link_filtered_becomes_null() {
        // When a single LocationLink (not array) is filtered, result should become null
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

        assert!(
            transformed["result"].is_null(),
            "Single filtered LocationLink should become null result"
        );
    }

    #[test]
    fn definition_response_single_location_link_same_region_transforms() {
        // Single LocationLink (not array) with same virtual URI should be transformed
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
        assert!(result.is_object(), "Result should remain an object");

        // targetUri transformed to host
        assert_eq!(result["targetUri"], host_uri);
        // targetRange transformed (0 + 10 = 10)
        assert_eq!(result["targetRange"]["start"]["line"], 10);
        // targetSelectionRange transformed (0 + 10 = 10)
        assert_eq!(result["targetSelectionRange"]["start"]["line"], 10);
        // originSelectionRange unchanged (already in host coordinates)
        assert_eq!(result["originSelectionRange"]["start"]["line"], 5);
    }

    #[test]
    fn definition_response_location_link_array_filters_cross_region() {
        // LocationLink array should filter cross-region URIs
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
        assert_eq!(
            result.len(),
            1,
            "Cross-region LocationLink should be filtered"
        );
        assert_eq!(result[0]["targetUri"], host_uri);
        assert_eq!(result[0]["targetRange"]["start"]["line"], 3);
    }

    // ==========================================================================
    // TypeDefinition request/response transformation tests
    // ==========================================================================

    #[test]
    fn type_definition_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_type_definition_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn type_definition_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_type_definition_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/typeDefinition");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // Implementation request/response transformation tests
    // ==========================================================================

    #[test]
    fn implementation_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_implementation_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn implementation_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_implementation_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/implementation");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // Declaration request/response transformation tests
    // ==========================================================================

    #[test]
    fn declaration_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request =
            build_bridge_declaration_request(&host_uri, host_position, "lua", "region-0", 3, 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn declaration_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_declaration_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/declaration");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    // ==========================================================================
    // References request/response transformation tests
    // ==========================================================================

    #[test]
    fn references_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            true, // include_declaration
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn references_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            true, // include_declaration
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/references");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn references_request_includes_context_with_include_declaration_true() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            true, // include_declaration = true
            42,
        );

        assert_eq!(
            request["params"]["context"]["includeDeclaration"], true,
            "Context should include includeDeclaration = true"
        );
    }

    #[test]
    fn references_request_includes_context_with_include_declaration_false() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_references_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            false, // include_declaration = false
            42,
        );

        assert_eq!(
            request["params"]["context"]["includeDeclaration"], false,
            "Context should include includeDeclaration = false"
        );
    }

    // ==========================================================================
    // Document highlight request/response transformation tests
    // ==========================================================================

    #[test]
    fn document_highlight_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_document_highlight_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn document_highlight_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_document_highlight_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/documentHighlight");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn document_highlight_response_transforms_ranges_to_host_coordinates() {
        // DocumentHighlight response is an array of items with range and optional kind
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 6 },
                        "end": { "line": 0, "character": 11 }
                    },
                    "kind": 1  // Text
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 0 },
                        "end": { "line": 2, "character": 5 }
                    },
                    "kind": 2  // Read
                },
                {
                    "range": {
                        "start": { "line": 4, "character": 0 },
                        "end": { "line": 4, "character": 5 }
                    }
                    // kind is optional
                }
            ]
        });
        let region_start_line = 3;

        let transformed =
            transform_document_highlight_response_to_host(response, region_start_line);

        let result = transformed["result"].as_array().unwrap();
        assert_eq!(result.len(), 3);

        // First highlight: line 0 + 3 = line 3
        assert_eq!(result[0]["range"]["start"]["line"], 3);
        assert_eq!(result[0]["range"]["end"]["line"], 3);
        assert_eq!(result[0]["range"]["start"]["character"], 6);
        assert_eq!(result[0]["kind"], 1);

        // Second highlight: line 2 + 3 = line 5
        assert_eq!(result[1]["range"]["start"]["line"], 5);
        assert_eq!(result[1]["range"]["end"]["line"], 5);
        assert_eq!(result[1]["kind"], 2);

        // Third highlight: line 4 + 3 = line 7
        assert_eq!(result[2]["range"]["start"]["line"], 7);
        assert_eq!(result[2]["range"]["end"]["line"], 7);
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
    // Rename request tests
    // ==========================================================================

    #[test]
    fn rename_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_rename_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            "newName",
            42,
        );

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn rename_request_translates_position_to_virtual_coordinates() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        // Host line 5, region starts at line 3 -> virtual line 2
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_start_line = 3;

        let request = build_bridge_rename_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            region_start_line,
            "newName",
            42,
        );

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/rename");
        assert_eq!(
            request["params"]["position"]["line"], 2,
            "Position line should be translated (5 - 3 = 2)"
        );
        assert_eq!(
            request["params"]["position"]["character"], 10,
            "Character should remain unchanged"
        );
    }

    #[test]
    fn rename_request_includes_new_name() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };

        let request = build_bridge_rename_request(
            &host_uri,
            host_position,
            "lua",
            "region-0",
            3,
            "renamedVariable",
            42,
        );

        assert_eq!(
            request["params"]["newName"], "renamedVariable",
            "Request should include newName parameter"
        );
    }

    // ==========================================================================
    // WorkspaceEdit transformation tests (for rename response)
    // ==========================================================================

    #[test]
    fn workspace_edit_transforms_textedit_ranges_in_changes_map() {
        // WorkspaceEdit with changes format: { [uri: string]: TextEdit[] }
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    virtual_uri: [
                        {
                            "range": {
                                "start": { "line": 0, "character": 6 },
                                "end": { "line": 0, "character": 9 }
                            },
                            "newText": "newVar"
                        },
                        {
                            "range": {
                                "start": { "line": 2, "character": 10 },
                                "end": { "line": 2, "character": 13 }
                            },
                            "newText": "newVar"
                        }
                    ]
                }
            }
        });
        let region_start_line = 3;
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: region_start_line,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        // The changes map key should be transformed from virtual URI to host URI
        let changes = transformed["result"]["changes"].as_object().unwrap();
        assert!(
            changes.contains_key(host_uri),
            "Changes should have host URI as key"
        );
        assert!(
            !changes.contains_key(virtual_uri),
            "Changes should not have virtual URI as key"
        );

        // Check that ranges are transformed
        let edits = changes[host_uri].as_array().unwrap();
        // First edit: line 0 + 3 = line 3
        assert_eq!(edits[0]["range"]["start"]["line"], 3);
        assert_eq!(edits[0]["range"]["end"]["line"], 3);
        // Second edit: line 2 + 3 = line 5
        assert_eq!(edits[1]["range"]["start"]["line"], 5);
        assert_eq!(edits[1]["range"]["end"]["line"], 5);
    }

    #[test]
    fn workspace_edit_transforms_textedit_ranges_in_document_changes() {
        // WorkspaceEdit with documentChanges format: TextDocumentEdit[]
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [
                    {
                        "textDocument": {
                            "uri": virtual_uri,
                            "version": 1
                        },
                        "edits": [
                            {
                                "range": {
                                    "start": { "line": 0, "character": 6 },
                                    "end": { "line": 0, "character": 9 }
                                },
                                "newText": "newVar"
                            },
                            {
                                "range": {
                                    "start": { "line": 2, "character": 10 },
                                    "end": { "line": 2, "character": 13 }
                                },
                                "newText": "newVar"
                            }
                        ]
                    }
                ]
            }
        });
        let region_start_line = 3;
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: region_start_line,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        // Check the documentChanges array
        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert_eq!(document_changes.len(), 1);

        // textDocument.uri should be transformed to host URI
        assert_eq!(
            document_changes[0]["textDocument"]["uri"], host_uri,
            "textDocument.uri should be transformed to host URI"
        );

        // Check that ranges are transformed
        let edits = document_changes[0]["edits"].as_array().unwrap();
        // First edit: line 0 + 3 = line 3
        assert_eq!(edits[0]["range"]["start"]["line"], 3);
        assert_eq!(edits[0]["range"]["end"]["line"], 3);
        // Second edit: line 2 + 3 = line 5
        assert_eq!(edits[1]["range"]["start"]["line"], 5);
        assert_eq!(edits[1]["range"]["end"]["line"], 5);
    }

    #[test]
    fn workspace_edit_filters_out_cross_region_virtual_uris_in_changes() {
        // Cross-region virtual URIs should be filtered out (different region_id)
        let request_virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let other_virtual_uri = "file:///.treesitter-ls/abc123/region-1.lua"; // Different region
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    request_virtual_uri: [
                        {
                            "range": {
                                "start": { "line": 0, "character": 6 },
                                "end": { "line": 0, "character": 9 }
                            },
                            "newText": "newVar"
                        }
                    ],
                    other_virtual_uri: [
                        {
                            "range": {
                                "start": { "line": 5, "character": 0 },
                                "end": { "line": 5, "character": 3 }
                            },
                            "newText": "newVar"
                        }
                    ]
                }
            }
        });
        let context = ResponseTransformContext {
            request_virtual_uri: request_virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 3,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        // Only the request's virtual URI edits should remain (transformed to host URI)
        let changes = transformed["result"]["changes"].as_object().unwrap();
        assert_eq!(
            changes.len(),
            1,
            "Should only have one entry (cross-region filtered)"
        );
        assert!(changes.contains_key(host_uri), "Should have host URI entry");
        assert!(
            !changes.contains_key(other_virtual_uri),
            "Cross-region virtual URI should be filtered out"
        );
    }

    #[test]
    fn workspace_edit_filters_out_cross_region_virtual_uris_in_document_changes() {
        // Cross-region virtual URIs should be filtered out from documentChanges
        let request_virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let other_virtual_uri = "file:///.treesitter-ls/abc123/region-1.lua"; // Different region
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [
                    {
                        "textDocument": {
                            "uri": request_virtual_uri,
                            "version": 1
                        },
                        "edits": [{
                            "range": {
                                "start": { "line": 0, "character": 6 },
                                "end": { "line": 0, "character": 9 }
                            },
                            "newText": "newVar"
                        }]
                    },
                    {
                        "textDocument": {
                            "uri": other_virtual_uri,
                            "version": 1
                        },
                        "edits": [{
                            "range": {
                                "start": { "line": 5, "character": 0 },
                                "end": { "line": 5, "character": 3 }
                            },
                            "newText": "newVar"
                        }]
                    }
                ]
            }
        });
        let context = ResponseTransformContext {
            request_virtual_uri: request_virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 3,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        // Only the request's virtual URI document change should remain
        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert_eq!(
            document_changes.len(),
            1,
            "Should only have one entry (cross-region filtered)"
        );
        assert_eq!(
            document_changes[0]["textDocument"]["uri"], host_uri,
            "Remaining entry should have host URI"
        );
    }

    #[test]
    fn workspace_edit_replaces_virtual_uri_key_with_host_uri_in_changes() {
        // Verify the virtual URI key is replaced with host URI
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    virtual_uri: [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 3 }
                        },
                        "newText": "foo"
                    }]
                }
            }
        });
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 0,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        let changes = transformed["result"]["changes"].as_object().unwrap();
        // Virtual URI key should be gone
        assert!(
            !changes.contains_key(virtual_uri),
            "Virtual URI key should be removed"
        );
        // Host URI key should exist
        assert!(
            changes.contains_key(host_uri),
            "Host URI key should be present"
        );
    }

    #[test]
    fn workspace_edit_replaces_virtual_uri_with_host_uri_in_document_changes() {
        // Verify textDocument.uri is replaced with host URI
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [{
                    "textDocument": {
                        "uri": virtual_uri,
                        "version": 1
                    },
                    "edits": [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 3 }
                        },
                        "newText": "foo"
                    }]
                }]
            }
        });
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 0,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        assert_eq!(
            document_changes[0]["textDocument"]["uri"], host_uri,
            "textDocument.uri should be replaced with host URI"
        );
    }

    #[test]
    fn workspace_edit_preserves_real_file_uris_in_changes() {
        // Real file URIs (external dependencies) should pass through unchanged
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let real_file_uri = "file:///usr/share/lua/5.4/module.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "changes": {
                    virtual_uri: [{
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 3 }
                        },
                        "newText": "foo"
                    }],
                    real_file_uri: [{
                        "range": {
                            "start": { "line": 10, "character": 5 },
                            "end": { "line": 10, "character": 8 }
                        },
                        "newText": "foo"
                    }]
                }
            }
        });
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 3,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        let changes = transformed["result"]["changes"].as_object().unwrap();
        // Real file URI should be preserved
        assert!(
            changes.contains_key(real_file_uri),
            "Real file URI should be preserved"
        );
        // Real file URI ranges should NOT be transformed (different coordinate space)
        let real_edits = changes[real_file_uri].as_array().unwrap();
        assert_eq!(
            real_edits[0]["range"]["start"]["line"], 10,
            "Real file ranges should not be transformed"
        );
    }

    #[test]
    fn workspace_edit_preserves_real_file_uris_in_document_changes() {
        // Real file URIs in documentChanges should pass through unchanged
        let virtual_uri = "file:///.treesitter-ls/abc123/region-0.lua";
        let real_file_uri = "file:///usr/share/lua/5.4/module.lua";
        let host_uri = "file:///project/doc.md";
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "documentChanges": [
                    {
                        "textDocument": {
                            "uri": virtual_uri,
                            "version": 1
                        },
                        "edits": [{
                            "range": {
                                "start": { "line": 0, "character": 0 },
                                "end": { "line": 0, "character": 3 }
                            },
                            "newText": "foo"
                        }]
                    },
                    {
                        "textDocument": {
                            "uri": real_file_uri,
                            "version": 1
                        },
                        "edits": [{
                            "range": {
                                "start": { "line": 10, "character": 5 },
                                "end": { "line": 10, "character": 8 }
                            },
                            "newText": "foo"
                        }]
                    }
                ]
            }
        });
        let context = ResponseTransformContext {
            request_virtual_uri: virtual_uri.to_string(),
            request_host_uri: host_uri.to_string(),
            request_region_start_line: 3,
        };

        let transformed = transform_workspace_edit_to_host(response, &context);

        let document_changes = transformed["result"]["documentChanges"].as_array().unwrap();
        // Should have both entries
        assert_eq!(
            document_changes.len(),
            2,
            "Both entries should be preserved"
        );

        // Find the real file entry
        let real_file_entry = document_changes
            .iter()
            .find(|dc| dc["textDocument"]["uri"] == real_file_uri)
            .expect("Real file entry should exist");

        // Real file URI should be preserved
        assert_eq!(
            real_file_entry["textDocument"]["uri"], real_file_uri,
            "Real file URI should be preserved unchanged"
        );

        // Real file ranges should NOT be transformed
        let real_edits = real_file_entry["edits"].as_array().unwrap();
        assert_eq!(
            real_edits[0]["range"]["start"]["line"], 10,
            "Real file ranges should not be transformed"
        );
    }

    #[test]
    fn workspace_edit_with_null_result_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });
        let context = ResponseTransformContext {
            request_virtual_uri: "file:///.treesitter-ls/abc123/region-0.lua".to_string(),
            request_host_uri: "file:///project/doc.md".to_string(),
            request_region_start_line: 3,
        };

        let transformed = transform_workspace_edit_to_host(response.clone(), &context);
        assert_eq!(transformed, response);
    }

    // ==========================================================================
    // Document link request/response transformation tests
    // ==========================================================================

    #[test]
    fn document_link_request_uses_virtual_uri() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let request = build_bridge_document_link_request(&host_uri, "lua", "region-0", 42);

        let uri_str = request["params"]["textDocument"]["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );
    }

    #[test]
    fn document_link_request_has_correct_method_and_structure() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let request = build_bridge_document_link_request(&host_uri, "lua", "region-0", 42);

        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/documentLink");
        // DocumentLinkParams only has textDocument field - no position
        assert!(
            request["params"].get("position").is_none(),
            "DocumentLinkParams should not have position field"
        );
    }

    #[test]
    fn document_link_request_different_languages_produce_different_extensions() {
        let host_uri = Url::parse("file:///project/doc.md").unwrap();

        let lua_request = build_bridge_document_link_request(&host_uri, "lua", "region-0", 1);
        let python_request = build_bridge_document_link_request(&host_uri, "python", "region-0", 2);

        let lua_uri = lua_request["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();
        let python_uri = python_request["params"]["textDocument"]["uri"]
            .as_str()
            .unwrap();

        assert!(lua_uri.ends_with(".lua"));
        assert!(python_uri.ends_with(".py"));
    }

    #[test]
    fn document_link_response_transforms_ranges_to_host_coordinates() {
        // DocumentLink[] with ranges that need transformation
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 8 },
                        "end": { "line": 0, "character": 20 }
                    },
                    "target": "file:///some/module.lua"
                },
                {
                    "range": {
                        "start": { "line": 2, "character": 8 },
                        "end": { "line": 2, "character": 15 }
                    },
                    "target": "https://example.com/docs"
                }
            ]
        });
        let region_start_line = 5;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        let links = transformed["result"].as_array().unwrap();
        assert_eq!(links.len(), 2);

        // First link: line 0 + 5 = 5
        assert_eq!(links[0]["range"]["start"]["line"], 5);
        assert_eq!(links[0]["range"]["end"]["line"], 5);
        assert_eq!(links[0]["range"]["start"]["character"], 8);
        assert_eq!(links[0]["range"]["end"]["character"], 20);
        assert_eq!(links[0]["target"], "file:///some/module.lua");

        // Second link: line 2 + 5 = 7
        assert_eq!(links[1]["range"]["start"]["line"], 7);
        assert_eq!(links[1]["range"]["end"]["line"], 7);
        assert_eq!(links[1]["target"], "https://example.com/docs");
    }

    #[test]
    fn document_link_response_preserves_target_tooltip_data_unchanged() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 1, "character": 0 },
                        "end": { "line": 1, "character": 10 }
                    },
                    "target": "file:///some/path.lua",
                    "tooltip": "Go to definition",
                    "data": { "custom": "value", "number": 123 }
                }
            ]
        });
        let region_start_line = 3;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        let link = &transformed["result"][0];
        assert_eq!(link["target"], "file:///some/path.lua");
        assert_eq!(link["tooltip"], "Go to definition");
        assert_eq!(link["data"]["custom"], "value");
        assert_eq!(link["data"]["number"], 123);
    }

    #[test]
    fn document_link_response_with_null_result_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let transformed = transform_document_link_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_link_response_with_empty_array_passes_through() {
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": []
        });

        let transformed = transform_document_link_response_to_host(response.clone(), 5);
        assert_eq!(transformed, response);
    }

    #[test]
    fn document_link_response_without_target_transforms_range() {
        // DocumentLink without target (target is optional per LSP spec)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "range": {
                        "start": { "line": 0, "character": 5 },
                        "end": { "line": 0, "character": 15 }
                    }
                }
            ]
        });
        let region_start_line = 10;

        let transformed = transform_document_link_response_to_host(response, region_start_line);

        let link = &transformed["result"][0];
        assert_eq!(link["range"]["start"]["line"], 10);
        assert_eq!(link["range"]["end"]["line"], 10);
        assert!(link.get("target").is_none() || link["target"].is_null());
    }
}
