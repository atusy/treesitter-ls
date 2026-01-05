# ADR-0016: Graceful Shutdown and Connection Lifecycle

## Status

Accepted

## Context

ADR-0013 (Async I/O Layer), ADR-0014 (Actor-Based Message Ordering), and ADR-0015 (Multi-Server Coordination) establish the communication architecture but do not specify shutdown behavior. This creates critical gaps:

1. **No LSP shutdown handshake**: LSP protocol requires `shutdown` request → `exit` notification sequence for clean server termination
2. **Undefined operation disposal**: What happens to pending operations, coalescing map contents, and queued requests during shutdown?
3. **No state for shutdown-in-progress**: ConnectionState (Initializing/Ready/Failed) has no "shutting down" state, creating race conditions for operations arriving during shutdown
4. **Multi-connection coordination unspecified**: How to shut down multiple concurrent language servers (parallel vs sequential, timeout handling)

Without graceful shutdown specification:
- Servers may not flush buffers or save state
- Operations hang indefinitely or receive no error response
- Process cleanup may leak resources (zombie processes, lock files)
- LSP protocol violations may corrupt server caches

## Decision

**Implement two-tier graceful shutdown with LSP protocol compliance and fail-fast operation disposal.**

### 1. Extend ConnectionState Enum

Add `Closing` and `Closed` states to ADR-0014's ConnectionState:

```rust
enum ConnectionState {
    Initializing,  // Writer loop started, initialization in progress
    Ready,         // Initialization completed successfully
    Failed,        // Initialization failed or writer loop panicked
    Closing,       // Shutdown initiated, draining operations
    Closed,        // Connection fully terminated
}
```

**State Transitions**:
```
Ready → Closing          (graceful shutdown initiated)
Initializing → Closing   (abort initialization, shutdown)
Closing → Closed         (shutdown completed or timed out)
Failed → Closed          (skip shutdown handshake, cleanup only)
```

**Operation Gating in Closing State**:
- New operations: Reject with `SERVER_NOT_INITIALIZED` error
- Coalescing map: Fail all operations with `REQUEST_FAILED`
- Order queue: Continue draining (send queued operations)
- Pending responses: Wait for responses up to global timeout

### 2. LSP Shutdown Handshake Sequence

Follow LSP specification's two-phase shutdown:

```
┌─────────────────────────────────────────────────────────────┐
│ Phase 1: Graceful Shutdown                                  │
│ ──────────────────────────────────────────────────────────  │
│ 1. Transition to Closing state                              │
│ 2. Fail all operations in coalescing map (REQUEST_FAILED)   │
│ 3. Send LSP shutdown request to server                      │
│ 4. Wait for shutdown response (until global timeout)        │
│ 5. Send LSP exit notification                               │
│ 6. Wait for process exit (until global timeout)             │
│ 7. Transition to Closed state                               │
└─────────────────────────────────────────────────────────────┘
                           │
                           │ Timeout expires
                           ▼
┌─────────────────────────────────────────────────────────────┐
│ Phase 2: Forced Shutdown                                    │
│ ──────────────────────────────────────────────────────────  │
│ 1. Send SIGTERM to server process                           │
│ 2. Wait for process death (implementation-defined timeout)  │
│ 3. Send SIGKILL if still alive                              │
│ 4. Transition to Closed state                               │
└─────────────────────────────────────────────────────────────┘
```

**Exception: Failed State Bypass**:
```
Failed → Closed (skip LSP handshake)
├─ stdin unavailable (writer loop panicked or process crashed)
├─ Send SIGTERM immediately
└─ Wait for process exit, then SIGKILL if needed
```

### 3. Operation Disposal Policy: Fail Immediately

**Decision**: Fail all in-flight operations immediately when shutdown begins.

**Rationale**:
- **Predictable latency**: Bounded shutdown time, no waiting for slow servers
- **Clear error semantics**: Operations receive explicit failure, not timeout
- **Simplicity**: No complex draining logic or partial completion tracking

**Operation Handling**:

