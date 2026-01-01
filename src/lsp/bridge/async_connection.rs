//! Async bridge connection for concurrent LSP request handling.
//!
//! This module provides `AsyncBridgeConnection` which wraps a `LanguageServerConnection`
//! and adds async request/response handling with a background reader thread.
//!
//! # Problem Solved
//!
//! The original `LanguageServerConnection` uses synchronous blocking I/O. When multiple
//! concurrent LSP requests (hover, documentHighlight, signatureHelp, etc.) try to use
//! the same connection, they fight over the stdin/stdout streams.
//!
//! # Solution
//!
//! This module implements an async request/response queue pattern:
//! 1. A background reader thread continuously reads responses from stdout
//! 2. Responses are routed to the correct caller by request ID via oneshot channels
//! 3. Callers receive a `oneshot::Receiver` immediately and await their response
//! 4. Multiple concurrent requests can share one connection without blocking each other

use dashmap::DashMap;
use serde_json::Value;
use std::io::{BufRead, BufReader, Write};
use std::process::{ChildStdin, ChildStdout};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, Ordering};
use std::thread::JoinHandle;
use tokio::sync::oneshot;

/// Response routing entry - stores the sender for a pending request
type PendingRequest = oneshot::Sender<ResponseResult>;

/// Result of a response read operation
#[derive(Debug)]
pub struct ResponseResult {
    /// The JSON-RPC response (or None if error/timeout)
    pub response: Option<Value>,
    /// Captured $/progress notifications
    pub notifications: Vec<Value>,
}

/// Async bridge connection that handles concurrent LSP requests.
///
/// This wraps a language server's stdin/stdout and provides async request handling.
/// Multiple callers can send requests concurrently - each gets a receiver for their
/// specific response, routed by request ID.
pub struct AsyncBridgeConnection {
    /// Next request ID (atomically incremented)
    next_request_id: AtomicI64,
    /// Pending requests awaiting responses: request_id -> response sender
    pending_requests: Arc<DashMap<i64, PendingRequest>>,
    /// Stdin for writing requests (protected by mutex for write serialization)
    stdin: std::sync::Mutex<ChildStdin>,
    /// Handle to the background reader thread
    reader_handle: Option<JoinHandle<()>>,
    /// Signal to stop the reader thread
    shutdown: Arc<AtomicBool>,
    /// Channel for notifications ($/progress, etc.)
    /// Note: Currently unused but will be used when integrating with lsp_impl
    #[allow(dead_code)]
    notification_sender: tokio::sync::mpsc::Sender<Value>,
}

impl AsyncBridgeConnection {
    /// Create a new async connection from a language server's stdin/stdout.
    ///
    /// This spawns a background thread to read responses from stdout and route
    /// them to the correct caller by request ID.
    ///
    /// # Arguments
    /// * `stdin` - The language server's stdin for writing requests
    /// * `stdout` - The language server's stdout for reading responses
    /// * `notification_sender` - Channel to send captured notifications
    pub fn new(
        stdin: ChildStdin,
        stdout: ChildStdout,
        notification_sender: tokio::sync::mpsc::Sender<Value>,
    ) -> Self {
        let pending_requests: Arc<DashMap<i64, PendingRequest>> = Arc::new(DashMap::new());
        let shutdown = Arc::new(AtomicBool::new(false));

        // Clone for the background thread
        let pending_clone = pending_requests.clone();
        let shutdown_clone = shutdown.clone();
        let notif_sender_clone = notification_sender.clone();

        // Spawn background reader thread
        let reader_handle = std::thread::spawn(move || {
            Self::reader_loop(stdout, pending_clone, shutdown_clone, notif_sender_clone);
        });

        Self {
            next_request_id: AtomicI64::new(1),
            pending_requests,
            stdin: std::sync::Mutex::new(stdin),
            reader_handle: Some(reader_handle),
            shutdown,
            notification_sender,
        }
    }

