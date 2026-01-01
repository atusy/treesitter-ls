//! User configuration loading for treesitter-ls.
//!
//! This module handles loading user-wide configuration from the XDG config directory.
//! User config location: $XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml
//! Fallback: ~/.config/treesitter-ls/treesitter-ls.toml

use crate::config::TreeSitterSettings;
use std::path::PathBuf;

/// Result type for user config loading operations.
pub type UserConfigResult<T> = Result<T, UserConfigError>;

/// Error type for user config loading.
#[derive(Debug)]
pub enum UserConfigError {
    /// TOML parse error with context
    ParseError {
        path: PathBuf,
        source: toml::de::Error,
    },
}

impl std::fmt::Display for UserConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserConfigError::ParseError { path, source } => {
                write!(
                    f,
                    "Failed to parse user config at {}: {}",
                    path.display(),
                    source
                )
            }
        }
    }
}

impl std::error::Error for UserConfigError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            UserConfigError::ParseError { source, .. } => Some(source),
        }
    }
}

/// Loads user configuration from the XDG config directory.
///
/// Returns:
/// - `Ok(Some(settings))` if the file exists and is valid TOML
/// - `Ok(None)` if the file does not exist (zero-config experience preserved)
/// - `Err(UserConfigError)` if the file exists but contains invalid TOML
pub fn load_user_config() -> UserConfigResult<Option<TreeSitterSettings>> {
    let path = match user_config_path() {
        Some(p) => p,
        None => return Ok(None), // No home directory, silently ignore
    };

    // Check if file exists
    if !path.exists() {
        return Ok(None); // Missing file is silently ignored (zero-config)
    }

    // Read and parse the file
    let contents = std::fs::read_to_string(&path).map_err(|_| UserConfigError::ParseError {
        path: path.clone(),
        source: toml::from_str::<TreeSitterSettings>("").unwrap_err(), // Placeholder error for IO issues
    })?;

    let settings = toml::from_str::<TreeSitterSettings>(&contents)
        .map_err(|e| UserConfigError::ParseError { path, source: e })?;

    Ok(Some(settings))
}

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
        return Some(
            PathBuf::from(xdg_config)
                .join("treesitter-ls")
                .join("treesitter-ls.toml"),
        );
    }

    // Fallback to ~/.config/treesitter-ls/treesitter-ls.toml
    // dirs::home_dir() returns the user's home directory
    dirs::home_dir().map(|home| {
        home.join(".config")
            .join("treesitter-ls")
            .join("treesitter-ls.toml")
    })
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

        assert!(
            path.is_some(),
            "user_config_path should return Some when XDG_CONFIG_HOME is set"
        );
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
        assert!(
            path.is_some(),
            "user_config_path should return Some even when XDG_CONFIG_HOME is unset"
        );
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

    #[test]
    fn load_user_config_returns_none_for_missing_file() {
        use tempfile::TempDir;

        // Save original XDG_CONFIG_HOME
        let original = env::var("XDG_CONFIG_HOME").ok();

        // Point to a temp directory where no config file exists
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        // SAFETY: Tests run single-threaded
        unsafe {
            env::set_var("XDG_CONFIG_HOME", temp_dir.path());
        }

        let result = load_user_config();

        // Restore original
        unsafe {
            match original {
                Some(val) => env::set_var("XDG_CONFIG_HOME", val),
                None => env::remove_var("XDG_CONFIG_HOME"),
            }
        }

        // Should succeed with None (no error, just missing file)
        assert!(
            result.is_ok(),
            "load_user_config should return Ok for missing file"
        );
        assert!(
            result.unwrap().is_none(),
            "load_user_config should return None when config file is missing"
        );
    }

    #[test]
    fn load_user_config_loads_valid_toml_file() {
        use std::fs;
        use tempfile::TempDir;

        // Save original XDG_CONFIG_HOME
        let original = env::var("XDG_CONFIG_HOME").ok();

        // Create temp directory with valid config file
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_dir = temp_dir.path().join("treesitter-ls");
        fs::create_dir_all(&config_dir).expect("failed to create config dir");

        let config_path = config_dir.join("treesitter-ls.toml");
        let config_content = r#"
            autoInstall = false
            searchPaths = ["/user/custom/path"]
        "#;
        fs::write(&config_path, config_content).expect("failed to write config file");

        // SAFETY: Tests run single-threaded
        unsafe {
            env::set_var("XDG_CONFIG_HOME", temp_dir.path());
        }

        let result = load_user_config();

        // Restore original
        unsafe {
            match original {
                Some(val) => env::set_var("XDG_CONFIG_HOME", val),
                None => env::remove_var("XDG_CONFIG_HOME"),
            }
        }

        // Should succeed with Some(settings)
        assert!(
            result.is_ok(),
            "load_user_config should return Ok for valid file"
        );
        let settings = result.unwrap();
        assert!(
            settings.is_some(),
            "load_user_config should return Some for existing file"
        );

        let settings = settings.unwrap();
        assert_eq!(
            settings.auto_install,
            Some(false),
            "should parse autoInstall"
        );
        assert_eq!(
            settings.search_paths,
            Some(vec!["/user/custom/path".to_string()]),
            "should parse searchPaths"
        );
    }

    #[test]
    fn load_user_config_returns_descriptive_error_for_invalid_toml() {
        use std::fs;
        use tempfile::TempDir;

        // Save original XDG_CONFIG_HOME
        let original = env::var("XDG_CONFIG_HOME").ok();

        // Create temp directory with invalid TOML config file
        let temp_dir = TempDir::new().expect("failed to create temp dir");
        let config_dir = temp_dir.path().join("treesitter-ls");
        fs::create_dir_all(&config_dir).expect("failed to create config dir");

        let config_path = config_dir.join("treesitter-ls.toml");
        // Invalid TOML: missing closing bracket
        let invalid_content = r#"
            autoInstall = true
            searchPaths = ["/path/one"
        "#;
        fs::write(&config_path, invalid_content).expect("failed to write config file");

        // SAFETY: Tests run single-threaded
        unsafe {
            env::set_var("XDG_CONFIG_HOME", temp_dir.path());
        }

        let result = load_user_config();

        // Restore original
        unsafe {
            match original {
                Some(val) => env::set_var("XDG_CONFIG_HOME", val),
                None => env::remove_var("XDG_CONFIG_HOME"),
            }
        }

        // Should return an error with descriptive message
        assert!(
            result.is_err(),
            "load_user_config should return Err for invalid TOML"
        );
        let error = result.unwrap_err();

        // Error message should include the file path
        let error_message = error.to_string();
        assert!(
            error_message.contains("treesitter-ls.toml"),
            "error should include file path, got: {}",
            error_message
        );

        // Error message should describe the parse issue
        assert!(
            error_message.contains("Failed to parse") || error_message.contains("parse"),
            "error should describe the parse failure, got: {}",
            error_message
        );
    }
}
