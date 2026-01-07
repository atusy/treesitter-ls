# ADR-0017: Timeout Precedence Hierarchy

**Date**: 2026-01-06
**Status**: Draft
**Type**: Cross-ADR Coordination
**Related ADRs**:
- [ADR-0013](0013-async-io-layer.md) § Idle Timeout
- [ADR-0015](0015-multi-server-coordination.md) § Response Aggregation
- [ADR-0016](0016-graceful-shutdown.md) § Shutdown Timeout

## Context

The async bridge architecture defines four distinct timeout systems across three ADRs:

1. **Initialization Timeout** (ADR-0013): Bounds server initialization time during startup
2. **Per-Request Timeout** (ADR-0015): Bounds user-facing latency for multi-server aggregation
3. **Idle Timeout** (ADR-0013): Detects hung servers (unresponsive to pending requests)
4. **Global Shutdown Timeout** (ADR-0016): Bounds total shutdown time

**Problem**: These timeout systems have overlapping responsibilities without clear precedence, causing non-deterministic behavior when multiple timeouts could fire simultaneously.

### Conflict Example

```
T0: Fan-out completion request to pyright + ruff (5s per-request timeout)
T2: Shutdown initiated (10s global timeout)
T3: pyright responds at T+3s
T4: ruff still pending

Which timeout fires first?
- Per-request timeout: T+5s (from T0)
- Global shutdown timeout: T+12s (from T2)
- Idle timeout: Implementation-defined (30-120s)

What happens to late responses?
- Should they be delivered or discarded?
- Do they reset idle timeout during shutdown?
```

### Impact Without Precedence Rules

- **Non-deterministic behavior**: Same scenario produces different outcomes based on timing
- **Timeout interactions undefined**: What happens when shutdown occurs during per-request timeout?
- **Late response handling ambiguous**: Accept during shutdown or discard?
- **Resource cleanup unclear**: Which timeout triggers connection state transitions?

## Decision

**Establish a four-tier timeout hierarchy with explicit precedence rules and interaction semantics.**

### Tier 0: Initialization Timeout (Connection Startup)

**Scope**: Per-connection during Initializing state only

**Duration**: Implementation-defined (typically 30-60 seconds)
- Longer than idle timeout (initialization is legitimately slow)
- Example: 60 seconds to allow for slow language server startup

**Trigger**: When `initialize` request sent to downstream server

**Action on Timeout**:
1. Log error: "Server initialization timeout after {duration}s"
2. Transition connection state: `Initializing` → `Failed`
3. Fail initialization request with `REQUEST_FAILED` (-32803)
4. Trigger circuit breaker (per ADR-0015 § Server Lifecycle)
5. Connection pool schedules retry with exponential backoff

**State Impact**: Connection transitions to `Failed` state

**Gating**:
- Enabled: Only during `Initializing` state
- Disabled: When entering `Closing` state (global shutdown takes precedence)

**Example**:
```
T0: Spawn server, send initialize request
    → Initialization timeout: 60s timer STARTS
    → Idle timeout: DISABLED (state = Initializing)

T8: Initialize response received
    → Initialization timeout: STOPPED
    → Connection state: Initializing → Ready
    → Idle timeout: ENABLED

Connection now ready for normal operations
```

**Failure Case**:
```
T0: Spawn server, send initialize
    → Initialization timeout: 60s timer STARTS

T60: Timeout fires (no initialize response)
    → Connection state: Initializing → Failed
    → Circuit breaker: record failure, backoff 500ms
    → Pool: retry with new server instance
```

### Tier 1: Per-Request Timeout (Application Layer)

**Scope**: Single upstream request (may fan out to multiple downstream servers)

**Duration**:
- Explicit requests (hover, definition): 5 seconds
- Incremental requests (completion): 2 seconds

**Trigger**: Only when n ≥ 2 downstream servers participate in aggregation

**Action**:
- Return partial results if at least one server responded
- Return REQUEST_FAILED if all servers timed out

