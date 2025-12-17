//! Installation module for Tree-sitter parsers and queries.
//!
//! This module provides functionality to download and install Tree-sitter
//! query files and compile parser shared libraries.

pub mod cache;
pub mod metadata;
pub mod parser;
pub mod queries;

use std::path::PathBuf;

/// Get the default data directory for treesitter-ls.
///
/// Platform-specific paths:
/// - Linux: ~/.local/share/treesitter-ls/
/// - macOS: ~/Library/Application Support/treesitter-ls/
/// - Windows: %APPDATA%/treesitter-ls/
pub fn default_data_dir() -> Option<PathBuf> {
    dirs::data_dir().map(|p| p.join("treesitter-ls"))
}

/// Result of installing a language (both parser and queries).
#[derive(Debug)]
pub struct InstallResult {
    /// The language that was installed.
    pub language: String,
    /// Path where the parser was installed, if successful.
    pub parser_path: Option<PathBuf>,
    /// Path where queries were installed, if successful.
    pub queries_path: Option<PathBuf>,
    /// Error message if parser install failed.
    pub parser_error: Option<String>,
    /// Error message if queries install failed.
    pub queries_error: Option<String>,
}

impl InstallResult {
    /// Check if the installation was fully successful.
    pub fn is_success(&self) -> bool {
        self.parser_error.is_none() && self.queries_error.is_none()
    }

    /// Check if at least one component was installed.
    pub fn is_partial_success(&self) -> bool {
        self.parser_path.is_some() || self.queries_path.is_some()
    }
}

/// Install a language asynchronously (both parser and queries).
///
/// This wraps the blocking install functions in `spawn_blocking` for use
/// in async contexts like the LSP server.
///
/// # Arguments
/// * `language` - The language to install (e.g., "lua", "rust")
/// * `data_dir` - The base data directory for treesitter-ls
/// * `force` - Whether to overwrite existing files
pub async fn install_language_async(
    language: String,
    data_dir: PathBuf,
    force: bool,
) -> InstallResult {
    let lang = language.clone();
    let dir = data_dir.clone();

    // Run blocking install operations in a separate thread pool
    tokio::task::spawn_blocking(move || {
        let mut result = InstallResult {
            language: lang.clone(),
            parser_path: None,
            queries_path: None,
            parser_error: None,
            queries_error: None,
        };

        // Install parser
        // For async/auto-install, always use cache (background operation)
        let parser_options = parser::InstallOptions {
            data_dir: dir.clone(),
            force,
            verbose: false,
            no_cache: false,
        };

        match parser::install_parser(&lang, &parser_options) {
            Ok(parser_result) => {
                result.parser_path = Some(parser_result.install_path);
            }
            Err(e) => {
                result.parser_error = Some(e.to_string());
            }
        }

        // Install queries
        match queries::install_queries(&lang, &dir, force) {
            Ok(query_result) => {
                result.queries_path = Some(query_result.install_path);
            }
            Err(e) => {
                result.queries_error = Some(e.to_string());
            }
        }

        result
    })
    .await
    .unwrap_or_else(|e| InstallResult {
        language,
        parser_path: None,
        queries_path: None,
        parser_error: Some(format!("Task panicked: {}", e)),
        queries_error: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_data_dir_returns_some() {
        let dir = default_data_dir();
        assert!(dir.is_some());
        let path = dir.unwrap();
        assert!(path.to_string_lossy().contains("treesitter-ls"));
    }

    #[test]
    fn test_install_result_success_check() {
        let success = InstallResult {
            language: "lua".to_string(),
            parser_path: Some(PathBuf::from("/tmp/parser")),
            queries_path: Some(PathBuf::from("/tmp/queries")),
            parser_error: None,
            queries_error: None,
        };
        assert!(success.is_success());
        assert!(success.is_partial_success());

        let partial = InstallResult {
            language: "lua".to_string(),
            parser_path: None,
            queries_path: Some(PathBuf::from("/tmp/queries")),
            parser_error: Some("Parser failed".to_string()),
            queries_error: None,
        };
        assert!(!partial.is_success());
        assert!(partial.is_partial_success());

        let failure = InstallResult {
            language: "lua".to_string(),
            parser_path: None,
            queries_path: None,
            parser_error: Some("Parser failed".to_string()),
            queries_error: Some("Queries failed".to_string()),
        };
        assert!(!failure.is_success());
        assert!(!failure.is_partial_success());
    }
}
