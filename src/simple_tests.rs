#[cfg(test)]
mod simple_tests {
    use crate::*;
    use std::collections::HashMap;

    mod configuration_tests {
        use super::*;

        #[test]
        fn should_parse_valid_configuration() {
            let config_json = r#"{
                "languages": {
                    "rust": {
                        "library": "/path/to/rust.so",
                        "filetypes": ["rs"],
                        "highlight": [
                            {"path": "/path/to/highlights.scm"},
                            {"query": "(identifier) @variable"}
                        ]
                    }
                }
            }"#;

            let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

            assert!(settings.runtimepath.is_none());
            assert!(settings.languages.contains_key("rust"));
            assert_eq!(
                settings.languages["rust"].library,
                Some("/path/to/rust.so".to_string())
            );
            assert_eq!(settings.languages["rust"].filetypes, vec!["rs"]);
            assert_eq!(settings.languages["rust"].highlight.len(), 2);

            match &settings.languages["rust"].highlight[0].source {
                HighlightSource::Path { path } => {
                    assert_eq!(path, "/path/to/highlights.scm");
                }
                _ => panic!("Expected Path variant"),
            }

            match &settings.languages["rust"].highlight[1].source {
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
        fn should_handle_empty_configurations() {
            let empty_json = r#"{
                "languages": {}
            }"#;

            let settings: TreeSitterSettings = serde_json::from_str(empty_json).unwrap();
            assert!(settings.languages.is_empty());
        }


        #[test]
        fn should_parse_runtimepath_configuration() {
            let config_json = r#"{
                "runtimepath": [
                    "/usr/local/lib/tree-sitter",
                    "/opt/tree-sitter/parsers"
                ],
                "languages": {
                    "rust": {
                        "filetypes": ["rs"],
                        "highlight": [
                            {"path": "/path/to/highlights.scm"}
                        ]
                    }
                }
            }"#;

            let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

            assert!(settings.runtimepath.is_some());
            assert_eq!(
                settings.runtimepath.unwrap(),
                vec!["/usr/local/lib/tree-sitter", "/opt/tree-sitter/parsers"]
            );
            assert!(settings.languages.contains_key("rust"));
            assert_eq!(settings.languages["rust"].library, None);
            assert_eq!(settings.languages["rust"].filetypes, vec!["rs"]);
        }

        #[test]
        fn should_parse_mixed_configuration_with_runtimepath_and_explicit_library() {
            let config_json = r#"{
                "runtimepath": ["/usr/local/lib/tree-sitter"],
                "languages": {
                    "rust": {
                        "library": "/custom/path/rust.so",
                        "filetypes": ["rs"],
                        "highlight": [
                            {"path": "/path/to/highlights.scm"}
                        ]
                    },
                    "python": {
                        "filetypes": ["py"],
                        "highlight": [
                            {"path": "/path/to/python.scm"}
                        ]
                    }
                }
            }"#;

            let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

            assert!(settings.runtimepath.is_some());
            assert_eq!(
                settings.runtimepath.unwrap(),
                vec!["/usr/local/lib/tree-sitter"]
            );

            // rust has explicit library path
            assert_eq!(
                settings.languages["rust"].library,
                Some("/custom/path/rust.so".to_string())
            );

            // python will use runtimepath
            assert_eq!(settings.languages["python"].library, None);
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
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 1,
                    character: 10,
                },
            };
            assert_eq!(range.start.line, 0);
            assert_eq!(range.end.line, 1);

            // Validate range ordering
            assert!(range.start.line <= range.end.line);
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
                assert!(
                    LEGEND_TYPES.contains(&token_type),
                    "Missing required token type: {:?}",
                    token_type
                );
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
                r#"{"languages": {"rust": {"library": "/path"}"#, // Missing closing braces
                r#"{"languages": {"rust": {"library": "/path", "highlight": [}}"#, // Invalid array
            ];

            for config in malformed_configs {
                let result = serde_json::from_str::<TreeSitterSettings>(config);
                assert!(result.is_err());
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

    }

    mod performance_tests {
        use super::*;


        #[test]
        fn should_handle_complex_configurations_efficiently() {
            let mut config = TreeSitterSettings {
                runtimepath: None,
                languages: HashMap::new(),
            };

            // Add multiple language configurations
            let languages = vec!["rust", "python", "javascript", "typescript", "go"];

            for lang in languages {
                config.languages.insert(
                    lang.to_string(),
                    LanguageConfig {
                        library: Some(format!("/usr/lib/libtree-sitter-{}.so", lang)),
                        filetypes: match lang {
                            "rust" => vec!["rs".to_string()],
                            "python" => vec!["py".to_string(), "pyi".to_string()],
                            "javascript" => vec!["js".to_string(), "jsx".to_string()],
                            "typescript" => vec!["ts".to_string(), "tsx".to_string()],
                            "go" => vec!["go".to_string()],
                            _ => vec![],
                        },
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
            }

            assert_eq!(config.languages.len(), 5);

            // Verify serialization/deserialization still works
            let json = serde_json::to_string(&config).unwrap();
            let deserialized: TreeSitterSettings = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.languages.len(), config.languages.len());
        }
    }
}
