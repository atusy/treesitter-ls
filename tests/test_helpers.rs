// Test helper utilities following TDD best practices
use std::collections::HashMap;
use tower_lsp::lsp_types::*;
use treesitter_ls::*;

pub struct TestSymbolBuilder {
    name: String,
    uri: Url,
    kind: SymbolKind,
    line: u32,
    character: u32,
    end_character: Option<u32>,
}

impl TestSymbolBuilder {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            uri: Url::from_file_path("/test/default.rs").unwrap(),
            kind: SymbolKind::FUNCTION,
            line: 0,
            character: 0,
            end_character: None,
        }
    }

    pub fn at_position(mut self, line: u32, character: u32) -> Self {
        self.line = line;
        self.character = character;
        self
    }

    pub fn with_kind(mut self, kind: SymbolKind) -> Self {
        self.kind = kind;
        self
    }

    pub fn in_file(mut self, path: &str) -> Self {
        self.uri = Url::from_file_path(path).unwrap();
        self
    }

    pub fn ending_at(mut self, end_character: u32) -> Self {
        self.end_character = Some(end_character);
        self
    }

    pub fn build(self) -> SymbolDefinition {
        let end_char = self
            .end_character
            .unwrap_or(self.character + self.name.len() as u32);

        SymbolDefinition {
            name: self.name,
            uri: self.uri,
            range: Range {
                start: Position {
                    line: self.line,
                    character: self.character,
                },
                end: Position {
                    line: self.line,
                    character: end_char,
                },
            },
            kind: self.kind,
        }
    }
}

pub struct TestConfigBuilder {
    languages: HashMap<String, LanguageConfig>,
}

impl TestConfigBuilder {
    pub fn new() -> Self {
        Self {
            languages: HashMap::new(),
        }
    }

    pub fn add_language(mut self, name: &str, library_path: &str) -> Self {
        let config = LanguageConfig {
            library: library_path.to_string(),
            filetypes: vec![],
            highlight: vec![],
        };
        self.languages.insert(name.to_string(), config);
        self
    }

    pub fn with_highlights(mut self, name: &str, highlights: Vec<HighlightItem>) -> Self {
        if let Some(config) = self.languages.get_mut(name) {
            config.highlight = highlights;
        }
        self
    }

    pub fn with_filetypes(mut self, name: &str, extensions: Vec<&str>) -> Self {
        let extensions: Vec<String> = extensions.into_iter().map(|s| s.to_string()).collect();
        if let Some(config) = self.languages.get_mut(name) {
            config.filetypes = extensions;
        }
        self
    }

    pub fn build(self) -> TreeSitterSettings {
        TreeSitterSettings {
            languages: self.languages,
        }
    }
}

pub fn create_test_highlight_path(path: &str) -> HighlightItem {
    HighlightItem {
        source: HighlightSource::Path {
            path: path.to_string(),
        },
    }
}

pub fn create_test_highlight_query(query: &str) -> HighlightItem {
    HighlightItem {
        source: HighlightSource::Query {
            query: query.to_string(),
        },
    }
}

pub fn create_test_position(line: u32, character: u32) -> Position {
    Position { line, character }
}

pub fn create_test_range(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Range {
    Range {
        start: Position {
            line: start_line,
            character: start_char,
        },
        end: Position {
            line: end_line,
            character: end_char,
        },
    }
}

pub fn create_test_uri(path: &str) -> Url {
    Url::from_file_path(path).unwrap_or_else(|_| Url::parse(&format!("file://{}", path)).unwrap())
}

// Mock data generators for testing
pub mod mock_data {
    use super::*;

    pub fn rust_function_symbols() -> Vec<SymbolDefinition> {
        vec![
            TestSymbolBuilder::new("main")
                .at_position(1, 3)
                .with_kind(SymbolKind::FUNCTION)
                .in_file("/test/main.rs")
                .build(),
            TestSymbolBuilder::new("helper")
                .at_position(5, 3)
                .with_kind(SymbolKind::FUNCTION)
                .in_file("/test/main.rs")
                .build(),
            TestSymbolBuilder::new("calculate")
                .at_position(10, 3)
                .with_kind(SymbolKind::FUNCTION)
                .in_file("/test/main.rs")
                .build(),
        ]
    }

    pub fn rust_struct_symbols() -> Vec<SymbolDefinition> {
        vec![
            TestSymbolBuilder::new("Config")
                .at_position(2, 7)
                .with_kind(SymbolKind::STRUCT)
                .in_file("/test/types.rs")
                .build(),
            TestSymbolBuilder::new("Person")
                .at_position(8, 7)
                .with_kind(SymbolKind::STRUCT)
                .in_file("/test/types.rs")
                .build(),
        ]
    }

    pub fn multi_language_config() -> TreeSitterSettings {
        TestConfigBuilder::new()
            .add_language("rust", "/usr/lib/libtree-sitter-rust.so")
            .with_highlights(
                "rust",
                vec![
                    create_test_highlight_path("/etc/treesitter/rust/highlights.scm"),
                    create_test_highlight_query("(function_item) @function"),
                ],
            )
            .with_filetypes("rust", vec!["rs"])
            .add_language("python", "/usr/lib/libtree-sitter-python.so")
            .with_highlights(
                "python",
                vec![create_test_highlight_query(
                    "(function_definition) @function",
                )],
            )
            .with_filetypes("python", vec!["py", "pyi"])
            .build()
    }

    pub fn invalid_config_variations() -> Vec<&'static str> {
        vec![
            r#"{"invalid": "structure"}"#,
            r#"{"languages": {}}"#,           // Missing language configs
            r#"{"languages": {"rust": {}}}"#, // Missing library and filetypes
            r#"{"languages": {"rust": {"library": ""}}}"#, // Empty library
            r#"{"languages": {"rust": {"library": "/lib/rust.so", "filetypes": []}}}"#, // Empty filetypes
        ]
    }
}

