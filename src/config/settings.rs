use serde::Deserialize;
use std::collections::HashMap;

pub type CaptureMapping = HashMap<String, String>;

#[derive(Debug, Clone, Deserialize, serde::Serialize, Default, PartialEq, Eq)]
pub struct QueryTypeMappings {
    #[serde(default)]
    pub highlights: CaptureMapping,
    #[serde(default)]
    pub locals: CaptureMapping,
    #[serde(default)]
    pub folds: CaptureMapping,
}

pub type CaptureMappings = HashMap<String, QueryTypeMappings>;

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LanguageConfig {
    pub library: Option<String>,
    #[serde(default)]
    pub filetypes: Vec<String>,
    /// Query file paths for syntax highlighting
    pub highlights: Option<Vec<String>>,
    /// Query file paths for locals/definitions
    pub locals: Option<Vec<String>>,
    /// Query file paths for language injections
    pub injections: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Deserialize, serde::Serialize)]
pub struct TreeSitterSettings {
    // Editor-agnostic name exposed to JSON as `searchPaths`.
    #[serde(rename = "searchPaths")]
    pub search_paths: Option<Vec<String>>,
    #[serde(default)]
    pub languages: HashMap<String, LanguageConfig>,
    #[serde(rename = "captureMappings", default)]
    pub capture_mappings: CaptureMappings,
    #[serde(rename = "autoInstall")]
    pub auto_install: Option<bool>,
}

// Domain types - internal representations used throughout the application

/// Per-language Tree-sitter language configuration surfaced to the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSettings {
    pub library: Option<String>,
    pub filetypes: Vec<String>,
    pub highlights: Vec<String>,
    pub locals: Option<Vec<String>>,
}

impl LanguageSettings {
    pub fn new(
        library: Option<String>,
        filetypes: Vec<String>,
        highlights: Vec<String>,
        locals: Option<Vec<String>>,
    ) -> Self {
        Self {
            library,
            filetypes,
            highlights,
            locals,
        }
    }
}

/// Workspace-wide Tree-sitter configuration as required by the domain.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct WorkspaceSettings {
    pub search_paths: Vec<String>,
    pub languages: HashMap<String, LanguageSettings>,
    pub capture_mappings: CaptureMappings,
    pub auto_install: bool,
}

impl WorkspaceSettings {
    pub fn new(
        search_paths: Vec<String>,
        languages: HashMap<String, LanguageSettings>,
        capture_mappings: CaptureMappings,
    ) -> Self {
        Self {
            search_paths,
            languages,
            capture_mappings,
            auto_install: true, // Default to true for zero-config experience
        }
    }

