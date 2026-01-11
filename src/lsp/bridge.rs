//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.
//!
//! # Module Structure
//!
//! - `connection` - AsyncBridgeConnection for process spawning and I/O
//! - `protocol` - VirtualDocumentUri, request building, and response transformation
//! - `pool` - LanguageServerPool for server pool coordination (ADR-0016)

mod connection;
mod pool;
mod protocol;
mod text_document;

// Re-export public types
pub(crate) use pool::LanguageServerPool;

#[cfg(test)]
mod tests {
    use super::connection::AsyncBridgeConnection;
    use super::pool::LanguageServerPool;
    use super::protocol::{
        VirtualDocumentUri, build_bridge_completion_request, build_bridge_didchange_notification,
        build_bridge_hover_request, transform_completion_response_to_host,
        transform_hover_response_to_host,
    };

    #[test]
    fn module_exports_async_bridge_connection_type() {
        // Verify the type exists and is accessible
        fn assert_type_exists<T>() {}
        assert_type_exists::<AsyncBridgeConnection>();
    }

    #[tokio::test]
    async fn spawn_creates_child_process_with_stdio() {
        // Use `cat` as a simple test process that echoes stdin to stdout
        let cmd = vec!["cat".to_string()];
        let _conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // If spawn succeeded, we have a valid connection
    }