**State Impact**: NONE - Does not affect connection state

**Example**:
```
T0: Completion request → pyright + ruff
T5: Per-request timeout fires
    ├─ pyright: responded at T3 ✅
    ├─ ruff: timed out ❌
    └─ Return: {isIncomplete: true, items: [pyright items]}

Connection state: Still Ready (timeout is per-request, not per-connection)
```

### Tier 2: Idle Timeout (Connection Health)

**Scope**: Per-connection health monitoring

**Duration**: 30-120 seconds (implementation-defined)

**Trigger**: State-based (per ADR-0013 Amendment 002)
- **Enabled**: Ready state + pending requests > 0
- **Disabled**: Initializing, Quiescent (no pending), Closing, Failed, Closed

**Action**:
1. Close connection
2. Fail all pending requests with INTERNAL_ERROR
3. Transition connection state: Ready → Failed
4. Trigger circuit breaker (per ADR-0015)
5. Connection pool spawns new instance

**State Impact**: Transitions to Failed

**Example**:
```
T0: Send hover request (pending: 0→1)
T1: Idle timer starts (30s)
T2: No response from server
T30: Idle timeout fires
     ├─ Fail pending request: INTERNAL_ERROR
     ├─ State: Ready → Failed
     └─ Circuit breaker: record failure

T31: Pool spawns new server instance
```

### Tier 3: Global Shutdown Timeout (System Layer)

**Scope**: All connections during shutdown

**Duration**: 5-15 seconds (implementation-defined)

**Trigger**: When shutdown initiated

**Action**:
1. Force kill all remaining server processes (SIGTERM → SIGKILL)
2. Fail all pending operations across all connections
3. Transition all connections: Any state → Closed

**State Impact**: Overrides all other timeouts (highest priority)

**Example**:
```
T0: Shutdown initiated (3 connections)
    └─ Global timeout: 10s timer STARTS

T5: pyright shutdown complete (Ready → Closed)
T6: ruff shutdown complete (Ready → Closed)
T10: lua-ls still hung → FORCE KILL
     ├─ SIGTERM sent
     ├─ Wait 1s
     └─ SIGKILL (guaranteed termination)

All connections: → Closed
```

## Interaction Rules

### Rule 1: Normal Operation (No Shutdown)

**Scenario**: Per-request timeout fires before idle timeout

```
Request sent → Per-request timeout (5s) → Idle timeout (30s)
                        ↓
                Return partial results
                Idle timer: RESET (activity detected)
```

**Behavior**:
- Per-request timeout returns partial results
- Idle timer resets on ANY stdout activity (response or notification)
- Connection remains in Ready state

### Rule 2: Shutdown Without Pending Requests

**Scenario**: Graceful shutdown with no pending operations

```
Connection state: Ready (quiescent, no pending requests)
Shutdown signal → Idle timeout: DISABLED
              → Per-request timeout: N/A (no requests)
              → Global timeout: ONLY timeout active
```

**Behavior**:
- Idle timeout disabled (per ADR-0013 Amendment 002)
- Only global timeout enforces bounded shutdown time

### Rule 3: Shutdown With Pending Requests

**Scenario**: Shutdown initiated while requests pending

```
T0: Request sent (pending: 1)
T1: Shutdown signal → State: Ready → Closing
T2: Idle timeout: DISABLED (state = Closing)
T3: Per-request timeout: Still running
T4: Global timeout: Starts (10s)

Precedence: Global timeout > Per-request timeout
```

**Behavior**:
- Idle timeout **STOPS** when entering Closing state (Duration::MAX)
- Per-request timeout continues but is bounded by global timeout
- Global timeout enforces hard deadline for ALL pending operations

**Timeline Example**:
```
T0: Per-request timeout would fire at T+5s
T2: Shutdown initiated (global timeout 10s)
T5: Per-request timeout fires (local to aggregation layer)
    └─ Returns partial results OR fails request
T7: Response arrives (late, after per-request timeout)
    └─ ACCEPT and deliver (before global timeout)
    └─ Resets idle timer (but idle timer already stopped, no effect)
T12: Global timeout fires (from T2)
     └─ Force kill all remaining servers
     └─ Fail all pending operations
```

