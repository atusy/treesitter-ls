use serde::Deserialize;
use std::collections::HashMap;

pub type CaptureMapping = HashMap<String, String>;

#[derive(Debug, Clone, Deserialize, serde::Serialize, Default)]
pub struct QueryTypeMappings {
    #[serde(default)]
    pub highlights: CaptureMapping,
    #[serde(default)]
    pub locals: CaptureMapping,
    #[serde(default)]
    pub injections: CaptureMapping,
    #[serde(default)]
    pub folds: CaptureMapping,
}

pub type CaptureMappings = HashMap<String, QueryTypeMappings>;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct HighlightItem {
    #[serde(flatten)]
    pub source: HighlightSource,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
#[serde(untagged)]
pub enum HighlightSource {
    Path { path: String },
    Query { query: String },
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LanguageConfig {
    pub library: Option<String>,
    pub filetypes: Vec<String>,
    #[serde(default)]
    pub highlight: Vec<HighlightItem>,
    pub locals: Option<Vec<HighlightItem>>,
}

#[derive(Debug, Deserialize, serde::Serialize)]
pub struct TreeSitterSettings {
    // Editor-agnostic name exposed to JSON as `searchPaths`.
    #[serde(rename = "searchPaths")]
    pub search_paths: Option<Vec<String>>,
    pub languages: HashMap<String, LanguageConfig>,
    #[serde(rename = "captureMappings", default)]
    pub capture_mappings: CaptureMappings,
}

#[cfg(test)]
mod tests {
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

        assert!(settings.search_paths.is_none());
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
    fn should_parse_configuration_with_locals() {
        let config_json = r#"{
            "languages": {
                "rust": {
                    "filetypes": ["rs"],
                    "highlight": [
                        {"path": "/path/to/highlights.scm"}
                    ],
                    "locals": [
                        {"path": "/path/to/locals.scm"}
                    ]
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.languages.contains_key("rust"));
        let rust_config = &settings.languages["rust"];

        // Verify locals configuration is parsed
        assert!(
            rust_config.locals.is_some(),
            "Locals configuration should be present"
        );
        let locals = rust_config.locals.as_ref().unwrap();
        assert_eq!(locals.len(), 1, "Should have one locals item");

        match &locals[0].source {
            HighlightSource::Path { path } => {
                assert_eq!(path, "/path/to/locals.scm");
            }
            _ => panic!("Expected Path variant for locals"),
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
    fn should_parse_searchpaths_configuration_basic() {
        let config_json = r#"{
            "searchPaths": [
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

        assert!(settings.search_paths.is_some());
        assert_eq!(
            settings.search_paths.unwrap(),
            vec!["/usr/local/lib/tree-sitter", "/opt/tree-sitter/parsers"]
        );
        assert!(settings.languages.contains_key("rust"));
        assert_eq!(settings.languages["rust"].library, None);
        assert_eq!(settings.languages["rust"].filetypes, vec!["rs"]);
    }

    #[test]
    fn should_parse_mixed_configuration_with_searchpaths_and_explicit_library() {
        let config_json = r#"{
            "searchPaths": ["/usr/local/lib/tree-sitter"],
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

        assert!(settings.search_paths.is_some());
        assert_eq!(settings.search_paths.unwrap(), vec!["/usr/local/lib/tree-sitter"]);

        // rust has explicit library path
        assert_eq!(
            settings.languages["rust"].library,
            Some("/custom/path/rust.so".to_string())
        );

        // python will use searchPaths
        assert_eq!(settings.languages["python"].library, None);
    }

    #[test]
    fn should_parse_searchpaths_configuration() {
        let config_json = r#"{
            "searchPaths": [
                "/data/tree-sitter",
                "/assets/ts"
            ],
            "languages": {
                "lua": {
                    "filetypes": ["lua"],
                    "highlight": [
                        {"query": "(identifier) @variable"}
                    ]
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            settings.search_paths.unwrap(),
            vec!["/data/tree-sitter", "/assets/ts"]
        );
        assert!(settings.languages.contains_key("lua"));
    }

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
    fn should_parse_capture_mappings() {
        let config_json = r#"{
            "languages": {
                "rust": {
                    "filetypes": ["rs"],
                    "highlight": [
                        {"path": "/path/to/highlights.scm"}
                    ]
                }
            },
            "captureMappings": {
                "_": {
                    "highlights": {
                        "variable.builtin": "variable.defaultLibrary",
                        "function.builtin": "function.defaultLibrary"
                    }
                },
                "rust": {
                    "highlights": {
                        "type.builtin": "type.defaultLibrary"
                    },
                    "locals": {
                        "definition.var": "definition.variable"
                    }
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();
        
        // Check capture mappings are parsed correctly
        assert!(settings.capture_mappings.contains_key("_"));
        assert!(settings.capture_mappings.contains_key("rust"));
        
        let wildcard_mappings = &settings.capture_mappings["_"].highlights;
        assert_eq!(wildcard_mappings.get("variable.builtin"), Some(&"variable.defaultLibrary".to_string()));
        assert_eq!(wildcard_mappings.get("function.builtin"), Some(&"function.defaultLibrary".to_string()));
        
        let rust_mappings = &settings.capture_mappings["rust"].highlights;
        assert_eq!(rust_mappings.get("type.builtin"), Some(&"type.defaultLibrary".to_string()));
        
        let rust_locals = &settings.capture_mappings["rust"].locals;
        assert_eq!(rust_locals.get("definition.var"), Some(&"definition.variable".to_string()));
    }

    #[test]
    fn should_handle_complex_configurations_efficiently() {
        let mut config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
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
                    locals: None,
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
