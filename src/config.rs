pub mod defaults;
pub mod settings;

pub use settings::{
    CaptureMapping, CaptureMappings, LanguageConfig, LanguageSettings, QueryTypeMappings,
    TreeSitterSettings, WorkspaceSettings,
};
use std::collections::HashMap;

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

                // Prefer primary bridge, fall back to fallback
                bridge: primary.bridge.or(fallback.bridge),
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

        // PBI-120: Use effective_parser() which prefers 'parser' over deprecated 'library'
        LanguageSettings::with_bridge(
            config.effective_parser().cloned(),
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
            parser: settings.library.clone(), // PBI-120: parser is the canonical field
            library: settings.library.clone(),
            // filetypes removed from LanguageConfig (PBI-061)
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

        WorkspaceSettings::with_bridge(
            search_paths,
            languages,
            capture_mappings,
            settings.auto_install.unwrap_or(true), // Default to true for zero-config
            settings.bridge.clone(),
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
            bridge: settings.bridge.clone(),
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
    // Override fallback entries with primary entries
    for (key, value) in primary {
        fallback.insert(key, value);
    }
    fallback
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
            bridge: None,
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
            bridge: None,
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
                parser: None,
                library: Some("/fallback/rust.so".to_string()),
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
            bridge: None,
        };

        let mut primary_languages = HashMap::new();
        primary_languages.insert(
            "rust".to_string(),
            LanguageConfig {
                parser: None,
                library: Some("/primary/rust.so".to_string()),
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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
            bridge: None,
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

    #[test]
    fn test_language_settings_from_config_preserves_injections() {
        let config = LanguageConfig {
            parser: None,
            library: Some("/path/to/parser.so".to_string()),
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
}