| Operation Location | Shutdown Action |
|-------------------|----------------|
| **Coalescing map** | Fail with `REQUEST_FAILED` ("connection closing") |
| **Order queue** | Continue processing (already dequeued, may be mid-write) |
| **Pending responses** | Fail with `REQUEST_FAILED` when timeout expires |
| **New operations** | Reject with `SERVER_NOT_INITIALIZED` (Closing state) |

**Why fail coalescing map but not order queue**: Operations in the order queue may be partially written to stdin. Aborting mid-write corrupts the protocol stream. Coalescing map operations haven't been serialized yet—safe to fail.

### 4. Shutdown Timeout Policy: Implementation-Defined

**Global timeout**: Implementation-defined duration (typically 5-15 seconds) for entire shutdown sequence across all connections.

**Rationale for global timeout**:
- Multi-server coordination requires bounded total time
- User experience: Shutdown shouldn't hang indefinitely
- Per-server timeout could multiply (5 servers × 5s = 25s unacceptable)
- Fast servers don't wait for slow servers to time out

**Timeout application**:
```rust
// Conceptual implementation
async fn shutdown_all_connections(connections: Vec<Connection>) {
    let global_timeout = Duration::from_secs(IMPL_DEFINED);

    tokio::time::timeout(global_timeout, async {
        // Shutdown all connections in parallel
        let shutdown_tasks = connections.iter()
            .map(|conn| conn.graceful_shutdown());

        futures::future::join_all(shutdown_tasks).await;
    }).await.unwrap_or_else(|_| {
        // Global timeout expired - force kill remaining
        force_kill_all(connections);
    });
}
```

### 5. Initialization Shutdown: Abort Immediately

**Decision**: Abort initialization and proceed to shutdown.

**Sequence**:
```
Connection state: Initializing
Shutdown signal arrives
├─ Transition: Initializing → Closing
├─ Fail pending initialization request (if sent)
├─ Send exit notification (skip shutdown request - server not initialized)
├─ Kill process (SIGTERM → SIGKILL)
└─ Transition: Closing → Closed
```

**Rationale**:
- Initialization may hang (slow server, network issue)
- Waiting for initialization during shutdown adds unbounded latency
- Server hasn't completed initialization—LSP shutdown request invalid
- Exit notification sufficient for cleanup

**Edge case**: Initialization response arrives during shutdown:
```
T0: Shutdown initiated (Initializing → Closing)
T1: Initialization response arrives from server
    └─ Discard response (connection already Closing)
    └─ Continue shutdown sequence (send exit, kill process)
```

### 6. Multi-Connection Shutdown: Parallel with Global Timeout

**Decision**: Shut down all connections in parallel with single global timeout.

**Coordination Strategy** (ADR-0015 integration):

```rust
// Router shutdown sequence
async fn shutdown_router() {
    // 1. Stop accepting new requests
    mark_router_shutting_down();

    // 2. Fail all pending routing decisions
    fail_pending_routes();

    // 3. Shutdown all connections in parallel
    let all_connections = connection_pool.all_connections();

    tokio::time::timeout(GLOBAL_TIMEOUT, async {
        let tasks = all_connections.iter()
            .map(|conn| async move {
                match conn.state() {
                    Failed => conn.cleanup_only(),  // Skip LSP handshake
                    _ => conn.graceful_shutdown(),   // Full LSP sequence
                }
            });

        futures::future::join_all(tasks).await;
    }).await.unwrap_or_else(|_| {
        // Global timeout - force kill stragglers
        log::warn!("Shutdown timeout - force killing remaining servers");
        force_kill_all(all_connections);
    });

    // 4. Clean up router resources
    cleanup_router_state();
}
```

**Why parallel**:
- **Bounded total time**: N servers shut down in O(1) time, not O(N)
- **Independent failures**: Hung server doesn't block others
- **User experience**: 3 servers × 5s sequential = 15s vs 5s parallel

**Exception handling**:
- If server A hangs during shutdown, it doesn't delay server B
- Global timeout ensures all servers forced-killed if needed
- Failed connections skip LSP handshake (fast path)

## Consequences

### Positive

**LSP Protocol Compliance**:
- Servers receive proper shutdown request → exit notification sequence
- Allows servers to flush buffers, save state, release locks
- Prevents cache corruption from abrupt termination

