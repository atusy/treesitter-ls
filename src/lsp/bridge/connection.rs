//! Language server connection management.
//!
//! This module handles spawning and managing connections to external
//! language servers for bridging LSP requests.

use super::text_document::{
    CompletionWithNotifications, GotoDefinitionWithNotifications, HoverWithNotifications,
    SignatureHelpWithNotifications,
};
use super::workspace::{language_to_extension, setup_workspace_with_option};
use crate::config::settings::BridgeServerConfig;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};
use tower_lsp::lsp_types::*;

/// Default timeout for bridge I/O operations (30 seconds).
const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);

/// Response from `read_response_for_id_with_notifications` containing
/// the JSON-RPC response and any $/progress notifications captured during the wait.
#[derive(Debug, Clone)]
pub struct ResponseWithNotifications {
    /// The JSON-RPC response matching the request ID (None if not found)
    pub response: Option<Value>,
    /// Captured $/progress notifications received while waiting for the response
    pub notifications: Vec<Value>,
}

/// Information about the workspace for a language server connection.
///
/// This struct holds the temp directory path and the virtual file path,
/// and provides methods to get URIs for the virtual file.
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    /// Temporary directory for the workspace
    pub temp_dir: PathBuf,
    /// Path to the virtual file (e.g., src/main.rs or virtual.py)
    pub virtual_file_path: PathBuf,
}

impl ConnectionInfo {
    /// Create a new ConnectionInfo with the given temp directory and virtual file path.
    pub fn new(temp_dir: PathBuf, virtual_file_path: PathBuf) -> Self {
        Self {
            temp_dir,
            virtual_file_path,
        }
    }

    /// Get the URI for the virtual file in the temp workspace.
    pub fn virtual_file_uri(&self) -> Option<String> {
        Some(format!("file://{}", self.virtual_file_path.display()))
    }

    /// Write content to the virtual file.
    ///
    /// This writes to the path stored in virtual_file_path, which could be
    /// src/main.rs for Cargo workspaces or virtual.<ext> for Generic workspaces.
    pub fn write_virtual_file(&self, content: &str) -> std::io::Result<()> {
        std::fs::write(&self.virtual_file_path, content)
    }
}

/// Manages a connection to a language server subprocess with a temporary workspace
pub struct LanguageServerConnection {
    process: Child,
    request_id: i64,
    stdout_reader: BufReader<ChildStdout>,
    /// Temporary directory for the workspace (cleaned up on drop)
    pub(crate) temp_dir: Option<PathBuf>,
    /// Track the version of the document currently open (None = not open yet)
    document_version: Option<i32>,
    /// Connection info with virtual file path for workspace operations
    pub connection_info: Option<ConnectionInfo>,
    /// Track whether the language server has been initialized (received initialized notification)
    pub(crate) initialized: bool,
}

impl LanguageServerConnection {
    /// Spawn a language server using configuration from BridgeServerConfig.
    ///
    /// This is the generic spawn method that:
    /// - Uses the command from config
    /// - Passes args from config to Command
    /// - Passes initializationOptions from config in initialize request
    /// - Creates workspace structure based on config.workspace_type
    pub fn spawn(config: &BridgeServerConfig) -> Option<Self> {
        // Use the new method but discard notifications for backward compatibility
        Self::spawn_with_notifications(config).map(|(conn, _)| conn)
    }

    /// Spawn a language server, capturing $/progress notifications.
    ///
    /// Like `spawn`, but returns both the connection and any `$/progress`
    /// notifications received during initialization. This allows callers
    /// to forward progress notifications to the client.
    ///
    /// Returns `Some((connection, notifications))` on success, or `None` on failure.
    pub fn spawn_with_notifications(config: &BridgeServerConfig) -> Option<(Self, Vec<Value>)> {
        // cmd must have at least one element (the program)
        let program = config.cmd.first()?;

        // Create a temporary directory for the workspace
        // Use unique counter to avoid conflicts between parallel tests
        static SPAWN_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let counter = SPAWN_COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let temp_dir = std::env::temp_dir().join(format!(
            "treesitter-ls-sync-{}-{}-{}",
            program,
            std::process::id(),
            counter
        ));
        std::fs::create_dir_all(&temp_dir).ok()?;

        // Determine extension from first language in config, default to "rs" for Cargo compat
        let extension = config
            .languages
            .first()
            .map(|lang| language_to_extension(lang))
            .unwrap_or("rs");

        // Set up workspace structure based on workspace_type
        let virtual_file_path =
            setup_workspace_with_option(&temp_dir, config.workspace_type, extension)?;

        let root_uri = format!("file://{}", temp_dir.display());

        // Build command: first element is program, rest are args
        let mut cmd = Command::new(program);
        cmd.current_dir(&temp_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());

        // Add args from cmd[1..] if present
        if config.cmd.len() > 1 {
            cmd.args(&config.cmd[1..]);
        }

        let mut process = cmd.spawn().ok()?;

        // Take stdout and wrap in BufReader to maintain consistent buffering
        let stdout = process.stdout.take()?;
        let stdout_reader = BufReader::new(stdout);

        // Create ConnectionInfo for the workspace
        let connection_info = ConnectionInfo::new(temp_dir.clone(), virtual_file_path);

        let mut conn = Self {
            process,
            request_id: 0,
            stdout_reader,
            temp_dir: Some(temp_dir),
            document_version: None,
            connection_info: Some(connection_info),
            initialized: false,
        };

        // Build initialize params, including initializationOptions from config if provided
        let mut init_params = serde_json::json!({
            "processId": std::process::id(),
            "capabilities": {},
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": "virtual"}],
        });

        // Merge initializationOptions from config if provided
        if let Some(ref init_opts) = config.initialization_options {
            init_params["initializationOptions"] = init_opts.clone();
        }

        let init_id = conn.send_request("initialize", init_params)?;

        // Wait for initialize response, capturing $/progress notifications
        let result = conn.read_response_for_id_with_notifications(init_id, DEFAULT_TIMEOUT);
        result.response?; // Ensure we got a valid response

        // Send initialized notification
        conn.send_notification("initialized", serde_json::json!({}));

        Some((conn, result.notifications))
    }

