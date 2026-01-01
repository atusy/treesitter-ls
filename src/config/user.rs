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

    // Fallback to ~/.config/treesitter-ls/treesitter-ls.toml
    // dirs::home_dir() returns the user's home directory
    dirs::home_dir().map(|home| home.join(".config").join("treesitter-ls").join("treesitter-ls.toml"))
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

    #[test]
    fn user_config_path_falls_back_to_home_config_when_xdg_unset() {
        // Save original value
        let original = env::var("XDG_CONFIG_HOME").ok();

        // Remove XDG_CONFIG_HOME to test fallback
        // SAFETY: Tests run single-threaded by default with --test-threads=1
        unsafe {
            env::remove_var("XDG_CONFIG_HOME");
        }

        let path = user_config_path();

        // Restore original value
        // SAFETY: Same as above - restoring original env state
        unsafe {
            if let Some(val) = original {
                env::set_var("XDG_CONFIG_HOME", val);
            }
        }

        // On a normal system, we should get a path even without XDG_CONFIG_HOME
        // It should fall back to $HOME/.config/treesitter-ls/treesitter-ls.toml
        assert!(path.is_some(), "user_config_path should return Some even when XDG_CONFIG_HOME is unset");
        let path = path.unwrap();

        // Verify the path structure - should end with treesitter-ls/treesitter-ls.toml
        // and contain ".config" in the path (the fallback behavior)
        let path_str = path.to_string_lossy();
        assert!(
            path_str.ends_with("treesitter-ls/treesitter-ls.toml"),
            "path should end with treesitter-ls/treesitter-ls.toml, got: {}",
            path_str
        );
        assert!(
            path_str.contains(".config"),
            "fallback path should contain .config, got: {}",
            path_str
        );
    }
}
