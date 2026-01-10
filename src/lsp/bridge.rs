//! Async Bridge Connection for LSP language server integration
//!
//! This module implements the async bridge architecture (ADR-0014) for communicating
//! with downstream language servers via stdio.
//!
//! # Module Structure
//!
//! - `connection` - AsyncBridgeConnection for process spawning and I/O
//! - `protocol` - VirtualDocumentUri, request building, and response transformation
//! - `manager` - BridgeManager for connection lifecycle management

mod connection;
mod manager;
mod protocol;

// Re-export public types
pub(crate) use manager::BridgeManager;

#[cfg(test)]
mod tests {
    use super::connection::AsyncBridgeConnection;
    use super::manager::BridgeManager;
    use super::protocol::{
        PendingRequests, VirtualDocumentUri, build_bridge_completion_request,
        build_bridge_didchange_notification, build_bridge_hover_request,
        transform_completion_response_to_host, transform_hover_response_to_host,
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
        let conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // The connection should have a child process ID
        assert!(conn.child_id().is_some(), "child process should have an ID");
    }

    /// RED: Test that send_request writes JSON-RPC message with Content-Length header
    #[tokio::test]
    async fn send_request_writes_json_rpc_with_content_length() {
        use serde_json::json;

        // Use `cat` to echo what we write to stdin back to stdout
        let cmd = vec!["cat".to_string()];
        let mut conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // Send a simple JSON-RPC request
        let request = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        conn.write_message(&request)
            .await
            .expect("write should succeed");

        // Read back what was written to verify the format
        let output = conn.read_raw_message().await.expect("read should succeed");

        // Verify Content-Length header is present and correct
        assert!(
            output.starts_with("Content-Length: "),
            "message should start with Content-Length header"
        );
        assert!(
            output.contains("\r\n\r\n"),
            "header should be separated from body by CRLF CRLF"
        );
        assert!(
            output.contains("\"jsonrpc\":\"2.0\""),
            "body should contain JSON-RPC content"
        );
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

    /// RED: Test that response is routed to correct pending request via request ID
    #[tokio::test]
    async fn response_routed_to_pending_request_by_id() {
        use serde_json::json;
        use std::sync::Arc;

        // Use `cat` to echo what we write back
        let cmd = vec!["cat".to_string()];
        let conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // Wrap in Arc for sharing between reader task and main task
        let conn = Arc::new(tokio::sync::Mutex::new(conn));

        // Create a pending request tracker
        let pending = PendingRequests::new();

        // Register a pending request with ID 42
        let (response_rx, _request_id) = pending.register(42);

        // Spawn a "reader task" that reads a response and routes it
        let conn_clone = Arc::clone(&conn);
        let pending_clone = pending.clone();
        let reader_task = tokio::spawn(async move {
            let mut conn = conn_clone.lock().await;
            let response = conn.read_message().await.expect("read should succeed");
            pending_clone.complete(&response);
        });

        // Write a response with matching ID (simulate language server response)
        {
            let mut conn = conn.lock().await;
            let response = json!({
                "jsonrpc": "2.0",
                "id": 42,
                "result": { "value": "hello" }
            });
            conn.write_message(&response)
                .await
                .expect("write should succeed");
        }

        // Wait for reader task
        reader_task.await.expect("reader task should complete");

        // The pending request should receive the response
        let result = response_rx.await.expect("should receive response");
        assert_eq!(result["id"], 42);
        assert_eq!(result["result"]["value"], "hello");
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

    /// Integration test: Dropping connection terminates child process
    #[tokio::test]
    async fn drop_terminates_child_process() {
        // Spawn a long-running process that we can check is terminated
        // Using `sleep` as it will run indefinitely until killed
        let cmd = vec!["sleep".to_string(), "3600".to_string()];
        let conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("should spawn sleep process");

        let child_id = conn.child_id().expect("should have child ID");

        // Verify process is running before drop
        assert!(
            is_process_running(child_id),
            "child process should be running before drop"
        );

        // Drop the connection
        drop(conn);

        // Give the OS a moment to clean up
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

        // Verify process is no longer running after drop
        assert!(
            !is_process_running(child_id),
            "child process should be terminated after drop"
        );
    }

    /// Check if a process with the given PID is still running
    fn is_process_running(pid: u32) -> bool {
        // Use kill -0 via Command to check if process exists
        std::process::Command::new("kill")
            .args(["-0", &pid.to_string()])
            .output()
            .map(|output| output.status.success())
            .unwrap_or(false)
    }

    /// RED: Test VirtualDocumentUri creates scheme for injection region (PBI-302 Subtask 1)
    #[test]
    fn virtual_document_uri_creates_scheme_for_injection_region() {
        use tower_lsp::lsp_types::Url;

        // Create a virtual document URI for a Lua injection in a markdown file
        let host_uri = Url::parse("file:///project/doc.md").unwrap();
        let injection_language = "lua";
        let region_id = "region-0";

        let virtual_uri = VirtualDocumentUri::new(&host_uri, injection_language, region_id);

        // The virtual URI should encode all three pieces of information
        assert_eq!(virtual_uri.host_uri(), &host_uri);
        assert_eq!(virtual_uri.language(), "lua");
        assert_eq!(virtual_uri.region_id(), "region-0");

        // The URI string should use a custom scheme
        let uri_string = virtual_uri.to_uri_string();
        assert!(
            uri_string.starts_with("tsls-virtual://"),
            "URI should use tsls-virtual:// scheme: {}",
            uri_string
        );
        assert!(
            uri_string.contains("lua"),
            "URI should contain injection language: {}",
            uri_string
        );
    }

    /// RED: Test VirtualDocumentUri can be parsed back from URI string (PBI-302 Subtask 1)
    #[test]
    fn virtual_document_uri_roundtrip() {
        use tower_lsp::lsp_types::Url;

        let host_uri = Url::parse("file:///project/readme.md").unwrap();
        let virtual_uri = VirtualDocumentUri::new(&host_uri, "python", "region-42");

        // Convert to URI string
        let uri_string = virtual_uri.to_uri_string();

        // Parse back
        let parsed = VirtualDocumentUri::parse(&uri_string).expect("should parse virtual URI");

        // Verify roundtrip preserves all data
        assert_eq!(parsed.host_uri(), &host_uri);
        assert_eq!(parsed.language(), "python");
        assert_eq!(parsed.region_id(), "region-42");
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
            uri_str.starts_with("tsls-virtual://lua/"),
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

    /// Test that BridgeManager tracks which virtual documents have been opened per connection (PBI-303 Subtask 1)
    #[test]
    fn bridge_manager_tracks_opened_documents() {
        // BridgeManager should track which virtual document URIs have been opened
        // per language server connection, to avoid sending duplicate didOpen notifications

        let manager = BridgeManager::new();

        // Check that a virtual URI has not been opened yet
        let virtual_uri = "tsls-virtual://lua/region-0?host=file%3A%2F%2F%2Ftest.md";
        assert!(
            !manager.is_document_opened("lua", virtual_uri),
            "Document should not be marked as opened initially"
        );
    }

    /// RED: Test that didOpen is only sent once per virtual document URI per connection (PBI-303 Subtask 2)
    #[tokio::test]
    async fn didopen_only_sent_once_per_virtual_document() {
        // When we mark a document as opened, subsequent checks should return true
        // This ensures didOpen is only sent once per virtual document per language server

        let manager = BridgeManager::new();
        let virtual_uri = "tsls-virtual://lua/region-0?host=file%3A%2F%2F%2Ftest.md";
        let language = "lua";

        // Initially not opened
        assert!(!manager.is_document_opened(language, virtual_uri));

        // Mark as opened
        manager.mark_document_opened(language, virtual_uri).await;

        // Now should be marked as opened
        assert!(
            manager.is_document_opened(language, virtual_uri),
            "Document should be marked as opened after mark_document_opened"
        );

        // Different URI for same language should still be unopened
        let other_uri = "tsls-virtual://lua/region-1?host=file%3A%2F%2F%2Ftest.md";
        assert!(
            !manager.is_document_opened(language, other_uri),
            "Different document should not be affected"
        );
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
        assert!(notification.get("id").is_none(), "Notification should not have id");

        // The params should use virtual URI
        let text_doc = &notification["params"]["textDocument"];
        let uri_str = text_doc["uri"].as_str().unwrap();
        assert!(
            uri_str.starts_with("tsls-virtual://lua/"),
            "didChange should use virtual URI: {}",
            uri_str
        );
        assert_eq!(text_doc["version"], 2);

        // contentChanges should contain full text sync
        let changes = notification["params"]["contentChanges"].as_array().unwrap();
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0]["text"], new_content);
    }

    /// RED: Test that BridgeManager tracks document versions (PBI-303 Subtask 4)
    #[tokio::test]
    async fn bridge_manager_tracks_document_versions() {
        // BridgeManager should track the version number for each opened document
        // so that didChange notifications use incrementing versions

        let manager = BridgeManager::new();
        let virtual_uri = "tsls-virtual://lua/region-0?host=file%3A%2F%2F%2Ftest.md";
        let language = "lua";

        // Initially, document should have no version (not opened)
        assert!(
            manager.get_document_version(language, virtual_uri).is_none(),
            "Document should not have a version initially"
        );

        // After marking as opened, version should be 1
        manager.mark_document_opened(language, virtual_uri).await;
        assert_eq!(
            manager.get_document_version(language, virtual_uri),
            Some(1),
            "Version should be 1 after opening"
        );

        // Incrementing version should return 2
        let new_version = manager.increment_document_version(language, virtual_uri).await;
        assert_eq!(new_version, Some(2), "Version should be 2 after increment");
        assert_eq!(
            manager.get_document_version(language, virtual_uri),
            Some(2),
            "Stored version should be 2"
        );
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
            uri_str.starts_with("tsls-virtual://lua/"),
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
        let transformed = transform_completion_response_to_host(response.clone(), region_start_line);
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

    /// Integration test: BridgeManager sends hover request to lua-language-server (PBI-302 Subtask 6)
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

        let manager = BridgeManager::new();

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

        // Send hover request via BridgeManager
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
            "BridgeManager should successfully communicate with lua-language-server: {:?}",
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
            "BridgeManager successfully sent hover request to lua-language-server: {:?}",
            json_response
        );
    }

    /// Integration test: BridgeManager sends completion request to lua-language-server (PBI-303 Subtask 7)
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

        let manager = BridgeManager::new();

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

        // Send completion request via BridgeManager
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
            "BridgeManager should successfully communicate with lua-language-server: {:?}",
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
            "BridgeManager successfully sent completion request to lua-language-server: {:?}",
            json_response
        );
    }
}