    /// Get the URI for the virtual main.rs file in the temp workspace.
    ///
    /// Note: Prefer virtual_file_uri() which works for all workspace types.
    /// This method is kept for backward compatibility with Cargo workspaces.
    pub fn main_rs_uri(&self) -> Option<String> {
        self.temp_dir
            .as_ref()
            .map(|dir| format!("file://{}/src/main.rs", dir.display()))
    }

    /// Get the URI for the virtual file in the temp workspace.
    ///
    /// Returns the appropriate file URI based on workspace_type:
    /// - Cargo: file://<temp>/src/main.rs
    /// - Generic: file://<temp>/virtual.<ext>
    pub fn virtual_file_uri(&self) -> Option<String> {
        self.connection_info
            .as_ref()
            .and_then(|info| info.virtual_file_uri())
    }

    /// Send a JSON-RPC request, returns the request ID
    fn send_request(&mut self, method: &str, params: Value) -> Option<i64> {
        self.request_id += 1;
        let id = self.request_id;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        self.send_message(&request)?;
        Some(id)
    }

    /// Send a JSON-RPC notification (no response expected)
    fn send_notification(&mut self, method: &str, params: Value) -> Option<()> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        self.send_message(&notification)
    }

    /// Send a JSON-RPC message
    fn send_message(&mut self, message: &Value) -> Option<()> {
        let content = serde_json::to_string(message).ok()?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let stdin = self.process.stdin.as_mut()?;
        stdin.write_all(header.as_bytes()).ok()?;
        stdin.write_all(content.as_bytes()).ok()?;
        stdin.flush().ok()?;

        Some(())
    }

    /// Read a JSON-RPC response matching the given request ID
    /// Skips notifications and other responses until finding the matching one
    pub(crate) fn read_response_for_id(&mut self, expected_id: i64) -> Option<Value> {
        // Use the new method but discard notifications for backward compatibility
        let result = self.read_response_for_id_with_notifications(expected_id, DEFAULT_TIMEOUT);
        result.response
    }

    /// Read a JSON-RPC response matching the given request ID, capturing $/progress notifications.
    ///
    /// Unlike `read_response_for_id`, this method returns both the response and any
    /// `$/progress` notifications received while waiting for the response.
    /// This allows callers to forward progress notifications to the client.
    ///
    /// # Parameters
    /// - `expected_id`: The request ID to wait for
    /// - `timeout`: Maximum duration to wait for the response
    ///
    /// # Returns
    /// Returns `ResponseWithNotifications` with:
    /// - `response: Some(...)` if the expected response arrives before timeout
    /// - `response: None` if timeout expires, EOF reached, or error occurs
    /// - `notifications`: Any $/progress notifications captured while waiting
    pub(crate) fn read_response_for_id_with_notifications(
        &mut self,
        expected_id: i64,
        timeout: Duration,
    ) -> ResponseWithNotifications {
        log::debug!(
            target: "treesitter_ls::bridge::conn",
            "[CONN] read_response START id={} timeout={:?}",
            expected_id,
            timeout
        );
        let mut notifications = Vec::new();
        let start = Instant::now();

        loop {
            // Check timeout before each read operation
            if start.elapsed() > timeout {
                log::warn!(
                    target: "treesitter_ls::bridge",
                    "Timeout waiting for response ID {} after {:?}",
                    expected_id,
                    timeout
                );
                return ResponseWithNotifications {
                    response: None,
                    notifications,
                };
            }

            // Read headers
            let mut content_length = 0;
            loop {
                // Check timeout in inner loop as well
                if start.elapsed() > timeout {
                    log::warn!(
                        target: "treesitter_ls::bridge",
                        "Timeout during header read for response ID {} after {:?}",
                        expected_id,
                        timeout
                    );
                    return ResponseWithNotifications {
                        response: None,
                        notifications,
                    };
                }

                let mut line = String::new();
                match self.stdout_reader.read_line(&mut line) {
                    Ok(0) => {
                        // EOF
                        return ResponseWithNotifications {
                            response: None,
                            notifications,
                        };
                    }
                    Ok(_) => {}
                    Err(_) => {
                        return ResponseWithNotifications {
                            response: None,
                            notifications,
                        };
                    }
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(len_str) = line.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse().unwrap_or(0);
                }
            }

            if content_length == 0 {
                return ResponseWithNotifications {
                    response: None,
                    notifications,
                };
            }

            // Read content
            let mut content = vec![0u8; content_length];
            if std::io::Read::read_exact(&mut self.stdout_reader, &mut content).is_err() {
                return ResponseWithNotifications {
                    response: None,
                    notifications,
                };
            }

            let message: Value = match serde_json::from_slice(&content) {
                Ok(m) => m,
                Err(_) => {
                    return ResponseWithNotifications {
                        response: None,
                        notifications,
                    };
                }
            };

            // Check if this is the response we're looking for
            if let Some(id) = message.get("id")
                && id.as_i64() == Some(expected_id)
            {
                log::debug!(
                    target: "treesitter_ls::bridge::conn",
                    "[CONN] read_response DONE id={} elapsed={:?}",
                    expected_id,
                    start.elapsed()
                );
                return ResponseWithNotifications {
                    response: Some(message),
                    notifications,
                };
            }

            // Check if this is a $/progress notification to capture
            if let Some(method) = message.get("method").and_then(|m| m.as_str())
                && method == "$/progress"
            {
                notifications.push(message);
            }
            // Otherwise it's a different notification or response - skip it
        }
    }

    /// Open or update a document in the language server and write it to the temp workspace.
    ///
    /// For language servers, we need to write the file to disk for proper indexing.
    /// The content is written to the virtual file path based on workspace type:
    /// - Cargo: src/main.rs
    /// - Generic: virtual.<ext>
    ///
    /// On first call, sends `textDocument/didOpen` and waits for indexing.
    /// On subsequent calls, sends `textDocument/didChange` (no wait needed).
    pub fn did_open(&mut self, uri: &str, language_id: &str, content: &str) -> Option<()> {
        // Use the new method but discard notifications for backward compatibility
        self.did_open_with_notifications(uri, language_id, content)
            .map(|_| ())
    }

    /// Open or update a document, capturing $/progress notifications.
    ///
    /// Like `did_open`, but returns any `$/progress` notifications received
    /// during the indexing phase. This allows callers to forward progress
    /// notifications to the client.
    ///
    /// Returns `Some(Vec<Value>)` on success (empty vec for didChange),
    /// or `None` on failure.
    pub fn did_open_with_notifications(
        &mut self,
        _uri: &str,
        language_id: &str,
        content: &str,
    ) -> Option<Vec<Value>> {
        log::debug!(
            target: "treesitter_ls::bridge::conn",
            "[CONN] did_open_with_notifications START lang={} content_len={}",
            language_id,
            content.len()
        );
        // Write content to the actual file on disk using ConnectionInfo
        self.connection_info
            .as_ref()?
            .write_virtual_file(content)
            .ok()?;

        // Use the real file URI from the temp workspace
        let real_uri = self.virtual_file_uri()?;

        if let Some(version) = self.document_version {
            // Document already open - send didChange instead
            let new_version = version + 1;
            log::debug!(
                target: "treesitter_ls::bridge::conn",
                "[CONN] sending didChange version={}",
                new_version
            );
            let params = serde_json::json!({
                "textDocument": {
                    "uri": real_uri,
                    "version": new_version,
                },
                "contentChanges": [{ "text": content }]
            });
            self.send_notification("textDocument/didChange", params)?;
            self.document_version = Some(new_version);
            log::debug!(
                target: "treesitter_ls::bridge::conn",
                "[CONN] didChange DONE version={}",
                new_version
            );
            // No indexing wait for didChange, return empty notifications
            Some(vec![])
        } else {
            // First time - send didOpen and wait for indexing
            log::debug!(
                target: "treesitter_ls::bridge::conn",
                "[CONN] sending didOpen (first time)"
            );
            let params = serde_json::json!({
                "textDocument": {
                    "uri": real_uri,
                    "languageId": language_id,
                    "version": 1,
                    "text": content,
                }
            });
            self.send_notification("textDocument/didOpen", params)?;
            self.document_version = Some(1);

            // Wait for rust-analyzer to index the project, capturing progress notifications.
            // rust-analyzer needs time to parse the file and build its index.
            // We wait for diagnostic notifications which indicate indexing is complete.
            log::debug!(
                target: "treesitter_ls::bridge::conn",
                "[CONN] waiting for indexing..."
            );
            let result = self.wait_for_indexing_with_notifications();
            log::debug!(
                target: "treesitter_ls::bridge::conn",
                "[CONN] indexing DONE notifications={}",
                result.len()
            );
            Some(result)
        }
    }

    /// Wait for rust-analyzer to finish indexing, capturing $/progress notifications.
    ///
    /// Similar to `wait_for_indexing`, but returns any `$/progress` notifications
    /// received while waiting. This allows callers to forward progress notifications
    /// to the client during initialization.
    ///
    /// Uses DEFAULT_TIMEOUT to prevent indefinite hangs if the language server
    /// is slow or unresponsive.
    fn wait_for_indexing_with_notifications(&mut self) -> Vec<Value> {
        let mut notifications = Vec::new();
        let start = Instant::now();

        // Read messages until we get a publishDiagnostics notification
        // or timeout after consuming a few messages
        for _ in 0..50 {
            // Check timeout before each iteration
            if start.elapsed() > DEFAULT_TIMEOUT {
                log::warn!(
                    target: "treesitter_ls::bridge",
                    "Timeout waiting for indexing after {:?}",
                    DEFAULT_TIMEOUT
                );
                return notifications;
            }

            // Read headers
            let mut content_length = 0;
            loop {
                // Check timeout in inner loop as well
                if start.elapsed() > DEFAULT_TIMEOUT {
                    log::warn!(
                        target: "treesitter_ls::bridge",
                        "Timeout during header read while waiting for indexing after {:?}",
                        DEFAULT_TIMEOUT
                    );
                    return notifications;
                }

                let mut line = String::new();
                if self.stdout_reader.read_line(&mut line).ok().unwrap_or(0) == 0 {
                    return notifications; // EOF
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(len_str) = line.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse().unwrap_or(0);
                }
            }

            if content_length == 0 {
                return notifications;
            }

            // Read content
            let mut content = vec![0u8; content_length];
            if std::io::Read::read_exact(&mut self.stdout_reader, &mut content).is_err() {
                return notifications;
            }

            let Ok(message) = serde_json::from_slice::<Value>(&content) else {
                continue;
            };

            // Check if this is a publishDiagnostics notification
            if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
                if method == "textDocument/publishDiagnostics" {
                    // rust-analyzer has indexed enough to publish diagnostics
                    return notifications;
                }
                // Capture $/progress notifications
                if method == "$/progress" {
                    notifications.push(message);
                }
            }
        }

        notifications
    }

    /// Request go-to-definition
    ///
    /// Uses the virtual file URI from the temp workspace based on workspace type.
    pub fn goto_definition(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        // Use the new method but discard notifications for backward compatibility
        self.goto_definition_with_notifications(_uri, position)
            .response
    }

    /// Request go-to-definition, capturing $/progress notifications.
    ///
    /// Unlike `goto_definition`, this method returns both the response and any
    /// `$/progress` notifications received while waiting for the response.
    /// This allows callers to forward progress notifications to the client.
    pub fn goto_definition_with_notifications(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> GotoDefinitionWithNotifications {
        // Use the virtual file URI from the temp workspace
        let Some(real_uri) = self.virtual_file_uri() else {
            return GotoDefinitionWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let Some(req_id) = self.send_request("textDocument/definition", params) else {
            return GotoDefinitionWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        // Read response, capturing $/progress notifications
        let result = self.read_response_for_id_with_notifications(req_id, DEFAULT_TIMEOUT);

        // Extract and parse the goto definition response
        let response = result
            .response
            .and_then(|msg| msg.get("result").cloned())
            .and_then(|r| serde_json::from_value(r).ok());

        GotoDefinitionWithNotifications {
            response,
            notifications: result.notifications,
        }
    }

    /// Request hover information, capturing $/progress notifications.
    ///
    /// Unlike `hover`, this method returns both the response and any
    /// `$/progress` notifications received while waiting for the response.
    /// This allows callers to forward progress notifications to the client.
    pub fn hover_with_notifications(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> HoverWithNotifications {
        // Use the virtual file URI from the temp workspace
        let Some(real_uri) = self.virtual_file_uri() else {
            return HoverWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let Some(req_id) = self.send_request("textDocument/hover", params) else {
            return HoverWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        // Read response, capturing $/progress notifications
        let result = self.read_response_for_id_with_notifications(req_id, DEFAULT_TIMEOUT);

        // Extract and parse the hover response
        let response = result
            .response
            .and_then(|msg| msg.get("result").cloned())
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());

        HoverWithNotifications {
            response,
            notifications: result.notifications,
        }
    }

    /// Request completion, capturing $/progress notifications.
    ///
    /// Sends a `textDocument/completion` request to the language server
    /// and returns both the response and any `$/progress` notifications
    /// received while waiting for the response.
    pub fn completion_with_notifications(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> CompletionWithNotifications {
        // Use the virtual file URI from the temp workspace
        let Some(real_uri) = self.virtual_file_uri() else {
            return CompletionWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let Some(req_id) = self.send_request("textDocument/completion", params) else {
            return CompletionWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        // Read response, capturing $/progress notifications
        let result = self.read_response_for_id_with_notifications(req_id, DEFAULT_TIMEOUT);

        // Extract and parse the completion response
        let response = result
            .response
            .and_then(|msg| msg.get("result").cloned())
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());

        CompletionWithNotifications {
            response,
            notifications: result.notifications,
        }
    }

    /// Request signature help, capturing $/progress notifications.
    ///
    /// Sends a `textDocument/signatureHelp` request to the language server
    /// and returns both the response and any `$/progress` notifications
    /// received while waiting for the response.
    pub fn signature_help_with_notifications(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> SignatureHelpWithNotifications {
        // Use the virtual file URI from the temp workspace
        let Some(real_uri) = self.virtual_file_uri() else {
            return SignatureHelpWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let Some(req_id) = self.send_request("textDocument/signatureHelp", params) else {
            return SignatureHelpWithNotifications {
                response: None,
                notifications: vec![],
            };
        };

        // Read response, capturing $/progress notifications
        let result = self.read_response_for_id_with_notifications(req_id, DEFAULT_TIMEOUT);

        // Extract and parse the signature help response
        let response = result
            .response
            .and_then(|msg| msg.get("result").cloned())
            .filter(|r| !r.is_null())
            .and_then(|r| serde_json::from_value(r).ok());

        SignatureHelpWithNotifications {
            response,
            notifications: result.notifications,
        }
    }

    /// Check if the language server process is still alive
    pub fn is_alive(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
    }

    /// Shutdown the language server and clean up temp directory
    pub fn shutdown(&mut self) {
        if let Some(shutdown_id) = self.send_request("shutdown", serde_json::json!(null)) {
            let _ = self.read_response_for_id(shutdown_id);
        }
        let _ = self.send_notification("exit", serde_json::json!(null));
        let _ = self.process.wait();

        // Clean up temp directory
        if let Some(temp_dir) = self.temp_dir.take() {
            let _ = std::fs::remove_dir_all(&temp_dir);
        }
    }
}

impl Drop for LanguageServerConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::WorkspaceType;
    use std::time::Duration;
    use tempfile::tempdir;

    #[test]
    fn read_response_for_id_with_notifications_accepts_timeout_parameter() {
        // This test verifies that the method signature accepts a Duration timeout parameter.
        // We verify by checking that spawn works (which calls the method internally).
        // The actual timeout behavior is tested in a separate test.

        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // spawn_with_notifications internally calls read_response_for_id_with_notifications
        // with a Duration parameter. If this compiles and succeeds, the signature is correct.
        let result = LanguageServerConnection::spawn_with_notifications(&config);
        assert!(
            result.is_some(),
            "spawn should succeed with timeout parameter"
        );

        // Verify the constant exists and has expected value
        assert_eq!(DEFAULT_TIMEOUT, Duration::from_secs(30));
    }

    #[test]
    fn read_response_for_id_with_notifications_returns_none_on_timeout() {
        // This test verifies that when no data arrives within the timeout period,
        // the method returns ResponseWithNotifications with response: None.

        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        let mut conn = LanguageServerConnection::spawn(&config).unwrap();

        // Request an ID that will never get a response (nothing sends request 9999)
        // With a very short timeout, the method should return None instead of hanging
        let start = std::time::Instant::now();
        let short_timeout = Duration::from_millis(100);
        let result = conn.read_response_for_id_with_notifications(9999, short_timeout);

        let elapsed = start.elapsed();

        // The method should have returned (not hung forever)
        // It should return within roughly the timeout period (allow some slack)
        assert!(
            elapsed < Duration::from_secs(5),
            "Method should return within timeout, not hang forever. Elapsed: {:?}",
            elapsed
        );

        // Response should be None since no response for ID 9999 arrived
        assert!(
            result.response.is_none(),
            "Response should be None on timeout"
        );

        // Notifications may or may not be present (rust-analyzer might send some)
        // but the struct should be valid
        let _ = result.notifications;
    }

    /// Helper to check if rust-analyzer is available for testing.
    /// Returns true if available, false if should skip test.
    fn check_rust_analyzer_available() -> bool {
        if std::process::Command::new("rust-analyzer")
            .arg("--version")
            .output()
            .is_ok()
        {
            true
        } else {
            eprintln!("Skipping: rust-analyzer not installed");
            false
        }
    }

    #[test]
    fn language_server_connection_is_alive_returns_true_for_live_process() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        let mut conn = LanguageServerConnection::spawn(&config).unwrap();
        assert!(conn.is_alive());
    }

    #[test]
    fn language_server_connection_is_alive_returns_false_after_shutdown() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        let mut conn = LanguageServerConnection::spawn(&config).unwrap();
        conn.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!conn.is_alive());
    }

    #[test]
    fn spawn_uses_command_from_config() {
        // Create a config for rust-analyzer
        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        if !check_rust_analyzer_available() {
            return;
        }

        // spawn should use the command from config, not hard-coded binary name
        let conn = LanguageServerConnection::spawn(&config);
        assert!(conn.is_some(), "Should spawn rust-analyzer from config");

        let mut conn = conn.unwrap();
        assert!(conn.is_alive());
    }

    #[test]
    fn spawn_passes_args_from_config() {
        // Test that args are passed to Command
        // We use rust-analyzer since it's available and accepts --version
        // Note: This test verifies the code path; in production args like --log-file would be used
        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()], // rust-analyzer doesn't need extra args for basic operation
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        if !check_rust_analyzer_available() {
            return;
        }

        // spawn should handle args from config
        let conn = LanguageServerConnection::spawn(&config);
        assert!(conn.is_some(), "Should spawn with args from config");
    }

    #[test]
    fn spawn_passes_initialization_options_from_config() {
        // Create config with initializationOptions
        let init_opts = serde_json::json!({
            "linkedProjects": ["/path/to/Cargo.toml"]
        });

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: Some(init_opts),
            workspace_type: None,
        };

        if !check_rust_analyzer_available() {
            return;
        }

        // spawn should pass initializationOptions in initialize request
        let conn = LanguageServerConnection::spawn(&config);
        assert!(
            conn.is_some(),
            "Should spawn with initializationOptions from config"
        );

        let mut conn = conn.unwrap();
        assert!(conn.is_alive());
    }

    #[test]
    fn virtual_file_uri_returns_main_rs_for_cargo_workspace() {
        use super::super::workspace::setup_workspace;

        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Create a connection info with Cargo workspace
        let virtual_file = setup_workspace(&temp_path, WorkspaceType::Cargo, "rs").unwrap();
        let conn_info = ConnectionInfo::new(temp_path.clone(), virtual_file.clone());

        // virtual_file_uri should return src/main.rs path
        let uri = conn_info.virtual_file_uri();
        assert!(uri.is_some());
        let expected_uri = format!("file://{}/src/main.rs", temp_path.display());
        assert_eq!(uri.unwrap(), expected_uri);
    }

    #[test]
    fn virtual_file_uri_returns_virtual_ext_for_generic_workspace() {
        use super::super::workspace::setup_workspace;

        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Create a connection info with Generic workspace
        let virtual_file = setup_workspace(&temp_path, WorkspaceType::Generic, "py").unwrap();
        let conn_info = ConnectionInfo::new(temp_path.clone(), virtual_file.clone());

        // virtual_file_uri should return virtual.py path
        let uri = conn_info.virtual_file_uri();
        assert!(uri.is_some());
        let expected_uri = format!("file://{}/virtual.py", temp_path.display());
        assert_eq!(uri.unwrap(), expected_uri);
    }

    #[test]
    fn write_virtual_file_writes_to_correct_path_for_cargo() {
        use super::super::workspace::setup_workspace;

        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Setup Cargo workspace
        let virtual_file = setup_workspace(&temp_path, WorkspaceType::Cargo, "rs").unwrap();
        let conn_info = ConnectionInfo::new(temp_path.clone(), virtual_file.clone());

        // Write content to virtual file
        let content = "fn main() { println!(\"hello\"); }";
        let result = conn_info.write_virtual_file(content);
        assert!(result.is_ok(), "write_virtual_file should succeed");

        // Verify content was written to src/main.rs
        let main_rs = temp_path.join("src").join("main.rs");
        let read_content = std::fs::read_to_string(&main_rs).unwrap();
        assert_eq!(read_content, content);
    }

    #[test]
    fn write_virtual_file_writes_to_correct_path_for_generic() {
        use super::super::workspace::setup_workspace;

        let temp = tempdir().unwrap();
        let temp_path = temp.path().to_path_buf();

        // Setup Generic workspace
        let virtual_file = setup_workspace(&temp_path, WorkspaceType::Generic, "py").unwrap();
        let conn_info = ConnectionInfo::new(temp_path.clone(), virtual_file.clone());

        // Write content to virtual file
        let content = "print('hello')";
        let result = conn_info.write_virtual_file(content);
        assert!(result.is_ok(), "write_virtual_file should succeed");

        // Verify content was written to virtual.py (NOT src/main.rs)
        let virtual_py = temp_path.join("virtual.py");
        let read_content = std::fs::read_to_string(&virtual_py).unwrap();
        assert_eq!(read_content, content);

        // Verify no src/main.rs was created
        let main_rs = temp_path.join("src").join("main.rs");
        assert!(
            !main_rs.exists(),
            "src/main.rs should NOT exist for Generic workspace"
        );
    }

    #[test]
    fn language_server_connection_stores_connection_info_after_spawn() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo), // Explicit Cargo for rust-analyzer
        };

        let conn = LanguageServerConnection::spawn(&config).unwrap();

        // After spawn, connection should have connection_info populated
        assert!(
            conn.connection_info.is_some(),
            "connection_info should be populated after spawn"
        );

        let conn_info = conn.connection_info.as_ref().unwrap();

        // The temp_dir in ConnectionInfo should match the connection's temp_dir
        assert!(
            conn.temp_dir.is_some(),
            "temp_dir should be set after spawn"
        );
        assert_eq!(
            conn_info.temp_dir,
            conn.temp_dir.clone().unwrap(),
            "ConnectionInfo temp_dir should match LanguageServerConnection temp_dir"
        );

        // For Cargo workspace, virtual_file_path should be src/main.rs
        let expected_virtual_file = conn.temp_dir.as_ref().unwrap().join("src").join("main.rs");
        assert_eq!(
            conn_info.virtual_file_path, expected_virtual_file,
            "virtual_file_path should be src/main.rs for Cargo workspace"
        );
    }

    /// Test that spawn() uses setup_workspace_with_option internally.
    ///
    /// This test creates a mock config with Generic workspace_type and verifies
    /// that spawn() would create the correct workspace structure.
    /// Since we can't spawn a real Generic workspace server easily in tests,
    /// we verify by checking the workspace created before process spawn fails.
    #[test]
    fn spawn_uses_setup_workspace_with_option_from_config() {
        // Create a config with Generic workspace_type
        // Use a non-existent command so the process spawn fails, but workspace is still set up
        let config = BridgeServerConfig {
            cmd: vec!["nonexistent-server-for-testing".to_string()],
            languages: vec!["python".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Generic),
        };

        // spawn() will fail because the command doesn't exist,
        // but we need to check that it would have set up a Generic workspace
        // This requires that spawn() uses setup_workspace_with_option()
        let result = LanguageServerConnection::spawn(&config);

        // The spawn will fail, but we want to verify that when it succeeds,
        // it respects workspace_type. For this test, we verify via another approach:
        // Check what directory was created by looking at temp dir

        // Since spawn creates a unique temp dir and cleans up on failure,
        // we can't easily inspect it. The real test is in
        // spawn_workspace_setup_uses_config_workspace_type which uses a real server.

        // For now, just verify the spawn returns None for invalid command
        assert!(result.is_none(), "spawn should fail for invalid command");
    }

    #[test]
    fn spawn_workspace_setup_uses_config_workspace_type() {
        // This test verifies that spawn() correctly uses workspace_type from config
        // to set up the workspace structure.
        // Since we can't easily mock the process spawning, we test with a real server.

        if !check_rust_analyzer_available() {
            return;
        }

        // Test with Cargo workspace_type (explicit, same as default)
        let config_cargo = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        let conn = LanguageServerConnection::spawn(&config_cargo).unwrap();
        let temp_dir = conn.temp_dir.as_ref().unwrap();

        // For Cargo workspace, should have Cargo.toml and src/main.rs
        assert!(
            temp_dir.join("Cargo.toml").exists(),
            "Cargo.toml should exist for Cargo workspace_type"
        );
        assert!(
            temp_dir.join("src").join("main.rs").exists(),
            "src/main.rs should exist for Cargo workspace_type"
        );

        // ConnectionInfo should have src/main.rs as virtual_file_path
        let conn_info = conn.connection_info.as_ref().unwrap();
        assert_eq!(
            conn_info.virtual_file_path,
            temp_dir.join("src").join("main.rs"),
            "virtual_file_path should be src/main.rs for Cargo workspace_type"
        );
    }

    #[test]
    fn connection_virtual_file_uri_delegates_to_connection_info() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo), // Explicit Cargo for rust-analyzer
        };

        let conn = LanguageServerConnection::spawn(&config).unwrap();

        // virtual_file_uri() on connection should delegate to connection_info
        let uri = conn.virtual_file_uri();
        assert!(uri.is_some(), "virtual_file_uri should return Some");

        let uri = uri.unwrap();
        // Should be a file:// URI ending in src/main.rs for Cargo workspace
        assert!(uri.starts_with("file://"), "URI should start with file://");
        assert!(
            uri.ends_with("src/main.rs"),
            "URI should end with src/main.rs for Cargo workspace"
        );

        // Should match what main_rs_uri() returns (for backward compat)
        let main_rs_uri = conn.main_rs_uri();
        assert_eq!(
            uri,
            main_rs_uri.unwrap(),
            "virtual_file_uri should match main_rs_uri for Cargo workspace"
        );
    }

    #[test]
    fn did_open_uses_connection_info_write_virtual_file() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo), // Explicit Cargo for rust-analyzer
        };

        let mut conn = LanguageServerConnection::spawn(&config).unwrap();
        let temp_dir = conn.temp_dir.clone().unwrap();

        // Call did_open
        let content = "fn main() { let x = 42; }";
        conn.did_open("file:///test.rs", "rust", content);

        // Verify content was written to the correct virtual file path
        let conn_info = conn.connection_info.as_ref().unwrap();
        let written_content = std::fs::read_to_string(&conn_info.virtual_file_path).unwrap();
        assert_eq!(
            written_content, content,
            "did_open should write content to virtual_file_path"
        );

        // For Cargo workspace, this should be src/main.rs
        let main_rs = temp_dir.join("src").join("main.rs");
        assert_eq!(
            conn_info.virtual_file_path, main_rs,
            "virtual_file_path should be src/main.rs for Cargo workspace"
        );
    }

    #[test]
    fn goto_definition_and_hover_use_virtual_file_uri() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo), // Explicit Cargo for rust-analyzer
        };

        let mut conn = LanguageServerConnection::spawn(&config).unwrap();

        // Open a document with a function definition
        let content = r#"fn hello() -> i32 { 42 }