### Rule 4: Late Response During Shutdown

**Scenario**: Response arrives after per-request timeout but before global timeout

**Decision**: **ACCEPT** late responses until global timeout

**Rationale**:
- Response provides useful information (better than nothing)
- Server is responsive (not hung, just slow)
- Late response resets idle timeout (serves as heartbeat)
  - (In Closing state, idle timeout is STOPPED, so reset has no effect)
- Global timeout still enforces hard deadline

**Example**:
```
T0: Request sent
T2: Shutdown → Closing state
T5: Per-request timeout fires → partial results returned
T7: Late response arrives → DELIVER (update partial results? no, already returned)
    Actually: Late response DISCARDED by aggregator (already returned partial)
T12: Global timeout → force kill

Clarification: Late responses are discarded at aggregation layer but
               do NOT hang the system (reader task processes them normally)
```

**Correction**: Late responses after per-request timeout are discarded by the aggregation layer (already returned partial results to client), but reader task continues processing them normally (doesn't block or hang).

### Rule 5: Shutdown During Initialization

**Scenario**: Shutdown initiated while connection is still initializing

```
T0: Server spawned, initialize request sent
    → Connection state: Initializing
    → Initialization timeout: 60s timer STARTS
    → Idle timeout: DISABLED

T5: Shutdown signal received
    → Connection state: Initializing → Closing
    → Initialization timeout: CANCELLED
    → Global shutdown timeout: 10s timer STARTS

Active timeouts: Global Shutdown (10s)
Precedence: Global Shutdown > Initialization
```

**Behavior**:
- Initialization timeout **CANCELLED** when entering Closing state
- Global shutdown timeout takes over as the only active timeout
- Connection immediately begins shutdown sequence:
  1. Skip LSP shutdown/exit (not yet initialized)
  2. Send SIGTERM to server process
  3. Wait for graceful deadline (80% of global timeout)
  4. Send SIGKILL if process still running

**Rationale**:
- Server is not yet ready for LSP protocol commands (no initialized state)
- Graceful LSP shutdown not applicable (requires initialized server)
- Force termination via SIGTERM/SIGKILL is appropriate
- Global timeout ensures bounded shutdown time even for stuck initialization

**Example Timeline**:
```
T0: Initialize request sent
    → State: Initializing
    → Init timeout: 60s
    → Idle timeout: DISABLED

T5: Shutdown signal
    → State: Closing
    → Init timeout: CANCELLED
    → Global timeout: 10s STARTS

T5: Begin shutdown sequence
    → Skip LSP shutdown (not initialized)
    → Send SIGTERM

T13: Graceful deadline (80% of 10s = 8s)
    → If process alive: send SIGTERM

T15: Global timeout fires (10s from T5)
    → If process alive: send SIGKILL
    → State: Closed
```

## State-Based Idle Timeout Lifecycle

Per ADR-0013 Amendment 002, idle timeout computation is state-based:

```rust
fn compute_idle_timeout(state: ConnectionState, pending_count: usize) -> Duration {
    match (state, pending_count) {
        (Initializing, _) => Duration::MAX,  // Disabled during init
        (Ready, 0) => Duration::MAX,         // Disabled when quiescent
        (Ready, _) => Duration::from_secs(IDLE_TIMEOUT),  // Active when pending > 0
        (Closing | Failed | Closed, _) => Duration::MAX,  // Disabled in terminal states
    }
}
```

**Guarantee**: Idle timeout NEVER fires during:
- Initialization (separate initialization timeout applies)
- Quiescent state (no pending requests)
- Shutdown (global timeout applies)
- Failed or Closed states (connection unusable)

## Timeout Precedence Summary Table

| Scenario | Active Timeouts | Precedence | Final Action |
|----------|----------------|------------|--------------|
| **Initialization** | Initialization only | N/A | Connection → Failed, retry with backoff |
| **Normal operation** | Per-request, Idle | Per-request → Idle resets | Partial results, connection stays Ready |
| **Single-server request** | Idle only | N/A (only one timeout) | Connection → Failed on timeout |
| **Shutdown (no pending)** | Global only | N/A | Clean shutdown or force kill |
| **Shutdown (pending)** | Per-request, Global | Global > Per-request | Force kill after global timeout |
| **Shutdown (initializing)** | Global only | Global > Initialization | Skip LSP shutdown, SIGTERM/SIGKILL |
| **Late response in shutdown** | Global only | Accept until global timeout | Deliver if before global timeout |

## Configuration Recommendations

### Timeout Values by Layer

| Timeout Type | Recommended Duration | Rationale |
|-------------|---------------------|-----------|
| **Per-Request** | 5s explicit, 2s incremental | User-facing latency bound, balance responsiveness vs false positives |
| **Idle** | 30-120s | Detect hung servers without false positives on slow operations |
| **Global Shutdown** | 5-15s | Total shutdown time, balance clean exit vs user wait |
| **Initialization** | 30-60s | Heavy servers (rust-analyzer) need time to index |

### Relationships

```
Initialization (60s)  > Idle (30-120s) > Per-request (5s)
Global Shutdown (10s) overrides Idle (any duration)

Note: Per-request timeout is NOT part of shutdown sequence
      (requests failed immediately during shutdown per ADR-0016 § 3)
```

### Constraint

**Design: Concurrent Global Timeout (User-Bounded)**

The global timeout is a **single ceiling** for the entire shutdown process. All phases (graceful, SIGTERM, SIGKILL) execute **concurrently within** this deadline, not sequentially.

```
Shutdown Timeline (Concurrent):

T0: Start global timeout (e.g., 10s), begin graceful shutdown
    ├─ Writer idle synchronization: ~2s
    ├─ LSP shutdown request/response: ~3-5s
    └─ LSP exit notification + wait for process

T0-T8: Graceful attempts (dynamically determined)
       If successful → Done early (total time: actual, not full timeout)

T8: Graceful timeout → Send SIGTERM (escalation heuristic)
    Reserve ~20% of global timeout for SIGTERM/SIGKILL

T8-T10: Wait for SIGTERM to work (~2s remaining)

T10: Global timeout expires → Send SIGKILL immediately
     Guaranteed termination

Maximum total shutdown time = Global Timeout (exactly)
```

**Constraint**:
```
Global Shutdown ≥ 1s (minimum practical value)

Recommended: 5-15s
  - 5s: Minimum for graceful LSP handshake attempt
  - 10s: Balanced (8s graceful + 2s SIGTERM)
  - 15s: Conservative (12s graceful + 3s SIGTERM)

Escalation heuristic: Reserve 20% of global timeout for SIGTERM
  - Global 10s → Graceful deadline 8s, SIGTERM reserve 2s
  - Global 5s → Graceful deadline 4s, SIGTERM reserve 1s
```

**Why Concurrent Design**:
- **User experience**: Guaranteed maximum wait time (critical for editor responsiveness)
- **Simplicity**: Single timer, one deadline, easier to implement and test
- **Parallel efficiency**: Multiple servers shut down concurrently (10s total, not N×10s)
- **Early completion**: If graceful succeeds quickly, total time is actual not timeout
- **Safety trade-off**: Language servers must handle abrupt termination (they already do for crashes)

**Rejected Alternative**: Sequential timeouts (graceful timeout → SIGTERM timeout → SIGKILL) would guarantee minimum time per phase but create unbounded total time (bad UX: 3 servers × 7s = 21s wait).

Recommended: 10s global timeout for balance between graceful exit opportunity and user responsiveness.

## Implementation Requirements

### Reader Task State-Based Timeout

```rust
async fn reader_task(
    stdout: ChildStdout,
    pending: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
    connection_state: Arc<AtomicConnectionState>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        // Compute idle timeout based on current state and pending count
        let idle_timeout = compute_idle_timeout(
            connection_state.get(),
            pending.len()
        );

        select! {
            result = read_message(&mut reader) => {
                // Process message...
            }
            _ = &mut shutdown_rx => {
                cleanup_and_fail_pending(&pending, "Connection shutting down");
                break;
            }
            _ = tokio::time::sleep(idle_timeout) => {
                // Guard: Only fire if still in Ready state with pending requests
                if connection_state.get() == ConnectionState::Ready && pending.len() > 0 {
                    log::warn!("Idle timeout fired");
                    cleanup_and_fail_pending(&pending, "Server idle timeout");
                    break;
                }
                // Else: Spurious timeout (state changed), continue
            }
        }
    }
}
```

### Aggregator Per-Request Timeout

```rust
async fn aggregate_responses(
    &self,
    upstream_id: i64,
    downstream_requests: Vec<(String, i64)>,  // (server_key, downstream_id)
) -> Result<Value> {
    let timeout_duration = if downstream_requests.len() >= 2 {
        // Multi-server: Apply per-request timeout
        Duration::from_secs(5)
    } else {
        // Single-server: No per-request timeout (idle timeout handles it)
        Duration::MAX
    };

    match tokio::time::timeout(timeout_duration, self.collect_responses(downstream_requests)).await {
        Ok(responses) if responses.is_empty() => {
            // Total failure
            Err(ResponseError {
                code: ErrorCode::RequestFailed,
                message: "All language servers timed out".to_string(),
            })
        }
        Ok(responses) => {
            // Partial or complete success
            Ok(self.merge_responses(responses))
        }
        Err(_) => {
            // Timeout - return whatever we have
            let responses = self.collect_partial_responses();
            if responses.is_empty() {
                Err(ResponseError {
                    code: ErrorCode::RequestFailed,
                    message: "Request timeout, no responses received".to_string(),
                })
            } else {
                Ok(self.merge_responses(responses))
            }
        }
    }
}
```

### Shutdown Global Timeout

```rust
async fn shutdown_all_connections(connections: Vec<Connection>) -> Result<()> {
    let global_timeout = Duration::from_secs(GLOBAL_SHUTDOWN_TIMEOUT);

    tokio::time::timeout(global_timeout, async {
        // Shutdown all connections in parallel
        let shutdown_tasks = connections.iter()
            .map(|conn| async move {
                match conn.state() {
                    ConnectionState::Failed => {
                        // Fast path: Skip LSP handshake, cleanup only
                        conn.force_cleanup().await
                    }
                    _ => {
                        // Normal path: Graceful shutdown with LSP handshake
                        conn.graceful_shutdown().await
                    }
                }
            });

        futures::future::join_all(shutdown_tasks).await;
    }).await.unwrap_or_else(|_| {
        // Global timeout expired - force kill stragglers
        log::warn!("Global shutdown timeout expired, force killing remaining servers");
        force_kill_all(connections);
    });

    Ok(())
}
```

## Testing Requirements

### Integration Tests

1. **Test: Per-request timeout does not affect connection state**
   ```rust
   #[tokio::test]
   async fn test_per_request_timeout_preserves_connection() {
       // Setup: Two servers (pyright fast, ruff slow)
       // Action: Request completion
       // Assert: Per-request timeout fires at 5s
       // Assert: Returns partial results (pyright only)
       // Assert: Connection state still Ready (not Failed)
       // Assert: Subsequent requests work normally
   }
   ```

2. **Test: Idle timeout disabled during shutdown**
   ```rust
   #[tokio::test]
   async fn test_idle_timeout_stops_during_shutdown() {
       // Setup: Connection with pending request
       // Setup: Idle timeout = 5s
       // Action: Initiate shutdown
       // Wait: 10 seconds
       // Assert: Idle timeout did NOT fire
       // Assert: Global timeout handles cleanup
   }
   ```

3. **Test: Global timeout overrides per-request timeout**
   ```rust
   #[tokio::test]
   async fn test_global_timeout_precedence() {
       // Setup: Request with 10s per-request timeout
       // Action: Shutdown with 3s global timeout
       // Assert: Shutdown completes at T+3s (not T+10s)
       // Assert: Request failed with "connection closing"
   }
   ```

4. **Test: Late response accepted before global timeout**
   ```rust
   #[tokio::test]
   async fn test_late_response_during_shutdown() {
       // Setup: Request sent, shutdown initiated
       // Action: Response arrives after per-request timeout but before global timeout
       // Assert: Response processed normally (reader doesn't hang)
       // Assert: Aggregator discards (already returned partial)
       // Assert: Shutdown completes normally
   }
   ```

## LSP Protocol Compliance

**LSP Spec**: Does not mandate specific timeout values, but requires:
- ✅ Every request receives exactly one response
- ✅ Cancellation is best-effort (server may complete before cancel)

**Compliance**: This ADR ensures:
- ✅ Timeouts trigger explicit error responses (not silent hangs)
- ✅ Per-request timeout returns partial results (user gets something)
- ✅ Idle timeout detects hung servers (prevents indefinite waits)
- ✅ Global timeout enforces bounded shutdown (system always terminates)

## Coordination With Other ADRs

### ADR-0013 (Async I/O Layer)

- **Updated**: Idle timeout lifecycle now state-based (Amendment 002)
- **Ensures**: Idle timeout STOPS during Closing state
- **Guarantees**: Reader task cleanup on all timeout paths

### ADR-0014 (Message Ordering)

- **Connection state**: Used to compute idle timeout duration
- **State transitions**: Failed state triggered by idle timeout
- **Pending operations**: Failed when idle timeout fires

### ADR-0015 (Multi-Server)

- **Per-request timeout**: Only applies to multi-server aggregation (n≥2)
- **Partial results**: Returned when per-request timeout fires
- **Circuit breaker**: Triggered by idle timeout (connection-level failure)

### ADR-0016 (Graceful Shutdown)

- **Global timeout**: Enforces bounded shutdown time
- **Idle timeout disabled**: When entering Closing state
- **Pending operations**: Failed by global timeout if still pending

## Migration Notes

**For Existing Implementations**:

1. Update reader task to compute idle timeout based on connection state
2. Ensure idle timeout stops (Duration::MAX) when state is Closing
3. Add per-request timeout only for multi-server aggregation (n≥2)
4. Document timeout values in configuration
5. Test timeout interaction scenarios (shutdown during timeout, late responses)

**Backward Compatibility**: Internal change, no API impact.

## Summary

**Decision**: Four-tier timeout hierarchy with explicit precedence rules

**Tiers**:
0. **Initialization** (startup): Bounds server initialization time during connection startup
1. **Per-Request** (application): Bounds user-facing latency for multi-server aggregation
2. **Idle** (connection): Detects hung servers when requests are pending
3. **Global Shutdown** (system): Enforces bounded shutdown time across all connections

**Precedence Rules**:
- Global shutdown > Initialization timeout (shutdown during init)
- Global shutdown > Per-request timeout (shutdown during request)
- Global shutdown > Idle timeout (idle timeout STOPS during shutdown)
- Idle timeout DISABLED during Initializing state (initialization timeout applies)
- Late responses accepted until global timeout

**Impact**:
- ✅ Deterministic timeout behavior
- ✅ Clear precedence when multiple timeouts active
- ✅ Bounded shutdown time guaranteed
- ✅ No timeout interaction races

**Effort**: Low - clarifies existing timeout systems, no new mechanisms

**Risk**: Low - strictly improves determinism, no breaking changes

**Priority**: CRITICAL - Required for predictable system behavior

---

**Author**: Architecture Review Team
**Reviewers**: (pending)
**Implementation**: Required before Phase 1
