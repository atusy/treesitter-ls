use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;

pub type CaptureMapping = HashMap<String, String>;

/// Workspace type for bridge language server connections.
///
/// Determines the project structure created in the temp directory:
/// - Cargo: Creates Cargo.toml and src/main.rs (for rust-analyzer)
/// - Generic: Creates only a virtual.<ext> file (for language servers that don't need project structure)
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WorkspaceType {
    /// Cargo workspace with Cargo.toml and src/main.rs
    Cargo,
    /// Generic workspace with just a virtual file
    Generic,
}

/// Configuration for a bridge language server.
///
/// This is used to configure external language servers (like rust-analyzer, pyright)
/// that treesitter-ls can redirect requests to for injection regions.
#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BridgeServerConfig {
    /// Command array: first element is the program, rest are arguments
    /// e.g., ["rust-analyzer"] or ["pyright-langserver", "--stdio"]
    pub cmd: Vec<String>,
    /// Languages this server handles (e.g., ["rust"], ["python"])
    pub languages: Vec<String>,
    /// Optional initialization options to pass to the server during initialize
    #[serde(rename = "initializationOptions")]
    pub initialization_options: Option<Value>,
    /// Workspace type for this server (defaults to None, meaning Generic)
    #[serde(rename = "workspaceType")]
    pub workspace_type: Option<WorkspaceType>,
}

/// Bridge settings containing configured language servers.
///
/// JSON schema:
/// ```json
/// {
///   "bridge": {
///     "servers": {
///       "rust-analyzer": { "cmd": ["rust-analyzer"], "languages": ["rust"], ... },
///       "pyright": { "cmd": ["pyright-langserver", "--stdio"], "languages": ["python"] }
///     }
///   }
/// }
/// ```
#[derive(Debug, Clone, Deserialize, serde::Serialize, Default, PartialEq, Eq)]
pub struct BridgeSettings {
    /// Map of server name to server configuration
    #[serde(default)]
    pub servers: HashMap<String, BridgeServerConfig>,
}

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

/// Configuration for a single query file (PBI-120 Phase 2).
///
/// Supports both explicit kind specification and inference from filename.
/// Examples:
/// - `{"path": "/path/to/highlights.scm"}` - kind inferred as "highlights"
/// - `{"path": "/path/to/custom.scm", "kind": "injections"}` - explicit kind
#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct QueryConfig {
    /// Path to the query file (.scm)
    pub path: String,
    /// Query kind (e.g., "highlights", "injections", "locals").
    /// If not specified, will be inferred from the filename.
    pub kind: Option<String>,
}

