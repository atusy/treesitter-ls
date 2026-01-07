# ADR-0014 Amendment 001: ConnectionState Machine Completeness

**Date**: 2026-01-06
**Status**: Proposed
**Amends**: [ADR-0014](0014-actor-based-message-ordering.md) § Connection State Tracking (lines 98-125), § Fail-Fast Error Handling (lines 206-218)
**Related**: [ADR-0016](0016-graceful-shutdown.md) § Extend ConnectionState Enum (lines 31-52)

## Issues Addressed

This amendment resolves three categories of state machine issues:

1. **State Definition Inconsistency**: `Closing` state comment differs between ADR-0014 and ADR-0016
2. **Missing State Transitions**: Undefined transitions (Failed→Closing, Closing→Failed, edge cases)
3. **Error Handling Gaps**: Panic handler mechanism unspecified

---

## Issue 1: State Definition Inconsistency

**ADR-0014 line 103**:
```rust
Closing,       // Graceful shutdown in progress (see ADR-0016)
```

**ADR-0016 line 35**:
```rust
Closing,       // Shutdown initiated, draining operations
```

**Problem**: "Draining operations" is misleading. Per ADR-0016 line 50, coalescing map operations are **failed immediately**, not drained. Only the order queue continues processing (and even that will be stopped per Amendment 0016-001).

**Inconsistency Impact**: LOW - semantic difference minor, but causes confusion about what "draining" means.

---

## Issue 2: Missing State Transitions

### 2.1 Missing: Failed → Closing

**Current State Transitions** (ADR-0014 lines 109-116):
```
Initializing → Ready     (initialization succeeds)
Initializing → Failed    (initialization fails or times out)
Initializing → Closing   (shutdown during initialization, see ADR-0016)
Ready → Failed           (writer loop panics or server crashes)
Ready → Closing          (graceful shutdown initiated, see ADR-0016)
Closing → Closed         (shutdown completed)
Failed → Closed          (cleanup after failure, skip graceful shutdown)
```

**Missing**: Failed → Closing

**Scenario**:
```
T0: Connection state: Ready
T1: Writer loop panics → Failed state
T2: Global shutdown signal arrives
T3: What happens?
    - ADR-0014 says: Failed → Closed (skip graceful shutdown)
    - ADR-0016 expects: All connections go through Closing state
```

**Question**: Should Failed connections transition to Closing during shutdown, or bypass directly to Closed?

### 2.2 Undefined: Closing → Failed

**Scenario**: Writer loop panics **during shutdown** (while in Closing state):

```
T0: Connection state: Closing (shutdown sequence started)
T1: Writer loop panics while processing remaining queue items
T2: ADR-0014 § Fail-Fast says: "Connection state transitions to Failed"
T3: But we're already in Closing state - do we rollback?
```

**Conflict**:
- ADR-0014 line 213: "Connection state transitions to `Failed`"
- ADR-0016 line 44: "Closing → Closed (shutdown completed or timed out)"

**Impact**: State machine deadlock - should final state be Failed or Closed?

### 2.3 Edge Case: Initialization Failure During Shutdown

**Scenario**:
```
T0: Connection state: Initializing
T1: Shutdown signal arrives → Closing state (ADR-0016 line 43)
T2: Initialization timeout fires (ADR-0013 Amendment 002)
T3: Should state be:
    a) Failed (initialization failed)
    b) Closing (shutdown already in progress)
```

---

## Issue 3: Error Handling Gaps - Panic Handler Mechanism

**ADR-0014 line 211-213**:
```
Strategy:
- Panic caught, all pending operations failed with INTERNAL_ERROR
- Connection state transitions to `Failed` (triggers circuit breaker)
```

**Missing**:
1. **WHO catches the panic?** (writer loop wrapper? spawn wrapper?)
2. **WHERE are pending operations stored?** (what data structure?)
3. **WHEN does state transition happen?** (before or after failing operations?)
4. **HOW to ensure response guarantee?** (what if send fails?)

**Impact**: Implementation ambiguity - different interpretations could lead to hangs or missed error responses.

---

## Amendments

### Amendment 1: Standardize State Comments

**Replace ADR-0014 line 103**:
```rust
Closing,       // Shutdown initiated, failing pending operations
```

