# ADR-0014 Amendment 002: Notification Drop Telemetry and Feedback

**Date**: 2026-01-06
**Status**: Proposed
**Amends**: [ADR-0014](0014-actor-based-message-ordering.md) § Non-Blocking Backpressure (lines 82-91)
**Related**: [ADR-0015](0015-multi-server-coordination.md) § Backpressure Handling (lines 254-323)

## Issue

ADR-0014 line 88 specifies:
```
- **Non-coalescable notifications** (didSave, willSave): Dropped under extreme backpressure
```

**Problem**: Silent notification drops cause state divergence between client and server without any feedback mechanism.

**Scenario**:
```
T0: User saves file
T1: Client sends didSave notification
T2: Order queue full (256 entries, slow initialization)
T3: ADR-0014: "Non-coalescable notifications: Dropped"
T4: Client assumes success (notifications have no response)
T5: Server never runs diagnostics on saved content
T6: User sees stale diagnostics indefinitely
```

**Impact**: CRITICAL
- **Silent data loss**: Critical lifecycle events (save, willSave) lost without trace
- **State divergence**: Client and server have inconsistent view of file state
- **Stale diagnostics**: User confusion due to outdated error highlighting
- **No recovery path**: Client doesn't know drop occurred, can't retry

**LSP Spec Context**:
> LSP Spec § Notifications: "A processed notification message must not send a response back."

This means clients **cannot** detect dropped notifications through protocol responses. Alternative feedback mechanisms are required.

---

## Amendment

**Add comprehensive telemetry, logging, and mitigation strategy for dropped notifications.**

### 1. Logging Requirements

**Replace ADR-0014 line 88** with:

```
- **Non-coalescable notifications** (didSave, willSave): Dropped under extreme backpressure with telemetry feedback
```

**Add after line 91**:

```
### Notification Drop Handling

When dropping non-coalescable notifications due to queue full:

**1. Log at WARN level** (always, unconditionally):
```rust
log::warn!(
    "Dropped {} notification for {} (queue {}/{}, state: {:?})",
    method,
    uri.unwrap_or("unknown"),
    queue_len,
    QUEUE_CAPACITY,
    connection_state
);
```

**2. Send telemetry event to client** (LSP `$/telemetry` notification):
```rust
// Send to client via reverse notification channel
client_notification_tx.send(Notification {
    method: "$/telemetry".to_string(),
    params: json!({
        "type": "notification_dropped",
        "severity": "warning",
        "data": {
            "method": method,
            "uri": uri,
            "reason": "queue_full",
            "queue_length": queue_len,
            "queue_capacity": QUEUE_CAPACITY,
            "connection_state": format!("{:?}", connection_state),
            "timestamp": SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_millis()
        }
    })
});
```

**LSP Compliance**: `$/telemetry` is a standard LSP notification clients can subscribe to for monitoring events. Clients like VSCode display telemetry in "Output" panel.

**3. Circuit breaker integration**:
```rust
// Track dropped notifications in rolling time window
self.circuit_breaker.record_dropped_notification();

// Thresholds (configurable)
if self.circuit_breaker.dropped_count_in_window(Duration::from_secs(10)) > 10 {
    log::error!(
        "Circuit breaker OPEN: >10 notifications dropped in 10s (connection unhealthy)"
    );
    self.circuit_breaker.open();
    // Connection marked as unhealthy, pool may spawn replacement
}
```

**Rationale**: Sustained notification drops indicate severe backpressure, suggesting connection is unhealthy. Opening circuit breaker triggers pool recovery mechanisms (spawn new instance, fail fast for new requests).

**4. State re-synchronization metadata**:

When next coalescable notification (didChange) is sent, include dropped event metadata:

```rust
// In coalescing map, track dropped events per URI
struct CoalescingEntry {
    operation: Operation,
    generation: u64,
    dropped_events: Vec<String>,  // NEW: Track dropped lifecycle events
}

// When processing didChange after didSave drop
if let Some(dropped_events) = coalescing_entry.dropped_events.take() {
    // Inject metadata into didChange params
    let mut params = operation.params.clone();
    params["metadata"] = json!({
        "saved": dropped_events.contains(&"textDocument/didSave"),
        "dropped_lifecycle_events": dropped_events
    });
}
```

**Server Interpretation** (optional, server-side improvement):
If server supports metadata, it can re-sync state:
- `"saved": true` → Trigger diagnostics on latest content
- `"dropped_lifecycle_events"` → Log warning, attempt recovery

**Fallback**: If server doesn't support metadata, it's silently ignored (LSP allows extra fields). At minimum, telemetry/logging provides visibility for debugging.
```

