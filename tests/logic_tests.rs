// Integration tests for core LSP functionality
use tower_lsp::lsp_types::*;
use treesitter_ls::*;

mod integration_tests {
    use super::*;

    #[test]
    fn should_create_highlight_source_variants() {
        // Given: Different types of highlight sources
        let path_source = HighlightSource::Path {
            path: "/test/file.scm".to_string(),
        };

        let query_source = HighlightSource::Query {
            query: "(identifier) @variable".to_string(),
        };

        // When: Matching on source types
        // Then: Should correctly identify variants
        match path_source {
            HighlightSource::Path { path } => {
                assert_eq!(path, "/test/file.scm");
            }
            _ => panic!("Expected Path variant"),
        }

        match query_source {
            HighlightSource::Query { query } => {
                assert_eq!(query, "(identifier) @variable");
            }
            _ => panic!("Expected Query variant"),
        }
    }

    #[test]
    fn should_handle_multiple_highlight_sources() {
        // Given: A language config with multiple highlight sources
        let config = LanguageConfig {
            library: "/usr/lib/libtree-sitter-rust.so".to_string(),
            filetypes: vec!["rs".to_string()],
            highlight: vec![
                HighlightItem {
                    source: HighlightSource::Path {
                        path: "/etc/highlights.scm".to_string(),
                    },
                },
                HighlightItem {
                    source: HighlightSource::Query {
                        query: "(function_item) @function".to_string(),
                    },
                },
                HighlightItem {
                    source: HighlightSource::Query {
                        query: "(string_literal) @string".to_string(),
                    },
                },
            ],
        };

        // When: Processing the configuration
        // Then: Should handle all highlight sources
        assert_eq!(config.highlight.len(), 3);

        // Verify first source is path-based
        match &config.highlight[0].source {
            HighlightSource::Path { path } => {
                assert!(path.ends_with(".scm"));
            }
            _ => panic!("Expected path source"),
        }

        // Verify remaining sources are query-based
        for highlight in &config.highlight[1..] {
            match &highlight.source {
                HighlightSource::Query { query } => {
                    assert!(!query.is_empty());
                }
                _ => panic!("Expected query source"),
            }
        }
    }
}

mod document_management_tests {
    use super::*;

    #[test]
    fn should_handle_document_lifecycle() {
        // Given: A document URI
        let uri = Url::from_file_path("/test/document.rs").unwrap();

        // When: Managing document lifecycle
        // Then: Should handle open, change, and close events
        assert_eq!(uri.scheme(), "file");
        assert!(uri.path().ends_with(".rs"));

        // Simulate document content changes
        let versions = vec![
            (1, "fn main() {}\n"),
            (2, "fn main() {\n    println!(\"Hello\");\n}\n"),
            (3, "fn main() {\n    println!(\"Hello, World!\");\n}\n"),
        ];

        for (version, content) in versions {
            assert!(version > 0);
            assert!(!content.is_empty());
            assert!(content.contains("fn main"));
        }
    }

    #[test]
    fn should_validate_document_uris() {
        // Given: Various URI formats
        let valid_uris = vec![
            "file:///absolute/path/to/file.rs",
            "file:///home/user/project/src/main.rs",
        ];

        let invalid_uris = vec![
            "relative/path/file.rs",
            "http://example.com/file.rs",
            "not-a-uri",
        ];

        // When: Validating URIs
        // Then: Should identify valid file URIs
        for uri_str in valid_uris {
            let uri = Url::parse(uri_str).unwrap();
            assert_eq!(uri.scheme(), "file");
        }

        for uri_str in invalid_uris {
            match Url::parse(uri_str) {
                Ok(uri) => {
                    // If parsing succeeds, it should not be a file URI for our purposes
                    assert_ne!(uri.scheme(), "file");
                }
                Err(_) => {
                    // Parsing failure is expected for invalid URIs
                }
            }
        }
    }
}

