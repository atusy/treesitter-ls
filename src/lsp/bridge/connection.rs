//! BridgeConnection for managing connections to language servers

use std::sync::atomic::{AtomicBool, Ordering};
use tokio::process::{Child, ChildStdin, ChildStdout};

/// Represents a connection to a bridged language server
#[allow(dead_code)] // Used in Phase 2 (real LSP communication)
pub struct BridgeConnection {
    /// Spawned language server process
    process: Child,
    /// Stdin handle for sending requests/notifications
    stdin: ChildStdin,
    /// Stdout handle for receiving responses/notifications
    stdout: ChildStdout,
    /// Tracks whether the connection has been initialized
    initialized: AtomicBool,
    /// Tracks whether didOpen notification has been sent
    did_open_sent: AtomicBool,
}

impl std::fmt::Debug for BridgeConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("BridgeConnection")
            .field("process_id", &self.process.id())
            .field("initialized", &self.initialized.load(Ordering::SeqCst))
            .field("did_open_sent", &self.did_open_sent.load(Ordering::SeqCst))
            .finish()
    }
}

impl BridgeConnection {
    /// Creates a new BridgeConnection by spawning a language server process
    ///
    /// # Arguments
    /// * `command` - Command to spawn (e.g., "lua-language-server")
    ///
    /// # Errors
    /// Returns error if:
    /// - Process fails to spawn
    /// - stdin/stdout handles cannot be obtained
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) async fn new(command: &str) -> Result<Self, String> {
        use tokio::process::Command;
        use std::process::Stdio;

        let mut child = Command::new(command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| format!("Failed to spawn {}: {}", command, e))?;

        let stdin = child.stdin.take()
            .ok_or_else(|| format!("Failed to obtain stdin for {}", command))?;
        let stdout = child.stdout.take()
            .ok_or_else(|| format!("Failed to obtain stdout for {}", command))?;

        Ok(Self {
            process: child,
            stdin,
            stdout,
            initialized: AtomicBool::new(false),
            did_open_sent: AtomicBool::new(false),
        })
    }

    /// Writes a JSON-RPC message with LSP Base Protocol framing
    ///
    /// Format: `Content-Length: N\r\n\r\n{json}`
    async fn write_message<W>(writer: &mut W, message: &serde_json::Value) -> Result<(), String>
    where
        W: tokio::io::AsyncWrite + Unpin,
    {
        use tokio::io::AsyncWriteExt;

        let json_str = serde_json::to_string(message)
            .map_err(|e| format!("Failed to serialize JSON: {}", e))?;

        let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);

        writer.write_all(content.as_bytes()).await
            .map_err(|e| format!("Failed to write message: {}", e))?;

        writer.flush().await
            .map_err(|e| format!("Failed to flush writer: {}", e))?;

        Ok(())
    }

    /// Reads a JSON-RPC message with LSP Base Protocol framing
    ///
    /// Expected format: `Content-Length: N\r\n\r\n{json}`
    async fn read_message<R>(reader: &mut R) -> Result<serde_json::Value, String>
    where
        R: tokio::io::AsyncRead + Unpin,
    {
        use tokio::io::AsyncBufReadExt;
        use tokio::io::AsyncReadExt;

        let mut reader = tokio::io::BufReader::new(reader);

        // Read header line: "Content-Length: N"
        let mut header_line = String::new();
        reader.read_line(&mut header_line).await
            .map_err(|e| format!("Failed to read header: {}", e))?;

        // Parse Content-Length
        let content_length = header_line
            .trim()
            .strip_prefix("Content-Length: ")
            .ok_or_else(|| format!("Missing Content-Length header, got: {}", header_line))?
            .parse::<usize>()
            .map_err(|e| format!("Invalid Content-Length value: {}", e))?;

        // Read separator line (should be empty "\r\n")
        let mut separator = String::new();
        reader.read_line(&mut separator).await
            .map_err(|e| format!("Failed to read separator: {}", e))?;

        if separator.trim() != "" {
            return Err(format!("Expected empty separator line, got: {:?}", separator));
        }

        // Read exactly content_length bytes for JSON body
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body).await
            .map_err(|e| format!("Failed to read message body: {}", e))?;

        // Parse JSON
        let message = serde_json::from_slice(&body)
            .map_err(|e| format!("Failed to parse JSON: {}", e))?;

        Ok(message)
    }

    /// Initializes the connection
    ///
    /// This is a fakeit implementation that does NOT send real LSP initialize
    /// request. It simply sets the initialized flag to true and returns Ok(()).
    #[allow(dead_code)] // Used in Phase 2 (real LSP communication)
    pub(crate) fn initialize(&self) -> Result<(), String> {
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use serde_json::json;

    #[tokio::test]
    async fn test_bridge_connection_spawns_process_with_valid_command() {
        // Test spawning a real process (use 'cat' as a simple test command)
        // This verifies tokio::process::Command integration
        let result = BridgeConnection::new("cat").await;

        assert!(result.is_ok(), "Failed to spawn process: {:?}", result.err());
        let connection = result.unwrap();

        // Verify process is alive (type checks - fields exist)
        let _stdin: &ChildStdin = &connection.stdin;
        let _stdout: &ChildStdout = &connection.stdout;

        // Initially should not be initialized
        assert!(!connection.initialized.load(Ordering::SeqCst));
        assert!(!connection.did_open_sent.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn test_bridge_connection_fails_with_invalid_command() {
        // Test that invalid command returns clear error
        let result = BridgeConnection::new("nonexistent-binary-xyz123").await;

        assert!(result.is_err(), "Should fail for nonexistent command");
        let error = result.unwrap_err();
        assert!(error.contains("Failed to spawn"), "Error should mention spawn failure: {}", error);
        assert!(error.contains("nonexistent-binary-xyz123"), "Error should mention command: {}", error);
    }

    #[tokio::test]
    async fn test_write_message_formats_with_content_length_header() {
        // Test LSP Base Protocol message framing
        let message = json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {}
        });

        let mut buffer = Vec::new();
        BridgeConnection::write_message(&mut buffer, &message).await.unwrap();

        let output = String::from_utf8(buffer).unwrap();

        // Should have Content-Length header
        assert!(output.starts_with("Content-Length: "), "Should start with Content-Length header");

        // Should have \r\n\r\n separator
        assert!(output.contains("\r\n\r\n"), "Should have \\r\\n\\r\\n separator");

        // Should end with JSON body
        let parts: Vec<&str> = output.split("\r\n\r\n").collect();
        assert_eq!(parts.len(), 2, "Should have exactly header and body");

        let header = parts[0];
        let body = parts[1];

        // Header should be Content-Length: N
        let content_length: usize = header.strip_prefix("Content-Length: ")
            .unwrap()
            .parse()
            .unwrap();

        // Body length should match Content-Length
        assert_eq!(body.len(), content_length, "Body length should match Content-Length header");

        // Body should be valid JSON
        let parsed: serde_json::Value = serde_json::from_str(body).unwrap();
        assert_eq!(parsed["method"], "initialize");
    }

    #[tokio::test]
    async fn test_read_message_parses_content_length_header() {
        // RED: Test reading LSP Base Protocol message
        let message = json!({"jsonrpc": "2.0", "id": 1, "result": {}});
        let json_str = serde_json::to_string(&message).unwrap();
        let content = format!("Content-Length: {}\r\n\r\n{}", json_str.len(), json_str);

        let mut reader = std::io::Cursor::new(content.as_bytes());
        let result = BridgeConnection::read_message(&mut reader).await.unwrap();

        assert_eq!(result["id"], 1);
        assert_eq!(result["result"], json!({}));
    }

    #[tokio::test]
    async fn test_read_message_fails_on_invalid_header() {
        // RED: Test error handling for malformed messages
        let content = "Invalid-Header: 123\r\n\r\n{}";
        let mut reader = std::io::Cursor::new(content.as_bytes());

        let result = BridgeConnection::read_message(&mut reader).await;
        assert!(result.is_err(), "Should fail on invalid header");

        let error = result.unwrap_err();
        assert!(error.contains("Content-Length"), "Error should mention Content-Length");
    }
}