    pub fn with_auto_install(
        search_paths: Vec<String>,
        languages: HashMap<String, LanguageSettings>,
        capture_mappings: CaptureMappings,
        auto_install: bool,
    ) -> Self {
        Self {
            search_paths,
            languages,
            capture_mappings,
            auto_install,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_parse_valid_configuration() {
        // NEW FORMAT: highlights is a simple array of path strings
        let config_json = r#"{
            "languages": {
                "rust": {
                    "library": "/path/to/rust.so",
                    "filetypes": ["rs"],
                    "highlights": [
                        "/path/to/highlights.scm",
                        "/path/to/custom.scm"
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

        let highlights = settings.languages["rust"].highlights.as_ref().unwrap();
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[0], "/path/to/highlights.scm");
        assert_eq!(highlights[1], "/path/to/custom.scm");
    }

    #[test]
    fn should_parse_configuration_with_locals() {
        // NEW FORMAT: both highlights and locals are simple path arrays
        let config_json = r#"{
            "languages": {
                "rust": {
                    "filetypes": ["rs"],
                    "highlights": ["/path/to/highlights.scm"],
                    "locals": ["/path/to/locals.scm"]
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
        assert_eq!(locals[0], "/path/to/locals.scm");
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
    fn should_handle_completely_empty_json_object() {
        // This is crucial for zero-config: init_options = {} should work
        let completely_empty = r#"{}"#;

        let settings: TreeSitterSettings = serde_json::from_str(completely_empty).unwrap();
        assert!(settings.languages.is_empty());
        assert!(settings.search_paths.is_none());
        assert!(settings.auto_install.is_none());
        assert!(settings.capture_mappings.is_empty());
    }

    #[test]
    fn should_handle_missing_languages_field() {
        let json_without_languages = r#"{
            "searchPaths": ["/some/path"],
            "captureMappings": {
                "_": {
                    "highlights": {}
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(json_without_languages).unwrap();
        assert!(settings.languages.is_empty());
        assert_eq!(settings.search_paths, Some(vec!["/some/path".to_string()]));
    }

    #[test]
    fn should_parse_toml_without_languages_field() {
        let toml_without_languages = r#"
            [captureMappings._.highlights]
        "#;

        let settings: TreeSitterSettings = toml::from_str(toml_without_languages).unwrap();
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
                    "highlights": ["/path/to/highlights.scm"]
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
                    "highlights": ["/path/to/highlights.scm"]
                },
                "python": {
                    "filetypes": ["py"],
                    "highlights": ["/path/to/python.scm"]
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.search_paths.is_some());
        assert_eq!(
            settings.search_paths.unwrap(),
            vec!["/usr/local/lib/tree-sitter"]
        );

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
                    "highlights": ["/path/to/highlights.scm"]
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
    fn should_handle_malformed_json_gracefully() {
        let malformed_configs = vec![
            r#"{"languages": {"rust": {"library": "/path"}"#, // Missing closing braces
            r#"{"languages": {"rust": {"library": "/path", "highlights": [}}"#, // Invalid array
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
                    "highlights": ["/path/to/highlights.scm"]
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
        assert_eq!(
            wildcard_mappings.get("variable.builtin"),
            Some(&"variable.defaultLibrary".to_string())
        );
        assert_eq!(
            wildcard_mappings.get("function.builtin"),
            Some(&"function.defaultLibrary".to_string())
        );

        let rust_mappings = &settings.capture_mappings["rust"].highlights;
        assert_eq!(
            rust_mappings.get("type.builtin"),
            Some(&"type.defaultLibrary".to_string())
        );

        let rust_locals = &settings.capture_mappings["rust"].locals;
        assert_eq!(
            rust_locals.get("definition.var"),
            Some(&"definition.variable".to_string())
        );
    }

    #[test]
    fn should_parse_highlights_as_vec_string() {
        // NEW FORMAT: highlights is a simple array of path strings
        let config_json = r#"{
            "languages": {
                "lua": {
                    "library": "/path/to/lua.so",
                    "highlights": [
                        "/path/to/highlights.scm",
                        "/path/to/custom.scm"
                    ]
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.languages.contains_key("lua"));
        let lua_config = &settings.languages["lua"];
        assert_eq!(lua_config.library, Some("/path/to/lua.so".to_string()));

        // NEW: highlights should be Vec<String>
        assert!(lua_config.highlights.is_some());
        let highlights = lua_config.highlights.as_ref().unwrap();
        assert_eq!(highlights.len(), 2);
        assert_eq!(highlights[0], "/path/to/highlights.scm");
        assert_eq!(highlights[1], "/path/to/custom.scm");
    }

    #[test]
    fn should_parse_auto_install_setting() {
        let config_json = r#"{
            "autoInstall": true,
            "languages": {}
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();
        assert_eq!(settings.auto_install, Some(true));

        // Test with false
        let config_false = r#"{
            "autoInstall": false,
            "languages": {}
        }"#;
        let settings_false: TreeSitterSettings = serde_json::from_str(config_false).unwrap();
        assert_eq!(settings_false.auto_install, Some(false));

        // Test missing (should be None)
        let config_missing = r#"{
            "languages": {}
        }"#;
        let settings_missing: TreeSitterSettings = serde_json::from_str(config_missing).unwrap();
        assert_eq!(settings_missing.auto_install, None);
    }

    #[test]
    fn should_handle_complex_configurations_efficiently() {
        let mut config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
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
                    highlights: Some(vec![format!("/etc/treesitter/{}/highlights.scm", lang)]),
                    locals: None,
                    injections: None,
                },
            );
        }

        assert_eq!(config.languages.len(), 5);

        // Verify serialization/deserialization still works
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: TreeSitterSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.languages.len(), config.languages.len());
    }

    #[test]
    fn should_parse_configuration_with_injections() {
        // Test that injections field can be parsed alongside highlights and locals
        let config_json = r#"{
            "languages": {
                "markdown": {
                    "filetypes": ["md"],
                    "highlights": ["/path/to/highlights.scm"],
                    "injections": ["/path/to/injections.scm"]
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.languages.contains_key("markdown"));
        let md_config = &settings.languages["markdown"];

        // Verify injections configuration is parsed
        assert!(
            md_config.injections.is_some(),
            "Injections configuration should be present"
        );
        let injections = md_config.injections.as_ref().unwrap();
        assert_eq!(injections.len(), 1, "Should have one injections item");
        assert_eq!(injections[0], "/path/to/injections.scm");
    }
}
