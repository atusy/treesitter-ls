//! LSP Redirection for injection regions
//!
//! This module handles redirecting LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, Command, Stdio};
use tower_lsp::lsp_types::*;

/// Manages a connection to a language server subprocess
pub struct LanguageServerConnection {
    process: Child,
    request_id: i64,
}

impl LanguageServerConnection {
    /// Spawn rust-analyzer and initialize it
    pub fn spawn_rust_analyzer() -> Option<Self> {
        let process = Command::new("rust-analyzer")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        let mut conn = Self {
            process,
            request_id: 0,
        };

        // Send initialize request
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "capabilities": {},
            "rootUri": null,
        });

        conn.send_request("initialize", init_params)?;

        // Wait for initialize response
        conn.read_response()?;

        // Send initialized notification
        conn.send_notification("initialized", serde_json::json!({}));

        Some(conn)
    }

    /// Send a JSON-RPC request
    fn send_request(&mut self, method: &str, params: Value) -> Option<()> {
        self.request_id += 1;
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": self.request_id,
            "method": method,
            "params": params,
        });

        self.send_message(&request)
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

    /// Read a JSON-RPC response
    fn read_response(&mut self) -> Option<Value> {
        let stdout = self.process.stdout.as_mut()?;
        let mut reader = BufReader::new(stdout);

        // Read headers
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            reader.read_line(&mut line).ok()?;
            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(len_str) = line.strip_prefix("Content-Length:") {
                content_length = len_str.trim().parse().ok()?;
            }
        }

        // Read content
        let mut content = vec![0u8; content_length];
        std::io::Read::read_exact(&mut reader, &mut content).ok()?;

        serde_json::from_slice(&content).ok()
    }

    /// Open a virtual document in the language server
    pub fn did_open(&mut self, uri: &str, language_id: &str, content: &str) -> Option<()> {
        let params = serde_json::json!({
            "textDocument": {
                "uri": uri,
                "languageId": language_id,
                "version": 1,
                "text": content,
            }
        });

        self.send_notification("textDocument/didOpen", params)
    }

    /// Request go-to-definition
    pub fn goto_definition(
        &mut self,
        uri: &str,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        let params = serde_json::json!({
            "textDocument": { "uri": uri },
            "position": { "line": position.line, "character": position.character },
        });

        self.send_request("textDocument/definition", params)?;

        // Read response
        let response = self.read_response()?;

        // Extract result
        let result = response.get("result")?;

        // Parse as GotoDefinitionResponse
        serde_json::from_value(result.clone()).ok()
    }

    /// Shutdown the language server
    pub fn shutdown(&mut self) {
        let _ = self.send_request("shutdown", serde_json::json!(null));
        let _ = self.read_response();
        let _ = self.send_notification("exit", serde_json::json!(null));
        let _ = self.process.wait();
    }
}

impl Drop for LanguageServerConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}