// Assertion helpers
pub mod assertions {
    use super::*;

    pub fn assert_symbol_at_position(
        symbol: &SymbolDefinition,
        expected_line: u32,
        expected_char: u32,
    ) {
        assert_eq!(
            symbol.range.start.line, expected_line,
            "Symbol {} not at expected line",
            symbol.name
        );
        assert_eq!(
            symbol.range.start.character, expected_char,
            "Symbol {} not at expected character",
            symbol.name
        );
    }

    pub fn assert_symbol_kind(symbol: &SymbolDefinition, expected_kind: SymbolKind) {
        assert_eq!(
            symbol.kind, expected_kind,
            "Symbol {} has wrong kind",
            symbol.name
        );
    }

    pub fn assert_valid_range(range: &Range) {
        assert!(
            range.start.line <= range.end.line,
            "Range start line after end line"
        );

        if range.start.line == range.end.line {
            assert!(
                range.start.character <= range.end.character,
                "Range start character after end character on same line"
            );
        }
    }

    pub fn assert_config_has_language(config: &TreeSitterSettings, language: &str) {
        assert!(
            config.languages.contains_key(language),
            "Config missing language: {}",
            language
        );
        assert!(
            !config.languages[language].filetypes.is_empty(),
            "Config missing filetypes for: {}",
            language
        );
    }

    pub fn assert_library_path_valid(path: &str) {
        assert!(!path.is_empty(), "Library path cannot be empty");
        assert!(
            path.contains("tree-sitter") || path.contains("lib"),
            "Library path should contain 'tree-sitter' or 'lib': {}",
            path
        );
    }
}

// Performance testing utilities
pub mod performance {
    use super::*;
    use std::time::{Duration, Instant};

    pub fn measure_time<F, R>(operation: F) -> (R, Duration)
    where
        F: FnOnce() -> R,
    {
        let start = Instant::now();
        let result = operation();
        let duration = start.elapsed();
        (result, duration)
    }

    pub fn assert_performance<F, R>(operation: F, max_duration: Duration, description: &str)
    where
        F: FnOnce() -> R,
    {
        let (_, duration) = measure_time(operation);
        assert!(
            duration <= max_duration,
            "{} took too long: {:?} (max: {:?})",
            description,
            duration,
            max_duration
        );
    }

    pub fn create_large_symbol_set(count: usize) -> Vec<SymbolDefinition> {
        (0..count)
            .map(|i| {
                TestSymbolBuilder::new(&format!("symbol_{}", i))
                    .at_position(i as u32, 0)
                    .with_kind(if i % 3 == 0 {
                        SymbolKind::FUNCTION
                    } else if i % 3 == 1 {
                        SymbolKind::STRUCT
                    } else {
                        SymbolKind::VARIABLE
                    })
                    .build()
            })
            .collect()
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn test_symbol_builder() {
        let symbol = TestSymbolBuilder::new("test_function")
            .at_position(5, 10)
            .with_kind(SymbolKind::FUNCTION)
            .in_file("/test/example.rs")
            .ending_at(25)
            .build();

        assert_eq!(symbol.name, "test_function");
        assert_eq!(symbol.range.start.line, 5);
        assert_eq!(symbol.range.start.character, 10);
        assert_eq!(symbol.range.end.character, 25);
        assert_eq!(symbol.kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn test_config_builder() {
        let config = TestConfigBuilder::new()
            .add_language("rust", "/lib/rust.so")
            .with_filetypes("rust", vec!["rs"])
            .build();

        assertions::assert_config_has_language(&config, "rust");
        assertions::assert_library_path_valid(&config.languages["rust"].library);
    }

    #[test]
    fn test_performance_measurement() {
        let (result, duration) = performance::measure_time(|| {
            // Simulate some work
            std::thread::sleep(std::time::Duration::from_millis(10));
            42
        });

        assert_eq!(result, 42);
        assert!(duration >= std::time::Duration::from_millis(10));
    }
}
