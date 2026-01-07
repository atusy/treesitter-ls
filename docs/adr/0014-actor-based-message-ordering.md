# ADR-0014: Actor-Based Message Ordering for Bridge Architecture

## Status

Proposed

**Supersedes**:
- [ADR-0012](0012-multi-ls-async-bridge-architecture.md) § Timeout-based control
- [ADR-0009](0009-async-bridge-architecture.md): Original async architecture

## Context

### Problems with Timeout-Based Control

ADR-0012 established timeout-based control for initialization and request superseding. This approach has three fundamental problems:

**1. Time-Based Control Doesn't Reflect System State**

Timeouts create artificial ceilings unrelated to actual readiness:
- Fixed timeout dilemma: too short fails unnecessarily, too long wastes user time
- Server variability: lua-ls initializes in 100ms, rust-analyzer takes 5-10s
- No feedback: timeout expiry doesn't indicate when server will actually be ready

**2. Notification/Request Ordering Violation**

Separate code paths create race conditions:
```
Notifications → channel → forwarder
Requests     → direct call → connection

Result: Requests can arrive before notifications (completion on stale content)
```

This is hidden by content-hash URIs but becomes catastrophic with stable URIs (PBI-200).

**3. Complexity from Per-Type State Management**

Timeout tracking requires separate pending maps per request type (completion, hover, signature_help), each with its own timeout task, cancellation logic, and cleanup.

### Insight from Python Prototype

The `handler2.py` prototype demonstrates a simpler event-driven approach:
- Single-writer loop serializes all writes
- Generation counters enable superseding without timeouts
- Immediate REQUEST_CANCELLED feedback instead of timeout waits

**Key insight**: Superseding provides the bounded wait. Users either get the latest result (when ready) or immediate cancellation (if superseded).

## Decision

**Adopt actor-based message ordering with event-driven superseding**, structured around five architectural principles:

### 1. Single-Writer Loop per Connection (Actor Pattern)

Each server connection has exactly one writer task consuming from a unified queue, ensuring FIFO ordering for all operations.

**Architecture**:
```
All Operations → Unified Order Queue → Single Writer Loop → Server stdin
```

**Guarantees**:
- Strict FIFO ordering (notifications and requests maintain sequence)
- No byte-level corruption (single writer, no interleaving)
- Prevents notification/request race (all flow through same channel)

### 2. Generation-Based Coalescing

Superseding uses monotonic generation counters instead of timeouts.

**Mechanism**:
- Each (URI, method) key has a generation counter
- New operation increments generation, supersedes old
- Coalescing map stores only latest operation per key
- Early cleanup: old operations freed immediately at enqueue time

**Benefits**:
- Event-driven (no artificial time limits)
- Memory efficient: O(unique keys) not O(total requests)
- Immediate feedback: superseded operations get REQUEST_CANCELLED instantly

### 3. Non-Blocking Backpressure

The bounded order queue (capacity: 256) uses `try_send()` to prevent deadlocks during slow initialization.

**Strategy**:
- **Coalescable operations** (didChange, completion): Queue full is safe—envelope stored in coalescing map, processed when queue drains
- **Non-coalescable notifications** (didSave, willSave): Dropped under extreme backpressure
- **Requests**: Return explicit SERVER_NOT_INITIALIZED or REQUEST_FAILED error

**Why non-blocking is essential**: Blocking `send().await` during initialization can freeze all LSP handler threads, creating complete system deadlock.

### 4. Connection State Tracking

Explicit connection state enum separates data flow from control flow.

**State Definition**:
```rust
enum ConnectionState {
    Initializing,  // Writer loop started, initialization in progress
    Ready,         // Initialization completed successfully
    Failed,        // Initialization failed or writer loop panicked
    Closing,       // Graceful shutdown in progress (see ADR-0016)
    Closed,        // Fully terminated
}
```

**State Transitions**:
```
Initializing → Ready     (initialization succeeds)
Initializing → Failed    (initialization fails or times out)
Initializing → Closing   (shutdown during initialization, see ADR-0016)
Ready → Failed           (writer loop panics or server crashes)
Ready → Closing          (graceful shutdown initiated, see ADR-0016)
Closing → Closed         (shutdown completed)
Failed → Closed          (cleanup after failure, skip graceful shutdown)
```

**Operation Gating**:
- Writer loop starts immediately in `Initializing` state (before initialization completes)
- **Notifications**: Flow through unconditionally when state is `Initializing` or `Ready`
  - "Unconditional" means **not gated on connection state** (can be sent during initialization)
  - BUT notifications ARE gated on **per-document lifecycle** (see Document Lifecycle below)
- **Requests**: Gated on state being `Ready` (return SERVER_NOT_INITIALIZED if `Initializing`, REQUEST_FAILED if `Failed`)

**Document Lifecycle Gating** (per downstream server, per document URI):