**Bounded Shutdown Latency**:
- Global timeout ensures shutdown completes in bounded time
- Fail-fast operation disposal (no draining) prevents hang
- Parallel multi-connection shutdown: O(1) not O(N)

**Clear Error Semantics**:
- Operations in flight receive explicit errors, not timeout
- New operations rejected immediately during shutdown (Closing state)
- Users see "connection closing" error, not silent hang

**Resource Cleanup**:
- SIGTERM → SIGKILL sequence ensures process termination
- No zombie processes or leaked file descriptors
- Lock files and caches properly released

**Multi-Server Resilience**:
- Hung server doesn't block shutdown of healthy servers
- Failed connections use fast path (skip LSP handshake)
- Global timeout prevents indefinite hang

### Negative

**No Operation Draining**:
- Operations in coalescing map never reach server (failed immediately)
- May surprise users expecting "finish pending work"
- Trade-off: Predictable shutdown time vs completion

**Failed Connections Bypass LSP**:
- Servers with Failed state don't receive shutdown request
- May leave caches in inconsistent state
- Mitigation: Servers should handle abrupt termination (crash recovery)

**Global Timeout Pressure**:
- Fast servers must wait for slow servers (up to timeout)
- Very slow servers force-killed even if making progress
- Alternative (per-server timeout) has worse UX (unbounded total time)

**Initialization Abort Abrupt**:
- Servers in Initializing state killed without completing setup
- May leave partial initialization state
- Trade-off: Shutdown latency vs initialization completion

### Neutral

**Implementation-Defined Timeout**:
- Flexibility for different deployment scenarios
- Must be documented/configurable for operators

**Closing State Overhead**:
- Adds complexity to state machine
- Necessary to prevent shutdown race conditions

## Implementation Guidance

### Phase 1: ConnectionState Extension

**Scope**: Add Closing/Closed states to ADR-0014's state machine.

**Key Changes**:
- Extend ConnectionState enum with Closing/Closed
- Add state transitions (Ready→Closing, Closing→Closed, Failed→Closed)
- Reject new operations in Closing state
- Fail coalescing map operations on transition to Closing

**Exit Criteria**:
- State machine correctly rejects operations during shutdown
- Coalescing map contents failed with REQUEST_FAILED
- Tests verify: operation during Closing returns error

### Phase 2: LSP Shutdown Handshake

**Scope**: Implement two-phase shutdown sequence.

**Key Changes**:
- Send LSP shutdown request when entering Closing state
- Wait for shutdown response (with timeout)
- Send LSP exit notification
- Wait for process exit
- Force kill on timeout (SIGTERM → SIGKILL)

**Exit Criteria**:
- LSP shutdown sequence completes for healthy servers
- Timeout triggers forced shutdown
- Failed state bypasses LSP handshake
- Process cleanup verified (no zombies)

### Phase 3: Multi-Connection Coordination

**Scope**: Parallel shutdown with global timeout (ADR-0015 integration).

**Key Changes**:
- Router shutdown broadcasts to all connections
- Parallel execution of shutdown tasks
- Global timeout wrapper
- Force kill stragglers on timeout

**Exit Criteria**:
- Multiple servers shut down in parallel
- Global timeout enforced
- Hung server doesn't block others
- Resource cleanup verified

## Related ADRs

- **[ADR-0013](0013-async-io-layer.md)**: Async I/O layer
  - Uses shutdown signal from `select!` pattern
  - ADR-0016 adds LSP handshake and process cleanup
- **[ADR-0014](0014-actor-based-message-ordering.md)**: Actor-based message ordering
  - Extends ConnectionState enum with Closing/Closed states
  - Defines operation disposal for coalescing map and pending requests
- **[ADR-0015](0015-multi-server-coordination.md)**: Multi-server coordination
  - ADR-0016 defines router shutdown coordination strategy
  - Parallel shutdown with global timeout

## References

**LSP Specification**: [Shutdown Request](https://microsoft.github.io/language-server-protocol/specifications/lsp/3.17/specification/#shutdown)
- Servers must receive `shutdown` request before `exit` notification
- Servers use shutdown phase to flush buffers and save state

**Process Management**: SIGTERM → SIGKILL pattern
- SIGTERM allows graceful cleanup
- SIGKILL guarantees termination (last resort)