**Replace ADR-0016 line 35**:
```rust
Closing,       // Shutdown initiated, failing pending operations
```

**Rationale**: Accurate description - coalescing map operations failed immediately, not drained.

---

### Amendment 2: Complete State Transition Matrix

**Replace ADR-0014 lines 108-117** with complete transition matrix:

```
**State Transitions** (Complete):

Normal Operation:
  Initializing → Ready     (initialization succeeds)
  Initializing → Failed    (initialization fails, times out, or server crashes)

Shutdown Paths:
  Initializing → Closing   (shutdown during initialization)
  Ready → Closing          (graceful shutdown initiated)
  Failed → Closing         (graceful shutdown initiated on failed connection)
  Closing → Closed         (shutdown completed or timed out)

Failure Paths:
  Ready → Failed           (writer loop panics or server crashes)
  Closing → Closed         (writer panic during shutdown - skip Failed state)
  Failed → Closed          (cleanup without shutdown, if shutdown not initiated)

Edge Cases:
  Initializing + Shutdown + Timeout → Closing (not Failed)
    Rationale: Shutdown in progress; initialization failure is moot

Priority Rules (when multiple transitions possible):
  1. If current state = Closing: All failures → Closed (skip Failed)
  2. If shutdown signal + current state = Failed: Failed → Closing → Closed
  3. Else: Normal transition rules apply
```

**Visual State Diagram**:
```
                    ┌─────────────┐
                    │Initializing │
                    └──────┬──────┘
                           │
          ┌────────────────┼────────────────┐
          │                │                │
       success         timeout/          shutdown
          │             failure          signal
          ▼                │                │
     ┌────────┐            │                │
     │ Ready  │            │                │
     └───┬────┘            │                │
         │                 │                │
    ┌────┼─────────────────┼────────────────┤
    │    │                 │                │
shutdown crash/         crash/              │
 signal  panic          panic               │
    │    │                 │                │
    ▼    ▼                 ▼                ▼
┌──────────┐          ┌────────┐      ┌─────────┐
│ Closing  │◄─────────┤ Failed │      │ (abort) │
└────┬─────┘ shutdown └────┬───┘      └─────────┘
     │        signal        │
     │                      │ no shutdown
     │                      │ (cleanup only)
     │                      │
     ▼                      ▼
┌──────────┐          ┌──────────┐
│  Closed  │◄─────────┤  Closed  │
└──────────┘          └──────────┘

Priority Rule: Closing state is "sticky" - once entered, all failures
               skip Failed state and go directly to Closed.
```

---

### Amendment 3: Explicit Panic Handler Specification

**Add after ADR-0014 line 217**:

```
### Panic Handler Implementation Requirements

**Scope**: Writer loop panic must be caught and handled without hanging.

**Architecture**:
```rust
// Connection spawns writer loop with panic handler
pub struct Connection {
    writer_handle: JoinHandle<Result<()>>,
    pending_operations: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
    state: Arc<AtomicConnectionState>,
    circuit_breaker: Arc<CircuitBreaker>,
}

impl Connection {
    pub fn spawn_writer_loop(&self) -> JoinHandle<Result<()>> {
        let pending = self.pending_operations.clone();
        let state = self.state.clone();
        let circuit_breaker = self.circuit_breaker.clone();

        tokio::spawn(async move {
            // Panic handler wrapper
            let result = AssertUnwindSafe(writer_loop(/* ... */))
                .catch_unwind()
                .await;

            match result {
                Ok(Ok(())) => {
                    log::debug!("Writer loop exited normally");
                }
                Ok(Err(e)) => {
                    log::error!("Writer loop error: {}", e);
                    Self::handle_writer_failure(pending, state, circuit_breaker).await;
                }
                Err(panic_info) => {
                    log::error!("Writer loop panicked: {:?}", panic_info);
                    Self::handle_writer_panic(pending, state, circuit_breaker).await;
                }
            }
        })
    }

