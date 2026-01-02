pub mod defaults;
pub mod settings;
pub mod user;

pub use settings::{
    BridgeServerConfig, CaptureMapping, CaptureMappings, LanguageConfig, LanguageSettings,
    QueryItem, QueryKind, QueryTypeMappings, TreeSitterSettings, WorkspaceSettings,
    infer_query_kind,
};
use std::collections::HashMap;
pub use user::{UserConfigError, UserConfigResult, load_user_config, user_config_path};

/// Resolve a key from a map with wildcard fallback and merging.
///
/// Implements ADR-0011 wildcard config inheritance:
/// - If both wildcard ("_") and specific key exist: merge them (specific overrides wildcard)
/// - If only wildcard exists: return wildcard
/// - If only specific key exists: return specific key
/// - If neither exists: return None
///
/// The merge creates a new QueryTypeMappings where specific values override wildcard values.
pub fn resolve_with_wildcard(map: &CaptureMappings, key: &str) -> Option<QueryTypeMappings> {
    let wildcard = map.get("_");
    let specific = map.get(key);

    match (wildcard, specific) {
        (Some(w), Some(s)) => {
            // Merge: start with wildcard, override with specific
            let mut merged_highlights = w.highlights.clone();
            for (k, v) in &s.highlights {
                merged_highlights.insert(k.clone(), v.clone());
            }

            let mut merged_locals = w.locals.clone();
            for (k, v) in &s.locals {
                merged_locals.insert(k.clone(), v.clone());
            }

            let mut merged_folds = w.folds.clone();
            for (k, v) in &s.folds {
                merged_folds.insert(k.clone(), v.clone());
            }

            Some(QueryTypeMappings {
                highlights: merged_highlights,
                locals: merged_locals,
                folds: merged_folds,
            })
        }
        (Some(w), None) => Some(w.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}

/// Resolve a language key from a map with wildcard fallback and merging.
///
/// Implements ADR-0011 wildcard config inheritance for languages HashMap:
/// - If both wildcard ("_") and specific key exist: merge them (specific overrides wildcard)
/// - If only wildcard exists: return wildcard
/// - If only specific key exists: return specific key
/// - If neither exists: return None
///
/// The merge creates a new LanguageConfig where specific values override wildcard values.
pub fn resolve_language_with_wildcard(
    map: &HashMap<String, LanguageConfig>,
    key: &str,
) -> Option<LanguageConfig> {
    let wildcard = map.get("_");
    let specific = map.get(key);

    match (wildcard, specific) {
        (Some(w), Some(s)) => {
            // Merge: start with wildcard, override with specific
            Some(LanguageConfig {
                library: s.library.clone().or_else(|| w.library.clone()),
                queries: s.queries.clone().or_else(|| w.queries.clone()),
                highlights: s.highlights.clone().or_else(|| w.highlights.clone()),
                locals: s.locals.clone().or_else(|| w.locals.clone()),
                injections: s.injections.clone().or_else(|| w.injections.clone()),
                bridge: s.bridge.clone().or_else(|| w.bridge.clone()),
            })
        }
        (Some(w), None) => Some(w.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}

/// Resolve a bridge language key from a map with wildcard fallback.
///
/// Implements ADR-0011 wildcard config inheritance for bridge HashMap:
/// - If both wildcard ("_") and specific key exist: return specific (no merge needed for single-field struct)
/// - If only wildcard exists: return wildcard
/// - If only specific key exists: return specific key
/// - If neither exists: return None
///
/// Note: BridgeLanguageConfig only has `enabled` field, so no merging is needed.
pub fn resolve_bridge_with_wildcard(
    map: &HashMap<String, settings::BridgeLanguageConfig>,
    key: &str,
) -> Option<settings::BridgeLanguageConfig> {
    let wildcard = map.get("_");
    let specific = map.get(key);

    match (wildcard, specific) {
        // Specific overrides wildcard entirely (no merge for single-field struct)
        (Some(_), Some(s)) => Some(s.clone()),
        (Some(w), None) => Some(w.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}

/// Resolve a language server key from a map with wildcard fallback and merging.
///
/// Implements ADR-0011 wildcard config inheritance for languageServers HashMap:
/// - If both wildcard ("_") and specific key exist: merge them (specific overrides wildcard)
/// - If only wildcard exists: return wildcard
/// - If only specific key exists: return specific key
/// - If neither exists: return None
///
/// The merge creates a new BridgeServerConfig where specific values override wildcard values.
pub fn resolve_language_server_with_wildcard(
    map: &HashMap<String, settings::BridgeServerConfig>,
    key: &str,
) -> Option<settings::BridgeServerConfig> {
    let wildcard = map.get("_");
    let specific = map.get(key);

    match (wildcard, specific) {
        (Some(w), Some(s)) => {
            // Merge: start with wildcard, override with specific
            Some(settings::BridgeServerConfig {
                // For Vec fields: use specific if non-empty, else wildcard
                cmd: if s.cmd.is_empty() {
                    w.cmd.clone()
                } else {
                    s.cmd.clone()
                },
                languages: if s.languages.is_empty() {
                    w.languages.clone()
                } else {
                    s.languages.clone()
                },
                // For Option fields: specific.or(wildcard)
                initialization_options: s
                    .initialization_options
                    .clone()
                    .or_else(|| w.initialization_options.clone()),
                workspace_type: s.workspace_type.or(w.workspace_type),
            })
        }
        (Some(w), None) => Some(w.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}

/// Resolve a LanguageSettings key from a map with wildcard fallback and merging.
///
/// Implements ADR-0011 wildcard config inheritance for WorkspaceSettings.languages HashMap:
/// - If both wildcard ("_") and specific key exist: merge them (specific overrides wildcard)
/// - If only wildcard exists: return wildcard
/// - If only specific key exists: return specific key
/// - If neither exists: return None
///
/// The merge creates a new LanguageSettings where specific values override wildcard values.
/// This is used by get_bridge_config_for_language to look up host language settings.
pub fn resolve_language_settings_with_wildcard(
    map: &HashMap<String, LanguageSettings>,
    key: &str,
) -> Option<LanguageSettings> {
    let wildcard = map.get("_");
    let specific = map.get(key);

    match (wildcard, specific) {
        (Some(w), Some(s)) => {
            // Merge: start with wildcard, override with specific
            Some(LanguageSettings {
                library: s.library.clone().or_else(|| w.library.clone()),
                // For Vec fields: use specific if non-empty, else wildcard
                highlights: if s.highlights.is_empty() {
                    w.highlights.clone()
                } else {
                    s.highlights.clone()
                },
                locals: s.locals.clone().or_else(|| w.locals.clone()),
                injections: s.injections.clone().or_else(|| w.injections.clone()),
                // For bridge: specific overrides wildcard entirely (no deep merge)
                bridge: s.bridge.clone().or_else(|| w.bridge.clone()),
            })
        }
        (Some(w), None) => Some(w.clone()),
        (None, Some(s)) => Some(s.clone()),
        (None, None) => None,
    }
}

/// Returns the default search paths for parsers and queries.
/// Uses the platform-specific data directory (via `dirs` crate):
/// - Linux: ~/.local/share/treesitter-ls
/// - macOS: ~/Library/Application Support/treesitter-ls
/// - Windows: %APPDATA%/treesitter-ls
///
/// Note: Returns the base directory only. The resolver functions append
/// "parser/" or "queries/" subdirectories as needed.
pub fn default_search_paths() -> Vec<String> {
    crate::install::default_data_dir()
        .map(|d| vec![d.to_string_lossy().to_string()])
        .unwrap_or_default()
}

/// Merge multiple TreeSitterSettings configs in order.
/// Later configs in the slice have higher precedence (override earlier ones).
/// Use this for layered config: `merge_all(&[defaults, user, project, session])`
pub fn merge_all(configs: &[Option<TreeSitterSettings>]) -> Option<TreeSitterSettings> {
    configs.iter().cloned().reduce(merge_settings).flatten()
}

/// Merge two TreeSitterSettings, preferring values from `primary` over `fallback`
pub fn merge_settings(
    fallback: Option<TreeSitterSettings>,
    primary: Option<TreeSitterSettings>,
) -> Option<TreeSitterSettings> {
    match (fallback, primary) {
        (None, None) => None,
        (Some(settings), None) => Some(settings),
        (None, Some(settings)) => Some(settings),
        (Some(fallback), Some(primary)) => {
            let merged = TreeSitterSettings {
                // Prefer primary search_paths, fall back to fallback
                search_paths: primary.search_paths.or(fallback.search_paths),

                // Merge languages: start with fallback, override with primary
                languages: merge_languages(fallback.languages, primary.languages),

                // Merge capture mappings: deep merge with primary taking precedence
                capture_mappings: merge_capture_mappings(
                    fallback.capture_mappings,
                    primary.capture_mappings,
                ),

                // Prefer primary auto_install, fall back to fallback
                auto_install: primary.auto_install.or(fallback.auto_install),

                // Deep merge language_servers HashMap
                language_servers: merge_language_servers(
                    fallback.language_servers,
                    primary.language_servers,
                ),
            };
            Some(merged)
        }
    }
}

impl From<&LanguageConfig> for LanguageSettings {
    fn from(config: &LanguageConfig) -> Self {
        let highlights = config.highlights.clone().unwrap_or_default();
        let locals = config.locals.clone();
        let injections = config.injections.clone();
        let bridge = config.bridge.clone();

        LanguageSettings::with_bridge(
            config.library.clone(),
            highlights,
            locals,
            injections,
            bridge,
        )
    }
}

impl From<&LanguageSettings> for LanguageConfig {
    fn from(settings: &LanguageSettings) -> Self {
        let highlights = if settings.highlights.is_empty() {
            None
        } else {
            Some(settings.highlights.clone())
        };
        let locals = settings.locals.clone();
        let injections = settings.injections.clone();

        LanguageConfig {
            library: settings.library.clone(),
            // filetypes removed from LanguageConfig (PBI-061)
            queries: None, // Conversion from LanguageSettings uses legacy fields
            highlights,
            locals,
            injections,
            bridge: settings.bridge.clone(),
        }
    }
}

impl From<&TreeSitterSettings> for WorkspaceSettings {
    fn from(settings: &TreeSitterSettings) -> Self {
        let languages = settings
            .languages
            .iter()
            .map(|(name, config)| (name.clone(), LanguageSettings::from(config)))
            .collect();
        let capture_mappings = settings
            .capture_mappings
            .iter()
            .map(|(lang, mappings)| {
                (
                    lang.clone(),
                    QueryTypeMappings {
                        highlights: mappings.highlights.clone(),
                        locals: mappings.locals.clone(),
                        folds: mappings.folds.clone(),
                    },
                )
            })
            .collect();

        // Use explicit search_paths if provided, otherwise use platform defaults
        let search_paths = settings
            .search_paths
            .clone()
            .unwrap_or_else(default_search_paths);

        WorkspaceSettings::with_language_servers(
            search_paths,
            languages,
            capture_mappings,
            settings.auto_install.unwrap_or(true), // Default to true for zero-config
            settings.language_servers.clone(),
        )
    }
}

impl From<TreeSitterSettings> for WorkspaceSettings {
    fn from(settings: TreeSitterSettings) -> Self {
        WorkspaceSettings::from(&settings)
    }
}

impl From<&WorkspaceSettings> for TreeSitterSettings {
    fn from(settings: &WorkspaceSettings) -> Self {
        let languages = settings
            .languages
            .iter()
            .map(|(name, config)| (name.clone(), LanguageConfig::from(config)))
            .collect();
        let capture_mappings = settings
            .capture_mappings
            .iter()
            .map(|(lang, mappings)| {
                (
                    lang.clone(),
                    QueryTypeMappings {
                        highlights: mappings.highlights.clone(),
                        locals: mappings.locals.clone(),
                        folds: mappings.folds.clone(),
                    },
                )
            })
            .collect();

        let search_paths = if settings.search_paths.is_empty() {
            None
        } else {
            Some(settings.search_paths.clone())
        };

        TreeSitterSettings {
            search_paths,
            languages,
            capture_mappings,
            auto_install: Some(settings.auto_install),
            language_servers: settings.language_servers.clone(),
        }
    }
}

impl From<WorkspaceSettings> for TreeSitterSettings {
    fn from(settings: WorkspaceSettings) -> Self {
        TreeSitterSettings::from(&settings)
    }
}

fn merge_languages(
    mut fallback: HashMap<String, LanguageConfig>,
    primary: HashMap<String, LanguageConfig>,
) -> HashMap<String, LanguageConfig> {
    // Deep merge: for each language key, merge individual LanguageConfig fields
    for (key, primary_config) in primary {
        fallback
            .entry(key)
            .and_modify(|fallback_config| {
                // primary.or(fallback) for each Option field
                fallback_config.library = primary_config
                    .library
                    .clone()
                    .or(fallback_config.library.take());
                fallback_config.queries = primary_config
                    .queries
                    .clone()
                    .or(fallback_config.queries.take());
                fallback_config.highlights = primary_config
                    .highlights
                    .clone()
                    .or(fallback_config.highlights.take());
                fallback_config.locals = primary_config
                    .locals
                    .clone()
                    .or(fallback_config.locals.take());
                fallback_config.injections = primary_config
                    .injections
                    .clone()
                    .or(fallback_config.injections.take());
                fallback_config.bridge = primary_config
                    .bridge
                    .clone()
                    .or(fallback_config.bridge.take());
            })
            .or_insert(primary_config);
    }
    fallback
}

fn merge_language_servers(
    fallback: Option<HashMap<String, settings::BridgeServerConfig>>,
    primary: Option<HashMap<String, settings::BridgeServerConfig>>,
) -> Option<HashMap<String, settings::BridgeServerConfig>> {
    match (fallback, primary) {
        (None, None) => None,
        (Some(servers), None) | (None, Some(servers)) => Some(servers),
        (Some(mut fallback_servers), Some(primary_servers)) => {
            // Deep merge: for each server key, merge individual BridgeServerConfig fields
            for (key, primary_config) in primary_servers {
                fallback_servers
                    .entry(key)
                    .and_modify(|fallback_config| {
                        // For Vec fields: use primary if non-empty, else keep fallback
                        if !primary_config.cmd.is_empty() {
                            fallback_config.cmd = primary_config.cmd.clone();
                        }
                        if !primary_config.languages.is_empty() {
                            fallback_config.languages = primary_config.languages.clone();
                        }
                        // For Option fields: primary.or(fallback)
                        fallback_config.initialization_options = primary_config
                            .initialization_options
                            .clone()
                            .or(fallback_config.initialization_options.take());
                        fallback_config.workspace_type = primary_config
                            .workspace_type
                            .or(fallback_config.workspace_type.take());
                    })
                    .or_insert(primary_config);
            }
            Some(fallback_servers)
        }
    }
}

fn merge_capture_mappings(
    mut fallback: CaptureMappings,
    primary: CaptureMappings,
) -> CaptureMappings {
    for (lang, primary_mappings) in primary {
        fallback
            .entry(lang)
            .and_modify(|fallback_mappings| {
                // Merge highlights
                for (k, v) in primary_mappings.highlights.clone() {
                    fallback_mappings.highlights.insert(k, v);
                }
                // Merge locals
                for (k, v) in primary_mappings.locals.clone() {
                    fallback_mappings.locals.insert(k, v);
                }
                // Merge folds
                for (k, v) in primary_mappings.folds.clone() {
                    fallback_mappings.folds.insert(k, v);
                }
            })
            .or_insert(primary_mappings);
    }
    fallback
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_merge_settings_with_none() {
        let result = merge_settings(None, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_settings_fallback_only() {
        let fallback = TreeSitterSettings {
            search_paths: Some(vec!["/path/to/fallback".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };
        let result = merge_settings(Some(fallback.clone()), None).unwrap();
        assert_eq!(
            result.search_paths,
            Some(vec!["/path/to/fallback".to_string()])
        );
    }

    #[test]
    fn test_merge_settings_primary_only() {
        let primary = TreeSitterSettings {
            search_paths: Some(vec!["/path/to/primary".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };
        let result = merge_settings(None, Some(primary.clone())).unwrap();
        assert_eq!(
            result.search_paths,
            Some(vec!["/path/to/primary".to_string()])
        );
    }

    #[test]
    fn test_merge_settings_prefer_primary() {
        let mut fallback_languages = HashMap::new();
        fallback_languages.insert(
            "rust".to_string(),
            LanguageConfig {
                library: Some("/fallback/rust.so".to_string()),
                queries: None,
                highlights: None,
                locals: None,
                injections: None,
                bridge: None,
            },
        );

        let fallback = TreeSitterSettings {
            search_paths: Some(vec!["/path/to/fallback".to_string()]),
            languages: fallback_languages,
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let mut primary_languages = HashMap::new();
        primary_languages.insert(
            "rust".to_string(),
            LanguageConfig {
                library: Some("/primary/rust.so".to_string()),
                queries: None,
                highlights: None,
                locals: None,
                injections: None,
                bridge: None,
            },
        );

        let primary = TreeSitterSettings {
            search_paths: Some(vec!["/path/to/primary".to_string()]),
            languages: primary_languages,
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let result = merge_settings(Some(fallback), Some(primary)).unwrap();

        // Primary search paths should win
        assert_eq!(
            result.search_paths,
            Some(vec!["/path/to/primary".to_string()])
        );

        // Primary language config should override fallback
        assert_eq!(
            result.languages["rust"].library,
            Some("/primary/rust.so".to_string())
        );
    }

    #[test]
    fn test_merge_capture_mappings() {
        let mut fallback_mappings = HashMap::new();
        let mut fallback_highlights = HashMap::new();
        fallback_highlights.insert(
            "variable.builtin".to_string(),
            "fallback.variable".to_string(),
        );
        fallback_highlights.insert(
            "function.builtin".to_string(),
            "fallback.function".to_string(),
        );

        fallback_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: fallback_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let fallback = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: fallback_mappings,
            auto_install: None,
            language_servers: None,
        };

        let mut primary_mappings = HashMap::new();
        let mut primary_highlights = HashMap::new();
        primary_highlights.insert(
            "variable.builtin".to_string(),
            "primary.variable".to_string(),
        );
        primary_highlights.insert("type.builtin".to_string(), "primary.type".to_string());

        primary_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: primary_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let primary = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: primary_mappings,
            auto_install: None,
            language_servers: None,
        };

        let result = merge_settings(Some(fallback), Some(primary)).unwrap();

        // Primary should override fallback for same keys
        assert_eq!(
            result.capture_mappings["_"].highlights["variable.builtin"],
            "primary.variable"
        );

        // Primary adds new keys
        assert_eq!(
            result.capture_mappings["_"].highlights["type.builtin"],
            "primary.type"
        );

        // Fallback keys not in primary should remain
        assert_eq!(
            result.capture_mappings["_"].highlights["function.builtin"],
            "fallback.function"
        );
    }

    #[test]
    fn test_capture_mapping_handles_at_prefix() {
        // Create capture mappings with "@" prefix
        let mut capture_mappings = CaptureMappings::new();

        let mut highlights = HashMap::new();
        highlights.insert("@module".to_string(), "@namespace".to_string());
        highlights.insert(
            "@module.builtin".to_string(),
            "@namespace.defaultLibrary".to_string(),
        );

        let query_type_mappings = QueryTypeMappings {
            highlights,
            locals: HashMap::new(),
            folds: HashMap::new(),
        };

        capture_mappings.insert("_".to_string(), query_type_mappings);

        // Verify the mapping exists and contains expected values
        assert!(capture_mappings.contains_key("_"));
        let wildcard_mappings = capture_mappings.get("_").unwrap();
        assert_eq!(
            wildcard_mappings.highlights.get("@module"),
            Some(&"@namespace".to_string())
        );
        assert_eq!(
            wildcard_mappings.highlights.get("@module.builtin"),
            Some(&"@namespace.defaultLibrary".to_string())
        );
    }

    #[test]
    fn test_default_search_paths_used_when_none_configured() {
        // When search_paths is None in TreeSitterSettings, WorkspaceSettings
        // should use the default data directory paths (not an empty vector)
        let settings = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let workspace: WorkspaceSettings = WorkspaceSettings::from(&settings);

        // Default paths should be populated (not empty)
        assert!(
            !workspace.search_paths.is_empty(),
            "search_paths should contain default data directory paths when not configured"
        );

        // Should contain parser and queries subdirectories
        let paths_str = workspace.search_paths.join("|");
        assert!(
            paths_str.contains("treesitter-ls"),
            "Default paths should include treesitter-ls directory: {:?}",
            workspace.search_paths
        );
    }

    #[test]
    fn test_explicit_search_paths_override_default() {
        // When search_paths is explicitly set, it should be used as-is
        let settings = TreeSitterSettings {
            search_paths: Some(vec!["/custom/path".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let workspace: WorkspaceSettings = WorkspaceSettings::from(&settings);

        // Should use explicit paths, not default
        assert_eq!(workspace.search_paths, vec!["/custom/path".to_string()]);
    }

    #[test]
    fn test_search_paths_can_include_default() {
        // Users can extend default paths by including them explicitly
        let default_paths = default_search_paths();
        let mut paths = vec!["/custom/path".to_string()];
        paths.extend(default_paths.clone());

        let settings = TreeSitterSettings {
            search_paths: Some(paths.clone()),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let workspace: WorkspaceSettings = WorkspaceSettings::from(&settings);

        // Should use the combined paths
        assert_eq!(workspace.search_paths.len(), 2); // 1 custom + 1 default (base dir only)
        assert_eq!(workspace.search_paths[0], "/custom/path");
        // Default paths follow
        for (i, default_path) in default_paths.iter().enumerate() {
            assert_eq!(&workspace.search_paths[i + 1], default_path);
        }
    }

    #[test]
    fn test_auto_install_default_true() {
        // PBI-019: autoInstall should default to true for zero-config experience
        // When auto_install is None (not specified), it should be true
        let settings = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None, // Not specified
            language_servers: None,
        };

        let workspace: WorkspaceSettings = WorkspaceSettings::from(&settings);

        assert!(
            workspace.auto_install,
            "auto_install should default to true when not specified"
        );
    }

    #[test]
    fn test_auto_install_explicit_true() {
        // Explicit true should be honored
        let settings = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(true),
            language_servers: None,
        };

        let workspace: WorkspaceSettings = WorkspaceSettings::from(&settings);
        assert!(workspace.auto_install);
    }

    #[test]
    fn test_auto_install_explicit_false() {
        // PBI-019: Users can explicitly disable autoInstall
        let settings = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(false),
            language_servers: None,
        };

        let workspace: WorkspaceSettings = WorkspaceSettings::from(&settings);

        assert!(
            !workspace.auto_install,
            "explicit autoInstall: false should be honored"
        );
    }

    #[test]
    fn test_default_search_paths_format() {
        // PBI-028: default_search_paths() should return base directory only.
        //
        // resolve_library_path() appends "parser/" to each search path,
        // so default_search_paths() should NOT include "parser" or "queries" subdirectories.
        //
        // WRONG: [".../treesitter-ls/parser", ".../treesitter-ls/queries"]
        //   -> resolve_library_path looks for ".../treesitter-ls/parser/parser/lua.so" (FAILS)
        //
        // CORRECT: [".../treesitter-ls"]
        //   -> resolve_library_path looks for ".../treesitter-ls/parser/lua.so" (WORKS)
        let paths = default_search_paths();

        // Should have exactly one path (the base directory)
        assert_eq!(
            paths.len(),
            1,
            "default_search_paths should return single base directory, got {:?}",
            paths
        );

        // The path should NOT end with "/parser" or "/queries"
        let path = &paths[0];
        assert!(
            !path.ends_with("/parser") && !path.ends_with("/queries"),
            "Path should be base directory, not subdirectory: {}",
            path
        );

        // The path should end with "treesitter-ls" (the base directory name)
        assert!(
            path.ends_with("treesitter-ls"),
            "Path should end with 'treesitter-ls': {}",
            path
        );
    }

    // PBI-150: merge_all() tests for multi-layer config merging

    #[test]
    fn test_merge_all_empty_slice_returns_none() {
        // Empty config slice should return None
        let result = merge_all(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_merge_all_single_some_returns_it() {
        // Single Some config should return that config
        let config = TreeSitterSettings {
            search_paths: Some(vec!["/path/one".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(true),
            language_servers: None,
        };
        let result = merge_all(&[Some(config.clone())]);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.search_paths, Some(vec!["/path/one".to_string()]));
        assert_eq!(result.auto_install, Some(true));
    }

    #[test]
    fn test_merge_all_scalar_later_wins() {
        // Later config's scalar values should override earlier ones
        // Simulates: user config has autoInstall=true, project has autoInstall=false
        let user_config = TreeSitterSettings {
            search_paths: Some(vec!["/user/path".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(true),
            language_servers: None,
        };
        let project_config = TreeSitterSettings {
            search_paths: Some(vec!["/project/path".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(false),
            language_servers: None,
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        // Project's values should win (later overrides earlier)
        assert_eq!(result.search_paths, Some(vec!["/project/path".to_string()]));
        assert_eq!(result.auto_install, Some(false));
    }

    #[test]
    fn test_merge_all_four_layers() {
        // Test the full 4-layer merge: defaults < user < project < session
        let defaults = TreeSitterSettings {
            search_paths: Some(vec!["/default/path".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(true),
            language_servers: None,
        };
        let user_config = TreeSitterSettings {
            search_paths: None, // Not overriding, should inherit from defaults
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(true),
            language_servers: None,
        };
        let project_config = TreeSitterSettings {
            search_paths: Some(vec!["/project/path".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None, // Not overriding, should inherit
            language_servers: None,
        };
        let session_config = TreeSitterSettings {
            search_paths: None, // Not overriding
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(false), // Session wins
            language_servers: None,
        };

        let result = merge_all(&[
            Some(defaults),
            Some(user_config),
            Some(project_config),
            Some(session_config),
        ]);

        assert!(result.is_some());
        let result = result.unwrap();

        // search_paths: project wins (later non-None override)
        assert_eq!(result.search_paths, Some(vec!["/project/path".to_string()]));
        // auto_install: session wins
        assert_eq!(result.auto_install, Some(false));
    }

    #[test]
    fn test_merge_all_skips_none_configs() {
        // None configs in the slice should be skipped
        let config = TreeSitterSettings {
            search_paths: Some(vec!["/path".to_string()]),
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: Some(true),
            language_servers: None,
        };

        let result = merge_all(&[None, Some(config.clone()), None]);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.search_paths, Some(vec!["/path".to_string()]));
    }

    #[test]
    fn test_language_settings_from_config_preserves_injections() {
        let config = LanguageConfig {
            library: Some("/path/to/parser.so".to_string()),
            queries: None,
            highlights: Some(vec!["/path/to/highlights.scm".to_string()]),
            locals: None,
            injections: Some(vec!["/path/to/injections.scm".to_string()]),
            bridge: None,
        };

        let settings: LanguageSettings = LanguageSettings::from(&config);

        // Verify injections is preserved in conversion
        assert!(
            settings.injections.is_some(),
            "Injections should be preserved in conversion"
        );
        let injections = settings.injections.as_ref().unwrap();
        assert_eq!(injections.len(), 1);
        assert_eq!(injections[0], "/path/to/injections.scm");
    }

    // PBI-150 Subtask 2: Deep merge for languages HashMap

    #[test]
    fn test_merge_all_languages_deep_merge() {
        // Project sets queries field, inherits parser and bridge from user config
        // This is the key behavior change from shallow to deep merge
        use settings::BridgeLanguageConfig;

        let mut user_languages = HashMap::new();
        let mut user_bridge = HashMap::new();
        user_bridge.insert("rust".to_string(), BridgeLanguageConfig { enabled: true });

        user_languages.insert(
            "python".to_string(),
            LanguageConfig {
                library: Some("/usr/lib/python.so".to_string()),
                queries: None,
                highlights: Some(vec!["/usr/share/python/highlights.scm".to_string()]),
                locals: Some(vec!["/usr/share/python/locals.scm".to_string()]),
                injections: None,
                bridge: Some(user_bridge),
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: user_languages,
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        // Project only overrides highlights for python
        let mut project_languages = HashMap::new();
        project_languages.insert(
            "python".to_string(),
            LanguageConfig {
                library: None, // Not specified - should inherit from user
                queries: None,
                highlights: Some(vec!["./queries/python-highlights.scm".to_string()]),
                locals: None, // Not specified - should inherit from user
                injections: None,
                bridge: None, // Not specified - should inherit from user
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: project_languages,
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        // Python should exist
        assert!(result.languages.contains_key("python"));
        let python = &result.languages["python"];

        // Library: inherited from user (project was None)
        assert_eq!(python.library, Some("/usr/lib/python.so".to_string()));

        // Highlights: overridden by project
        assert_eq!(
            python.highlights,
            Some(vec!["./queries/python-highlights.scm".to_string()])
        );

        // Locals: inherited from user (project was None)
        assert_eq!(
            python.locals,
            Some(vec!["/usr/share/python/locals.scm".to_string()])
        );

        // Bridge: inherited from user (project was None)
        assert!(python.bridge.is_some());
        let bridge = python.bridge.as_ref().unwrap();
        assert!(bridge.get("rust").unwrap().enabled);
    }

    #[test]
    fn test_merge_all_languages_adds_new_keys() {
        // User has python, project adds rust - both should exist
        let mut user_languages = HashMap::new();
        user_languages.insert(
            "python".to_string(),
            LanguageConfig {
                library: Some("/usr/lib/python.so".to_string()),
                queries: None,
                highlights: None,
                locals: None,
                injections: None,
                bridge: None,
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: user_languages,
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let mut project_languages = HashMap::new();
        project_languages.insert(
            "rust".to_string(),
            LanguageConfig {
                library: Some("/project/rust.so".to_string()),
                queries: None,
                highlights: None,
                locals: None,
                injections: None,
                bridge: None,
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: project_languages,
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: None,
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        // Both languages should exist
        assert!(result.languages.contains_key("python"));
        assert!(result.languages.contains_key("rust"));

        // Python from user
        assert_eq!(
            result.languages["python"].library,
            Some("/usr/lib/python.so".to_string())
        );

        // Rust from project
        assert_eq!(
            result.languages["rust"].library,
            Some("/project/rust.so".to_string())
        );
    }

    // PBI-150 Subtask 3: Deep merge for languageServers HashMap

    #[test]
    fn test_merge_all_language_servers_deep_merge() {
        // Project adds initializationOptions to rust-analyzer, inherits cmd and languages from user
        use serde_json::json;
        use settings::{BridgeServerConfig, WorkspaceType};

        let mut user_servers = HashMap::new();
        user_servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec!["rust-analyzer".to_string()],
                languages: vec!["rust".to_string()],
                initialization_options: None,
                workspace_type: Some(WorkspaceType::Cargo),
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: Some(user_servers),
        };

        // Project only adds initializationOptions
        let mut project_servers = HashMap::new();
        project_servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec![],       // Empty, should inherit from user
                languages: vec![], // Empty, should inherit from user
                initialization_options: Some(json!({ "linkedProjects": ["./Cargo.toml"] })),
                workspace_type: None, // Should inherit from user
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: Some(project_servers),
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        assert!(result.language_servers.is_some());
        let servers = result.language_servers.as_ref().unwrap();
        assert!(servers.contains_key("rust-analyzer"));

        let ra = &servers["rust-analyzer"];

        // cmd: inherited from user (project was empty)
        assert_eq!(ra.cmd, vec!["rust-analyzer".to_string()]);

        // languages: inherited from user (project was empty)
        assert_eq!(ra.languages, vec!["rust".to_string()]);

        // initializationOptions: added by project
        assert!(ra.initialization_options.is_some());
        let init_opts = ra.initialization_options.as_ref().unwrap();
        assert!(init_opts.get("linkedProjects").is_some());

        // workspaceType: inherited from user
        assert_eq!(ra.workspace_type, Some(WorkspaceType::Cargo));
    }

    #[test]
    fn test_merge_all_language_servers_adds_new_server() {
        // User has rust-analyzer, project adds pyright - both should exist
        use settings::BridgeServerConfig;

        let mut user_servers = HashMap::new();
        user_servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec!["rust-analyzer".to_string()],
                languages: vec!["rust".to_string()],
                initialization_options: None,
                workspace_type: None,
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: Some(user_servers),
        };

        let mut project_servers = HashMap::new();
        project_servers.insert(
            "pyright".to_string(),
            BridgeServerConfig {
                cmd: vec!["pyright-langserver".to_string(), "--stdio".to_string()],
                languages: vec!["python".to_string()],
                initialization_options: None,
                workspace_type: None,
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: HashMap::new(),
            auto_install: None,
            language_servers: Some(project_servers),
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        assert!(result.language_servers.is_some());
        let servers = result.language_servers.as_ref().unwrap();

        // Both servers should exist
        assert!(servers.contains_key("rust-analyzer"));
        assert!(servers.contains_key("pyright"));

        // rust-analyzer from user
        assert_eq!(
            servers["rust-analyzer"].cmd,
            vec!["rust-analyzer".to_string()]
        );

        // pyright from project
        assert_eq!(
            servers["pyright"].cmd,
            vec!["pyright-langserver".to_string(), "--stdio".to_string()]
        );
    }

    // PBI-150 Subtask 4: Deep merge for captureMappings (already implemented, verify via merge_all)

    #[test]
    fn test_merge_all_capture_mappings_deep_merge() {
        // Project overrides variable.builtin, inherits function.builtin from user config
        let mut user_mappings = HashMap::new();
        let mut user_highlights = HashMap::new();
        user_highlights.insert(
            "variable.builtin".to_string(),
            "variable.defaultLibrary".to_string(),
        );
        user_highlights.insert(
            "function.builtin".to_string(),
            "function.defaultLibrary".to_string(),
        );

        user_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: user_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: user_mappings,
            auto_install: None,
            language_servers: None,
        };

        // Project only overrides variable.builtin
        let mut project_mappings = HashMap::new();
        let mut project_highlights = HashMap::new();
        project_highlights.insert(
            "variable.builtin".to_string(),
            "project.variable".to_string(),
        );

        project_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: project_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: project_mappings,
            auto_install: None,
            language_servers: None,
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        // variable.builtin: overridden by project
        assert_eq!(
            result.capture_mappings["_"].highlights["variable.builtin"],
            "project.variable"
        );

        // function.builtin: inherited from user
        assert_eq!(
            result.capture_mappings["_"].highlights["function.builtin"],
            "function.defaultLibrary"
        );
    }

    #[test]
    fn test_merge_all_capture_mappings_adds_new_language() {
        // User has wildcard "_", project adds "rust" - both should exist
        let mut user_mappings = HashMap::new();
        let mut user_highlights = HashMap::new();
        user_highlights.insert("variable.builtin".to_string(), "user.variable".to_string());

        user_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: user_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: user_mappings,
            auto_install: None,
            language_servers: None,
        };

        let mut project_mappings = HashMap::new();
        let mut rust_highlights = HashMap::new();
        rust_highlights.insert("type.builtin".to_string(), "rust.type".to_string());

        project_mappings.insert(
            "rust".to_string(),
            QueryTypeMappings {
                highlights: rust_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: project_mappings,
            auto_install: None,
            language_servers: None,
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        // Both language keys should exist
        assert!(result.capture_mappings.contains_key("_"));
        assert!(result.capture_mappings.contains_key("rust"));

        // Wildcard from user
        assert_eq!(
            result.capture_mappings["_"].highlights["variable.builtin"],
            "user.variable"
        );

        // Rust from project
        assert_eq!(
            result.capture_mappings["rust"].highlights["type.builtin"],
            "rust.type"
        );
    }

    #[test]
    fn test_merge_all_capture_mappings_locals_and_folds() {
        // Verify deep merge works for locals and folds, not just highlights
        let mut user_mappings = HashMap::new();
        let mut user_locals = HashMap::new();
        user_locals.insert(
            "definition.var".to_string(),
            "definition.variable".to_string(),
        );
        let mut user_folds = HashMap::new();
        user_folds.insert("fold.comment".to_string(), "comment".to_string());

        user_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: HashMap::new(),
                locals: user_locals,
                folds: user_folds,
            },
        );

        let user_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: user_mappings,
            auto_install: None,
            language_servers: None,
        };

        // Project overrides one locals, adds one folds
        let mut project_mappings = HashMap::new();
        let mut project_locals = HashMap::new();
        project_locals.insert(
            "definition.var".to_string(),
            "project.definition".to_string(),
        );
        let mut project_folds = HashMap::new();
        project_folds.insert("fold.function".to_string(), "function".to_string());

        project_mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: HashMap::new(),
                locals: project_locals,
                folds: project_folds,
            },
        );

        let project_config = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: project_mappings,
            auto_install: None,
            language_servers: None,
        };

        let result = merge_all(&[Some(user_config), Some(project_config)]);
        assert!(result.is_some());
        let result = result.unwrap();

        // locals.definition.var: overridden by project
        assert_eq!(
            result.capture_mappings["_"].locals["definition.var"],
            "project.definition"
        );

        // folds.fold.comment: inherited from user
        assert_eq!(
            result.capture_mappings["_"].folds["fold.comment"],
            "comment"
        );

        // folds.fold.function: added by project
        assert_eq!(
            result.capture_mappings["_"].folds["fold.function"],
            "function"
        );
    }

    // PBI-152: Wildcard Config Inheritance (ADR-0011)

    #[test]
    fn test_resolve_with_wildcard_returns_wildcard_when_specific_absent() {
        // ADR-0011: Missing specific key -> use wildcard entirely
        // When captureMappings only has "_" and we ask for "python",
        // we should get the wildcard's mappings
        let mut mappings = CaptureMappings::new();

        let mut wildcard_highlights = HashMap::new();
        wildcard_highlights.insert("variable".to_string(), "variable".to_string());
        wildcard_highlights.insert(
            "variable.builtin".to_string(),
            "variable.defaultLibrary".to_string(),
        );

        mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: wildcard_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Resolve for "python" which doesn't exist - should return wildcard
        let result = resolve_with_wildcard(&mappings, "python");

        assert!(result.is_some(), "Should return Some when wildcard exists");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.highlights.get("variable"),
            Some(&"variable".to_string())
        );
        assert_eq!(
            resolved.highlights.get("variable.builtin"),
            Some(&"variable.defaultLibrary".to_string())
        );
    }

    #[test]
    fn test_resolve_with_wildcard_merges_wildcard_with_specific_key() {
        // ADR-0011: When both wildcard and specific key exist, merge them
        // Rust-specific adds type.builtin, inherits variable.* from wildcard
        let mut mappings = CaptureMappings::new();

        // Wildcard has variable mappings
        let mut wildcard_highlights = HashMap::new();
        wildcard_highlights.insert("variable".to_string(), "variable".to_string());
        wildcard_highlights.insert(
            "variable.builtin".to_string(),
            "variable.defaultLibrary".to_string(),
        );
        wildcard_highlights.insert("function".to_string(), "function".to_string());

        mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: wildcard_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Rust-specific adds type.builtin
        let mut rust_highlights = HashMap::new();
        rust_highlights.insert(
            "type.builtin".to_string(),
            "type.defaultLibrary".to_string(),
        );

        mappings.insert(
            "rust".to_string(),
            QueryTypeMappings {
                highlights: rust_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Resolve for "rust" - should merge wildcard + rust-specific
        let result = resolve_with_wildcard(&mappings, "rust");

        assert!(result.is_some(), "Should return merged mappings");
        let resolved = result.unwrap();

        // Inherited from wildcard
        assert_eq!(
            resolved.highlights.get("variable"),
            Some(&"variable".to_string()),
            "Should inherit 'variable' from wildcard"
        );
        assert_eq!(
            resolved.highlights.get("variable.builtin"),
            Some(&"variable.defaultLibrary".to_string()),
            "Should inherit 'variable.builtin' from wildcard"
        );
        assert_eq!(
            resolved.highlights.get("function"),
            Some(&"function".to_string()),
            "Should inherit 'function' from wildcard"
        );

        // Added by rust-specific
        assert_eq!(
            resolved.highlights.get("type.builtin"),
            Some(&"type.defaultLibrary".to_string()),
            "Should include rust-specific 'type.builtin'"
        );
    }

    // PBI-153: Languages Wildcard Inheritance (ADR-0011)

    #[test]
    fn test_resolve_language_with_wildcard_returns_wildcard_when_specific_absent() {
        // ADR-0011: languages['rust'] inherits from languages['_']
        // When languages only has "_" and we ask for "rust",
        // we should get the wildcard's settings
        let mut languages: HashMap<String, LanguageConfig> = HashMap::new();

        // Wildcard has library and bridge settings
        let mut wildcard_bridge = HashMap::new();
        wildcard_bridge.insert(
            "rust".to_string(),
            settings::BridgeLanguageConfig { enabled: true },
        );

        languages.insert(
            "_".to_string(),
            LanguageConfig {
                library: Some("/default/path.so".to_string()),
                queries: None,
                highlights: Some(vec!["/default/highlights.scm".to_string()]),
                locals: None,
                injections: None,
                bridge: Some(wildcard_bridge),
            },
        );

        // Resolve for "rust" which doesn't exist - should return wildcard
        let result = resolve_language_with_wildcard(&languages, "rust");

        assert!(result.is_some(), "Should return Some when wildcard exists");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.library,
            Some("/default/path.so".to_string()),
            "Should inherit library from wildcard"
        );
        assert!(
            resolved.bridge.is_some(),
            "Should inherit bridge from wildcard"
        );
        let bridge = resolved.bridge.as_ref().unwrap();
        assert!(
            bridge.get("rust").is_some_and(|c| c.enabled),
            "Should inherit bridge settings from wildcard"
        );
    }

    #[test]
    fn test_specific_values_override_wildcards_at_both_levels() {
        // ADR-0011: python.bridge.javascript overrides _.bridge._ settings
        // Setup:
        // - languages._ has bridge._ with enabled = true (default)
        // - languages.python has bridge.javascript with enabled = false (override)
        // - We ask for bridge setting for "javascript" in "python" -> should get enabled = false
        // - We ask for bridge setting for "rust" in "python" -> should get enabled = true (from _)
        let mut languages: HashMap<String, LanguageConfig> = HashMap::new();

        // Wildcard language with wildcard bridge (default enabled = true)
        let mut wildcard_bridge = HashMap::new();
        wildcard_bridge.insert(
            "_".to_string(),
            settings::BridgeLanguageConfig { enabled: true },
        );

        languages.insert(
            "_".to_string(),
            LanguageConfig {
                library: Some("/default/path.so".to_string()),
                queries: None,
                highlights: Some(vec!["/default/highlights.scm".to_string()]),
                locals: None,
                injections: None,
                bridge: Some(wildcard_bridge),
            },
        );

        // Python-specific: disable bridging to JavaScript, but inherit _ for library
        let mut python_bridge = HashMap::new();
        python_bridge.insert(
            "javascript".to_string(),
            settings::BridgeLanguageConfig { enabled: false },
        );

        languages.insert(
            "python".to_string(),
            LanguageConfig {
                library: None, // Should inherit from _
                queries: None,
                highlights: None, // Should inherit from _
                locals: None,
                injections: None,
                bridge: Some(python_bridge),
            },
        );

        // Resolve for "python" - should merge with wildcard
        let resolved_lang = resolve_language_with_wildcard(&languages, "python");
        assert!(resolved_lang.is_some(), "Should resolve python language");
        let lang_config = resolved_lang.unwrap();

        // Library should be inherited from wildcard
        assert_eq!(
            lang_config.library,
            Some("/default/path.so".to_string()),
            "Python should inherit library from wildcard"
        );

        // Bridge should be from python-specific (not merged with wildcard bridge)
        assert!(lang_config.bridge.is_some(), "Python should have bridge");
        let bridge = lang_config.bridge.as_ref().unwrap();

        // JavaScript: python-specific override (enabled = false)
        let js_resolved = resolve_bridge_with_wildcard(bridge, "javascript");
        assert!(js_resolved.is_some(), "Should resolve javascript bridge");
        assert!(
            !js_resolved.unwrap().enabled,
            "Python's javascript bridge should be disabled (override)"
        );

        // Rust: not in python bridge, should NOT inherit from _.bridge._
        // because python has its own bridge map that doesn't include _
        let rust_resolved = resolve_bridge_with_wildcard(bridge, "rust");
        assert!(
            rust_resolved.is_none(),
            "Python's rust bridge should not resolve (no wildcard in python's bridge)"
        );
    }

    #[test]
    fn test_specific_bridge_with_nested_wildcard() {
        // ADR-0011: Test case where python.bridge includes _ wildcard
        // - languages.python.bridge._ = enabled: true (python-specific default)
        // - languages.python.bridge.javascript = enabled: false (override)
        // - rust should inherit from python.bridge._ (enabled = true)
        let mut languages: HashMap<String, LanguageConfig> = HashMap::new();

        // Python with its own wildcard bridge
        let mut python_bridge = HashMap::new();
        python_bridge.insert(
            "_".to_string(),
            settings::BridgeLanguageConfig { enabled: true }, // Python's own default
        );
        python_bridge.insert(
            "javascript".to_string(),
            settings::BridgeLanguageConfig { enabled: false }, // Override for JS
        );

        languages.insert(
            "python".to_string(),
            LanguageConfig {
                library: Some("/python/path.so".to_string()),
                queries: None,
                highlights: None,
                locals: None,
                injections: None,
                bridge: Some(python_bridge),
            },
        );

        let resolved_lang = resolve_language_with_wildcard(&languages, "python");
        assert!(resolved_lang.is_some());
        let lang_config = resolved_lang.unwrap();
        let bridge = lang_config.bridge.as_ref().unwrap();

        // JavaScript: specific override
        let js_resolved = resolve_bridge_with_wildcard(bridge, "javascript");
        assert!(js_resolved.is_some());
        assert!(
            !js_resolved.unwrap().enabled,
            "JavaScript should be disabled"
        );

        // Rust: inherits from python's bridge._
        let rust_resolved = resolve_bridge_with_wildcard(bridge, "rust");
        assert!(rust_resolved.is_some());
        assert!(
            rust_resolved.unwrap().enabled,
            "Rust should inherit from python.bridge._"
        );
    }

    #[test]
    fn test_nested_wildcard_resolution_outer_then_inner() {
        // ADR-0011: Nested wildcard resolution applies outer then inner
        // Resolution order:
        // 1. Resolve outer: languages._ -> languages.python
        // 2. Resolve inner: bridge._ -> bridge.rust
        //
        // Setup:
        // languages._ has bridge._ with enabled = true
        // languages.python is NOT defined (should inherit from _)
        // We ask for bridge setting for "rust" in "python" -> should get enabled = true
        let mut languages: HashMap<String, LanguageConfig> = HashMap::new();

        // Wildcard language with wildcard bridge
        let mut wildcard_bridge = HashMap::new();
        wildcard_bridge.insert(
            "_".to_string(),
            settings::BridgeLanguageConfig { enabled: true },
        );

        languages.insert(
            "_".to_string(),
            LanguageConfig {
                library: Some("/default/path.so".to_string()),
                queries: None,
                highlights: None,
                locals: None,
                injections: None,
                bridge: Some(wildcard_bridge),
            },
        );

        // Resolve for "python" which doesn't exist - should get wildcard language
        let resolved_lang = resolve_language_with_wildcard(&languages, "python");
        assert!(
            resolved_lang.is_some(),
            "Should resolve to wildcard language"
        );

        // Then resolve bridge for "rust" within the resolved language
        let lang_config = resolved_lang.unwrap();
        assert!(
            lang_config.bridge.is_some(),
            "Resolved language should have bridge"
        );
        let bridge = lang_config.bridge.as_ref().unwrap();

        let resolved_bridge = resolve_bridge_with_wildcard(bridge, "rust");
        assert!(
            resolved_bridge.is_some(),
            "Should resolve to wildcard bridge"
        );
        assert!(
            resolved_bridge.unwrap().enabled,
            "Nested wildcard resolution: languages._.bridge._ should apply to python.bridge.rust"
        );
    }

    #[test]
    fn test_resolve_bridge_with_wildcard_returns_wildcard_when_specific_absent() {
        // ADR-0011: bridge['javascript'] inherits from bridge['_']
        // When bridge only has "_" and we ask for "javascript",
        // we should get the wildcard's enabled setting
        let mut bridge: HashMap<String, settings::BridgeLanguageConfig> = HashMap::new();

        // Wildcard has enabled = true
        bridge.insert(
            "_".to_string(),
            settings::BridgeLanguageConfig { enabled: true },
        );

        // Resolve for "javascript" which doesn't exist - should return wildcard
        let result = resolve_bridge_with_wildcard(&bridge, "javascript");

        assert!(result.is_some(), "Should return Some when wildcard exists");
        let resolved = result.unwrap();
        assert!(resolved.enabled, "Should inherit enabled from wildcard");
    }

    #[test]
    fn test_resolve_with_wildcard_specific_overrides_same_capture_name() {
        // ADR-0011: Specific key values override wildcard values for same capture name
        // Example: rust has different "function" mapping than wildcard
        let mut mappings = CaptureMappings::new();

        // Wildcard has function -> function
        let mut wildcard_highlights = HashMap::new();
        wildcard_highlights.insert("function".to_string(), "function".to_string());
        wildcard_highlights.insert("variable".to_string(), "variable".to_string());

        mappings.insert(
            "_".to_string(),
            QueryTypeMappings {
                highlights: wildcard_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Rust overrides function mapping to suppress it (empty string)
        let mut rust_highlights = HashMap::new();
        rust_highlights.insert("function".to_string(), "".to_string());

        mappings.insert(
            "rust".to_string(),
            QueryTypeMappings {
                highlights: rust_highlights,
                locals: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        // Resolve for "rust" - rust's "function" should override wildcard's "function"
        let result = resolve_with_wildcard(&mappings, "rust");

        assert!(result.is_some(), "Should return merged mappings");
        let resolved = result.unwrap();

        // Overridden by rust-specific (empty string suppresses the token)
        assert_eq!(
            resolved.highlights.get("function"),
            Some(&"".to_string()),
            "Rust-specific 'function' should override wildcard 'function'"
        );

        // Still inherited from wildcard
        assert_eq!(
            resolved.highlights.get("variable"),
            Some(&"variable".to_string()),
            "Should still inherit 'variable' from wildcard"
        );
    }

    // PBI-154: languageServers Wildcard Inheritance (ADR-0011)

    #[test]
    fn test_resolve_language_server_with_wildcard_returns_wildcard_when_specific_absent() {
        // ADR-0011: languageServers['rust-analyzer'] inherits from languageServers['_']
        // When languageServers only has "_" and we ask for "rust-analyzer",
        // we should get the wildcard's settings
        use settings::BridgeServerConfig;

        let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

        // Wildcard has default settings
        servers.insert(
            "_".to_string(),
            BridgeServerConfig {
                cmd: vec!["default-lsp".to_string()],
                languages: vec!["any".to_string()],
                initialization_options: None,
                workspace_type: Some(settings::WorkspaceType::Generic),
            },
        );

        // Resolve for "rust-analyzer" which doesn't exist - should return wildcard
        let result = resolve_language_server_with_wildcard(&servers, "rust-analyzer");

        assert!(result.is_some(), "Should return Some when wildcard exists");
        let resolved = result.unwrap();
        assert_eq!(
            resolved.cmd,
            vec!["default-lsp".to_string()],
            "Should inherit cmd from wildcard"
        );
        assert_eq!(
            resolved.languages,
            vec!["any".to_string()],
            "Should inherit languages from wildcard"
        );
        assert_eq!(
            resolved.workspace_type,
            Some(settings::WorkspaceType::Generic),
            "Should inherit workspace_type from wildcard"
        );
    }

    #[test]
    fn test_resolve_language_server_with_wildcard_specific_overrides_wildcard() {
        // ADR-0011: Server-specific values override wildcard values
        // When both wildcard and specific server exist, specific values take precedence
        use serde_json::json;
        use settings::{BridgeServerConfig, WorkspaceType};

        let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

        // Wildcard has default settings
        servers.insert(
            "_".to_string(),
            BridgeServerConfig {
                cmd: vec!["default-lsp".to_string()],
                languages: vec!["any".to_string()],
                initialization_options: Some(json!({ "defaultOption": true })),
                workspace_type: Some(WorkspaceType::Generic),
            },
        );

        // rust-analyzer overrides cmd and workspace_type, but not languages
        servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec!["rust-analyzer".to_string()],
                languages: vec![], // Empty means inherit from wildcard
                initialization_options: Some(json!({ "linkedProjects": ["./Cargo.toml"] })),
                workspace_type: Some(WorkspaceType::Cargo),
            },
        );

        // Resolve for "rust-analyzer" - should merge with wildcard
        let result = resolve_language_server_with_wildcard(&servers, "rust-analyzer");

        assert!(result.is_some(), "Should return merged config");
        let resolved = result.unwrap();

        // cmd: overridden by specific
        assert_eq!(
            resolved.cmd,
            vec!["rust-analyzer".to_string()],
            "Should use rust-analyzer's cmd"
        );

        // languages: inherited from wildcard (specific was empty)
        assert_eq!(
            resolved.languages,
            vec!["any".to_string()],
            "Should inherit languages from wildcard since specific is empty"
        );

        // workspace_type: overridden by specific
        assert_eq!(
            resolved.workspace_type,
            Some(WorkspaceType::Cargo),
            "Should use rust-analyzer's workspace_type"
        );

        // initialization_options: overridden by specific
        let init_opts = resolved.initialization_options.unwrap();
        assert!(
            init_opts.get("linkedProjects").is_some(),
            "Should use rust-analyzer's initialization_options"
        );
        assert!(
            init_opts.get("defaultOption").is_none(),
            "Should NOT inherit wildcard's initialization_options"
        );
    }

    #[test]
    fn test_resolve_language_server_with_wildcard_returns_none_when_neither_exists() {
        // ADR-0011: Neither wildcard nor specific key -> return None
        use settings::BridgeServerConfig;

        let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

        // Only pyright exists, no wildcard
        servers.insert(
            "pyright".to_string(),
            BridgeServerConfig {
                cmd: vec!["pyright-langserver".to_string()],
                languages: vec!["python".to_string()],
                initialization_options: None,
                workspace_type: None,
            },
        );

        // Resolve for "rust-analyzer" which doesn't exist and no wildcard
        let result = resolve_language_server_with_wildcard(&servers, "rust-analyzer");

        assert!(
            result.is_none(),
            "Should return None when neither wildcard nor specific key exists"
        );
    }

    #[test]
    fn test_resolve_language_server_with_wildcard_specific_only() {
        // ADR-0011: No wildcard, only specific key -> return specific
        use settings::BridgeServerConfig;

        let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

        // Only rust-analyzer exists, no wildcard
        servers.insert(
            "rust-analyzer".to_string(),
            BridgeServerConfig {
                cmd: vec!["rust-analyzer".to_string()],
                languages: vec!["rust".to_string()],
                initialization_options: None,
                workspace_type: Some(settings::WorkspaceType::Cargo),
            },
        );

        // Resolve for "rust-analyzer" - should return it directly
        let result = resolve_language_server_with_wildcard(&servers, "rust-analyzer");

        assert!(
            result.is_some(),
            "Should return Some when specific key exists"
        );
        let resolved = result.unwrap();
        assert_eq!(
            resolved.cmd,
            vec!["rust-analyzer".to_string()],
            "Should return specific config"
        );
    }
}
