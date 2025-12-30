//! LSP Redirection for injection regions
//!
//! This module handles redirecting LSP requests for code inside injection regions
//! (e.g., Rust code blocks in Markdown) to appropriate language servers.

use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdout, Command, Stdio};
use tower_lsp::lsp_types::*;

/// Manages a connection to a language server subprocess with a temporary workspace
pub struct LanguageServerConnection {
    process: Child,
    request_id: i64,
    stdout_reader: BufReader<ChildStdout>,
    /// Temporary directory for the workspace (cleaned up on drop)
    temp_dir: Option<PathBuf>,
}

impl LanguageServerConnection {
    /// Spawn rust-analyzer with a temporary Cargo project workspace.
    ///
    /// rust-analyzer requires a Cargo project context for go-to-definition to work.
    /// This creates a minimal temp project that gets cleaned up on drop.
    pub fn spawn_rust_analyzer() -> Option<Self> {
        // Create a temporary directory for the Cargo project
        let temp_dir =
            std::env::temp_dir().join(format!("treesitter-ls-ra-{}", std::process::id()));
        let src_dir = temp_dir.join("src");
        std::fs::create_dir_all(&src_dir).ok()?;

        // Write minimal Cargo.toml
        let cargo_toml = temp_dir.join("Cargo.toml");
        std::fs::write(
            &cargo_toml,
            "[package]\nname = \"virtual\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
        )
        .ok()?;

        // Create empty main.rs (will be overwritten by did_open)
        let main_rs = src_dir.join("main.rs");
        std::fs::write(&main_rs, "").ok()?;

        let root_uri = format!("file://{}", temp_dir.display());

        let mut process = Command::new("rust-analyzer")
            .current_dir(&temp_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;

        // Take stdout and wrap in BufReader to maintain consistent buffering
        let stdout = process.stdout.take()?;
        let stdout_reader = BufReader::new(stdout);

        let mut conn = Self {
            process,
            request_id: 0,
            stdout_reader,
            temp_dir: Some(temp_dir),
        };

        // Send initialize request with workspace root
        let init_params = serde_json::json!({
            "processId": std::process::id(),
            "capabilities": {},
            "rootUri": root_uri,
            "workspaceFolders": [{"uri": root_uri, "name": "virtual"}],
        });

        let init_id = conn.send_request("initialize", init_params)?;

        // Wait for initialize response
        conn.read_response_for_id(init_id)?;

        // Send initialized notification
        conn.send_notification("initialized", serde_json::json!({}));

        Some(conn)
    }

    /// Get the URI for the virtual main.rs file in the temp workspace
    pub fn main_rs_uri(&self) -> Option<String> {
        self.temp_dir
            .as_ref()
            .map(|dir| format!("file://{}/src/main.rs", dir.display()))
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
    fn read_response_for_id(&mut self, expected_id: i64) -> Option<Value> {
        loop {
            // Read headers
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if self.stdout_reader.read_line(&mut line).ok()? == 0 {
                    return None; // EOF
                }
                let line = line.trim();
                if line.is_empty() {
                    break;
                }
                if let Some(len_str) = line.strip_prefix("Content-Length:") {
                    content_length = len_str.trim().parse().ok()?;
                }
            }

            if content_length == 0 {
                return None;
            }

            // Read content
            let mut content = vec![0u8; content_length];
            std::io::Read::read_exact(&mut self.stdout_reader, &mut content).ok()?;

            let message: Value = serde_json::from_slice(&content).ok()?;

            // Check if this is the response we're looking for
            if let Some(id) = message.get("id")
                && id.as_i64() == Some(expected_id)
            {
                return Some(message);
            }
            // Otherwise it's a notification or different response - skip it
        }
    }

    /// Open a document in the language server and write it to the temp workspace.
    ///
    /// For rust-analyzer, we need to write the file to disk for proper indexing.
    /// The content is written to src/main.rs in the temp workspace.
    pub fn did_open(&mut self, _uri: &str, language_id: &str, content: &str) -> Option<()> {
        // Write content to the actual file on disk (rust-analyzer needs this)
        if let Some(temp_dir) = &self.temp_dir {
            let main_rs = temp_dir.join("src").join("main.rs");
            std::fs::write(&main_rs, content).ok()?;
        }

        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        let params = serde_json::json!({
            "textDocument": {
                "uri": real_uri,
                "languageId": language_id,
                "version": 1,
                "text": content,
            }
        });

        self.send_notification("textDocument/didOpen", params)?;

        // Wait for rust-analyzer to index the project.
        // rust-analyzer needs time to parse the file and build its index.
        // We wait for diagnostic notifications which indicate indexing is complete.
        self.wait_for_indexing();

        Some(())
    }

    /// Wait for rust-analyzer to finish indexing by consuming messages until we see diagnostics
    fn wait_for_indexing(&mut self) {
        // Read messages until we get a publishDiagnostics notification
        // or timeout after consuming a few messages
        for _ in 0..50 {
            // Read headers
            let mut content_length = 0;
            loop {
                let mut line = String::new();
                if self.stdout_reader.read_line(&mut line).ok().unwrap_or(0) == 0 {
                    return; // EOF
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
                return;
            }

            // Read content
            let mut content = vec![0u8; content_length];
            if std::io::Read::read_exact(&mut self.stdout_reader, &mut content).is_err() {
                return;
            }

            let Ok(message) = serde_json::from_slice::<Value>(&content) else {
                continue;
            };

            // Check if this is a publishDiagnostics notification
            if let Some(method) = message.get("method").and_then(|m| m.as_str())
                && method == "textDocument/publishDiagnostics"
            {
                // rust-analyzer has indexed enough to publish diagnostics
                return;
            }
        }
    }

    /// Request go-to-definition
    ///
    /// Uses the actual file URI from the temp workspace, not the virtual URI.
    pub fn goto_definition(
        &mut self,
        _uri: &str,
        position: Position,
    ) -> Option<GotoDefinitionResponse> {
        // Use the real file URI from the temp workspace
        let real_uri = self.main_rs_uri()?;

        let params = serde_json::json!({
            "textDocument": { "uri": real_uri },
            "position": { "line": position.line, "character": position.character },
        });

        let req_id = self.send_request("textDocument/definition", params)?;

        // Read response (skipping notifications until we get the matching response)
        let response = self.read_response_for_id(req_id)?;

        // Extract result
        let result = response.get("result")?;

        // Parse as GotoDefinitionResponse
        serde_json::from_value(result.clone()).ok()
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

use std::collections::HashMap;

/// A pooled connection to a language server process
pub struct PooledConnection {
    process: Child,
}

impl PooledConnection {
    /// Spawn a new process with the given command and arguments
    pub fn spawn(command: &str, args: &[&str]) -> Option<Self> {
        let process = Command::new(command)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .ok()?;
        Some(Self { process })
    }

    /// Check if the process is still alive
    pub fn is_alive(&mut self) -> bool {
        matches!(self.process.try_wait(), Ok(None))
    }

    /// Kill the process
    pub fn kill(&mut self) {
        let _ = self.process.kill();
        let _ = self.process.wait();
    }
}

/// Pool of language server connections for reuse across requests
pub struct ServerPool {
    connections: HashMap<String, PooledConnection>,
}

impl ServerPool {
    /// Create a new empty server pool
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }

    /// Spawn a new server process and add it to the pool
    pub fn spawn_server(&mut self, name: &str, command: &str, args: &[&str]) -> Option<()> {
        let conn = PooledConnection::spawn(command, args)?;
        self.connections.insert(name.to_string(), conn);
        Some(())
    }

    /// Get a connection for the given server name, if one exists
    pub fn get(&self, name: &str) -> Option<&PooledConnection> {
        self.connections.get(name)
    }
}

impl Default for ServerPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_pool_new_returns_none_for_unknown_server() {
        let pool = ServerPool::new();
        let conn = pool.get("rust-analyzer");
        assert!(conn.is_none());
    }

    #[test]
    fn server_pool_spawn_then_get_returns_some() {
        let mut pool = ServerPool::new();
        // Spawn a simple server (use 'cat' as a mock - it reads stdin and exits)
        pool.spawn_server("test-server", "cat", &[]);
        let conn = pool.get("test-server");
        assert!(conn.is_some());
    }

    #[test]
    fn server_pool_get_twice_returns_same_connection() {
        let mut pool = ServerPool::new();
        pool.spawn_server("test-server", "cat", &[]);

        // Get connection twice and verify they point to the same instance
        let conn1 = pool.get("test-server").unwrap();
        let conn2 = pool.get("test-server").unwrap();

        // Use pointer equality to verify same connection
        assert!(std::ptr::eq(conn1, conn2));
    }

    #[test]
    fn pooled_connection_is_alive_returns_true_for_live_process() {
        let mut conn = PooledConnection::spawn("cat", &[]).unwrap();
        assert!(conn.is_alive());
    }

    #[test]
    fn pooled_connection_is_alive_returns_false_for_dead_process() {
        let mut conn = PooledConnection::spawn("cat", &[]).unwrap();
        // Kill the process
        conn.kill();
        // Wait a bit for process to terminate
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(!conn.is_alive());
    }

    #[test]
    fn server_pool_get_or_spawn_respawns_dead_connection() {
        let mut pool = ServerPool::new();
        pool.spawn_server("test-server", "cat", &[]);

        // Get connection and kill it
        {
            let conn = pool.get_mut("test-server").unwrap();
            conn.kill();
        }

        // get_or_spawn should spawn a new connection
        let conn = pool.get_or_spawn("test-server", "cat", &[]).unwrap();
        assert!(conn.is_alive());
    }
}