    async fn handle_writer_panic(
        pending: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
        state: Arc<AtomicConnectionState>,
        circuit_breaker: Arc<CircuitBreaker>,
    ) {
        // CRITICAL: Fail pending operations BEFORE state transition
        // This ensures LSP response guarantee (every request gets response)

        let count = pending.len();
        log::warn!("Failing {} pending operations due to writer panic", count);

        // 1. Fail all pending operations (FIRST - response guarantee)
        for entry in pending.iter() {
            if let Some((_, response_tx)) = pending.remove(entry.key()) {
                let error = ResponseError {
                    code: ErrorCode::InternalError,
                    message: "Writer loop panicked".to_string(),
                    data: None,
                };

                // Send error response (ignore send failures)
                let _ = response_tx.send(Err(error));
            }
        }

        // 2. Check current state (SECOND - determines final state)
        let current_state = state.get();

        match current_state {
            ConnectionState::Closing => {
                // Exception: Panic during shutdown → Closed (not Failed)
                log::warn!("Writer panic during shutdown, transitioning to Closed");
                state.set(ConnectionState::Closed);
                // Do NOT record circuit breaker failure (shutdown already in progress)
            }
            _ => {
                // Normal: Panic → Failed
                log::error!("Writer panic, transitioning to Failed");
                state.set(ConnectionState::Failed);

                // 3. Trigger circuit breaker (THIRD - enables recovery)
                circuit_breaker.record_failure();
            }
        }
    }

    async fn handle_writer_failure(
        pending: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
        state: Arc<AtomicConnectionState>,
        circuit_breaker: Arc<CircuitBreaker>,
    ) {
        // Same as panic handler, but different error message
        // (Error return vs panic - both are failures)
        Self::handle_writer_panic(pending, state, circuit_breaker).await;
    }
}
```

**Guarantees**:
1. ✅ Every pending request receives error response (LSP compliance)
2. ✅ Responses sent BEFORE state transition (atomic from client perspective)
3. ✅ State transition respects Closing state priority (no rollback)
4. ✅ Circuit breaker triggered (enables automatic recovery)

**Why This Order Matters**:
```
CORRECT Order:
  1. Send error responses → Client sees errors
  2. Transition state → Pool sees failure
  3. Trigger circuit breaker → Recovery begins

WRONG Order (if state transition first):
  1. Transition state → Pool spawns new instance
  2. New instance starts accepting requests
  3. Send error responses ← Old responses mixed with new connection!
  4. Client confused (response for old connection on new connection)
```

**Special Case: Closing State Exception**:
```rust
if current_state == ConnectionState::Closing {
    // Don't transition to Failed - we're already shutting down
    state.set(ConnectionState::Closed);
    // Don't trigger circuit breaker - not a recoverable failure
}
```

**Rationale**:
- Shutdown already in progress → Failed state rollback creates confusion
- Circuit breaker shouldn't trigger during intentional shutdown
- Final state should be Closed (lifecycle complete) not Failed (needs recovery)
```

---

### Amendment 4: Error Code Corrections

**Issue**: ADR-0016 line 49 uses wrong error code for Closing state:
```
- New operations: Reject with `SERVER_NOT_INITIALIZED` error
```

**Problem**: `SERVER_NOT_INITIALIZED` (-32002) semantically means "initialize request not yet completed". But in Closing state, the server **was** initialized - it's now shutting down.

**Fix**: Replace ADR-0016 line 49:
```
- New operations: Reject with `REQUEST_FAILED` error ("connection closing")
```

**Update ADR-0014 Operation Gating** (add Closing state):

**Replace ADR-0014 lines 119-124** with:

```
**Operation Gating by Connection State**:

| State | New Notifications | New Requests | Rationale |
|-------|------------------|--------------|-----------|
| **Initializing** | Allow (per-document lifecycle gating) | Reject: `SERVER_NOT_INITIALIZED` (-32002) | Server not ready for requests |
| **Ready** | Allow | Allow | Normal operation |
| **Failed** | Reject | Reject: `REQUEST_FAILED` (-32803) | Connection unusable |
| **Closing** | Reject | Reject: `REQUEST_FAILED` (-32803, "closing") | Shutdown in progress |
| **Closed** | Reject | Reject: `REQUEST_FAILED` (-32803, "closed") | Connection terminated |

**Error Messages**:
- `SERVER_NOT_INITIALIZED`: "Server {name} still initializing ({elapsed}s)"
- `REQUEST_FAILED` (Failed): "Server {name} connection failed: {reason}"
- `REQUEST_FAILED` (Closing): "Server {name} is shutting down"
- `REQUEST_FAILED` (Closed): "Server {name} connection closed"
```

---

## Testing Requirements

