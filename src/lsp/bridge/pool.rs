//! Language server connection pooling.
//!
//! This module provides a pool for reusing language server connections
//! across multiple LSP requests.

use super::connection::LanguageServerConnection;
use crate::config::settings::BridgeServerConfig;
use dashmap::DashMap;
use serde_json::Value;
use std::sync::Arc;
use std::time::Instant;

/// Pool of language server connections for reuse across requests.
/// Thread-safe via DashMap. Each connection is keyed by a unique name (typically server name).
///
/// Previously named RustAnalyzerPool - now generalized for any language server configured
/// via BridgeServerConfig.
pub struct LanguageServerPool {
    connections: Arc<DashMap<String, (LanguageServerConnection, Instant)>>,
}

impl LanguageServerPool {
    /// Create a new empty pool
    pub fn new() -> Self {
        Self {
            connections: Arc::new(DashMap::new()),
        }
    }

    /// Get or create a language server connection for the given key.
    /// Returns None if spawn fails.
    /// The connection is removed from the pool during use and must be returned via `return_connection`.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning a new connection if needed
    pub fn take_connection(
        &self,
        key: &str,
        config: &BridgeServerConfig,
    ) -> Option<LanguageServerConnection> {
        log::debug!(
            target: "treesitter_ls::bridge::pool",
            "[POOL] take_connection START key={}",
            key
        );
        // Try to take existing connection
        if let Some((_, (mut conn, _))) = self.connections.remove(key) {
            // Check if connection is still alive; if dead, spawn a new one
            if conn.is_alive() {
                log::debug!(
                    target: "treesitter_ls::bridge::pool",
                    "[POOL] take_connection REUSED key={}",
                    key
                );
                return Some(conn);
            }
            log::debug!(
                target: "treesitter_ls::bridge::pool",
                "[POOL] take_connection DEAD, respawning key={}",
                key
            );
            // Connection is dead, drop it and spawn a new one
        }
        // Spawn new one using config
        log::debug!(
            target: "treesitter_ls::bridge::pool",
            "[POOL] take_connection SPAWNING key={}",
            key
        );
        let result = LanguageServerConnection::spawn(config);
        log::debug!(
            target: "treesitter_ls::bridge::pool",
            "[POOL] take_connection DONE key={} success={}",
            key,
            result.is_some()
        );
        result
    }

    /// Return a connection to the pool for reuse
    pub fn return_connection(&self, key: &str, conn: LanguageServerConnection) {
        log::debug!(
            target: "treesitter_ls::bridge::pool",
            "[POOL] return_connection key={}",
            key
        );
        self.connections
            .insert(key.to_string(), (conn, Instant::now()));
    }

    /// Check if the pool has a connection for the given key
    pub fn has_connection(&self, key: &str) -> bool {
        self.connections.contains_key(key)
    }

    /// Spawn a language server connection in the background without blocking.
    ///
    /// This is used for eager pre-warming of connections (e.g., when did_open
    /// detects injection regions). The connection will be stored in the pool
    /// once spawned, ready for subsequent requests.
    ///
    /// This is a no-op if a connection for this key already exists in the pool.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning the connection
    pub fn spawn_in_background(&self, key: &str, config: &BridgeServerConfig) {
        // No-op if connection already exists
        if self.has_connection(key) {
            return;
        }

        // Clone data needed for the background task
        let key = key.to_string();
        let config = config.clone();

        // We need a reference to self.connections for the background task
        // Since DashMap is Send + Sync, we can clone the Arc reference
        let connections = self.connections.clone();

        tokio::spawn(async move {
            // Use spawn_blocking since LanguageServerConnection::spawn does blocking I/O
            let result =
                tokio::task::spawn_blocking(move || LanguageServerConnection::spawn(&config)).await;

            match result {
                Ok(Some(conn)) => {
                    connections.insert(key.clone(), (conn, Instant::now()));
                    log::info!(
                        target: "treesitter_ls::eager_spawn",
                        "Background spawn completed for {}",
                        key
                    );
                }
                Ok(None) => {
                    log::debug!(
                        target: "treesitter_ls::eager_spawn",
                        "Background spawn returned None for {}",
                        key
                    );
                }
                Err(e) => {
                    log::warn!(
                        target: "treesitter_ls::eager_spawn",
                        "Background spawn failed for {}: {}",
                        key,
                        e
                    );
                }
            }
        });
    }

