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

/// Configuration for a single bridged language within a host filetype.
///
/// Used in the bridge filter map to control whether a specific injection language
/// should be bridged. Example: `{ python = { enabled = true } }`.
#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct BridgeLanguageConfig {
    /// Whether bridging is enabled for this language
    pub enabled: bool,
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

/// Query type for treesitter query files.
///
/// Used in the unified `queries` field to specify what kind of query a file contains.
/// When not specified, the kind is inferred from the filename pattern.
#[derive(Debug, Clone, Copy, Deserialize, serde::Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum QueryKind {
    /// Syntax highlighting queries
    Highlights,
    /// Local definitions/references queries (for scope analysis)
    Locals,
    /// Language injection queries (for embedded languages)
    Injections,
}

/// A single query file configuration entry.
///
/// Used in the unified `queries` array to specify query files with optional type.
/// Example: `{ path = "./highlights.scm", kind = "highlights" }`
#[derive(Debug, Clone, Deserialize, serde::Serialize, PartialEq, Eq)]
pub struct QueryItem {
    /// Path to the query file (required)
    pub path: String,
    /// Query type: highlights, locals, or injections (optional - inferred from filename if omitted)
    pub kind: Option<QueryKind>,
}

#[derive(Debug, Clone, Deserialize, serde::Serialize)]
pub struct LanguageConfig {
    pub library: Option<String>,
    /// Unified query file configuration (new format)
    /// Each entry has a path and optional kind (inferred from filename if omitted)
    pub queries: Option<Vec<QueryItem>>,
    /// Query file paths for syntax highlighting (legacy field)
    pub highlights: Option<Vec<String>>,
    /// Query file paths for locals/definitions (legacy field)
    pub locals: Option<Vec<String>>,
    /// Query file paths for language injections (legacy field)
    pub injections: Option<Vec<String>>,
    /// Languages to bridge for this host filetype (map format).
    /// - None (omitted): Bridge ALL configured languages (default behavior)
    /// - Some({}): Bridge NOTHING (disable bridging for this host)
    /// - Some({ python: { enabled: true } }): Bridge only enabled languages
    pub bridge: Option<HashMap<String, BridgeLanguageConfig>>,
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
    /// Language servers for bridging LSP requests to injection regions.
    /// Map of server name to server configuration.
    #[serde(rename = "languageServers")]
    pub language_servers: Option<HashMap<String, BridgeServerConfig>>,
}

// Domain types - internal representations used throughout the application