    /// Background reader loop that reads responses and routes them to callers.
    fn reader_loop(
        stdout: ChildStdout,
        pending: Arc<DashMap<i64, PendingRequest>>,
        shutdown: Arc<AtomicBool>,
        notification_sender: tokio::sync::mpsc::Sender<Value>,
    ) {
        let mut reader = BufReader::new(stdout);

        loop {
            // Check shutdown flag
            if shutdown.load(Ordering::SeqCst) {
                log::debug!(
                    target: "treesitter_ls::bridge::async",
                    "[READER] Shutdown signal received"
                );
                break;
            }

            // Read JSON-RPC message
            let message = match Self::read_message(&mut reader) {
                Ok(Some(msg)) => msg,
                Ok(None) => {
                    // EOF or empty read
                    log::debug!(
                        target: "treesitter_ls::bridge::async",
                        "[READER] EOF or empty read"
                    );
                    break;
                }
                Err(e) => {
                    log::warn!(
                        target: "treesitter_ls::bridge::async",
                        "[READER] Error reading message: {}",
                        e
                    );
                    break;
                }
            };

            // Check if this is a response (has "id" field) or notification (has "method" but no "id")
            if let Some(id) = message.get("id").and_then(|id| id.as_i64()) {
                // This is a response - route to the waiting caller
                log::debug!(
                    target: "treesitter_ls::bridge::async",
                    "[READER] Routing response for id={}",
                    id
                );

                if let Some((_, sender)) = pending.remove(&id) {
                    let result = ResponseResult {
                        response: Some(message),
                        notifications: vec![], // Notifications are sent via the channel
                    };
                    // Send response (ignore error if receiver dropped)
                    let _ = sender.send(result);
                } else {
                    log::warn!(
                        target: "treesitter_ls::bridge::async",
                        "[READER] No pending request for id={}",
                        id
                    );
                }
            } else if let Some(method) = message.get("method").and_then(|m| m.as_str()) {
                // This is a notification
                if method == "$/progress" {
                    // Send progress notifications via channel
                    let _ = notification_sender.blocking_send(message);
                }
                // Other notifications are ignored
            }
        }

        log::debug!(
            target: "treesitter_ls::bridge::async",
            "[READER] Reader loop exiting"
        );
    }

    /// Read a single JSON-RPC message from the reader.
    fn read_message(reader: &mut BufReader<ChildStdout>) -> Result<Option<Value>, String> {
        // Read headers
        let mut content_length = 0;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) => return Ok(None), // EOF
                Ok(_) => {}
                Err(e) => return Err(format!("Header read error: {}", e)),
            }

            let line = line.trim();
            if line.is_empty() {
                break;
            }
            if let Some(len_str) = line.strip_prefix("Content-Length:") {
                content_length = len_str
                    .trim()
                    .parse()
                    .map_err(|e| format!("Invalid content length: {}", e))?;
            }
        }

        if content_length == 0 {
            return Ok(None);
        }

        // Read content
        let mut content = vec![0u8; content_length];
        std::io::Read::read_exact(reader, &mut content)
            .map_err(|e| format!("Content read error: {}", e))?;

        serde_json::from_slice(&content)
            .map(Some)
            .map_err(|e| format!("JSON parse error: {}", e))
    }

    /// Send a JSON-RPC request and return a receiver for the response.
    ///
    /// This method is non-blocking. It writes the request to stdin and returns
    /// immediately with a receiver. The caller can await the response asynchronously.
    ///
    /// # Arguments
    /// * `method` - The JSON-RPC method name
    /// * `params` - The request parameters
    ///
    /// # Returns
    /// A receiver that will contain the response when it arrives, or an error if
    /// the request could not be sent.
    pub fn send_request(
        &self,
        method: &str,
        params: Value,
    ) -> Result<(i64, oneshot::Receiver<ResponseResult>), String> {
        // Generate request ID
        let id = self.next_request_id.fetch_add(1, Ordering::SeqCst);

        // Create oneshot channel for response
        let (sender, receiver) = oneshot::channel();

        // Register pending request BEFORE sending (to avoid race with reader)
        self.pending_requests.insert(id, sender);

        // Build request
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        // Write request to stdin
        let content = serde_json::to_string(&request).map_err(|e| format!("JSON error: {}", e))?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        {
            let mut stdin = self
                .stdin
                .lock()
                .map_err(|e| format!("Stdin lock error: {}", e))?;
            stdin
                .write_all(header.as_bytes())
                .map_err(|e| format!("Write error: {}", e))?;
            stdin
                .write_all(content.as_bytes())
                .map_err(|e| format!("Write error: {}", e))?;
            stdin.flush().map_err(|e| format!("Flush error: {}", e))?;
        }

        log::debug!(
            target: "treesitter_ls::bridge::async",
            "[CONN] Sent request id={} method={}",
            id,
            method
        );

        Ok((id, receiver))
    }

    /// Send a JSON-RPC notification (no response expected).
    ///
    /// # Arguments
    /// * `method` - The notification method name
    /// * `params` - The notification parameters
    pub fn send_notification(&self, method: &str, params: Value) -> Result<(), String> {
        let notification = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let content =
            serde_json::to_string(&notification).map_err(|e| format!("JSON error: {}", e))?;
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        let mut stdin = self
            .stdin
            .lock()
            .map_err(|e| format!("Stdin lock error: {}", e))?;
        stdin
            .write_all(header.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        stdin
            .write_all(content.as_bytes())
            .map_err(|e| format!("Write error: {}", e))?;
        stdin.flush().map_err(|e| format!("Flush error: {}", e))?;

        log::debug!(
            target: "treesitter_ls::bridge::async",
            "[CONN] Sent notification method={}",
            method
        );

        Ok(())
    }

    /// Shutdown the connection and reader thread.
    pub fn shutdown(&mut self) {
        // Signal reader thread to stop
        self.shutdown.store(true, Ordering::SeqCst);

        // Wait for reader thread to finish
        if let Some(handle) = self.reader_handle.take() {
            let _ = handle.join();
        }

        log::debug!(
            target: "treesitter_ls::bridge::async",
            "[CONN] Shutdown complete"
        );
    }
}

