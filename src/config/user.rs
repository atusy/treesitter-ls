//! User configuration loading for treesitter-ls.
//!
//! This module handles loading user-wide configuration from the XDG config directory.
//! User config location: $XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml
//! Fallback: ~/.config/treesitter-ls/treesitter-ls.toml

use std::path::PathBuf;

/// Returns the path to the user configuration file.
///
/// The path is determined by:
/// 1. If $XDG_CONFIG_HOME is set: $XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml
/// 2. Otherwise: ~/.config/treesitter-ls/treesitter-ls.toml
///
/// Returns None if the home directory cannot be determined.
pub fn user_config_path() -> Option<PathBuf> {
    // Check XDG_CONFIG_HOME first
    if let Ok(xdg_config) = std::env::var("XDG_CONFIG_HOME") {
        return Some(PathBuf::from(xdg_config).join("treesitter-ls").join("treesitter-ls.toml"));
    }

    // TODO: Fallback to ~/.config
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;

    #[test]
    fn user_config_path_uses_xdg_config_home_when_set() {
        // Save original value
        let original = env::var("XDG_CONFIG_HOME").ok();

        // Set XDG_CONFIG_HOME to a custom path
        // SAFETY: Tests run single-threaded by default with --test-threads=1
        // This is safe in test context where env manipulation is expected
        unsafe {
            env::set_var("XDG_CONFIG_HOME", "/custom/config");
        }

        let path = user_config_path();

        // Restore original value
        // SAFETY: Same as above - restoring original env state
        unsafe {
            match original {
                Some(val) => env::set_var("XDG_CONFIG_HOME", val),
                None => env::remove_var("XDG_CONFIG_HOME"),
            }
        }

        assert!(path.is_some(), "user_config_path should return Some when XDG_CONFIG_HOME is set");
        let path = path.unwrap();
        assert_eq!(
            path,
            PathBuf::from("/custom/config/treesitter-ls/treesitter-ls.toml"),
            "should use XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml"
        );
    }
}