```
Client → treesitter-ls → downstream server

┌────────────────────────────────────────────────────────────┐
│ Before didOpen sent to downstream:                         │
│ - didChange → DROP (don't queue, don't forward)            │
│ - didOpen contains complete accumulated state              │
│                                                            │
│ After didOpen sent to downstream:                          │
│ - didChange → FORWARD immediately                          │
│ - didSave, willSave → FORWARD immediately                  │
└────────────────────────────────────────────────────────────┘
```

**Example scenario** (multi-server initialization):
```
Client edits markdown → treesitter-ls spawns pyright (Initializing)
  ├─ Client sends didChange → treesitter-ls DROPS (pyright hasn't received didOpen yet)
  ├─ pyright initialization completes
  ├─ treesitter-ls sends didOpen(virtual-doc) to pyright (contains ALL accumulated changes)
  └─ Future didChange → treesitter-ls FORWARDS to pyright normally
```

**Why drop instead of queue**: The `didOpen` notification contains the complete document text at the time it's sent. Accumulated edits are already included. Queuing `didChange` notifications would create duplicate state updates.

**Multi-server benefit**: Fast-initializing servers (lua-ls: 100ms) respond immediately while slow servers (rust-analyzer: 5-10s) return explicit errors, preventing 5-10 second hangs. Multi-server router (ADR-0015) can distinguish temporary unavailability (`Initializing`) from permanent failure (`Failed`) for graceful degradation.

### 5. Request Cancellation Handling

**Cancellation from upstream** (via `$/cancelRequest` from ADR-0015) targets enqueued requests before they reach the downstream server.

**Cancellation Strategy** (per request state):

| Request State | Cancellation Action |
|---------------|-------------------|
| **In coalescing map** | Remove from map |
| **In order queue** (not yet dequeued) | Mark for skipping |
| **Already superseded** | Ignore (already superseded) |
| **Already sent to downstream** | N/A (handled by ADR-0015 propagation) |

**Cancellation API** (called by multi-server router):

```rust
async fn cancel_request(&self, request_id: i64) -> bool {
    // Try to remove from coalescing map
    if coalescing_map.remove_by_id(request_id).is_some() {
        return true; // Cancelled successfully
    }

    // Try to mark in order queue (if not yet dequeued)
    if order_queue.mark_cancelled(request_id).is_some() {
        return true; // Cancelled successfully
    }

    // Not found in map or queue
    // Either: (1) Already superseded
    //         (2) Already sent to downstream (ADR-0015 handles propagation)
    //         (3) Already completed
    false // Not cancelled (already processed)
}
```

**Writer loop handling** (when dequeuing marked request):

```rust
loop {
    let operation = order_queue.recv().await;

    if operation.is_cancelled() {
        // Skip cancelled operations
        continue;
    }

    // Process normally...
}
```

**Coordination with ADR-0015**: Multi-server router calls `cancel_request()` on all connections for the upstream request. If `false` (request already sent), router propagates `$/cancelRequest` to downstream servers per ADR-0015 § Cancellation Propagation.

### 6. Fail-Fast Error Handling

Writer loop panics use fail-fast pattern (not restart) because `ChildStdin` cannot be cloned.

**Strategy**:
- Panic caught, all pending operations failed with INTERNAL_ERROR
- Connection state transitions to `Failed` (triggers circuit breaker)
- No restart attempt (stdin consumed, restart creates silent permanent hang)
- Connection pool spawns new server instance with fresh stdin

**Recovery time**: ~100-500ms (respawn) vs. infinite hang (restart attempt).

## Consequences

### Positive

**Event-Driven Control**
- Adapts naturally to server initialization time
- Users receive immediate feedback (REQUEST_CANCELLED) instead of waiting for timeout
- No artificial ceiling based on fixed time values

**Guaranteed Message Ordering**
- Unified queue ensures notifications and requests maintain order
- Eliminates didChange → completion race condition
- Critical for stable URIs (PBI-200)

**Simpler State Management**
- Single generation counter per key (not per-request-type pending maps)
- No timeout tasks or expiry tracking
- Automatic cleanup through coalescing map

**Memory Efficiency**
- Bounded by O(unique URIs × methods) not O(total requests)
- Early cleanup: stale operations freed at enqueue time
- Prevents OOM during slow initialization (10+ seconds with heavy editing)

**Multi-Server Coordination**
- Initialization flag enables router to skip uninitialized servers
- Partial results from fast servers without waiting for slow ones
- No spurious protocol errors from requests to uninitialized servers

**Robust Error Handling**
- Deadlock prevention via non-blocking backpressure
- Silent hang prevention via fail-fast panic handling
- Explicit errors enable graceful degradation

**LSP Compliance**
- Every request receives response (result or error)
- Standard error codes (REQUEST_CANCELLED: -32800)
- Maintains protocol semantics

### Negative

**Per-URI State Overhead**
- Memory grows with active virtual documents (bounded by O(documents × methods))
- Typical: 3-50 entries; worst case: ~500 entries
- Mitigation: Clean up on didClose

**Connection-Level Failure**
- Writer loop panic fails entire connection (not just one operation)
- Requires connection pool to spawn new instance
- Trade-off: Better than silent permanent hang