---

### 2. Dropped Notification Categories

**Add classification for different notification types**:

```
**Drop Severity by Notification Type**:

| Notification Type | Drop Impact | Mitigation Strategy |
|------------------|-------------|---------------------|
| **textDocument/didSave** | HIGH - Diagnostics stale | Re-sync via didChange metadata |
| **textDocument/willSave** | MEDIUM - Pre-save hooks missed | Best-effort, informational only |
| **textDocument/didClose** | LOW - Resource leak risk | Server GC handles cleanup |
| **Custom notifications** | VARIES - Application-specific | Log + telemetry for visibility |

**Priority**: didSave has highest impact (diagnostics critical for UX). Prioritize logging/telemetry for these drops.
```

---

### 3. Alternative Mitigation: Buffered Overflow Queue

**Add section on future enhancement**:

```
### Future Enhancement: Overflow Buffer for Critical Notifications

**Problem**: Even with telemetry, dropped didSave notifications cause poor UX.

**Enhancement**: Add small overflow buffer (16 entries) for critical notifications:

```rust
struct ConnectionActor {
    order_queue: mpsc::Sender<Operation>,  // Main queue (256)
    overflow_buffer: VecDeque<Notification>,  // Overflow queue (16, notifications only)
}

impl ConnectionActor {
    async fn enqueue_notification(&mut self, notification: Notification) {
        match self.order_queue.try_send(notification.clone()) {
            Ok(_) => {
                // Normal path: enqueued successfully
            }
            Err(TrySendError::Full(_)) => {
                // Backpressure path
                if notification.method == "textDocument/didSave" {
                    // Critical: Buffer in overflow queue
                    if self.overflow_buffer.len() < 16 {
                        self.overflow_buffer.push_back(notification);
                        log::debug!("Buffered didSave in overflow queue");
                    } else {
                        // Overflow buffer also full - drop with telemetry
                        self.drop_with_telemetry(notification);
                    }
                } else {
                    // Non-critical: Drop with telemetry
                    self.drop_with_telemetry(notification);
                }
            }
        }
    }

    async fn drain_overflow_buffer(&mut self) {
        // Called periodically when order_queue has capacity
        while !self.overflow_buffer.is_empty() {
            if let Ok(_) = self.order_queue.try_send(self.overflow_buffer[0].clone()) {
                self.overflow_buffer.pop_front();
                log::debug!("Drained notification from overflow buffer");
            } else {
                break;  // Queue full again, wait for next drain
            }
        }
    }
}
```

**Tradeoff**: Adds complexity (two-tier queuing) but prevents critical notification loss. Recommended for Phase 2+.
```

---

## Implementation Requirements

### Connection Actor Changes