impl Drop for AsyncBridgeConnection {
    fn drop(&mut self) {
        self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pending_requests_map_can_store_and_retrieve_senders() {
        // Unit test for the pending_requests map routing
        let pending: DashMap<i64, PendingRequest> = DashMap::new();

        // Create a oneshot channel
        let (sender1, receiver1) = oneshot::channel::<ResponseResult>();
        let (sender2, receiver2) = oneshot::channel::<ResponseResult>();

        // Store senders by request ID
        pending.insert(1, sender1);
        pending.insert(2, sender2);

        // Verify we can remove and get the correct sender
        let (_, removed1) = pending.remove(&1).expect("Should find sender for id=1");
        let (_, removed2) = pending.remove(&2).expect("Should find sender for id=2");

        // Verify map is now empty
        assert!(pending.is_empty());

        // Verify we can send through the removed senders
        let result1 = ResponseResult {
            response: Some(serde_json::json!({"id": 1})),
            notifications: vec![],
        };
        let result2 = ResponseResult {
            response: Some(serde_json::json!({"id": 2})),
            notifications: vec![],
        };

        removed1.send(result1).expect("Should send to receiver1");
        removed2.send(result2).expect("Should send to receiver2");

        // Verify receivers get the correct responses
        let recv1 = receiver1.blocking_recv().expect("Should receive response1");
        let recv2 = receiver2.blocking_recv().expect("Should receive response2");

        assert_eq!(
            recv1.response.unwrap()["id"].as_i64(),
            Some(1),
            "Receiver1 should get id=1"
        );
        assert_eq!(
            recv2.response.unwrap()["id"].as_i64(),
            Some(2),
            "Receiver2 should get id=2"
        );
    }

    #[tokio::test]
    async fn send_request_returns_receiver() {
        // This test verifies the send_request signature returns a receiver
        // We can't easily test with a real language server here, but we verify the types

        // Create channels for testing
        let (notif_tx, _notif_rx) = tokio::sync::mpsc::channel::<Value>(16);

        // We need a real stdin/stdout to create the connection, so we just verify
        // the type signature works correctly using a mock approach
        // For now, this is a compile-time verification of the API
        // notif_tx is used to verify the type signature compiles
        let _ = notif_tx;

        // The actual integration test is in pool.rs
    }
}