/// Per-language Tree-sitter language configuration surfaced to the domain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LanguageSettings {
    pub library: Option<String>,
    pub highlights: Vec<String>,
    pub locals: Option<Vec<String>>,
    pub injections: Option<Vec<String>>,
    /// Languages to bridge for this host filetype (map format).
    /// - None (omitted): Bridge ALL configured languages (default behavior)
    /// - Some({}): Bridge NOTHING (disable bridging for this host)
    /// - Some({ python: { enabled: true } }): Bridge only enabled languages
    pub bridge: Option<HashMap<String, BridgeLanguageConfig>>,
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
        bridge: Option<HashMap<String, BridgeLanguageConfig>>,
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
    /// - `false` if `bridge` is `Some({})` (empty map: bridge nothing)
    /// - `true` if `bridge` contains the language with `enabled: true`
    /// - `false` otherwise (language not in map, or `enabled: false`)
    pub fn is_language_bridgeable(&self, injection_language: &str) -> bool {
        match &self.bridge {
            None => true,                         // Default: bridge all configured languages
            Some(map) if map.is_empty() => false, // Empty map: bridge nothing
            Some(map) => map
                .get(injection_language)
                .is_some_and(|config| config.enabled),
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
    pub language_servers: Option<HashMap<String, BridgeServerConfig>>,
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
            language_servers: None,
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
            language_servers: None,
        }
    }

    pub fn with_language_servers(
        search_paths: Vec<String>,
        languages: HashMap<String, LanguageSettings>,
        capture_mappings: CaptureMappings,
        auto_install: bool,
        language_servers: Option<HashMap<String, BridgeServerConfig>>,
    ) -> Self {
        Self {
            search_paths,
            languages,
            capture_mappings,
            auto_install,
            language_servers,
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
            language_servers: None,
        };

        // Add multiple language configurations
        let languages = vec!["rust", "python", "javascript", "typescript", "go"];

        for lang in languages {
            config.languages.insert(
                lang.to_string(),
                LanguageConfig {
                    library: Some(format!("/usr/lib/libtree-sitter-{}.so", lang)),
                    queries: None,
                    highlights: Some(vec![format!("/etc/treesitter/{}/highlights.scm", lang)]),
                    locals: None,
                    injections: None,
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
            library: Some("/path/to/parser.so".to_string()),
            queries: None,
            highlights: Some(vec!["/path/to/highlights.scm".to_string()]),
            locals: None,
            injections: None,
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
    fn should_parse_language_config_with_bridge_map_enabled() {
        // PBI-120: LanguageConfig should parse bridge field as HashMap<String, BridgeLanguageConfig>
        // Example: bridge = { python = { enabled = true }, r = { enabled = true } }
        let config_json = r#"{
            "library": "/path/to/parser.so",
            "highlights": ["/path/to/highlights.scm"],
            "bridge": {
                "python": { "enabled": true },
                "r": { "enabled": true }
            }
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(config.bridge.is_some(), "bridge field should be Some");
        let bridge = config.bridge.unwrap();
        assert_eq!(bridge.len(), 2);
        assert!(bridge.get("python").unwrap().enabled);
        assert!(bridge.get("r").unwrap().enabled);
    }

    #[test]
    fn should_parse_language_config_with_empty_bridge_map() {
        // PBI-120: Empty bridge map should disable all bridging
        // bridge: {} disables all bridging for that host filetype
        let config_json = r#"{
            "library": "/path/to/parser.so",
            "highlights": ["/path/to/highlights.scm"],
            "bridge": {}
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(
            config.bridge.is_some(),
            "bridge field should be Some(empty map)"
        );
        let bridge = config.bridge.unwrap();
        assert!(bridge.is_empty(), "bridge map should be empty");
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
        // PBI-120: Empty bridge map disables all bridging for that host filetype
        let settings = LanguageSettings::with_bridge(
            None,
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
            Some(HashMap::new()), // Empty map disables all bridging
        );

        // Empty map should disable all bridging
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

    #[test]
    fn test_bridge_filter_allows_enabled_languages() {
        // PBI-120: Only languages with enabled: true should be bridgeable
        let mut bridge = HashMap::new();
        bridge.insert("python".to_string(), BridgeLanguageConfig { enabled: true });
        bridge.insert("r".to_string(), BridgeLanguageConfig { enabled: true });

        let settings = LanguageSettings::with_bridge(
            None,
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
            Some(bridge),
        );

        // Enabled languages should be allowed
        assert!(
            settings.is_language_bridgeable("python"),
            "python should be in bridge filter"
        );
        assert!(
            settings.is_language_bridgeable("r"),
            "r should be in bridge filter"
        );

        // Languages not in map should NOT be allowed
        assert!(
            !settings.is_language_bridgeable("rust"),
            "rust should not be in bridge filter"
        );
        assert!(
            !settings.is_language_bridgeable("javascript"),
            "javascript should not be in bridge filter"
        );
    }

    #[test]
    fn test_bridge_filter_disabled_language() {
        // PBI-120: Languages with enabled: false should not be bridgeable
        let mut bridge = HashMap::new();
        bridge.insert("python".to_string(), BridgeLanguageConfig { enabled: true });
        bridge.insert("r".to_string(), BridgeLanguageConfig { enabled: false });

        let settings = LanguageSettings::with_bridge(
            None,
            vec!["/path/to/highlights.scm".to_string()],
            None,
            None,
            Some(bridge),
        );

        // python with enabled: true should be allowed
        assert!(
            settings.is_language_bridgeable("python"),
            "python should be bridgeable"
        );
        // r with enabled: false should NOT be allowed
        assert!(
            !settings.is_language_bridgeable("r"),
            "r with enabled: false should not be bridgeable"
        );
    }

    #[test]
    fn should_parse_language_servers_at_root() {
        // PBI-119: languageServers field should be at root level of init_options
        // This replaces the nested bridge.servers structure with a flatter schema
        let config_json = r#"{
            "searchPaths": ["/usr/local/lib"],
            "languageServers": {
                "rust-analyzer": {
                    "cmd": ["rust-analyzer"],
                    "languages": ["rust"],
                    "workspaceType": "cargo"
                },
                "pyright": {
                    "cmd": ["pyright-langserver", "--stdio"],
                    "languages": ["python"]
                }
            }
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.language_servers.is_some());
        let servers = settings.language_servers.as_ref().unwrap();
        assert_eq!(servers.len(), 2);

        // Check rust-analyzer config
        assert!(servers.contains_key("rust-analyzer"));
        let ra = &servers["rust-analyzer"];
        assert_eq!(ra.cmd, vec!["rust-analyzer".to_string()]);
        assert_eq!(ra.languages, vec!["rust".to_string()]);
        assert_eq!(ra.workspace_type, Some(WorkspaceType::Cargo));

        // Check pyright config
        assert!(servers.contains_key("pyright"));
        let py = &servers["pyright"];
        assert_eq!(
            py.cmd,
            vec!["pyright-langserver".to_string(), "--stdio".to_string()]
        );
        assert_eq!(py.languages, vec!["python".to_string()]);
    }

    #[test]
    fn should_parse_language_servers_empty() {
        // PBI-119: Empty languageServers should be valid
        let config_json = r#"{
            "languageServers": {}
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.language_servers.is_some());
        assert!(settings.language_servers.as_ref().unwrap().is_empty());
    }

    #[test]
    fn should_parse_without_language_servers() {
        // PBI-119: Missing languageServers should be None (backward compatibility)
        let config_json = r#"{
            "searchPaths": ["/usr/local/lib"]
        }"#;

        let settings: TreeSitterSettings = serde_json::from_str(config_json).unwrap();

        assert!(settings.language_servers.is_none());
    }

    #[test]
    fn should_parse_bridge_language_config() {
        // PBI-120: BridgeLanguageConfig should deserialize with enabled field
        // Example: bridge = { python = { enabled = true } }
        let config_json = r#"{
            "enabled": true
        }"#;

        let config: BridgeLanguageConfig = serde_json::from_str(config_json).unwrap();
        assert!(config.enabled);

        // Test disabled
        let config_false_json = r#"{
            "enabled": false
        }"#;
        let config_false: BridgeLanguageConfig = serde_json::from_str(config_false_json).unwrap();
        assert!(!config_false.enabled);
    }

    #[test]
    fn should_parse_language_config_with_bridge_map() {
        // PBI-120: LanguageConfig.bridge should be HashMap<String, BridgeLanguageConfig>
        // Example: bridge = { python = { enabled = true }, r = { enabled = false } }
        let config_json = r#"{
            "library": "/path/to/parser.so",
            "highlights": ["/path/to/highlights.scm"],
            "bridge": {
                "python": { "enabled": true },
                "r": { "enabled": false }
            }
        }"#;

        let config: LanguageConfig = serde_json::from_str(config_json).unwrap();

        assert!(config.bridge.is_some(), "bridge field should be Some");
        let bridge = config.bridge.unwrap();
        assert_eq!(bridge.len(), 2);
        assert!(bridge.get("python").unwrap().enabled);
        assert!(!bridge.get("r").unwrap().enabled);
    }

    // PBI-151: Unified query configuration with QueryItem struct
    #[test]
    fn should_parse_query_item_with_path_and_kind() {
        // QueryItem should have path (required) and kind (optional) fields
        // kind can be "highlights", "locals", or "injections"
        let toml_str = r#"
            path = "/path/to/highlights.scm"
            kind = "highlights"
        "#;

        let item: QueryItem = toml::from_str(toml_str).unwrap();
        assert_eq!(item.path, "/path/to/highlights.scm");
        assert_eq!(item.kind, Some(QueryKind::Highlights));
    }

    #[test]
    fn should_parse_query_item_without_kind() {
        // kind is optional - defaults to None (type inference happens later)
        let toml_str = r#"
            path = "/path/to/custom.scm"
        "#;

        let item: QueryItem = toml::from_str(toml_str).unwrap();
        assert_eq!(item.path, "/path/to/custom.scm");
        assert!(item.kind.is_none());
    }

    #[test]
    fn should_parse_query_kind_enum_variants() {
        // QueryKind enum should have Highlights, Locals, Injections variants
        let highlights_toml = r#"path = "/a.scm"
kind = "highlights""#;
        let locals_toml = r#"path = "/b.scm"
kind = "locals""#;
        let injections_toml = r#"path = "/c.scm"
kind = "injections""#;

        let h: QueryItem = toml::from_str(highlights_toml).unwrap();
        let l: QueryItem = toml::from_str(locals_toml).unwrap();
        let i: QueryItem = toml::from_str(injections_toml).unwrap();

        assert_eq!(h.kind, Some(QueryKind::Highlights));
        assert_eq!(l.kind, Some(QueryKind::Locals));
        assert_eq!(i.kind, Some(QueryKind::Injections));
    }

    #[test]
    fn should_parse_queries_array_in_language_config() {
        // LanguageConfig should have queries: Option<Vec<QueryItem>>
        let config_toml = r#"
            library = "/path/to/parser.so"
            [[queries]]
            path = "/path/to/highlights.scm"

            [[queries]]
            path = "/path/to/locals.scm"
            kind = "locals"
        "#;

        let config: LanguageConfig = toml::from_str(config_toml).unwrap();
        assert!(config.queries.is_some());
        let queries = config.queries.unwrap();
        assert_eq!(queries.len(), 2);
        assert_eq!(queries[0].path, "/path/to/highlights.scm");
        assert!(queries[0].kind.is_none());
        assert_eq!(queries[1].path, "/path/to/locals.scm");
        assert_eq!(queries[1].kind, Some(QueryKind::Locals));
    }
}