```rust
use std::time::{SystemTime, UNIX_EPOCH};
use std::collections::VecDeque;

pub struct ConnectionActor {
    // Existing fields...
    order_queue: mpsc::Sender<Operation>,
    coalescing_map: HashMap<(String, String), CoalescingEntry>,
    circuit_breaker: Arc<CircuitBreaker>,

    // NEW: Telemetry channel
    client_notification_tx: mpsc::Sender<Notification>,

    // NEW: Dropped event tracking per URI
    dropped_events_by_uri: HashMap<String, Vec<String>>,
}

struct CoalescingEntry {
    operation: Operation,
    generation: u64,
    dropped_events: Vec<String>,  // NEW
}

impl ConnectionActor {
    /// Handle notification drop with full telemetry
    fn drop_notification_with_telemetry(
        &mut self,
        method: &str,
        uri: Option<&str>,
        queue_len: usize,
        state: ConnectionState,
    ) {
        // 1. Log warning
        log::warn!(
            "Dropped {} notification for {} (queue {}/{}, state: {:?})",
            method,
            uri.unwrap_or("unknown"),
            queue_len,
            QUEUE_CAPACITY,
            state
        );

        // 2. Send telemetry to client
        let telemetry = Notification {
            method: "$/telemetry".to_string(),
            params: json!({
                "type": "notification_dropped",
                "severity": "warning",
                "data": {
                    "method": method,
                    "uri": uri,
                    "reason": "queue_full",
                    "queue_length": queue_len,
                    "queue_capacity": QUEUE_CAPACITY,
                    "connection_state": format!("{:?}", state),
                    "timestamp": SystemTime::now()
                        .duration_since(UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                }
            })
        };

        // Send via reverse notification channel (best-effort)
        if let Err(e) = self.client_notification_tx.try_send(telemetry) {
            log::debug!("Failed to send telemetry: {}", e);
            // Don't propagate error - telemetry is best-effort
        }

        // 3. Circuit breaker integration
        self.circuit_breaker.record_dropped_notification();

        let dropped_count = self.circuit_breaker
            .dropped_count_in_window(Duration::from_secs(10));

        if dropped_count > 10 {
            log::error!(
                "Circuit breaker OPEN: {} notifications dropped in 10s",
                dropped_count
            );
            self.circuit_breaker.open();
        }

        // 4. Track for metadata injection on next didChange
        if let Some(uri) = uri {
            self.dropped_events_by_uri
                .entry(uri.to_string())
                .or_insert_with(Vec::new)
                .push(method.to_string());
        }
    }

    /// Inject dropped event metadata into coalescable notification
    fn inject_dropped_event_metadata(
        &mut self,
        uri: &str,
        params: &mut serde_json::Value,
    ) {
        if let Some(dropped_events) = self.dropped_events_by_uri.remove(uri) {
            log::info!(
                "Re-syncing state for {}: injecting {} dropped events into metadata",
                uri,
                dropped_events.len()
            );

            // Add metadata field to params
            if let Some(obj) = params.as_object_mut() {
                obj["metadata"] = json!({
                    "saved": dropped_events.contains(&"textDocument/didSave".to_string()),
                    "dropped_lifecycle_events": dropped_events
                });
            }
        }
    }

    async fn enqueue_operation(&mut self, operation: Operation) -> Result<()> {
        // Existing coalescing logic...

        // When enqueuing to order queue
        match self.order_queue.try_send(operation.clone()) {
            Ok(_) => Ok(()),
            Err(TrySendError::Full(_)) => {
                if operation.is_notification() {
                    // Notification path - drop with telemetry
                    self.drop_notification_with_telemetry(
                        &operation.method,
                        operation.uri.as_deref(),
                        QUEUE_CAPACITY,
                        self.state.get(),
                    );

                    // Don't return error - notification drop is not a failure
                    Ok(())
                } else {
                    // Request path - return error to client
                    Err(anyhow!("Queue full, request rejected"))
                }
            }
            Err(e) => Err(e.into()),
        }
    }
}
```

### Circuit Breaker Extension

```rust
pub struct CircuitBreaker {
    // Existing fields...
    state: Arc<AtomicU8>,
    failure_count: Arc<AtomicUsize>,

    // NEW: Dropped notification tracking
    dropped_notifications: Arc<Mutex<VecDeque<Instant>>>,  // Timestamps
}

impl CircuitBreaker {
    pub fn record_dropped_notification(&self) {
        let now = Instant::now();
        let mut drops = self.dropped_notifications.lock().unwrap();
        drops.push_back(now);

        // Keep only last 60 seconds of data
        while let Some(&oldest) = drops.front() {
            if now.duration_since(oldest) > Duration::from_secs(60) {
                drops.pop_front();
            } else {
                break;
            }
        }
    }

    pub fn dropped_count_in_window(&self, window: Duration) -> usize {
        let now = Instant::now();
        let drops = self.dropped_notifications.lock().unwrap();

        drops.iter()
            .filter(|&&ts| now.duration_since(ts) <= window)
            .count()
    }
}
```

---

## Testing Requirements

### Unit Tests

1. **Test: Dropped notification generates telemetry**
   ```rust
   #[tokio::test]
   async fn test_dropped_notification_telemetry() {
       // Setup: Connection with full order queue
       // Action: Enqueue non-coalescable notification (didSave)
       // Assert: Notification dropped
       // Assert: WARN log emitted
       // Assert: $/telemetry notification sent to client
       // Assert: Telemetry contains method, uri, reason="queue_full"
   }
   ```

2. **Test: Circuit breaker opens on sustained drops**
   ```rust
   #[tokio::test]
   async fn test_circuit_breaker_on_notification_drops() {
       // Setup: Connection with circuit breaker
       // Action: Drop 11 notifications in 5 seconds
       // Assert: Circuit breaker state = OPEN
       // Assert: Error log includes "Circuit breaker OPEN"
   }
   ```

3. **Test: Metadata injection on didChange after didSave drop**
   ```rust
   #[tokio::test]
   async fn test_metadata_injection_after_drop() {
       // Setup: Drop didSave notification
       // Action: Enqueue didChange notification for same URI
       // Assert: didChange params include metadata.saved=true
       // Assert: didChange params include metadata.dropped_lifecycle_events
       // Assert: dropped_events_by_uri cleared for URI
   }
   ```

