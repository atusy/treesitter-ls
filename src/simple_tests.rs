#[cfg(test)]
mod simple_tests {
    use crate::*;

    #[test]
    fn test_configuration_parsing() {
        let config_json = r#"{
            "treesitter": {
                "rust": {
                    "library": "/path/to/rust.so",
                    "highlight": [
                        {"path": "/path/to/highlights.scm"},
                        {"query": "(identifier) @variable"}
                    ]
                }
            },
            "filetypes": {
                "rust": ["rs"]
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.treesitter.contains_key("rust"));
        assert_eq!(settings.treesitter["rust"].library, "/path/to/rust.so");
        assert_eq!(settings.treesitter["rust"].highlight.len(), 2);

        match &settings.treesitter["rust"].highlight[0].source {
            HighlightSource::Path { path } => {
                assert_eq!(path, "/path/to/highlights.scm");
            }
            _ => panic!("Expected Path variant"),
        }

        match &settings.treesitter["rust"].highlight[1].source {
            HighlightSource::Query { query } => {
                assert_eq!(query, "(identifier) @variable");
            }
            _ => panic!("Expected Query variant"),
        }
    }

    #[test]
    fn test_highlight_source_deserialization() {
        // Test path-based highlight
        let path_json = r#"{"path": "/test/highlights.scm"}"#;
        let path_item: HighlightItem = serde_json::from_str(path_json).unwrap();
        
        match path_item.source {
            HighlightSource::Path { path } => {
                assert_eq!(path, "/test/highlights.scm");
            }
            _ => panic!("Expected Path variant"),
        }

        // Test query-based highlight
        let query_json = r#"{"query": "(function_item) @function"}"#;
        let query_item: HighlightItem = serde_json::from_str(query_json).unwrap();
        
        match query_item.source {
            HighlightSource::Query { query } => {
                assert_eq!(query, "(function_item) @function");
            }
            _ => panic!("Expected Query variant"),
        }
    }

    #[test]
    fn test_language_config_deserialization() {
        let config_json = r#"{
            "library": "/usr/lib/libtree-sitter-rust.so",
            "highlight": [
                {"path": "/etc/highlights.scm"},
                {"query": "(comment) @comment"}
            ]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        assert_eq!(config.library, "/usr/lib/libtree-sitter-rust.so");
        assert_eq!(config.highlight.len(), 2);
    }

    #[test]
    fn test_url_creation() {
        let path = "/tmp/test.rs";
        let url = Url::from_file_path(path).unwrap();
        assert!(url.as_str().contains("test.rs"));
    }

    #[test]
    fn test_position_creation() {
        let pos = Position {
            line: 10,
            character: 5,
        };
        assert_eq!(pos.line, 10);
        assert_eq!(pos.character, 5);
    }

    #[test]
    fn test_range_creation() {
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 1, character: 10 },
        };
        assert_eq!(range.start.line, 0);
        assert_eq!(range.end.line, 1);
    }

    #[test]
    fn test_symbol_definition_creation() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let range = Range {
            start: Position { line: 0, character: 0 },
            end: Position { line: 0, character: 10 },
        };

        let def = SymbolDefinition {
            name: "test_function".to_string(),
            uri: uri.clone(),
            range,
            kind: SymbolKind::FUNCTION,
        };

        assert_eq!(def.name, "test_function");
        assert_eq!(def.uri, uri);
        assert_eq!(def.kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn test_symbol_reference_creation() {
        let uri = Url::parse("file:///test.rs").unwrap();
        let range = Range {
            start: Position { line: 5, character: 10 },
            end: Position { line: 5, character: 20 },
        };

        let ref_item = SymbolReference {
            name: "my_var".to_string(),
            uri: uri.clone(),
            range,
        };

        assert_eq!(ref_item.name, "my_var");
        assert_eq!(ref_item.uri, uri);
    }

    #[test]
    fn test_legend_types_constants() {
        // Test that our legend types are valid
        assert!(LEGEND_TYPES.len() > 0);
        assert!(LEGEND_TYPES.contains(&SemanticTokenType::FUNCTION));
        assert!(LEGEND_TYPES.contains(&SemanticTokenType::VARIABLE));
        assert!(LEGEND_TYPES.contains(&SemanticTokenType::KEYWORD));
    }
}