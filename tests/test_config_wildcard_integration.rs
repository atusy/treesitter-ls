//! Integration tests for ADR-0011 wildcard config inheritance and user config loading.
//!
//! These tests verify that:
//! - resolve_language_server_with_wildcard is properly exported and works correctly
//! - User config files are loaded and merged with project configs
//!
//! PBI-154: languageServers Wildcard Config Inheritance
//! PBI-155: Wire Config APIs into Application

use std::collections::HashMap;
use treesitter_ls::config::{
    load_user_config, merge_all, resolve_language_server_with_wildcard,
    settings::BridgeServerConfig, settings::WorkspaceType, TreeSitterSettings,
};

/// Verify that languageServers._ provides defaults that are inherited by specific servers.
///
/// This test simulates the configuration:
/// ```toml
/// [languageServers._]
/// workspaceType = "generic"
///
/// [languageServers.rust-analyzer]
/// cmd = ["rust-analyzer"]
/// languages = ["rust"]
/// # workspaceType should be inherited from _ as "generic"
/// ```
#[test]
fn test_language_server_inherits_workspace_type_from_wildcard() {
    let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

    // Wildcard defines default workspace_type
    servers.insert(
        "_".to_string(),
        BridgeServerConfig {
            cmd: vec![],
            languages: vec![],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Generic),
        },
    );

    // rust-analyzer doesn't specify workspace_type
    servers.insert(
        "rust-analyzer".to_string(),
        BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: None, // Should inherit from wildcard
        },
    );

    // Resolve rust-analyzer config with wildcard inheritance
    let resolved = resolve_language_server_with_wildcard(&servers, "rust-analyzer");

    assert!(resolved.is_some(), "Should resolve rust-analyzer config");
    let config = resolved.unwrap();

    // Verify inheritance from wildcard
    assert_eq!(
        config.workspace_type,
        Some(WorkspaceType::Generic),
        "Should inherit workspace_type from wildcard"
    );

    // Verify specific values are preserved
    assert_eq!(
        config.cmd,
        vec!["rust-analyzer".to_string()],
        "Should preserve rust-analyzer's cmd"
    );
    assert_eq!(
        config.languages,
        vec!["rust".to_string()],
        "Should preserve rust-analyzer's languages"
    );
}

/// Verify that a completely new server falls back to wildcard defaults.
///
/// This test simulates asking for a server that doesn't exist in the config,
/// but should still get settings from the wildcard.
#[test]
fn test_unconfigured_server_uses_wildcard_defaults() {
    let mut servers: HashMap<String, BridgeServerConfig> = HashMap::new();

    // Wildcard provides complete default configuration
    servers.insert(
        "_".to_string(),
        BridgeServerConfig {
            cmd: vec!["default-lsp".to_string()],
            languages: vec!["any".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Generic),
        },
    );

    // Ask for a server that doesn't exist
    let resolved = resolve_language_server_with_wildcard(&servers, "my-custom-lsp");

    assert!(resolved.is_some(), "Should resolve to wildcard config");
    let config = resolved.unwrap();

    // Should get all values from wildcard
    assert_eq!(
        config.cmd,
        vec!["default-lsp".to_string()],
        "Should use wildcard cmd"
    );
    assert_eq!(
        config.languages,
        vec!["any".to_string()],
        "Should use wildcard languages"
    );
    assert_eq!(
        config.workspace_type,
        Some(WorkspaceType::Generic),
        "Should use wildcard workspace_type"
    );
}

// ============================================================================
// PBI-155: User Config Loading Integration Tests
// ============================================================================