    /// Spawn a language server connection in the background, forwarding $/progress notifications.
    ///
    /// Like `spawn_in_background`, but sends any `$/progress` notifications captured
    /// during spawn AND initial indexing through the provided channel. This allows the
    /// caller to forward progress notifications to the LSP client.
    ///
    /// After spawning, this also opens an empty file to trigger workspace indexing,
    /// which is when rust-analyzer sends most of its progress notifications (Loading
    /// crates, Indexing, etc.).
    ///
    /// This is a no-op if a connection for this key already exists in the pool.
    ///
    /// # Arguments
    /// * `key` - Unique key for this connection (typically server name)
    /// * `config` - Configuration for spawning the connection
    /// * `notification_sender` - Channel sender for forwarding $/progress notifications
    pub fn spawn_in_background_with_notifications(
        &self,
        key: &str,
        config: &BridgeServerConfig,
        notification_sender: tokio::sync::mpsc::Sender<Value>,
    ) {
        // No-op if connection already exists
        if self.has_connection(key) {
            return;
        }

        // Clone data needed for the background task
        let key = key.to_string();
        let config = config.clone();

        // We need a reference to self.connections for the background task
        // Since DashMap is Send + Sync, we can clone the Arc reference
        let connections = self.connections.clone();

        tokio::spawn(async move {
            // Use spawn_blocking since LanguageServerConnection::spawn_with_notifications does blocking I/O
            let result = tokio::task::spawn_blocking(move || {
                let spawn_result = LanguageServerConnection::spawn_with_notifications(&config);

                // If spawn succeeded, also trigger initial indexing by opening an empty file
                // This is where rust-analyzer sends most progress notifications
                if let Some((mut conn, mut spawn_notifications)) = spawn_result {
                    // Get the virtual file URI for the connection
                    if let Some(virtual_uri) = conn.virtual_file_uri() {
                        // Determine language_id from config
                        let language_id = config
                            .languages
                            .first()
                            .map(|s| s.as_str())
                            .unwrap_or("rust");

                        // Open an empty file to trigger indexing - this captures progress notifications
                        if let Some(indexing_notifications) =
                            conn.did_open_with_notifications(&virtual_uri, language_id, "")
                        {
                            spawn_notifications.extend(indexing_notifications);
                        }
                    }
                    Some((conn, spawn_notifications))
                } else {
                    None
                }
            })
            .await;

            match result {
                Ok(Some((conn, notifications))) => {
                    connections.insert(key.clone(), (conn, Instant::now()));

                    log::info!(
                        target: "treesitter_ls::eager_spawn",
                        "Background spawn completed for {} with {} notifications",
                        key,
                        notifications.len()
                    );

                    // Send captured notifications through the channel
                    for notification in notifications {
                        if notification_sender.send(notification).await.is_err() {
                            log::debug!(
                                target: "treesitter_ls::eager_spawn",
                                "Notification receiver dropped for {}",
                                key
                            );
                            break;
                        }
                    }
                }
                Ok(None) => {
                    log::debug!(
                        target: "treesitter_ls::eager_spawn",
                        "Background spawn returned None for {}",
                        key
                    );
                }
                Err(e) => {
                    log::warn!(
                        target: "treesitter_ls::eager_spawn",
                        "Background spawn failed for {}: {}",
                        key,
                        e
                    );
                }
            }
        });
    }
}

impl Default for LanguageServerPool {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::settings::WorkspaceType;

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
    fn language_server_pool_respawns_dead_connection() {
        if !check_rust_analyzer_available() {
            return;
        }

        let pool = LanguageServerPool::new();
        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: Some(WorkspaceType::Cargo),
        };

        // First take spawns a new connection
        let mut conn = pool.take_connection("test-key", &config).unwrap();
        assert!(conn.is_alive());

        // Kill the process to simulate a crash
        conn.shutdown();
        std::thread::sleep(std::time::Duration::from_millis(50));
        assert!(!conn.is_alive());

        // Return the dead connection to the pool
        pool.return_connection("test-key", conn);

        // Next take should detect the dead connection and respawn
        let mut conn2 = pool.take_connection("test-key", &config).unwrap();
        assert!(
            conn2.is_alive(),
            "Pool should have respawned dead connection"
        );
    }

    #[tokio::test]
    async fn spawn_in_background_method_exists_and_returns_immediately() {
        use std::time::{Duration, Instant};

        // Create a config that will work (if rust-analyzer is available)
        // or use a non-existent command to test the non-blocking aspect
        let config = BridgeServerConfig {
            cmd: vec!["nonexistent-server-for-testing-eager-spawn".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let pool = LanguageServerPool::new();

        // spawn_in_background should return immediately (not block on spawn failure)
        // If rust-analyzer were available, it would spawn in background
        let start = Instant::now();
        pool.spawn_in_background("test-key", &config);
        let elapsed = start.elapsed();

        // The call should return immediately (well under 100ms)
        // Actual spawn happens asynchronously
        assert!(
            elapsed < Duration::from_millis(100),
            "spawn_in_background should return immediately, took {:?}",
            elapsed
        );
    }

    // Note: spawn_in_background_stores_connection_in_pool_after_spawn test removed
    // because it requires blocking I/O waiting for rust-analyzer.
    // This functionality is covered by E2E tests in tests/test_lsp_definition.lua

    #[tokio::test]
    async fn spawn_in_background_is_noop_if_connection_exists() {
        if !check_rust_analyzer_available() {
            return;
        }

        let config = BridgeServerConfig {
            cmd: vec!["rust-analyzer".to_string()],
            languages: vec!["rust".to_string()],
            initialization_options: None,
            workspace_type: None,
        };

        let pool = LanguageServerPool::new();

        // First, get a connection via take_connection (synchronous spawn)
        let conn = pool.take_connection("rust-analyzer", &config);
        assert!(conn.is_some(), "Should spawn connection");

        // Return it to the pool
        pool.return_connection("rust-analyzer", conn.unwrap());
        assert!(pool.has_connection("rust-analyzer"));

        // Now call spawn_in_background - should be a no-op since connection exists
        pool.spawn_in_background("rust-analyzer", &config);

        // Connection should still be in pool (not removed or modified)
        assert!(
            pool.has_connection("rust-analyzer"),
            "Connection should still be in pool after no-op spawn_in_background"
        );
    }

    // Note: take_connection_reuses_prewarmed_connection_from_pool test removed
    // because it requires blocking I/O waiting for rust-analyzer.
    // This functionality is covered by E2E tests in tests/test_lsp_definition.lua
}
