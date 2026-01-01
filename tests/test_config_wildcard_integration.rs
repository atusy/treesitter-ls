//! Integration tests for ADR-0011 wildcard config inheritance.
//!
//! These tests verify that the resolve_language_server_with_wildcard function
//! is properly exported and works correctly when used from outside the config module.
//!
//! PBI-154: languageServers Wildcard Config Inheritance

use std::collections::HashMap;
use treesitter_ls::config::{
    resolve_language_server_with_wildcard, settings::BridgeServerConfig, settings::WorkspaceType,
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