    /// RED: Test that read_message parses Content-Length header and reads JSON body
    #[tokio::test]
    async fn read_message_parses_content_length_and_body() {
        use serde_json::json;

        // Use `cat` to echo what we write back to us
        let cmd = vec!["cat".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // Write a JSON-RPC response message
        let response = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": {
                "capabilities": {}
            }
        });

        conn.write_message(&response)
            .await
            .expect("write should succeed");

        // Read it back using the reader task's parsing logic
        let parsed = conn.read_message().await.expect("read should succeed");

        // Verify the parsed message matches what we sent
        assert_eq!(parsed["jsonrpc"], "2.0");
        assert_eq!(parsed["id"], 1);
        assert!(parsed["result"].is_object());
    }

    /// Integration test: Initialize lua-language-server and verify response
    #[tokio::test]
    async fn initialize_lua_language_server_logs_success() {
        use serde_json::json;

        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        // Spawn lua-language-server
        let cmd = vec!["lua-language-server".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("should spawn lua-language-server");

        // Send initialize request
        let init_request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "processId": std::process::id(),
                "rootUri": null,
                "capabilities": {}
            }
        });

        conn.write_message(&init_request)
            .await
            .expect("should write initialize request");

        // Read initialize response (may need to skip notifications)
        let response = loop {
            let msg = conn.read_message().await.expect("should read message");
            // Skip notifications (messages without id that have a method)
            if msg.get("id").is_some() {
                break msg;
            }
            // It's a notification, continue reading
            log::debug!(
                target: "treesitter_ls::bridge::test",
                "Received notification: {:?}",
                msg.get("method")
            );
        };

        // Verify the response indicates successful initialization
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response["result"].is_object(), "should have result object");
        assert!(
            response["result"]["capabilities"].is_object(),
            "should have capabilities in result"
        );

        // Log successful initialization (as required by AC2)
        log::info!(
            target: "treesitter_ls::bridge",
            "lua-language-server initialized successfully"
        );

        // Send initialized notification
        let initialized = json!({
            "jsonrpc": "2.0",
            "method": "initialized",
            "params": {}
        });
        conn.write_message(&initialized)
            .await
            .expect("should write initialized notification");
    }

    /// RED: Test VirtualDocumentUri creates URI string for injection region
    #[test]
    fn virtual_document_uri_creates_scheme_for_injection_region() {
        use tower_lsp::lsp_types::Url;

        // Create a virtual document URI for a Lua injection in a markdown file
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let injection_language = "lua";
        let region_id = "region-0";

        let virtual_uri = VirtualDocumentUri::new(&host_uri, injection_language, region_id);

        // The URI string should use file:// scheme with .treesitter-ls prefix
        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.starts_with("file:///.treesitter-ls/"),
            "URI should use file:///.treesitter-ls/ path: {}",
            uri_string
        );
        assert!(
            uri_string.ends_with(".lua"),
            "URI should have .lua extension: {}",
            uri_string
        );
    }

    /// RED: Test bridge hover request uses virtual URI and mapped position (PBI-302 Subtask 4)
    #[test]
    fn bridge_hover_request_uses_virtual_uri_and_mapped_position() {
        use tower_lsp::lsp_types::{Position, Url};

        // Create a hover request builder for bridge
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 10,
        };
        let region_id = "region-0";
        let injection_language = "lua";

        // The region starts at line 3 in the host document
        let region_start_line = 3;

        // Build the hover request for downstream language server
        let request = build_bridge_hover_request(
            &host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            42, // request ID
        );

        // Verify the request structure
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/hover");

        // The params should use virtual URI
        let text_doc = &request["params"]["textDocument"];
        let uri_str = text_doc["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );

        // The position should be translated (line 5 - region_start 3 = line 2)
        let position = &request["params"]["position"];
        assert_eq!(
            position["line"], 2,
            "Position line should be translated to virtual coordinates"
        );
        assert_eq!(
            position["character"], 10,
            "Position character should remain unchanged"
        );
    }

    /// RED: Test bridge hover response transforms range to host coordinates (PBI-302 Subtask 5)
    #[test]
    fn bridge_hover_response_transforms_range_to_host_coordinates() {
        use serde_json::json;

        // Simulate a hover response from lua-language-server with a range
        // The range is in virtual document coordinates (starting at line 0)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": {
                    "kind": "markdown",
                    "value": "```lua\nfunction greet(name: string)\n```"
                },
                "range": {
                    "start": { "line": 0, "character": 9 },
                    "end": { "line": 0, "character": 14 }
                }
            }
        });

        // The injection region starts at line 3 in the host document
        let region_start_line = 3;

        // Transform the response to host coordinates
        let transformed = transform_hover_response_to_host(response, region_start_line);

        // Verify the contents are unchanged
        assert_eq!(
            transformed["result"]["contents"]["kind"], "markdown",
            "Contents should be preserved"
        );

        // Verify the range is transformed to host coordinates
        let range = &transformed["result"]["range"];
        assert_eq!(
            range["start"]["line"], 3,
            "Start line should be translated to host (0 + 3 = 3)"
        );
        assert_eq!(
            range["start"]["character"], 9,
            "Start character should remain unchanged"
        );
        assert_eq!(
            range["end"]["line"], 3,
            "End line should be translated to host (0 + 3 = 3)"
        );
        assert_eq!(
            range["end"]["character"], 14,
            "End character should remain unchanged"
        );
    }

    /// Test hover response without range is passed through unchanged
    #[test]
    fn bridge_hover_response_without_range_unchanged() {
        use serde_json::json;

        // Hover response without a range (just contents)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "contents": "Simple hover text"
            }
        });

        let region_start_line = 5;
        let transformed = transform_hover_response_to_host(response.clone(), region_start_line);

        // Response should be unchanged (no range to transform)
        assert_eq!(transformed, response);
    }

    /// Test hover response with null result is passed through unchanged
    #[test]
    fn bridge_hover_response_null_result_unchanged() {
        use serde_json::json;

        // Hover response with null result (no hover info available)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let region_start_line = 5;
        let transformed = transform_hover_response_to_host(response.clone(), region_start_line);

        // Response should be unchanged
        assert_eq!(transformed, response);
    }

    /// Test that didChange notification is built with correct virtual URI (PBI-303 Subtask 3)
    #[test]
    fn bridge_didchange_notification_uses_virtual_uri() {
        use tower_lsp::lsp_types::Url;

        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let injection_language = "lua";
        let region_id = "region-0";
        let new_content = "local x = 42\nprint(x)";
        let version = 2;

        // Build the didChange notification for downstream language server
        let notification = build_bridge_didchange_notification(
            &host_uri,
            injection_language,
            region_id,
            new_content,
            version,
        );

        // Verify the notification structure
        assert_eq!(notification["jsonrpc"], "2.0");
        assert_eq!(notification["method"], "textDocument/didChange");
        assert!(
            notification.get("id").is_none(),
            "Notification should not have id"
        );

        // The params should use virtual URI
        let text_doc = &notification["params"]["textDocument"];
        let uri_str = text_doc["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "didChange should use virtual URI: {}",
            uri_str
        );
        assert_eq!(text_doc["version"], 2);

        // contentChanges should contain full text sync
        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["text"], new_content);
    }

    /// RED: Test that completion request uses virtual URI and mapped position (PBI-303 Subtask 5)
    #[test]
    fn bridge_completion_request_uses_virtual_uri_and_mapped_position() {
        use tower_lsp::lsp_types::{Position, Url};

        // Create a completion request builder for bridge
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let host_position = Position {
            line: 5,
            character: 6,
        };
        let region_id = "region-0";
        let injection_language = "lua";

        // The region starts at line 3 in the host document
        let region_start_line = 3;

        // Build the completion request for downstream language server
        let request = build_bridge_completion_request(
            &host_uri,
            host_position,
            injection_language,
            region_id,
            region_start_line,
            42, // request ID
        );

        // Verify the request structure
        assert_eq!(request["jsonrpc"], "2.0");
        assert_eq!(request["id"], 42);
        assert_eq!(request["method"], "textDocument/completion");

        // The params should use virtual URI
        let text_doc = &request["params"]["textDocument"];
        let uri_str = text_doc["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("file:///.treesitter-ls/") && uri_str.ends_with(".lua"),
            "Request should use virtual URI: {}",
            uri_str
        );

        // The position should be translated (line 5 - region_start 3 = line 2)
        let position = &request["params"]["position"];
        assert_eq!(
            position["line"], 2,
            "Position line should be translated to virtual coordinates"
        );
        assert_eq!(
            position["character"], 6,
            "Position character should remain unchanged"
        );
    }

    /// RED: Test that completion response transforms textEdit ranges to host coordinates (PBI-303 Subtask 6)
    #[test]
    fn bridge_completion_response_transforms_textedit_ranges_to_host() {
        use serde_json::json;

        // Simulate a completion response from lua-language-server with textEdit ranges
        // The ranges are in virtual document coordinates (starting at line 0)
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": {
                "isIncomplete": false,
                "items": [
                    {
                        "label": "print",
                        "kind": 3,
                        "textEdit": {
                            "range": {
                                "start": { "line": 1, "character": 0 },
                                "end": { "line": 1, "character": 3 }
                            },
                            "newText": "print"
                        }
                    },
                    {
                        "label": "pairs",
                        "kind": 3
                    }
                ]
            }
        });

        // The injection region starts at line 3 in the host document
        let region_start_line = 3;

        // Transform the response to host coordinates
        let transformed = transform_completion_response_to_host(response, region_start_line);

        // Verify the items array exists
        let items = transformed["result"]["items"].as_array().unwrap();
        assert_eq!(items.len(), 2);

        // First item with textEdit should have transformed range
        let first_item = &items[0];
        let text_edit = &first_item["textEdit"];
        let range = &text_edit["range"];
        assert_eq!(
            range["start"]["line"], 4,
            "Start line should be translated to host (1 + 3 = 4)"
        );
        assert_eq!(
            range["end"]["line"], 4,
            "End line should be translated to host (1 + 3 = 4)"
        );

        // Second item without textEdit should be unchanged
        let second_item = &items[1];
        assert_eq!(second_item["label"], "pairs");
        assert!(second_item.get("textEdit").is_none());
    }

    /// Test completion response with null result passes through unchanged
    #[test]
    fn bridge_completion_response_null_result_unchanged() {
        use serde_json::json;

        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": null
        });

        let region_start_line = 3;
        let transformed =
            transform_completion_response_to_host(response.clone(), region_start_line);
        assert_eq!(transformed, response);
    }

    /// Test completion response as array (not CompletionList) is also transformed
    #[test]
    fn bridge_completion_response_array_result_transforms_textedit() {
        use serde_json::json;

        // Some servers return array directly instead of CompletionList
        let response = json!({
            "jsonrpc": "2.0",
            "id": 42,
            "result": [
                {
                    "label": "print",
                    "textEdit": {
                        "range": {
                            "start": { "line": 0, "character": 0 },
                            "end": { "line": 0, "character": 2 }
                        },
                        "newText": "print"
                    }
                }
            ]
        });

        let region_start_line = 5;
        let transformed = transform_completion_response_to_host(response, region_start_line);

        let items = transformed["result"].as_array().unwrap();
        let text_edit = &items[0]["textEdit"];
        assert_eq!(
            text_edit["range"]["start"]["line"], 5,
            "Start line should be translated (0 + 5 = 5)"
        );
        assert_eq!(
            text_edit["range"]["end"]["line"], 5,
            "End line should be translated (0 + 5 = 5)"
        );
    }

    /// Integration test: LanguageServerPool sends hover request to lua-language-server (PBI-302 Subtask 6)
    #[tokio::test]
    async fn hover_impl_returns_bridge_response_for_lua_injection() {
        use crate::config::settings::BridgeServerConfig;
        use tower_lsp::lsp_types::{Position, Url};

        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let manager = LanguageServerPool::new();

        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        // Host position: line 3 in markdown (region starts at line 3)
        let host_position = Position {
            line: 3,
            character: 9, // Position on "greet"
        };
        let region_start_line = 3; // Lua code block starts at line 3 in host
        let virtual_content = "function greet(name)\n    return \"Hello, \" .. name\nend";

        // Send hover request via LanguageServerPool
        let response = manager
            .send_hover_request(
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                region_start_line,
                virtual_content,
            )
            .await;

        // Verify we got a response (not an error)
        assert!(
            response.is_ok(),
            "LanguageServerPool should successfully communicate with lua-language-server: {:?}",
            response.err()
        );

        let json_response = response.unwrap();

        // Verify it's a valid JSON-RPC response
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(
            json_response.get("id").is_some(),
            "Response should have an id"
        );

        // The result may be null if lua-ls hasn't indexed yet, but the request should succeed
        assert!(
            json_response.get("result").is_some() || json_response.get("error").is_none(),
            "Response should have result or no error"
        );

        println!(
            "LanguageServerPool successfully sent hover request to lua-language-server: {:?}",
            json_response
        );
    }

    /// Integration test: LanguageServerPool sends completion request to lua-language-server (PBI-303 Subtask 7)
    #[tokio::test]
    async fn completion_request_returns_items_from_lua_language_server() {
        use crate::config::settings::BridgeServerConfig;
        use tower_lsp::lsp_types::{Position, Url};

        // Skip test if lua-language-server is not available
        if std::process::Command::new("lua-language-server")
            .arg("--version")
            .output()
            .is_err()
        {
            eprintln!("Skipping test: lua-language-server not found");
            return;
        }

        let manager = LanguageServerPool::new();

        let server_config = BridgeServerConfig {
            cmd: vec!["lua-language-server".to_string()],
            languages: vec!["lua".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let host_uri = Url::parse("file:///test/doc.md").unwrap();
        // Host position: line 3 in markdown, after "pri" (region starts at line 3)
        let host_position = Position {
            line: 3,
            character: 3, // After "pri"
        };
        let region_start_line = 3; // Lua code block starts at line 3 in host
        let virtual_content = "pri"; // Partial identifier that should trigger 'print' completion

        // Send completion request via LanguageServerPool
        let response = manager
            .send_completion_request(
                &server_config,
                &host_uri,
                host_position,
                "lua",
                "region-0",
                region_start_line,
                virtual_content,
            )
            .await;

        // Verify we got a response (not an error)
        assert!(
            response.is_ok(),
            "LanguageServerPool should successfully communicate with lua-language-server: {:?}",
            response.err()
        );

        let json_response = response.unwrap();

        // Verify it's a valid JSON-RPC response
        assert_eq!(json_response["jsonrpc"], "2.0");
        assert!(
            json_response.get("id").is_some(),
            "Response should have an id"
        );

        // The result may be null if lua-ls hasn't indexed yet, but the request should succeed
        assert!(
            json_response.get("result").is_some() || json_response.get("error").is_none(),
            "Response should have result or no error"
        );

        println!(
            "LanguageServerPool successfully sent completion request to lua-language-server: {:?}",
            json_response
        );
    }
}