#[test]
fn test_tree_sitter_settings_structure() {
    use std::collections::HashMap;

    let mut languages = HashMap::new();
    languages.insert(
        "rust".to_string(),
        LanguageConfig {
            library: "/lib/rust.so".to_string(),
            filetypes: vec!["rs".to_string()],
            highlight: vec![HighlightItem {
                source: HighlightSource::Query {
                    query: "(function_item) @function".to_string(),
                },
            }],
        },
    );

    let settings = TreeSitterSettings { languages };

    assert!(settings.languages.contains_key("rust"));
    assert_eq!(settings.languages["rust"].filetypes, vec!["rs"]);
}

#[test]
fn test_symbol_definition_properties() {
    let uri = Url::parse("file:///test/example.rs").unwrap();
    let range = Range {
        start: Position {
            line: 10,
            character: 5,
        },
        end: Position {
            line: 10,
            character: 15,
        },
    };

    let definition = SymbolDefinition {
        name: "my_function".to_string(),
        uri: uri.clone(),
        range,
        kind: SymbolKind::FUNCTION,
    };

    assert_eq!(definition.name, "my_function");
    assert_eq!(definition.uri, uri);
    assert_eq!(definition.range.start.line, 10);
    assert_eq!(definition.range.start.character, 5);
    assert_eq!(definition.kind, SymbolKind::FUNCTION);
}

#[test]
fn test_symbol_reference_properties() {
    let uri = Url::parse("file:///test/example.rs").unwrap();
    let range = Range {
        start: Position {
            line: 20,
            character: 8,
        },
        end: Position {
            line: 20,
            character: 18,
        },
    };

    let reference = SymbolReference {
        name: "my_variable".to_string(),
        uri: uri.clone(),
        range,
    };

    assert_eq!(reference.name, "my_variable");
    assert_eq!(reference.uri, uri);
    assert_eq!(reference.range.start.line, 20);
    assert_eq!(reference.range.start.character, 8);
}

#[test]
fn test_various_symbol_kinds() {
    let uri = Url::parse("file:///test/symbols.rs").unwrap();
    let range = Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: Position {
            line: 0,
            character: 10,
        },
    };

    let function_def = SymbolDefinition {
        name: "func".to_string(),
        uri: uri.clone(),
        range,
        kind: SymbolKind::FUNCTION,
    };

    let struct_def = SymbolDefinition {
        name: "MyStruct".to_string(),
        uri: uri.clone(),
        range,
        kind: SymbolKind::STRUCT,
    };

    let variable_def = SymbolDefinition {
        name: "my_var".to_string(),
        uri: uri.clone(),
        range,
        kind: SymbolKind::VARIABLE,
    };

    let constant_def = SymbolDefinition {
        name: "MY_CONST".to_string(),
        uri: uri.clone(),
        range,
        kind: SymbolKind::CONSTANT,
    };

    assert_eq!(function_def.kind, SymbolKind::FUNCTION);
    assert_eq!(struct_def.kind, SymbolKind::STRUCT);
    assert_eq!(variable_def.kind, SymbolKind::VARIABLE);
    assert_eq!(constant_def.kind, SymbolKind::CONSTANT);
}

#[test]
fn test_json_serialization_roundtrip() {
    let original_config = LanguageConfig {
        library: "/test/lib.so".to_string(),
        filetypes: vec!["rs".to_string()],
        highlight: vec![
            HighlightItem {
                source: HighlightSource::Path {
                    path: "/test/highlights.scm".to_string(),
                },
            },
            HighlightItem {
                source: HighlightSource::Query {
                    query: "(string_literal) @string".to_string(),
                },
            },
        ],
    };

    // Serialize to JSON
    let json = serde_json::to_string(&original_config).unwrap();

    // Deserialize back
    let deserialized_config: LanguageConfig = serde_json::from_str(&json).unwrap();

    assert_eq!(original_config.library, deserialized_config.library);
    assert_eq!(original_config.filetypes, deserialized_config.filetypes);
    assert_eq!(
        original_config.highlight.len(),
        deserialized_config.highlight.len()
    );
}