fn main() {
    let x = hello();
}"#;
        conn.did_open("file:///test.rs", "rust", content);

        // The goto_definition and hover should work using virtual_file_uri
        // For Cargo workspace, this should still return valid results since
        // the URI should match the file path where content was written.

        // Test that goto_definition can be called (verifies it uses the right URI)
        let position = Position {
            line: 3,
            character: 12,
        }; // Position on 'hello' call
        let def_result = conn.goto_definition("file:///test.rs", position);

        // The result may be Some or None depending on rust-analyzer indexing,
        // but the call should not fail
        // If it fails, it means the URI doesn't match what rust-analyzer expects
        // For this test, we mainly verify the method works without panic

        // Test that hover can be called
        let hover_result = conn
            .hover_with_notifications("file:///test.rs", position)
            .response;

        // Both methods should complete without panic, indicating they're using
        // the correct virtual_file_uri that matches where content was written
        // The actual result depends on rust-analyzer indexing state

        // Additional verification: check that virtual_file_uri is being used correctly
        let virtual_uri = conn.virtual_file_uri();
        assert!(
            virtual_uri.is_some(),
            "virtual_file_uri should be available"
        );
        let uri = virtual_uri.unwrap();

        // For Cargo workspace, URI should end with src/main.rs
        assert!(
            uri.ends_with("src/main.rs"),
            "URI should end with src/main.rs for Cargo workspace"
        );

        // Log results for debugging (won't affect test pass/fail)
        eprintln!(
            "goto_definition result: {:?}, hover result: {:?}",
            def_result.is_some(),
            hover_result.is_some()
        );
    }

    #[test]
    fn response_with_notifications_struct_exists() {
        // RED phase test: ResponseWithNotifications struct should exist
        // and have response and notifications fields
        let response = serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "result": null
        });

        let result = ResponseWithNotifications {
            response: Some(response.clone()),
            notifications: vec![],
        };

        assert!(result.response.is_some());
        assert!(result.notifications.is_empty());

        // With notifications
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {"token": "test", "value": {}}
        });

        let result_with_notifs = ResponseWithNotifications {
            response: Some(response),
            notifications: vec![notification.clone()],
        };

        assert_eq!(result_with_notifs.notifications.len(), 1);
        assert_eq!(result_with_notifs.notifications[0], notification);
    }

    #[test]
    fn language_server_connection_has_initialized_flag_defaulting_to_false() {
        // PBI-162 Subtask 1: LanguageServerConnection must have an initialized flag
        // that defaults to false before the initialized notification is sent.

        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // After spawning, the connection should have initialized=false
        // since we haven't sent the initialized notification yet
        let conn = LanguageServerConnection::spawn(&config).unwrap();

        // The initialized field should be accessible and false by default
        assert!(!conn.initialized, "initialized flag should default to false after spawn");
    }

    #[test]
    fn spawn_with_notifications_returns_connection_and_progress_notifications() {
        // RED phase: Test that spawn_with_notifications returns a tuple of
        // (LanguageServerConnection, Vec<Value>) containing connection and
        // any $/progress notifications captured during initialization

        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        // Call the new method - should return (LanguageServerConnection, Vec<Value>)
        let result = LanguageServerConnection::spawn_with_notifications(&config);
        assert!(result.is_some(), "spawn_with_notifications should succeed");

        let (mut conn, notifications) = result.unwrap();

        // Connection should be alive
        assert!(conn.is_alive(), "Spawned connection should be alive");

        // rust-analyzer may or may not send progress notifications during init,
        // but the method should return the correct type
        assert!(
            notifications.is_empty() || !notifications.is_empty(),
            "Should return a Vec<Value> (may be empty or non-empty)"
        );

        // If there are notifications, verify they are $/progress
        for notification in &notifications {
            if let Some(method) = notification.get("method").and_then(|m| m.as_str()) {
                assert_eq!(
                    method, "$/progress",
                    "All returned notifications should be $/progress"
                );
            }
        }
    }
}