/// PBI-155 Subtask 4: Verify user config is loaded from XDG_CONFIG_HOME
///
/// This test:
/// 1. Creates a temp directory for user config
/// 2. Sets XDG_CONFIG_HOME to the temp directory
/// 3. Creates a user config file with unique settings
/// 4. Calls load_user_config() and verifies settings are loaded
#[test]
fn test_user_config_loaded_from_xdg_config_home() {
    use std::env;
    use std::fs;
    use tempfile::TempDir;

    // Save original XDG_CONFIG_HOME
    let original_xdg = env::var("XDG_CONFIG_HOME").ok();

    // Create temp directory for user config
    let user_config_dir = TempDir::new().expect("failed to create user config temp dir");

    // Set up user config with unique searchPath
    let treesitter_config_dir = user_config_dir.path().join("treesitter-ls");
    fs::create_dir_all(&treesitter_config_dir).expect("failed to create config dir");
    let user_config_content = r#"
        searchPaths = ["/unique/user/search/path/for/test"]
        autoInstall = false
    "#;
    fs::write(
        treesitter_config_dir.join("treesitter-ls.toml"),
        user_config_content,
    )
    .expect("failed to write user config");

    // Point XDG_CONFIG_HOME to our temp directory
    // SAFETY: Tests run single-threaded with --test-threads=1
    unsafe {
        env::set_var("XDG_CONFIG_HOME", user_config_dir.path());
    }

    // Load user config
    let result = load_user_config();

    // Restore original XDG_CONFIG_HOME
    unsafe {
        match original_xdg {
            Some(val) => env::set_var("XDG_CONFIG_HOME", val),
            None => env::remove_var("XDG_CONFIG_HOME"),
        }
    }

    // Verify config was loaded
    assert!(
        result.is_ok(),
        "load_user_config should return Ok, got: {:?}",
        result.err()
    );
    let config = result.unwrap();
    assert!(
        config.is_some(),
        "load_user_config should return Some when config exists"
    );

    let settings = config.unwrap();
    assert_eq!(
        settings.search_paths,
        Some(vec!["/unique/user/search/path/for/test".to_string()]),
        "User config searchPaths should be loaded"
    );
    assert_eq!(
        settings.auto_install,
        Some(false),
        "User config autoInstall should be loaded"
    );
}

/// PBI-155: Verify merge_all correctly merges user config with project config
///
/// This test verifies that when we have:
/// - User config: searchPaths = ["/user/path"], autoInstall = false
/// - Project config: autoInstall = true
///
/// The merged result should have:
/// - searchPaths from user (inherited)
/// - autoInstall from project (override)
#[test]
fn test_merge_all_user_config_with_project_config() {
    // Create user config
    let user_config = TreeSitterSettings {
        search_paths: Some(vec!["/user/path".to_string()]),
        languages: HashMap::new(),
        capture_mappings: HashMap::new(),
        auto_install: Some(false),
        language_servers: None,
    };

    // Create project config (overrides autoInstall, doesn't specify searchPaths)
    let project_config = TreeSitterSettings {
        search_paths: None, // Should inherit from user
        languages: HashMap::new(),
        capture_mappings: HashMap::new(),
        auto_install: Some(true), // Overrides user's false
        language_servers: None,
    };

    // Merge: user < project
    let merged = merge_all(&[Some(user_config), Some(project_config)]);

    assert!(merged.is_some(), "merge_all should return Some");
    let result = merged.unwrap();

    // searchPaths: inherited from user (project was None)
    assert_eq!(
        result.search_paths,
        Some(vec!["/user/path".to_string()]),
        "searchPaths should be inherited from user config"
    );

    // autoInstall: overridden by project
    assert_eq!(
        result.auto_install,
        Some(true),
        "autoInstall should be overridden by project config"
    );
}

/// PBI-155: Verify 3-layer merge (user < project < init_options)
///
/// This ensures the full config stack works correctly.
#[test]
fn test_merge_all_three_layers() {
    // Layer 1: User config (lowest priority)
    let user_config = TreeSitterSettings {
        search_paths: Some(vec!["/user/path".to_string()]),
        languages: HashMap::new(),
        capture_mappings: HashMap::new(),
        auto_install: Some(false),
        language_servers: None,
    };

    // Layer 2: Project config
    let project_config = TreeSitterSettings {
        search_paths: Some(vec!["/project/path".to_string()]),
        languages: HashMap::new(),
        capture_mappings: HashMap::new(),
        auto_install: None, // Not specified, should inherit from user
        language_servers: None,
    };

    // Layer 3: Init options (highest priority)
    let init_options = TreeSitterSettings {
        search_paths: None, // Not specified, should use project's
        languages: HashMap::new(),
        capture_mappings: HashMap::new(),
        auto_install: Some(true), // Overrides user's false
        language_servers: None,
    };

    // Merge all three layers
    let merged = merge_all(&[Some(user_config), Some(project_config), Some(init_options)]);

    assert!(merged.is_some(), "merge_all should return Some");
    let result = merged.unwrap();

    // searchPaths: from project (overrides user, init_options was None)
    assert_eq!(
        result.search_paths,
        Some(vec!["/project/path".to_string()]),
        "searchPaths should come from project config"
    );

    // autoInstall: from init_options (highest priority)
    assert_eq!(
        result.auto_install,
        Some(true),
        "autoInstall should come from init_options (highest priority)"
    );
}
