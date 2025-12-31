//! Installation module for Tree-sitter parsers and queries.
//!
//! This module provides functionality to download and install Tree-sitter
//! query files and compile parser shared libraries.

pub mod cache;
pub mod metadata;
pub mod parser;
pub mod queries;

pub use parser::parser_file_exists;

use std::path::PathBuf;

/// Get the default data directory for treesitter-ls.
///
/// Uses XDG Base Directory specification for cross-platform consistency:
/// 1. If `XDG_DATA_HOME` environment variable is set, uses `$XDG_DATA_HOME/treesitter-ls/`
/// 2. Otherwise, falls back to `~/.local/share/treesitter-ls/` on all platforms
///
/// This provides consistent paths across Linux and macOS, matching tools like Neovim.
pub fn default_data_dir() -> Option<PathBuf> {
    // Check XDG_DATA_HOME first
    if let Ok(xdg_data_home) = std::env::var("XDG_DATA_HOME") {
        return Some(PathBuf::from(xdg_data_home).join("treesitter-ls"));
    }

    // Fall back to ~/.local/share/treesitter-ls/ on all platforms
    dirs::home_dir().map(|p| p.join(".local/share/treesitter-ls"))
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
    use std::sync::Mutex;

    // Serial test lock to prevent env var race conditions between tests
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_data_dir_returns_some() {
        let dir = default_data_dir();
        assert!(dir.is_some());
        let path = dir.unwrap();
        assert!(path.to_string_lossy().contains("treesitter-ls"));
    }

    #[test]
    fn test_default_data_dir_with_xdg_data_home() {
        let _lock = ENV_LOCK.lock().unwrap();

        // Save original value
        let original = std::env::var("XDG_DATA_HOME").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent access to env vars
        unsafe {
            // Set XDG_DATA_HOME to a test value
            std::env::set_var("XDG_DATA_HOME", "/tmp/test-xdg-data");
        }

        let dir = default_data_dir();
        assert!(dir.is_some());
        let path = dir.unwrap();
        assert_eq!(path, PathBuf::from("/tmp/test-xdg-data/treesitter-ls"));

        // SAFETY: Restoring original value, still holding ENV_LOCK
        unsafe {
            match original {
                Some(val) => std::env::set_var("XDG_DATA_HOME", val),
                None => std::env::remove_var("XDG_DATA_HOME"),
            }
        }
    }

    #[test]
    fn test_default_data_dir_without_xdg_data_home() {
        let _lock = ENV_LOCK.lock().unwrap();

        // Save original value
        let original = std::env::var("XDG_DATA_HOME").ok();

        // SAFETY: We hold ENV_LOCK to prevent concurrent access to env vars
        unsafe {
            // Ensure XDG_DATA_HOME is not set
            std::env::remove_var("XDG_DATA_HOME");
        }

        let dir = default_data_dir();
        assert!(dir.is_some());
        let path = dir.unwrap();

        // Should fall back to ~/.local/share/treesitter-ls on all platforms
        let expected = dirs::home_dir().unwrap().join(".local/share/treesitter-ls");
        assert_eq!(path, expected);
        // Also verify it does NOT use platform-specific paths like ~/Library/Application Support/
        assert!(
            !path.to_string_lossy().contains("Library"),
            "Path should not contain 'Library' (macOS-specific)"
        );
        assert!(
            path.to_string_lossy().contains(".local/share"),
            "Path should use .local/share"
        );

        // SAFETY: Restoring original value, still holding ENV_LOCK
        unsafe {
            if let Some(val) = original {
                std::env::set_var("XDG_DATA_HOME", val);
            }
        }
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