### State Transition Tests

1. **Test: Failed → Closing → Closed (graceful shutdown of failed connection)**
   ```rust
   #[tokio::test]
   async fn test_failed_connection_graceful_shutdown() {
       // Setup: Connection in Failed state
       // Action: Global shutdown signal
       // Assert: State transitions Failed → Closing → Closed
       // Assert: Cleanup executed
   }
   ```

2. **Test: Closing state sticky (panic during shutdown → Closed not Failed)**
   ```rust
   #[tokio::test]
   async fn test_panic_during_shutdown_goes_to_closed() {
       // Setup: Connection in Closing state
       // Action: Writer loop panics
       // Assert: Final state = Closed (NOT Failed)
       // Assert: Circuit breaker NOT triggered
   }
   ```

3. **Test: Initialization timeout during shutdown → Closing (not Failed)**
   ```rust
   #[tokio::test]
   async fn test_init_timeout_during_shutdown() {
       // Setup: Connection Initializing, shutdown signal sent
       // Setup: State = Closing
       // Action: Initialization timeout fires
       // Assert: State remains Closing (NOT Failed)
       // Assert: Shutdown completes normally
   }
   ```

### Panic Handler Tests

4. **Test: Panic handler sends responses before state transition**
   ```rust
   #[tokio::test]
   async fn test_panic_handler_response_order() {
       // Setup: 5 pending requests
       // Action: Writer loop panics
       // Assert: All 5 responses received
       // Assert: All responses have INTERNAL_ERROR code
       // Assert: State transition happens after responses sent
   }
   ```

5. **Test: Panic handler response guarantee**
   ```rust
   #[tokio::test]
   async fn test_panic_every_request_gets_response() {
       // Setup: 100 pending requests
       // Action: Trigger writer panic
       // Assert: Exactly 100 error responses received
       // Assert: No orphaned requests
       // Assert: No timeout hangs
   }
   ```

### Error Code Tests

6. **Test: Correct error codes by state**
   ```rust
   #[tokio::test]
   async fn test_error_codes_by_state() {
       // Initializing: SERVER_NOT_INITIALIZED (-32002)
       // Failed: REQUEST_FAILED (-32803)
       // Closing: REQUEST_FAILED (-32803, message includes "closing")
       // Closed: REQUEST_FAILED (-32803, message includes "closed")
   }
   ```

---

## State Machine Completeness Checklist

✅ All state transitions defined (including edge cases)
✅ Priority rules specified (Closing state sticky)
✅ Error handling specified (panic handler mechanism)
✅ Response guarantees documented (fail pending before state transition)
✅ Error codes consistent (REQUEST_FAILED for Closing, not SERVER_NOT_INITIALIZED)
✅ State comments standardized (ADR-0014 and ADR-0016 match)
✅ Testing requirements defined (6 test scenarios)

---

## Coordination With Other ADRs

### ADR-0013 (Async I/O Layer)

- Reader task cleanup (Amendment 0013-001) complements writer panic handler
- Both ensure pending requests receive responses on abnormal exit
- Reader handles server crash; writer panic handler handles bridge crash

### ADR-0015 (Multi-Server Coordination)

- Circuit breaker triggered by panic handler (line 213)
- Pool spawns new instance when connection transitions to Failed
- Router distinguishes Failed (temporary) vs Closing (intentional shutdown)

### ADR-0016 (Graceful Shutdown)

- State transition matrix now complete (includes Failed → Closing)
- Panic during shutdown exception documented (Closing → Closed, not Failed)
- Error code corrected (REQUEST_FAILED, not SERVER_NOT_INITIALIZED)

---

## Summary

**Changes**:
1. Standardized `Closing` state comment across ADRs
2. Completed state transition matrix with priority rules
3. Specified panic handler implementation requirements
4. Corrected error codes for Closing state

**Impact**:
- Eliminates state machine ambiguity
- Prevents implementation divergence
- Ensures LSP response guarantee on panic
- Clear error semantics for users

**Effort**: Medium - panic handler wrapper, state transition logic updates

**Risk**: Low - strictly improves correctness, no breaking changes

**Priority**: HIGH - Required for Phase 1 (state machine must be well-defined)

---

**Author**: Architecture Review Team
**Reviewers**: (pending)
**Implementation**: Required before Phase 1
