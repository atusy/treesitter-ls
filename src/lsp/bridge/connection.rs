//! Async connection to downstream language server processes.
//!
//! This module provides the core connection type for communicating with
//! language servers via stdio using async I/O.

use std::io;
use std::process::Stdio;

use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};

/// Async connection to a downstream language server process.
///
/// Manages the lifecycle of a child process running a language server,
/// providing async I/O for LSP JSON-RPC communication over stdio.
///
/// # Architecture (ADR-0014)
///
/// - Uses `tokio::process::Command` for process spawning
/// - Dedicated async reader task with `select!` for cancellation
/// - Pending request tracking via `DashMap<RequestId, oneshot::Sender>`
pub(crate) struct AsyncBridgeConnection {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl AsyncBridgeConnection {
    /// Spawn a new language server process.
    ///
    /// # Arguments
    /// * `cmd` - Command and arguments to spawn (e.g., `["lua-language-server"]`)
    ///
    /// # Returns
    /// A new `AsyncBridgeConnection` with stdio pipes connected to the child process.
    pub(crate) async fn spawn(cmd: Vec<String>) -> io::Result<Self> {
        let (program, args) = cmd.split_first().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "command must not be empty")
        })?;

        let mut child = Command::new(program)
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stdin"))?;

        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| io::Error::other("failed to capture stdout"))?;

        Ok(Self {
            child,
            stdin,
            stdout: BufReader::new(stdout),
        })
    }

    /// Write a JSON-RPC message to the child process stdin.
    ///
    /// Formats the message with LSP Content-Length header:
    /// `Content-Length: <length>\r\n\r\n<json>`
    pub(crate) async fn write_message(&mut self, message: &serde_json::Value) -> io::Result<()> {
        let body = serde_json::to_string(message)?;
        let header = format!("Content-Length: {}\r\n\r\n", body.len());

        self.stdin.write_all(header.as_bytes()).await?;
        self.stdin.write_all(body.as_bytes()).await?;
        self.stdin.flush().await?;

        Ok(())
    }

    /// Read the raw bytes of an LSP message body from stdout.
    ///
    /// Parses the Content-Length header, reads the separator, and returns the body bytes.
    async fn read_message_bytes(&mut self) -> io::Result<Vec<u8>> {
        use tokio::io::AsyncReadExt;

        // Read header line
        let mut header_line = String::new();
        self.stdout.read_line(&mut header_line).await?;

        // Parse content length
        let content_length: usize = header_line
            .strip_prefix("Content-Length: ")
            .and_then(|s| s.trim().parse().ok())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "invalid Content-Length"))?;

        // Read empty line (CRLF separator)
        let mut empty_line = String::new();
        self.stdout.read_line(&mut empty_line).await?;

        // Read body
        let mut body = vec![0u8; content_length];
        self.stdout.read_exact(&mut body).await?;

        Ok(body)
    }

    /// Read and parse a JSON-RPC message from the child process stdout.
    ///
    /// Parses the Content-Length header and reads the JSON body.
    pub(crate) async fn read_message(&mut self) -> io::Result<serde_json::Value> {
        let body = self.read_message_bytes().await?;
        serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
    }
}

impl Drop for AsyncBridgeConnection {
    fn drop(&mut self) {
        // Kill the child process to prevent orphans (AC3)
        // start_kill() is non-blocking and signals the process to terminate
        if let Err(e) = self.child.start_kill() {
            log::warn!(
                target: "treesitter_ls::bridge",
                "Failed to kill child process: {}",
                e
            );
        } else {
            log::debug!(
                target: "treesitter_ls::bridge",
                "Killed child process {:?}",
                self.child.id()
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn spawn_creates_child_process_with_stdio() {
        // Use `cat` as a simple test process that echoes stdin to stdout
        let cmd = vec!["cat".to_string()];
        let _conn = AsyncBridgeConnection::spawn(cmd)
            .await
            .expect("spawn should succeed");

        // If spawn succeeded, we have a valid connection
    }

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
            "result": { "capabilities": {} }
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
    async fn initialize_lua_language_server() {
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
            if msg.get("id").is_some() {
                break msg;
            }
            // Skip notifications
        };

        // Verify the response indicates successful initialization
        assert_eq!(response["jsonrpc"], "2.0");
        assert_eq!(response["id"], 1);
        assert!(response["result"].is_object(), "should have result object");
        assert!(
            response["result"]["capabilities"].is_object(),
            "should have capabilities"
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
}
