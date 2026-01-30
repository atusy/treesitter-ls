//! Shutdown coordination for downstream language servers.
//!
//! This module contains the shutdown-related methods for LanguageServerPool,
//! implementing graceful and forced shutdown per ADR-0017 (Graceful Shutdown).

use std::sync::Arc;

use super::{ConnectionState, GlobalShutdownTimeout, LanguageServerPool};

impl LanguageServerPool {
    /// Drains a JoinSet, logging any task panics with the provided context.
    pub(super) async fn drain_join_set(
        join_set: &mut tokio::task::JoinSet<()>,
        task_context: &str,
    ) {
        while let Some(result) = join_set.join_next().await {
            if let Err(e) = result {
                log::error!(
                    target: "kakehashi::bridge",
                    "{} panicked: {}",
                    task_context,
                    e
                );
            }
        }
    }

    /// Initiate graceful shutdown of all connections.
    ///
    /// Called during LSP server shutdown to cleanly terminate all downstream
    /// language servers. Performs LSP shutdown/exit handshake per ADR-0017.
    ///
    /// # Usage
    ///
    /// This method should be called exactly once during the LSP `shutdown` handler.
    /// Multiple concurrent calls are safe (due to state machine monotonicity) but
    /// wasteful, as connections already in Closing/Closed state are skipped.
    ///
    /// # Shutdown Behavior by State
    ///
    /// - Ready/Initializing: Perform full LSP shutdown handshake
    /// - Failed: Skip LSP handshake, go directly to Closed (stdin unavailable)
    /// - Closing/Closed: Already shutting down, skip
    ///
    /// All shutdowns run in parallel with a global timeout (ADR-0017).
    /// Uses the default GlobalShutdownTimeout (10s) per ADR-0018.
    pub(crate) async fn shutdown_all(&self) {
        self.shutdown_all_with_timeout(GlobalShutdownTimeout::default())
            .await;
    }

    /// Initiate graceful shutdown of all connections with a global timeout.
    ///
    /// This is the primary shutdown method per ADR-0017. It wraps parallel shutdown
    /// of all connections under a single global ceiling. When the timeout expires,
    /// remaining connections are force-killed with SIGTERM->SIGKILL escalation.
    ///
    /// # Arguments
    /// * `timeout` - Global shutdown timeout (5-15s per ADR-0018)
    ///
    /// # Behavior
    ///
    /// 1. All Ready/Initializing connections begin graceful shutdown in parallel
    /// 2. Failed connections transition directly to Closed (skip LSP handshake)
    /// 3. If global timeout expires before all complete:
    ///    - Remaining connections receive force_kill (SIGTERM->SIGKILL on Unix)
    ///    - All connections transition to Closed state
    ///
    /// # Example
    ///
    /// ```ignore
    /// let timeout = GlobalShutdownTimeout::new(Duration::from_secs(10))?;
    /// pool.shutdown_all_with_timeout(timeout).await;
    /// ```
    pub(crate) async fn shutdown_all_with_timeout(&self, timeout: GlobalShutdownTimeout) {
        // Track connections that were skipped for logging (minimize lock duration)
        let mut failed_connections: Vec<String> = Vec::new();
        let mut already_closing: Vec<String> = Vec::new();

        // Collect handles to shutdown - release lock before async operations
        let handles_to_shutdown: Vec<(String, Arc<super::ConnectionHandle>)> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .filter_map(|(language, handle)| match handle.state() {
                    ConnectionState::Ready | ConnectionState::Initializing => {
                        Some((language.clone(), Arc::clone(handle)))
                    }
                    ConnectionState::Failed => {
                        failed_connections.push(language.clone());
                        handle.complete_shutdown();
                        None
                    }
                    ConnectionState::Closing | ConnectionState::Closed => {
                        already_closing.push(language.clone());
                        None
                    }
                })
                .collect()
        };

        // Log after releasing lock
        for language in failed_connections {
            log::debug!(
                target: "kakehashi::bridge",
                "Shutting down {} connection (Failed → Closed)",
                language
            );
        }
        for language in already_closing {
            log::debug!(
                target: "kakehashi::bridge",
                "Connection {} already shutting down or closed",
                language
            );
        }

        if handles_to_shutdown.is_empty() {
            return;
        }

        // Spawn graceful shutdown tasks into JoinSet (outside timeout so we can abort on timeout)
        let mut join_set = tokio::task::JoinSet::new();
        for (language, handle) in handles_to_shutdown {
            join_set.spawn(async move {
                log::debug!(
                    target: "kakehashi::bridge",
                    "Performing graceful shutdown for {} connection",
                    language
                );
                if let Err(e) = handle.graceful_shutdown().await {
                    log::warn!(
                        target: "kakehashi::bridge",
                        "Graceful shutdown failed for {}: {}",
                        language, e
                    );
                }
            });
        }

        // Wait for all shutdowns to complete with global timeout
        let graceful_result = tokio::time::timeout(
            timeout.as_duration(),
            Self::drain_join_set(&mut join_set, "Shutdown task"),
        )
        .await;

        // Handle timeout: abort remaining tasks and force-kill connections
        if graceful_result.is_err() {
            log::warn!(
                target: "kakehashi::bridge",
                "Global shutdown timeout ({:?}) expired, force-killing remaining connections",
                timeout.as_duration()
            );

            // Abort still-running graceful shutdown tasks to avoid duplicate logs and wasted work.
            // Note: force_kill is idempotent (returns early if process exited), so any race is harmless.
            join_set.abort_all();

            self.force_kill_all().await;
        }
    }

    /// Force-kill all connections with platform-appropriate escalation.
    ///
    /// This is the fallback when global shutdown timeout expires.
    /// Per ADR-0017, this method terminates all non-closed connections and
    /// transitions them to Closed state.
    ///
    /// # Platform-Specific Behavior
    ///
    /// **Unix**: Uses SIGTERM->SIGKILL escalation (2s grace period)
    /// **Windows**: Uses TerminateProcess directly (no grace period)
    ///
    /// The method executes kills in parallel to minimize total shutdown time.
    ///
    /// # Single-Writer Loop (ADR-0015)
    ///
    /// Uses `graceful_shutdown()` which handles the writer task coordination
    /// and process killing. Since we're past the global timeout, this is best-effort.
    async fn force_kill_all(&self) {
        // Collect handles to force-kill (minimize lock duration - no logging inside lock)
        let handles_with_info: Vec<(String, ConnectionState, Arc<super::ConnectionHandle>)> = {
            let connections = self.connections.lock().await;
            connections
                .iter()
                .filter_map(|(language, handle)| {
                    let state = handle.state();
                    if state != ConnectionState::Closed {
                        Some((language.clone(), state, Arc::clone(handle)))
                    } else {
                        None
                    }
                })
                .collect()
        };

        // Force-kill all connections in parallel.
        // Using JoinSet for parallel execution ensures O(1) force-kill time for N connections.
        // We use graceful_shutdown which handles writer task coordination and process killing.
        let mut join_set = tokio::task::JoinSet::new();
        for (language, state, handle) in handles_with_info {
            log::debug!(
                target: "kakehashi::bridge",
                "Force-killing {} connection (state: {:?})",
                language,
                state
            );
            join_set.spawn(async move {
                // graceful_shutdown handles: cancel writer, reclaim, send shutdown/exit, kill process
                let _ = handle.graceful_shutdown().await;
            });
        }

        // Wait for all force-kills to complete
        Self::drain_join_set(&mut join_set, "Force-kill task").await;
    }
}
