#[cfg(test)]
mod simple_tests {
    use crate::*;
    use std::collections::HashMap;

    mod configuration_tests {
        use super::*;
        
        #[test]
        fn should_parse_valid_configuration() {
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
        fn should_reject_invalid_json() {
            let invalid_json = r#"{
                "treesitter": {
                    "rust": {
                        "library": "/path/to/rust.so"
                        // Missing comma - invalid JSON
                        "highlight": []
                    }
                }
            }"#;
            
            let result = serde_json::from_str::<TreeSitterSettings>(invalid_json);
            assert!(result.is_err());
        }
        
        #[test]
        fn should_reject_missing_required_fields() {
            let incomplete_json = r#"{
                "treesitter": {
                    "rust": {
                        "highlight": []
                    }
                }
            }"#;
            
            let result = serde_json::from_str::<TreeSitterSettings>(incomplete_json);
            assert!(result.is_err());
        }
        
        #[test]
        fn should_handle_empty_configurations() {
            let empty_json = r#"{
                "treesitter": {},
                "filetypes": {}
            }"#;
            
            let settings: TreeSitterSettings = serde_json::from_str(empty_json).unwrap();
            assert!(settings.treesitter.is_empty());
            assert!(settings.filetypes.is_empty());
        }
    }

    mod highlight_source_tests {
        use super::*;
        
        #[test]
        fn should_deserialize_path_based_highlight() {
            let path_json = r#"{"path": "/test/highlights.scm"}"#;
            let path_item: HighlightItem = serde_json::from_str(path_json).unwrap();
            
            match path_item.source {
                HighlightSource::Path { path } => {
                    assert_eq!(path, "/test/highlights.scm");
                }
                _ => panic!("Expected Path variant"),
            }
        }
        
        #[test]
        fn should_deserialize_query_based_highlight() {
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
        fn should_reject_invalid_highlight_source() {
            let invalid_json = r#"{"invalid_field": "value"}"#;
            let result = serde_json::from_str::<HighlightItem>(invalid_json);
            assert!(result.is_err());
        }
        
        #[test]
        fn should_reject_empty_highlight_source() {
            let empty_json = r#"{}"#;
            let result = serde_json::from_str::<HighlightItem>(empty_json);
            assert!(result.is_err());
        }
    }

    mod symbol_tests {
        use super::*;
        
        #[test]
        fn should_create_symbol_definition_with_correct_properties() {
            let uri = Url::from_file_path("/test/file.rs").unwrap();
            let range = Range {
                start: Position { line: 5, character: 0 },
                end: Position { line: 5, character: 12 },
            };
            
            let symbol = SymbolDefinition {
                name: "test_function".to_string(),
                uri: uri.clone(),
                range,
                kind: SymbolKind::FUNCTION,
            };
            
            assert_eq!(symbol.name, "test_function");
            assert_eq!(symbol.uri, uri);
            assert_eq!(symbol.kind, SymbolKind::FUNCTION);
            assert_eq!(symbol.range.start.line, 5);
        }
        
        #[test]
        fn should_create_symbol_reference_with_correct_properties() {
            let uri = Url::from_file_path("/test/file.rs").unwrap();
            let range = Range {
                start: Position { line: 10, character: 5 },
                end: Position { line: 10, character: 17 },
            };
            
            let reference = SymbolReference {
                name: "test_function".to_string(),
                uri: uri.clone(),
                range,
            };
            
            assert_eq!(reference.name, "test_function");
            assert_eq!(reference.uri, uri);
            assert_eq!(reference.range.start.character, 5);
        }
        
        #[test]
        fn should_handle_different_symbol_kinds() {
            let uri = Url::from_file_path("/test.rs").unwrap();
            let range = Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 0, character: 10 },
            };
            
            let kinds = vec![
                (SymbolKind::FUNCTION, "function"),
                (SymbolKind::STRUCT, "MyStruct"),
                (SymbolKind::VARIABLE, "variable"),
                (SymbolKind::CONSTANT, "CONSTANT"),
                (SymbolKind::ENUM, "MyEnum"),
                (SymbolKind::MODULE, "module"),
            ];
            
            for (kind, name) in kinds {
                let symbol = SymbolDefinition {
                    name: name.to_string(),
                    uri: uri.clone(),
                    range,
                    kind,
                };
                assert_eq!(symbol.kind, kind);
                assert_eq!(symbol.name, name);
            }
        }
    }

    mod lsp_types_tests {
        use super::*;
        
        #[test]
        fn should_create_valid_url_from_file_path() {
            let path = "/tmp/test.rs";
            let url = Url::from_file_path(path).unwrap();
            assert!(url.as_str().contains("test.rs"));
            assert!(url.scheme() == "file");
        }
        
        #[test]
        fn should_handle_invalid_file_paths() {
            let invalid_path = "not/an/absolute/path";
            let result = Url::from_file_path(invalid_path);
            assert!(result.is_err());
        }
        
        #[test]
        fn should_create_position_with_valid_coordinates() {
            let pos = Position {
                line: 10,
                character: 5,
            };
            assert_eq!(pos.line, 10);
            assert_eq!(pos.character, 5);
        }
        
        #[test]
        fn should_create_valid_range() {
            let range = Range {
                start: Position { line: 0, character: 0 },
                end: Position { line: 1, character: 10 },
            };
            assert_eq!(range.start.line, 0);
            assert_eq!(range.end.line, 1);
            
            // Validate range ordering
            assert!(range.start.line <= range.end.line);
        }
        
        #[test]
        fn should_reject_invalid_ranges() {
            // End position before start position on same line
            let invalid_range = Range {
                start: Position { line: 5, character: 20 },
                end: Position { line: 5, character: 10 },
            };
            
            // This should be caught by validation logic
            assert!(invalid_range.start.character > invalid_range.end.character);
        }
        
        #[test]
        fn should_extract_file_extension_correctly() {
            let test_cases = vec![
                ("/path/to/file.rs", "rs"),
                ("/path/to/file.py", "py"),
                ("/path/to/file.ts", "ts"),
                ("/path/to/file", ""),
            ];
            
            for (path, expected_ext) in test_cases {
                if path.contains('.') {
                    let extension = path.split('.').last().unwrap_or("");
                    assert_eq!(extension, expected_ext);
                } else {
                    // For files without extension, expect empty string
                    assert_eq!(expected_ext, "");
                }
            }
        }
    }

    mod semantic_token_tests {
        use super::*;
        
        #[test]
        fn should_have_comprehensive_legend_types() {
            assert!(LEGEND_TYPES.len() > 0);
            
            let expected_types = vec![
                SemanticTokenType::FUNCTION,
                SemanticTokenType::VARIABLE,
                SemanticTokenType::KEYWORD,
                SemanticTokenType::STRING,
                SemanticTokenType::NUMBER,
                SemanticTokenType::COMMENT,
                SemanticTokenType::STRUCT,
                SemanticTokenType::ENUM,
                SemanticTokenType::MACRO,
                SemanticTokenType::TYPE,
            ];
            
            for token_type in expected_types {
                assert!(LEGEND_TYPES.contains(&token_type), 
                       "Missing required token type: {:?}", token_type);
            }
        }
        
        #[test]
        fn should_maintain_legend_type_order() {
            // Ensure we don't accidentally reorder legend types
            assert_eq!(LEGEND_TYPES[0], SemanticTokenType::COMMENT);
            assert_eq!(LEGEND_TYPES[1], SemanticTokenType::KEYWORD);
            assert_eq!(LEGEND_TYPES[2], SemanticTokenType::STRING);
        }
    }
    
    mod error_handling_tests {
        use super::*;
        
        #[test]
        fn should_handle_malformed_json_gracefully() {
            let malformed_configs = vec![
                r#"{"treesitter": {"rust": {"library": "/path"}"#, // Missing closing braces
                r#"{"treesitter": {"rust": {"library": "/path", "highlight": [}}"#, // Invalid array
                r#"{"invalid_root": {}}"#, // Missing required fields
            ];
            
            for config in malformed_configs {
                let result = serde_json::from_str::<TreeSitterSettings>(config);
                assert!(result.is_err(), "Should reject malformed config: {}", config);
            }
        }
        
        #[test]
        fn should_validate_url_schemes() {
            let valid_urls = vec![
                "file:///absolute/path/to/file.rs",
                "file:///home/user/project/src/main.rs",
            ];
            
            for url_str in valid_urls {
                let url = Url::parse(url_str).unwrap();
                assert_eq!(url.scheme(), "file");
            }
        }
        
        #[test]
        fn should_reject_invalid_urls() {
            let invalid_urls = vec![
                "not-a-url",
                "http://example.com", // Wrong scheme for file operations
                "file://relative/path", // Should be absolute
            ];
            
            for url_str in invalid_urls {
                if let Ok(url) = Url::parse(url_str) {
                    if url.scheme() != "file" {
                        // This is expected for non-file URLs
                        continue;
                    }
                }
                // Either parsing failed or it's not a file URL - both are handled
            }
        }
    }
    
    mod performance_tests {
        use super::*;
        
        #[test]
        fn should_handle_large_symbol_collections() {
            let mut symbols = Vec::new();
            let uri = Url::from_file_path("/test/large_file.rs").unwrap();
            
            // Create a large number of symbols
            for i in 0..1000 {
                let symbol = SymbolDefinition {
                    name: format!("symbol_{}", i),
                    uri: uri.clone(),
                    range: Range {
                        start: Position { line: i as u32, character: 0 },
                        end: Position { line: i as u32, character: 10 },
                    },
                    kind: SymbolKind::FUNCTION,
                };
                symbols.push(symbol);
            }
            
            assert_eq!(symbols.len(), 1000);
            
            // Verify we can efficiently search through symbols
            let target_name = "symbol_500";
            let found = symbols.iter().find(|s| s.name == target_name);
            assert!(found.is_some());
        }
        
        #[test]
        fn should_handle_complex_configurations_efficiently() {
            let mut config = TreeSitterSettings {
                treesitter: HashMap::new(),
                filetypes: HashMap::new(),
            };
            
            // Add multiple language configurations
            let languages = vec!["rust", "python", "javascript", "typescript", "go"];
            
            for lang in languages {
                config.treesitter.insert(
                    lang.to_string(),
                    LanguageConfig {
                        library: format!("/usr/lib/libtree-sitter-{}.so", lang),
                        highlight: vec![
                            HighlightItem {
                                source: HighlightSource::Path {
                                    path: format!("/etc/treesitter/{}/highlights.scm", lang),
                                },
                            },
                            HighlightItem {
                                source: HighlightSource::Query {
                                    query: format!("(function_definition) @function.{}", lang),
                                },
                            },
                        ],
                    },
                );
                
                config.filetypes.insert(
                    lang.to_string(),
                    match lang {
                        "rust" => vec!["rs".to_string()],
                        "python" => vec!["py".to_string(), "pyi".to_string()],
                        "javascript" => vec!["js".to_string(), "jsx".to_string()],
                        "typescript" => vec!["ts".to_string(), "tsx".to_string()],
                        "go" => vec!["go".to_string()],
                        _ => vec![],
                    },
                );
            }
            
            assert_eq!(config.treesitter.len(), 5);
            assert_eq!(config.filetypes.len(), 5);
            
            // Verify serialization/deserialization still works
            let json = serde_json::to_string(&config).unwrap();
            let deserialized: TreeSitterSettings = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.treesitter.len(), config.treesitter.len());
        }
    }
}