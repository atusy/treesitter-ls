//! Installation module for Tree-sitter parsers and queries.
//!
//! This module provides functionality to download and install Tree-sitter
//! query files from the nvim-treesitter repository.

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
}