/// Infer the query kind from a filename or path.
///
/// Extracts the file stem (filename without extension) and returns it as the kind.
/// Examples:
/// - "highlights.scm" -> "highlights"
/// - "/path/to/injections.scm" -> "injections"
/// - "locals.scm" -> "locals"
pub fn infer_query_kind(path: &str) -> &str {
    std::path::Path::new(path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("unknown")
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LanguageConfig {
    /// Path to the tree-sitter parser shared library.
    /// This is the new canonical field name (PBI-120).
    pub parser: Option<String>,
    /// Deprecated: Use `parser` instead.
    /// Path to the tree-sitter parser shared library.
    pub library: Option<String>,
    /// Query file paths for syntax highlighting
    pub highlights: Option<Vec<String>>,
    /// Query file paths for locals/definitions
    pub locals: Option<Vec<String>>,
    /// Query file paths for language injections
    pub injections: Option<Vec<String>>,
    /// Unified query configuration (PBI-120 Phase 2).
    /// Each entry specifies a query file path and optional kind.
    /// If kind is not specified, it will be inferred from the filename.
    pub queries: Option<Vec<QueryConfig>>,
    /// Languages to bridge for this host filetype.
    /// - None (omitted): Bridge ALL configured languages (default behavior)
    /// - Some([]): Bridge NOTHING (disable bridging for this host)
    /// - Some(["python", "r"]): Bridge only the specified languages
    pub bridge: Option<Vec<String>>,
}

impl LanguageConfig {
    /// Check if this config uses the deprecated `library` field.
    ///
    /// Returns true if `library` is set but `parser` is not,
    /// indicating the user is using the deprecated field name.
    pub fn uses_deprecated_library(&self) -> bool {
        self.library.is_some() && self.parser.is_none()
    }

    /// Check if this config uses deprecated query fields (highlights, injections, locals).
    ///
    /// Returns true if any of the old query fields are set but `queries` is not,
    /// indicating the user is using the deprecated field names.
    pub fn uses_deprecated_query_fields(&self) -> bool {
        self.queries.is_none()
            && (self.highlights.is_some() || self.injections.is_some() || self.locals.is_some())
    }

    /// Get the effective parser path, preferring `parser` over `library`.
    ///
    /// This method returns the parser path, using the new `parser` field
    /// if available, otherwise falling back to the deprecated `library` field.
    pub fn effective_parser(&self) -> Option<&String> {
        self.parser.as_ref().or(self.library.as_ref())
    }

    /// Get the unified queries list, combining `queries` field with old format fields.
    ///
    /// Returns a vector of QueryConfig with `kind` resolved:
    /// - If `queries` field is present, returns it with inferred kinds filled in
    /// - Otherwise, converts old `highlights`, `injections`, `locals` fields to QueryConfig
    ///
    /// Each QueryConfig will have its `kind` set:
    /// - Explicit `kind` from the config is preserved
    /// - Otherwise, inferred from the filename using `infer_query_kind`
    pub fn effective_queries(&self) -> Vec<QueryConfig> {
        if let Some(queries) = &self.queries {
            // New format: resolve kinds for each query
            queries
                .iter()
                .map(|q| QueryConfig {
                    path: q.path.clone(),
                    kind: q
                        .kind
                        .clone()
                        .or_else(|| Some(infer_query_kind(&q.path).to_string())),
                })
                .collect()
        } else {
            // Old format: convert highlights, injections, locals to QueryConfig
            let mut result = Vec::new();

            if let Some(highlights) = &self.highlights {
                for path in highlights {
                    result.push(QueryConfig {
                        path: path.clone(),
                        kind: Some("highlights".to_string()),
                    });
                }
            }

            if let Some(injections) = &self.injections {
                for path in injections {
                    result.push(QueryConfig {
                        path: path.clone(),
                        kind: Some("injections".to_string()),
                    });
                }
            }

            if let Some(locals) = &self.locals {
                for path in locals {
                    result.push(QueryConfig {
                        path: path.clone(),
                        kind: Some("locals".to_string()),
                    });
                }
            }

            result
        }
    }
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
    /// Optional bridge settings for configuring external language servers
    #[serde(default)]
    pub bridge: Option<BridgeSettings>,
}

impl TreeSitterSettings {
    /// Log deprecation warnings for any deprecated configuration fields.
    ///
    /// Call this after deserializing settings to warn users about deprecated field usage.
    pub fn log_deprecation_warnings(&self) {
        for (lang_name, config) in &self.languages {
            if config.uses_deprecated_library() {
                log::warn!(
                    "[{}] Configuration uses deprecated 'library' field. Please use 'parser' instead. \
                     The 'library' field will be removed in a future version.",
                    lang_name
                );
            }
            if config.uses_deprecated_query_fields() {
                log::warn!(
                    "[{}] Configuration uses deprecated 'highlights', 'injections', or 'locals' fields. \
                     Please use 'queries' array instead. These fields will be removed in a future version.",
                    lang_name
                );
            }
        }
    }
}

// Domain types - internal representations used throughout the application

/// Per-language Tree-sitter language configuration surfaced to the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSettings {
    pub library: Option<String>,
    pub highlights: Vec<String>,
    pub locals: Option<Vec<String>>,
    pub injections: Option<Vec<String>>,
    /// Languages to bridge for this host filetype.
    /// - None (omitted): Bridge ALL configured languages (default behavior)
    /// - Some([]): Bridge NOTHING (disable bridging for this host)
    /// - Some(["python", "r"]): Bridge only the specified languages
    pub bridge: Option<Vec<String>>,
}

impl LanguageSettings {
    pub fn new(
        library: Option<String>,
        highlights: Vec<String>,
        locals: Option<Vec<String>>,
        injections: Option<Vec<String>>,
    ) -> Self {
        Self {
            library,
            highlights,
            locals,
            injections,
            bridge: None,
        }
    }

    /// Create LanguageSettings with bridge filter configuration.
    pub fn with_bridge(
        library: Option<String>,
        highlights: Vec<String>,
        locals: Option<Vec<String>>,
        injections: Option<Vec<String>>,
        bridge: Option<Vec<String>>,
    ) -> Self {
        Self {
            library,
            highlights,
            locals,
            injections,
            bridge,
        }
    }

    /// Check if a language is allowed for bridging based on the bridge filter.
    ///
    /// Returns:
    /// - `true` if `bridge` is `None` (default: bridge all languages)
    /// - `false` if `bridge` is `Some([])` (empty: bridge nothing)
    /// - `true` if `bridge` contains the injection language
    /// - `false` otherwise
    pub fn is_language_bridgeable(&self, injection_language: &str) -> bool {
        match &self.bridge {
            None => true, // Default: bridge all configured languages
            Some(allowed) if allowed.is_empty() => false, // Empty: bridge nothing
            Some(allowed) => allowed.iter().any(|l| l == injection_language),
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
    pub bridge: Option<BridgeSettings>,
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
            bridge: None,
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
            bridge: None,
        }
    }

    pub fn with_bridge(
        search_paths: Vec<String>,
        languages: HashMap<String, LanguageSettings>,
        capture_mappings: CaptureMappings,
        auto_install: bool,
        bridge: Option<BridgeSettings>,
    ) -> Self {
        Self {
            search_paths,
            languages,
            capture_mappings,
            auto_install,
            bridge,
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
        // filetypes field removed in PBI-061

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
        // filetypes field removed in PBI-061
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
            bridge: None,
        };

        // Add multiple language configurations
        let languages = vec!["rust", "python", "javascript", "typescript", "go"];

        for lang in languages {
            config.languages.insert(
                lang.to_string(),
                LanguageConfig {
                    parser: None,
                    library: Some(format!("/usr/lib/libtree-sitter-{}.so", lang)),
                    highlights: Some(vec![format!("/etc/treesitter/{}/highlights.scm", lang)]),
                    locals: None,
                    injections: None,
                    queries: None,
                    bridge: None,
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
    fn should_not_have_filetypes_field_in_language_config() {
        // PBI-061: filetypes field should be removed from LanguageConfig
        // Language detection now relies entirely on languageId from DidOpen
        let config = LanguageConfig {
            parser: None,
            library: Some("/path/to/parser.so".to_string()),
            highlights: Some(vec!["/path/to/highlights.scm".to_string()]),
            locals: None,
            injections: None,
            queries: None,
            bridge: None,
        };

        // Serialize and verify no filetypes in output
        let json = serde_json::to_string(&config).unwrap();
        assert!(
            !json.contains("filetypes"),
            "LanguageConfig should not have filetypes field, but found in JSON: {}",
            json
        );
    }

    #[test]
    fn should_not_have_filetypes_field_in_language_settings() {
        // PBI-061 S48.3: filetypes field should be removed from LanguageSettings
        // Language detection relies on languageId from DidOpen, not config filetypes
        let settings = LanguageSettings::new(
            Some("/path/to/parser.so".to_string()),
            // No filetypes parameter - constructor should only take 4 args
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
        );

        assert_eq!(settings.library, Some("/path/to/parser.so".to_string()));
        assert_eq!(settings.highlights, vec!["/path/to/highlights.scm"]);
    }

    #[test]
    fn should_parse_bridge_server_config() {
        // Test that BridgeServerConfig can deserialize all fields:
        // cmd (required), languages (required), initialization_options (optional)
        let config_json = r#"{
            "cmd": ["rust-analyzer", "--log-file", "/tmp/ra.log"],
            "languages": ["rust"],
            "initializationOptions": {
                "linkedProjects": ["/path/to/Cargo.toml"]
            }
        }"#;

        let config: BridgeServerConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.cmd,
            vec![
                "rust-analyzer".to_string(),
                "--log-file".to_string(),
                "/tmp/ra.log".to_string()
            ]
        );
        assert_eq!(config.languages, vec!["rust".to_string()]);
        assert!(config.initialization_options.is_some());
        let init_opts = config.initialization_options.unwrap();
        assert!(init_opts.get("linkedProjects").is_some());
    }

    #[test]
    fn should_parse_bridge_server_config_minimal() {
        // Test that only required fields need to be present
        let config_json = r#"{
            "cmd": ["pyright"],
            "languages": ["python"]
        }"#;

        let config: BridgeServerConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.cmd, vec!["pyright".to_string()]);
        assert_eq!(config.languages, vec!["python".to_string()]);
        assert!(config.initialization_options.is_none());
    }

    #[test]
    fn should_parse_bridge_settings() {
        // Test that BridgeSettings deserializes from servers map
        let config_json = r#"{
            "servers": {
                "rust-analyzer": {
                    "cmd": ["rust-analyzer"],
                    "languages": ["rust"],
                    "initializationOptions": {
                        "linkedProjects": ["/path/to/Cargo.toml"]
                    }
                },
                "pyright": {
                    "cmd": ["pyright-langserver", "--stdio"],
                    "languages": ["python"]
                }
            }
        }"#;

        let settings: BridgeSettings = serde_json::from_str(config_json).unwrap();

        assert_eq!(settings.servers.len(), 2);
        assert!(settings.servers.contains_key("rust-analyzer"));
        assert!(settings.servers.contains_key("pyright"));

        let ra = &settings.servers["rust-analyzer"];
        assert_eq!(ra.cmd, vec!["rust-analyzer".to_string()]);
        assert_eq!(ra.languages, vec!["rust".to_string()]);

        let py = &settings.servers["pyright"];
        assert_eq!(
            py.cmd,
            vec!["pyright-langserver".to_string(), "--stdio".to_string()]
        );
    }

    #[test]
    fn should_parse_bridge_settings_empty() {
        // Test that empty servers map is valid
        let config_json = r#"{ "servers": {} }"#;

        let settings: BridgeSettings = serde_json::from_str(config_json).unwrap();
        assert!(settings.servers.is_empty());
    }

    #[test]
    fn should_parse_treesitter_settings_with_bridge() {
        // Test that TreeSitterSettings includes optional bridge field
        let config_json = r#"{
            "searchPaths": ["/usr/local/lib"],
            "bridge": {
                "servers": {
                    "rust-analyzer": {
                        "cmd": ["rust-analyzer"],
                        "languages": ["rust"]
                    }
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.bridge.is_some());
        let bridge = settings.bridge.unwrap();
        assert!(bridge.servers.contains_key("rust-analyzer"));
        assert_eq!(
            bridge.servers["rust-analyzer"].cmd,
            vec!["rust-analyzer".to_string()]
        );
    }

    #[test]
    fn should_parse_treesitter_settings_without_bridge() {
        // Backward compatibility: missing bridge field should be None
        let config_json = r#"{
            "searchPaths": ["/usr/local/lib"],
            "languages": {}
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.bridge.is_none());
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

    #[test]
    fn should_parse_bridge_server_config_with_workspace_type_cargo() {
        // Test that BridgeServerConfig can deserialize workspace_type field with value 'cargo'
        let config_json = r#"{
            "cmd": ["rust-analyzer"],
            "languages": ["rust"],
            "workspaceType": "cargo"
        }"#;

        let config: BridgeServerConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.cmd, vec!["rust-analyzer".to_string()]);
        assert_eq!(config.languages, vec!["rust".to_string()]);
        assert_eq!(config.workspace_type, Some(WorkspaceType::Cargo));
    }

    #[test]
    fn should_parse_bridge_server_config_with_workspace_type_generic() {
        // Test that BridgeServerConfig can deserialize workspace_type field with value 'generic'
        let config_json = r#"{
            "cmd": ["pyright-langserver", "--stdio"],
            "languages": ["python"],
            "workspaceType": "generic"
        }"#;

        let config: BridgeServerConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.cmd,
            vec!["pyright-langserver".to_string(), "--stdio".to_string()]
        );
        assert_eq!(config.languages, vec!["python".to_string()]);
        assert_eq!(config.workspace_type, Some(WorkspaceType::Generic));
    }

    #[test]
    fn should_parse_bridge_server_config_without_workspace_type_defaults_to_none() {
        // Test that missing workspace_type defaults to None
        // The caller should treat None as Generic (changed from Cargo in PBI-105)
        let config_json = r#"{
            "cmd": ["rust-analyzer"],
            "languages": ["rust"]
        }"#;

        let config: BridgeServerConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.cmd, vec!["rust-analyzer".to_string()]);
        assert!(
            config.workspace_type.is_none(),
            "Missing workspace_type should be None"
        );
    }

    #[test]
    fn should_parse_language_config_with_bridge_array() {
        // PBI-108: LanguageConfig should parse bridge field as Option<Vec<String>>
        // AC1: languages.<filetype>.bridge accepts an array of language names
        let config_json = r#"{
            "library": "/path/to/parser.so",
            "highlights": ["/path/to/highlights.scm"],
            "bridge": ["python", "r"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(config.bridge.is_some(), "bridge field should be Some");
        let bridge = config.bridge.unwrap();
        assert_eq!(bridge.len(), 2);
        assert_eq!(bridge[0], "python");
        assert_eq!(bridge[1], "r");
    }

    #[test]
    fn should_parse_language_config_with_empty_bridge_array() {
        // PBI-108: Empty bridge array should disable all bridging
        // AC2: bridge: [] disables all bridging for that host filetype
        let config_json = r#"{
            "library": "/path/to/parser.so",
            "highlights": ["/path/to/highlights.scm"],
            "bridge": []
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(config.bridge.is_some(), "bridge field should be Some([])");
        let bridge = config.bridge.unwrap();
        assert!(bridge.is_empty(), "bridge array should be empty");
    }

    #[test]
    fn should_parse_language_config_without_bridge_field() {
        // PBI-108: Omitted bridge field should be None (bridges all languages)
        // AC3: bridge omitted or null bridges all configured languages
        let config_json = r#"{
            "library": "/path/to/parser.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            config.bridge.is_none(),
            "omitted bridge field should be None"
        );
    }

    #[test]
    fn test_bridge_filter_null_bridges_all_languages() {
        // PBI-108 AC3: bridge omitted or null bridges all configured languages
        // When bridge is None (default), all languages should be bridgeable
        let settings = LanguageSettings::new(
            None,
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
        );

        // Default (None) should bridge all languages
        assert!(
            settings.is_language_bridgeable("python"),
            "None bridge should allow python"
        );
        assert!(
            settings.is_language_bridgeable("r"),
            "None bridge should allow r"
        );
        assert!(
            settings.is_language_bridgeable("rust"),
            "None bridge should allow rust"
        );
        assert!(
            settings.is_language_bridgeable("any_language"),
            "None bridge should allow any language"
        );
    }

    #[test]
    fn test_bridge_filter_empty_disables_bridging() {
        // PBI-108 AC2: bridge: [] disables all bridging for that host filetype
        let settings = LanguageSettings::with_bridge(
            None,
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
            Some(vec![]), // Empty array disables all bridging
        );

        // Empty array should disable all bridging
        assert!(
            !settings.is_language_bridgeable("python"),
            "Empty bridge should not allow python"
        );
        assert!(
            !settings.is_language_bridgeable("r"),
            "Empty bridge should not allow r"
        );
        assert!(
            !settings.is_language_bridgeable("rust"),
            "Empty bridge should not allow rust"
        );
    }

    // PBI-120: parser field as alias for library
    #[test]
    fn should_deserialize_parser_field_as_library_alias() {
        // TDD GREEN: parser field should be accepted and deserialize correctly
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.parser,
            Some("/path/to/lua.so".to_string()),
            "parser field should deserialize correctly"
        );
    }

    #[test]
    fn should_deserialize_library_field_for_backwards_compat() {
        // PBI-120: library field must continue to work for backwards compatibility
        let config_json = r#"{
            "library": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.library,
            Some("/path/to/lua.so".to_string()),
            "library field should still deserialize for backwards compatibility"
        );
    }

    #[test]
    fn should_detect_deprecated_library_field_usage() {
        // PBI-120: When 'library' is used without 'parser', it's deprecated usage
        let config_json = r#"{
            "library": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        // uses_deprecated_library returns true when library is set but parser is not
        assert!(
            config.uses_deprecated_library(),
            "should detect deprecated library field usage"
        );
    }

    #[test]
    fn should_not_detect_deprecation_when_parser_is_used() {
        // PBI-120: When 'parser' is used, no deprecation warning needed
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        // uses_deprecated_library returns false when parser is set
        assert!(
            !config.uses_deprecated_library(),
            "should not flag deprecation when parser is used"
        );
    }

    #[test]
    fn effective_parser_should_prefer_parser_over_library() {
        // PBI-120: effective_parser() should prefer parser over library
        let config_json = r#"{
            "parser": "/new/path/to/parser.so",
            "library": "/old/path/to/library.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.effective_parser(),
            Some(&"/new/path/to/parser.so".to_string()),
            "should prefer parser field when both are set"
        );
    }

    #[test]
    fn effective_parser_should_fall_back_to_library() {
        // PBI-120: When only library is set, effective_parser() should return it
        let config_json = r#"{
            "library": "/path/to/library.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.effective_parser(),
            Some(&"/path/to/library.so".to_string()),
            "should fall back to library when parser is not set"
        );
    }

    #[test]
    fn effective_parser_should_return_none_when_neither_set() {
        // PBI-120: When neither parser nor library is set, return None
        let config_json = r#"{
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(
            config.effective_parser(),
            None,
            "should return None when neither parser nor library is set"
        );
    }

    // PBI-120 Phase 2: QueryConfig type tests
    #[test]
    fn should_deserialize_query_config_with_path_only() {
        // TDD RED: QueryConfig should deserialize {path: "/p.scm"} with kind as None
        let config_json = r#"{"path": "/path/to/highlights.scm"}"#;

        let config: QueryConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.path, "/path/to/highlights.scm");
        assert!(
            config.kind.is_none(),
            "kind should be None when not specified"
        );
    }

    #[test]
    fn should_deserialize_query_config_with_path_and_kind() {
        // TDD RED: QueryConfig should deserialize {path: "/p.scm", kind: "highlights"}
        let config_json = r#"{"path": "/path/to/custom.scm", "kind": "injections"}"#;

        let config: QueryConfig = serde_json::from_str(config_json).unwrap();

        assert_eq!(config.path, "/path/to/custom.scm");
        assert_eq!(config.kind, Some("injections".to_string()));
    }

    // PBI-120 Phase 2: infer_query_kind tests
    #[test]
    fn infer_query_kind_extracts_highlights_from_filename() {
        // TDD RED: infer_query_kind("highlights.scm") should return "highlights"
        assert_eq!(infer_query_kind("highlights.scm"), "highlights");
    }

    #[test]
    fn infer_query_kind_extracts_injections_from_filename() {
        // TDD RED: infer_query_kind("injections.scm") should return "injections"
        assert_eq!(infer_query_kind("injections.scm"), "injections");
    }

    #[test]
    fn infer_query_kind_extracts_locals_from_path() {
        // TDD RED: infer_query_kind("/path/to/locals.scm") should return "locals"
        assert_eq!(infer_query_kind("/path/to/locals.scm"), "locals");
    }

    #[test]
    fn infer_query_kind_extracts_custom_from_filename() {
        // TDD RED: infer_query_kind("custom.scm") should return "custom"
        assert_eq!(infer_query_kind("custom.scm"), "custom");
    }

    // PBI-120 Phase 2: queries field in LanguageConfig tests
    #[test]
    fn should_deserialize_language_config_with_queries_array() {
        // TDD RED: LanguageConfig should deserialize queries: [{path: '/p.scm'}]
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "queries": [
                {"path": "/path/to/highlights.scm"},
                {"path": "/path/to/injections.scm"}
            ]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(config.queries.is_some(), "queries field should be present");
        let queries = config.queries.unwrap();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].path, "/path/to/highlights.scm");
        assert_eq!(queries[1].path, "/path/to/injections.scm");
    }

    #[test]
    fn should_deserialize_language_config_with_queries_and_explicit_kind() {
        // TDD RED: queries with explicit kind should preserve kind
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "queries": [
                {"path": "/path/to/custom.scm", "kind": "injections"}
            ]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        let queries = config.queries.unwrap();
        assert_eq!(queries[0].kind, Some("injections".to_string()));
    }

    #[test]
    fn should_deserialize_language_config_without_queries() {
        // TDD RED: LanguageConfig without queries field should have None
        let config_json = r#"{
            "parser": "/path/to/lua.so"
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            config.queries.is_none(),
            "queries should be None when not specified"
        );
    }

    // PBI-120 Phase 2: effective_queries method tests
    #[test]
    fn effective_queries_returns_queries_field_when_present() {
        // TDD RED: When queries field is set, effective_queries returns it with kind resolved
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "queries": [
                {"path": "/path/to/highlights.scm"},
                {"path": "/path/to/custom.scm", "kind": "injections"}
            ]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        let effective = config.effective_queries();

        assert_eq!(effective.len(), 2);
        // First query: kind inferred from filename
        assert_eq!(effective[0].path, "/path/to/highlights.scm");
        assert_eq!(effective[0].kind, Some("highlights".to_string()));
        // Second query: explicit kind preserved
        assert_eq!(effective[1].path, "/path/to/custom.scm");
        assert_eq!(effective[1].kind, Some("injections".to_string()));
    }

    #[test]
    fn effective_queries_converts_old_highlights_field() {
        // TDD RED: When only old highlights field is set, effective_queries converts it
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        let effective = config.effective_queries();

        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].path, "/path/to/highlights.scm");
        assert_eq!(effective[0].kind, Some("highlights".to_string()));
    }

    #[test]
    fn effective_queries_converts_old_injections_field() {
        // TDD RED: When old injections field is set, effective_queries converts it
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "injections": ["/path/to/injections.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        let effective = config.effective_queries();

        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].path, "/path/to/injections.scm");
        assert_eq!(effective[0].kind, Some("injections".to_string()));
    }

    #[test]
    fn effective_queries_converts_old_locals_field() {
        // TDD RED: When old locals field is set, effective_queries converts it
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "locals": ["/path/to/locals.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        let effective = config.effective_queries();

        assert_eq!(effective.len(), 1);
        assert_eq!(effective[0].path, "/path/to/locals.scm");
        assert_eq!(effective[0].kind, Some("locals".to_string()));
    }

    #[test]
    fn effective_queries_combines_all_old_fields() {
        // TDD RED: When all old fields are set, effective_queries combines them
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"],
            "injections": ["/path/to/injections.scm"],
            "locals": ["/path/to/locals.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        let effective = config.effective_queries();

        assert_eq!(effective.len(), 3);
        // Order: highlights, injections, locals (by old field order)
        assert_eq!(effective[0].kind, Some("highlights".to_string()));
        assert_eq!(effective[1].kind, Some("injections".to_string()));
        assert_eq!(effective[2].kind, Some("locals".to_string()));
    }

    #[test]
    fn effective_queries_returns_empty_when_no_queries() {
        // TDD RED: When no query fields are set, effective_queries returns empty
        let config_json = r#"{
            "parser": "/path/to/lua.so"
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();
        let effective = config.effective_queries();

        assert!(effective.is_empty());
    }

    // PBI-120 Phase 2: deprecation warning tests
    #[test]
    fn uses_deprecated_query_fields_detects_highlights() {
        // TDD RED: Should detect deprecated highlights field usage
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "highlights": ["/path/to/highlights.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            config.uses_deprecated_query_fields(),
            "should detect deprecated highlights field"
        );
    }

    #[test]
    fn uses_deprecated_query_fields_detects_injections() {
        // TDD RED: Should detect deprecated injections field usage
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "injections": ["/path/to/injections.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            config.uses_deprecated_query_fields(),
            "should detect deprecated injections field"
        );
    }

    #[test]
    fn uses_deprecated_query_fields_detects_locals() {
        // TDD RED: Should detect deprecated locals field usage
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "locals": ["/path/to/locals.scm"]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            config.uses_deprecated_query_fields(),
            "should detect deprecated locals field"
        );
    }

    #[test]
    fn uses_deprecated_query_fields_returns_false_for_queries() {
        // TDD RED: Should NOT detect deprecation when using queries field
        let config_json = r#"{
            "parser": "/path/to/lua.so",
            "queries": [{"path": "/path/to/highlights.scm"}]
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            !config.uses_deprecated_query_fields(),
            "should not flag deprecation when using queries field"
        );
    }

    #[test]
    fn uses_deprecated_query_fields_returns_false_when_no_queries() {
        // TDD RED: Should NOT flag deprecation when no query fields are set
        let config_json = r#"{
            "parser": "/path/to/lua.so"
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            !config.uses_deprecated_query_fields(),
            "should not flag deprecation when no query fields are set"
        );
    }

    #[test]
    fn test_bridge_filter_allows_specified_languages() {
        // PBI-108 AC1: languages.<filetype>.bridge accepts an array of language names
        // Only languages in the array should be bridgeable
        let settings = LanguageSettings::with_bridge(
            None,
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
            Some(vec!["python".to_string(), "r".to_string()]),
        );

        // Specified languages should be allowed
        assert!(
            settings.is_language_bridgeable("python"),
            "python should be in bridge filter"
        );
        assert!(
            settings.is_language_bridgeable("r"),
            "r should be in bridge filter"
        );

        // Non-specified languages should NOT be allowed
        assert!(
            !settings.is_language_bridgeable("rust"),
            "rust should not be in bridge filter"
        );
        assert!(
            !settings.is_language_bridgeable("javascript"),
            "javascript should not be in bridge filter"
        );
    }
}