**Notification Dropping Under Extreme Backpressure**
- Non-coalescable notifications can be dropped if queue full
- Only under extreme conditions (256+ operations queued)
- Coalescable notifications (didChange) never dropped (stored in map)

### Neutral

**Explicit Action Requests**
- Non-incremental requests (definition, references, rename) don't supersede
- Each explicit user action receives response
- Same as ADR-0012 behavior

**Backward Compatibility**
- External LSP interface unchanged
- Internal refactor only

## Implementation Guidance

### Phase 1: Unified Order Queue with Coalescing Map

**Scope**: Replace separate notification/request paths with unified actor pattern.

**Key Changes**:
- Define unified operation type (notification | request)
- Implement coalescing map for supersede-able operations
- Implement single order queue for ALL operations (FIFO)
- Single writer loop consuming from unified queue
- Non-blocking `try_send()` with operation-aware backpressure

**Exit Criteria**:
- Strict FIFO ordering maintained (no notification/request races)
- Memory bounded during initialization
- No deadlocks when queue fills
- Tests verify: didChange → request sequences, queue backpressure scenarios

### Phase 2: Generation-Based Superseding

**Scope**: Integrate generation counters with coalescing map.

**Key Changes**:
- Generation counter per (URI, method) key
- Immediate REQUEST_CANCELLED on supersede
- Early cleanup via map replacement
- Remove timeout-based pending request tracking

**Exit Criteria**:
- Superseded operations receive immediate cancellation
- Only latest operation per key reaches server
- No timeout tasks (event-driven)

### Phase 3: Connection State Management

**Scope**: Add connection state tracking to prevent protocol violations.

**Key Changes**:
- Connection state enum in connection struct (Initializing | Ready | Failed | Closed)
- Writer loop starts immediately in `Initializing` state (before initialization)
- Notifications flow unconditionally when `Initializing` or `Ready`, requests gated on `Ready` state
- Integration with router for multi-server coordination

**Exit Criteria**:
- Requests during initialization return SERVER_NOT_INITIALIZED (state: `Initializing`)
- Requests to failed connections return REQUEST_FAILED (state: `Failed`)
- Notifications flow immediately (establish document state)
- Multi-server setups: fast servers respond without waiting for slow ones

### Phase 4: Fail-Fast Panic Handling

**Scope**: Implement fail-fast pattern for writer loop panics.

**Key Changes**:
- Panic handler wraps writer loop
- Panic caught, all pending operations failed with INTERNAL_ERROR
- Connection state transitions to `Failed` (circuit breaker integration)
- No restart attempt (stdin cannot be cloned)

**Exit Criteria**:
- Panic fails connection explicitly (no silent hang)
- Connection state transitions to `Failed` on panic
- Circuit breaker triggered on failure
- Connection pool integration for respawn

### Phase 5: Stable URI Integration (PBI-200)

**Scope**: Verify superseding works with stable virtual URIs.

**Dependencies**: PBI-200 (Stable Virtual Document Identity)

**Key Changes**:
- Update supersede key extraction for stable URIs
- Per-URI lifecycle tracking (didOpen/didClose)
- Cleanup coalescing map on didClose

**Exit Criteria**:
- didChange + request ordering maintained
- No resource leaks as documents open/close

## Architectural Constraints

### Non-Negotiable Requirements

1. **Single order queue**: Dual channels break FIFO guarantee
2. **Non-blocking sends**: Blocking creates deadlock risk
3. **Fail-fast on panic**: Restart creates silent hang (stdin consumed)
4. **Early queue processing**: Writer loop must start before initialization

### Implementation Freedom

Implementations may vary on:
- Specific capacity values (bounded queue size, map limits)
- Error message formatting
- Logging and observability details
- Cleanup strategies (eager vs. lazy)
- Performance optimizations (batching, buffering)

## Related ADRs

- **[ADR-0012](0012-multi-ls-async-bridge-architecture.md)**: Multi-LS async bridge architecture
  - ADR-0014 supersedes timeout-based control while maintaining LSP compliance
- **[ADR-0015](0015-multi-server-coordination.md)**: Multi-server coordination
  - Relies on ADR-0014's ConnectionState for router integration
- **[ADR-0013](0013-async-io-layer.md)**: Async I/O layer
  - ADR-0014 builds on tokio runtime, uses ChildStdin from process spawning
- **[ADR-0016](0016-graceful-shutdown.md)**: Graceful shutdown and connection lifecycle
  - Extends ADR-0014's ConnectionState with Closing state for graceful shutdown coordination
- **[ADR-0007](0007-language-server-bridge-virtual-document-model.md)**: Virtual document model
  - ADR-0014 requires stable URIs (PBI-200) for effective superseding

## References

**Source Prototype**: `__ignored/handler2.py` (lines 69-216)

**Root Cause Analysis**: `__ignored/plan-fix-hang.md` (Root Cause #8)

**Architecture Review**: `__ignored/review-adr.md` (identified deadlock, initialization race, and panic hang issues)

**Critical Dependency**: PBI-200 (Stable Virtual Document Identity) - without stable URIs, generation counters reset on every edit
