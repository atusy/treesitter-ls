pub mod defaults;
pub mod settings;
pub mod user;

pub use settings::{
    CaptureMapping, CaptureMappings, LanguageConfig, LanguageSettings, QueryItem, QueryKind,
    QueryTypeMappings, TreeSitterSettings, WorkspaceSettings, infer_query_kind,
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
pub fn resolve_with_wildcard(
    map: &CaptureMappings,
    key: &str,
) -> Option<QueryTypeMappings> {
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
}
