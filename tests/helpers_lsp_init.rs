//! LSP initialization helpers for E2E tests.
//!
//! Provides reusable initialization patterns to reduce duplication across E2E tests.

use crate::helpers::lsp_client::LspClient;
use serde_json::json;

/// Initialize LSP client with Rust bridge configuration.
///
/// This helper encapsulates the common initialization pattern used across E2E tests:
/// - Send initialize request with bridge configuration for Rust in Markdown
/// - Send initialized notification to complete handshake
///
/// # Arguments
/// * `client` - The LspClient to initialize
///
/// # Example
/// ```
/// let mut client = LspClient::new();
/// initialize_with_rust_bridge(&mut client);
/// ```
pub(crate) fn initialize_with_rust_bridge(client: &mut LspClient) {
    // Initialize with bridge configuration
    // This matches the minimal_init.lua setup used by Neovim E2E tests
    let _init_response = client.send_request(
        "initialize",
        json!({
            "processId": std::process::id(),
            "rootUri": null,
            "capabilities": {},
            "initializationOptions": {
                "languages": {
                    "markdown": {
                        "bridge": {
                            "rust": { "enabled": true }
                        }
                    }
                },
                "languageServers": {
                    "rust-analyzer": {
                        "cmd": ["rust-analyzer"],
                        "languages": ["rust"],
                        "workspaceType": "cargo"
                    }
                }
            }
        }),
    );

    // Send initialized notification (required by LSP protocol)
    client.send_notification("initialized", json!({}));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_with_rust_bridge() {
        let mut client = LspClient::new();
        initialize_with_rust_bridge(&mut client);

        // Verify client is still functional by sending a shutdown request
        let response = client.send_request("shutdown", json!(null));
        assert!(response.get("result").is_some());
    }
}