4. **Test: Rolling window for dropped notification count**
   ```rust
   #[tokio::test]
   async fn test_dropped_notification_rolling_window() {
       // Setup: Drop 5 notifications at T0
       // Setup: Drop 5 notifications at T15
       // Action: Check dropped_count_in_window(10s) at T16
       // Assert: Count = 5 (first 5 outside window)
   }
   ```

### Integration Tests

5. **Test: Client receives telemetry in VSCode**
   ```rust
   #[tokio::test]
   async fn test_client_telemetry_visibility() {
       // Setup: Real LSP client (mock or VSCode integration)
       // Action: Trigger notification drop
       // Assert: Client Output panel shows telemetry event
       // Assert: Event includes method, uri, reason
   }
   ```

6. **Test: No memory leak from unbounded dropped events**
   ```rust
   #[tokio::test]
   async fn test_dropped_events_memory_bounded() {
       // Setup: Connection with 1000 URIs
       // Action: Drop didSave for each URI
       // Action: Never send didChange (metadata never injected)
       // Assert: dropped_events_by_uri size remains bounded (e.g., <1000 entries)
       // Assert: No unbounded memory growth
       // Note: May need TTL for dropped events (e.g., expire after 60s)
   }
   ```

---

## LSP Protocol Compliance

**LSP Spec § Notifications**: Does not mandate notification delivery guarantees.

> "A processed notification message must not send a response back."

**Implications**:
- ✅ Notifications may be dropped (spec allows this)
- ✅ Using `$/telemetry` for feedback is LSP-compliant
- ✅ Metadata injection is optional (extra fields ignored by spec)

**Compliance**: This amendment improves reliability beyond LSP requirements while maintaining full spec compliance.

---

## Performance Impact

**Telemetry Overhead**:
- Log write: ~1-5μs (async, non-blocking)
- Telemetry send: ~10-50μs (try_send, non-blocking)
- Circuit breaker check: ~1μs (atomic read + mutex for rolling window)

**Total**: <100μs per dropped notification (only incurred under backpressure)

**Benefit**: Prevents silent state divergence, enables monitoring and debugging.

**Tradeoff**: Acceptable - overhead only applies during pathological backpressure, when system already degraded.

---

## Coordination With Other ADRs

### ADR-0015 (Multi-Server Coordination)

- Multi-server backpressure (lines 254-323) applies same telemetry strategy
- Each connection has independent circuit breaker
- Per-server notification drop telemetry identifies problematic servers

### ADR-0013 (Async I/O Layer)

- Reader task unaffected (only processes responses, not notifications)
- Writer loop handles telemetry notifications via reverse channel
- No impact on request/response lifecycle

### ADR-0016 (Graceful Shutdown)

- During shutdown, notification drops expected (connection closing)
- Telemetry still sent (visibility for debugging)
- Circuit breaker NOT triggered during Closing state (intentional degradation)

---

## Migration Notes

**For Existing Implementations**:

1. Add `client_notification_tx` channel to ConnectionActor
2. Implement `drop_notification_with_telemetry()` helper
3. Update `enqueue_operation()` to call helper on drop
4. Extend CircuitBreaker with dropped notification tracking
5. Test telemetry visibility in client (VSCode Output panel)
6. Consider adding overflow buffer for critical notifications (Phase 2+)

**Backward Compatibility**: Client receives new `$/telemetry` notifications (optional, clients can ignore).

---

## Summary

**Change**: Add comprehensive telemetry, logging, and circuit breaker integration for dropped notifications.

**Components**:
1. WARN-level logging for all dropped notifications
2. `$/telemetry` events sent to client (LSP-compliant)
3. Circuit breaker opens on sustained drops (>10 in 10s)
4. Metadata injection for state re-synchronization on next coalescable notification
5. Future: Overflow buffer for critical notifications (didSave)

**Impact**:
- ✅ Visibility: Operators and users aware of notification drops
- ✅ Debuggability: Telemetry enables root cause analysis
- ✅ Health monitoring: Circuit breaker detects unhealthy connections
- ✅ State recovery: Metadata injection provides re-sync mechanism

**Effort**: Medium - requires telemetry channel, circuit breaker extension, metadata tracking

**Risk**: Low - best-effort telemetry, doesn't affect core request/response path

**Priority**: CRITICAL - Silent data loss unacceptable for production systems

---

**Author**: Architecture Review Team
**Reviewers**: (pending)
**Implementation**: Required before Phase 1
