# ADR-0014: Actor-Based Message Ordering for Bridge Architecture

| | |
|---|---|
| **Status** | Draft |
| **Date** | 2026-01-06 |

**Supersedes**:
- [ADR-0012](0012-multi-ls-async-bridge-architecture.md) § Timeout-based control
- [ADR-0009](0009-async-bridge-architecture.md): Original async architecture

## Context

### Problems with Previous Approach

ADR-0012 established timeout-based control for initialization and request superseding. This approach had three fundamental problems:

**1. Time-Based Control Doesn't Reflect System State**

Timeouts create artificial ceilings unrelated to actual readiness:
- Fixed timeout dilemma: too short fails unnecessarily, too long wastes user time
- Server variability: lua-ls initializes in 100ms, rust-analyzer takes 5-10s
- No feedback: timeout expiry doesn't indicate when server will actually be ready

**2. Notification/Request Ordering Violation**

Separate code paths create race conditions where requests can arrive before notifications, leading to completion on stale content. This is hidden by content-hash URIs but becomes catastrophic with stable URIs.

**3. Complexity from Per-Type State Management**

Timeout tracking requires separate pending maps per request type, each with its own timeout task, cancellation logic, and cleanup.

### Key Architectural Insight

Event-driven superseding offers a simpler alternative to timeout-based control:
- Single-writer loop serializes all writes (prevents protocol corruption)
- Generation counters enable superseding without timeouts (event-driven, not time-based)
- Immediate REQUEST_CANCELLED feedback instead of timeout waits (better UX)

**Superseding provides the bounded wait**: Users either get the latest result (when ready) or immediate cancellation (if superseded).

## Decision

**Adopt actor-based message ordering with event-driven superseding**, structured around six architectural principles.

### Architecture Overview

```
┌──────────────────────────────────────────────────────────┐
│              Per-Connection Actor Pattern                │
│                                                          │
│  Unified Operation Stream:                               │
│  ┌──────────────────────────────────────────────────┐    │
│  │ Notifications + Requests                         │    │
│  │   (didChange, hover, completion, etc.)           │    │
│  └─────────────────┬────────────────────────────────┘    │
│                    │                                     │
│                    ▼                                     │
│  ┌──────────────────────────────────────────────────┐    │
│  │         Coalescing Map (superseding)             │    │
│  │  Key: (URI, method)                              │    │
│  │  Value: Latest operation + generation            │    │
│  │  - Stores only latest per key                    │    │
│  │  - Early cleanup (superseded ops freed)          │    │
│  └─────────────────┬────────────────────────────────┘    │
│                    │                                     │
│                    ▼                                     │
│  ┌──────────────────────────────────────────────────┐    │
│  │           Unified Order Queue (FIFO)             │    │
│  │  Bounded capacity (256)                          │    │
│  │  - Ensures FIFO ordering                         │    │
│  │  - Non-blocking backpressure (try_send)          │    │
│  └─────────────────┬────────────────────────────────┘    │
│                    │                                     │
│                    ▼                                     │
│  ┌──────────────────────────────────────────────────┐    │
│  │         Single Writer Loop (Actor)               │    │
│  │  - Dequeues from order queue                     │    │
│  │  - Atomic claim from coalescing map              │    │
│  │  - Writes to server stdin                        │    │
│  └─────────────────┬────────────────────────────────┘    │
│                    │                                     │
└────────────────────┼──────────────────────────────────────┘
                     ▼
              Server stdin (serialized)
```

### 1. Single-Writer Loop (Actor Pattern)

Each server connection has exactly one writer task consuming from a unified queue, ensuring FIFO ordering for all operations.

**Key Properties:**
- Strict FIFO ordering (notifications and requests maintain sequence)
- No byte-level corruption (single writer, no interleaving)
- Prevents notification/request race (all flow through same channel)

### 2. Generation-Based Coalescing

Superseding uses monotonic generation counters instead of timeouts.

**Mechanism:**
- Each (URI, method) key has a generation counter
- New operation increments generation, supersedes old
- Coalescing map stores only latest operation per key
- Early cleanup: old operations freed immediately at enqueue time

**Benefits:**
- Event-driven (no artificial time limits)
- Memory efficient: O(unique keys) not O(total requests)
- Immediate feedback: superseded operations get REQUEST_CANCELLED instantly

**Race Prevention (Supersede vs Writer Dequeue):**

The atomic claim pattern prevents double-response violations:

```rust
// In writer loop
loop {
    let operation_ref = order_queue.recv().await;
    let id = operation_ref.id;  // LSP spec: integer | string

    if let Some(key) = operation_ref.coalescing_key {
        // Atomic claim from coalescing map
        match coalescing_map.remove(&key) {
            Some((gen, claimed_id, envelope)) if claimed_id == id => {
                // SUCCESS: We own this operation, proceed
            }
            _ => {
                // SUPERSEDED: Skip (already got REQUEST_CANCELLED)
                continue;
            }
        }
    }

    // Safe to send to downstream server
    write_request(id, method, params).await?;
}
```

**Memory Management:**

Coalescing map grows unbounded without cleanup. The solution: hook `textDocument/didClose` to remove all entries for closed documents, bounding memory by concurrent open documents (not historical total).

### 3. Non-Blocking Backpressure

The bounded order queue (capacity: 256) uses `try_send()` to prevent deadlocks during slow initialization.

**Strategy by Operation Type:**

| Operation Type | Queue Full Behavior | Rationale |
|---------------|-------------------|-----------|
| **Coalescable** (didChange, completion) | Store in map, skip queue | Envelope persisted, processed when queue drains |
| **Non-coalescable notifications** (didSave, willSave) | Drop with telemetry | Extreme backpressure, recoverable via next didChange |
| **Requests** | Explicit error | Return SERVER_NOT_INITIALIZED or REQUEST_FAILED |

**Notification Drop Telemetry:**
- Log at WARN level (always)
- Send `$/telemetry` event to client (LSP standard)
- Circuit breaker integration (sustained drops → connection unhealthy)
- State re-synchronization metadata (track dropped events, inject into next didChange)

### 4. Connection State Tracking

Explicit connection state enum separates data flow from control flow.

**State Machine:**

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
     │        signal       │
     │                     │ no shutdown
     │                     │ (cleanup only)
     │                     │
     └──────────┬──────────┘
                │
                ▼
           ┌──────────┐
           │  Closed  │  (terminal state)
           └──────────┘
