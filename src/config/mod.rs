pub mod settings;

pub use settings::{
    CaptureMapping, CaptureMappings, HighlightItem, HighlightSource, LanguageConfig,
    QueryTypeMappings, TreeSitterSettings,
};
use std::collections::HashMap;

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
            };
            Some(merged)
        }
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
                // Merge injections
                for (k, v) in primary_mappings.injections.clone() {
                    fallback_mappings.injections.insert(k, v);
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
                filetypes: vec!["rs".to_string()],
                highlight: vec![],
                locals: None,
            },
        );

        let fallback = TreeSitterSettings {
            search_paths: Some(vec!["/path/to/fallback".to_string()]),
            languages: fallback_languages,
            capture_mappings: HashMap::new(),
        };

        let mut primary_languages = HashMap::new();
        primary_languages.insert(
            "rust".to_string(),
            LanguageConfig {
                library: Some("/primary/rust.so".to_string()),
                filetypes: vec!["rs".to_string(), "rust".to_string()],
                highlight: vec![],
                locals: None,
            },
        );

        let primary = TreeSitterSettings {
            search_paths: Some(vec!["/path/to/primary".to_string()]),
            languages: primary_languages,
            capture_mappings: HashMap::new(),
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
        assert_eq!(result.languages["rust"].filetypes, vec!["rs", "rust"]);
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
                injections: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let fallback = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: fallback_mappings,
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
                injections: HashMap::new(),
                folds: HashMap::new(),
            },
        );

        let primary = TreeSitterSettings {
            search_paths: None,
            languages: HashMap::new(),
            capture_mappings: primary_mappings,
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
}