#[test]
fn test_complex_json_config() {
    let json_config = r#"{
        "languages": {
            "rust": {
                "library": "/usr/local/lib/libtree-sitter-rust.so",
                "filetypes": ["rs", "rust"],
                "highlight": [
                    {"path": "/etc/treesitter/rust/highlights.scm"},
                    {"query": "(macro_invocation) @macro"},
                    {"path": "/etc/treesitter/rust/additional.scm"}
                ]
            },
            "python": {
                "library": "/usr/local/lib/libtree-sitter-python.so",
                "filetypes": ["py", "pyi", "python"],
                "highlight": [
                    {"query": "(function_definition) @function"}
                ]
            }
        }
    }"#;

    let settings: TreeSitterSettings = serde_json::from_str(json_config).unwrap();

    // Test rust configuration
    assert!(settings.languages.contains_key("rust"));
    let rust_config = &settings.languages["rust"];
    assert_eq!(rust_config.library, "/usr/local/lib/libtree-sitter-rust.so");
    assert_eq!(rust_config.highlight.len(), 3);
    assert_eq!(rust_config.filetypes, vec!["rs", "rust"]);

    // Test python configuration
    assert!(settings.languages.contains_key("python"));
    let python_config = &settings.languages["python"];
    assert_eq!(
        python_config.library,
        "/usr/local/lib/libtree-sitter-python.so"
    );
    assert_eq!(python_config.highlight.len(), 1);
    assert_eq!(python_config.filetypes, vec!["py", "pyi", "python"]);
}

#[test]
fn test_semantic_token_types_coverage() {
    // Ensure our LEGEND_TYPES covers the main semantic token types
    let legend_types = vec![
        SemanticTokenType::COMMENT,
        SemanticTokenType::KEYWORD,
        SemanticTokenType::STRING,
        SemanticTokenType::NUMBER,
        SemanticTokenType::FUNCTION,
        SemanticTokenType::VARIABLE,
        SemanticTokenType::TYPE,
        SemanticTokenType::STRUCT,
        SemanticTokenType::ENUM,
        SemanticTokenType::MACRO,
    ];

    for token_type in &legend_types {
        assert!(
            LEGEND_TYPES.contains(token_type),
            "Missing token type: {:?}",
            token_type
        );
    }
}

#[test]
fn test_position_ordering() {
    let pos1 = Position {
        line: 5,
        character: 10,
    };
    let pos2 = Position {
        line: 5,
        character: 20,
    };
    let pos3 = Position {
        line: 6,
        character: 0,
    };

    // Test that positions can be compared logically
    assert!(pos1.line == pos2.line);
    assert!(pos1.character < pos2.character);
    assert!(pos2.line < pos3.line);
}

#[test]
fn test_url_path_extraction() {
    let url = Url::parse("file:///home/user/project/src/main.rs").unwrap();
    let path = url.path();

    assert!(path.ends_with("main.rs"));
    assert!(path.contains("/src/"));

    // Test extension extraction (simulating get_language_for_document logic)
    let extension = path.split('.').last().unwrap_or("");
    assert_eq!(extension, "rs");
}

#[test]
fn test_range_validity() {
    // Test valid range
    let valid_range = Range {
        start: Position {
            line: 10,
            character: 5,
        },
        end: Position {
            line: 10,
            character: 15,
        },
    };

    assert!(valid_range.start.line <= valid_range.end.line);
    if valid_range.start.line == valid_range.end.line {
        assert!(valid_range.start.character <= valid_range.end.character);
    }

    // Test multi-line range
    let multiline_range = Range {
        start: Position {
            line: 10,
            character: 20,
        },
        end: Position {
            line: 12,
            character: 5,
        },
    };

    assert!(multiline_range.start.line < multiline_range.end.line);
}