```

**Operation Gating:**
- Writer loop starts immediately in `Initializing` state (before initialization completes)
- **Notifications**: Flow unconditionally when `Initializing` or `Ready` (establish document state)
- **Requests**: Gated on `Ready` state (return SERVER_NOT_INITIALIZED if `Initializing`, REQUEST_FAILED if `Failed`)

**Document Lifecycle Gating:**

Per downstream server, per document URI:
- **Before didOpen sent**: didChange → DROP (don't queue, don't forward)
- **After didOpen sent**: didChange, didSave, willSave → FORWARD immediately

The `didOpen` notification contains the complete accumulated state, making queued `didChange` notifications redundant.

### 5. Request Cancellation Handling

Cancellation from upstream (via `$/cancelRequest`) targets enqueued requests before they reach the downstream server.

**Cancellation Strategy by Request State:**

| Request State | Cancellation Action |
|--------------|---------------------|
| **In coalescing map** | Remove from map |
| **In order queue** (not yet dequeued) | Mark for skipping |
| **Already superseded** | Ignore (already got REQUEST_CANCELLED) |
| **Already sent to downstream** | N/A (handled by ADR-0015 propagation) |

**Coordination with ADR-0015:** Multi-server router calls `cancel_request()` on all connections. If `false` (request already sent), router propagates `$/cancelRequest` to downstream servers.

### 6. Fail-Fast Error Handling

Writer loop panics use fail-fast pattern (not restart) because `ChildStdin` cannot be cloned.

**Strategy:**
- Panic caught, all pending operations failed with INTERNAL_ERROR
- Connection state transitions to `Failed` (triggers circuit breaker)
- No restart attempt (stdin consumed, restart creates silent permanent hang)
- Connection pool spawns new server instance with fresh stdin

**Recovery time**: ~100-500ms (respawn) vs. infinite hang (restart attempt).

**Panic Handler Order:**
1. **First**: Fail all pending operations (LSP response guarantee)
2. **Second**: Check current state (determines final state)
3. **Third**: Trigger circuit breaker (enables recovery)

**Special Case**: Panic during `Closing` state → `Closed` (not `Failed`), skip circuit breaker.

## Consequences

### Positive

**Event-Driven Control:**
- Adapts naturally to server initialization time
- Immediate feedback (REQUEST_CANCELLED) instead of waiting for timeout
- No artificial ceiling based on fixed time values

**Guaranteed Message Ordering:**
- Unified queue ensures notifications and requests maintain order
- Eliminates didChange → completion race condition
- Critical for stable URIs (PBI-200)

**Simpler State Management:**
- Single generation counter per key (not per-request-type pending maps)
- No timeout tasks or expiry tracking
- Automatic cleanup through coalescing map

**Memory Efficiency:**
- Bounded by O(unique URIs × methods) not O(total requests)
- Early cleanup: stale operations freed at enqueue time
- Prevents OOM during slow initialization

**Multi-Server Coordination:**
- State tracking enables router to skip uninitialized servers
- Partial results from fast servers without waiting for slow ones
- No spurious protocol errors from requests to uninitialized servers

**Robust Error Handling:**
- Deadlock prevention via non-blocking backpressure
- Silent hang prevention via fail-fast panic handling
- Explicit errors enable graceful degradation

**LSP Compliance:**
- Every request receives response (result or error)
- Standard error codes (REQUEST_CANCELLED: -32800)
- Maintains protocol semantics

### Negative

**Per-URI State Overhead:**
- Memory grows with active virtual documents (bounded by O(documents × methods))
- Typical: 3-50 entries; worst case: ~500 entries
- Mitigation: Clean up on didClose

**Connection-Level Failure:**
- Writer loop panic fails entire connection (not just one operation)
- Requires connection pool to spawn new instance
- Trade-off: Better than silent permanent hang

**Notification Dropping Under Extreme Backpressure:**
- Non-coalescable notifications can be dropped if queue full
- Only under extreme conditions (256+ operations queued)
- Coalescable notifications (didChange) never dropped (stored in map)

### Neutral

**Explicit Action Requests:**
- Non-incremental requests (definition, references, rename) don't supersede
- Each explicit user action receives response
- Same as ADR-0012 behavior

**Backward Compatibility:**
- External LSP interface unchanged
- Internal refactor only

## Alternatives Considered

### Alternative 1: Timeout-Based Superseding (ADR-0012)

Continue using timeouts to determine when to abandon old requests.

**Rejected Reasons:**

1. **Time-based control disconnected from readiness**: Fixed timeout values don't adapt to server variability
2. **Additional complexity**: Separate timeout tasks per request type, cleanup logic, cancellation handling
3. **Poor user feedback**: Timeout expiry tells users "we gave up" not "here's the latest result"
4. **Memory overhead**: Must track all pending requests until timeout (not just latest)

**Comparison:**

| Aspect | Timeout-Based | Generation-Based |
|--------|--------------|------------------|
| **Wait time** | Fixed (e.g., 5s) | Event-driven (immediate or complete) |
| **Memory** | O(total requests) | O(unique keys) |
| **Feedback** | Timeout error | REQUEST_CANCELLED or latest result |
| **Complexity** | Per-type timeout tasks | Single generation counter |

### Alternative 2: Dual Channels (Separate Notification/Request Paths)

Maintain separate channels for notifications and requests.

**Rejected Reasons:**

1. **Ordering violation**: Requests can overtake notifications, causing stale content issues
2. **Critical with stable URIs**: Race condition becomes catastrophic (PBI-200)
3. **Complexity**: Two code paths, two sets of backpressure handling
4. **No FIFO guarantee**: Must manually coordinate ordering

**Why single channel is essential**: LSP semantics require `didChange` to be processed before subsequent `completion` on the same URI.

### Alternative 3: Writer Loop Restart on Panic

Attempt to restart the writer loop after panic instead of failing the connection.

**Rejected Reasons:**

1. **Silent permanent hang**: `ChildStdin` consumed by panic, cannot be cloned, restart creates zombie writer
2. **Resource leak**: Original stdin handle lost, new writer cannot write
3. **Debugging nightmare**: Appears to work but silently fails
4. **Better alternative exists**: Respawn entire connection with fresh stdin (~100-500ms)

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

**Critical Dependency**: PBI-200 (Stable Virtual Document Identity) - without stable URIs, generation counters reset on every edit, preventing effective superseding across document edits

**Design Pattern Origins**: Event-driven superseding with generation counters emerged from analysis of timeout-based control limitations in ADR-0012. The pattern combines actor model principles (single-writer loop) with optimistic concurrency control (generation numbers).

## Amendment History

- **2026-01-06**: Merged [Amendment 001](0014-actor-based-message-ordering-amendment-001.md) - Completed state machine with all transitions, panic handler implementation requirements, and error code corrections
- **2026-01-06**: Merged [Amendment 002](0014-actor-based-message-ordering-amendment-002.md) - Added comprehensive notification drop telemetry, circuit breaker integration, and state re-synchronization metadata to prevent silent data loss
