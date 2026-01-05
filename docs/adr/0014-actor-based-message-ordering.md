# ADR-0014: Actor-Based Message Ordering for Bridge Architecture

## Status

Accepted

**Supersedes**:
- [ADR-0012](0012-multi-ls-async-bridge-architecture.md) § Timeout-based control: Request superseding with timeout (deprecated)
- [ADR-0009](0009-async-bridge-architecture.md): Original async architecture (completely replaced)

## Context

### Current Problems with Timeout-Based Control

ADR-0012 established a timeout-based control mechanism for handling requests during initialization:

1. **Requests during initialization window**: Wait with timeout (5s default)
2. **Request superseding**: Newer requests supersede older ones, with timeout as the bounded wait
3. **Separate code paths**: Notifications and requests use different pathways

This approach has fundamental issues:

**1. Time-Based vs. Event-Based Control**

Timeouts are an artificial ceiling that don't reflect actual system state:

- **Fixed timeout dilemma**: Too short → requests fail unnecessarily; too long → users wait excessively
- **Server variability**: Different language servers have vastly different initialization times (lua-ls: 100ms, rust-analyzer: 5s+)
- **System load sensitivity**: Timeouts fail under high load even when system is healthy
- **No feedback mechanism**: Timeout expiry doesn't inform whether server will be ready in 1ms or 1 minute

**2. Notification/Request Ordering Violation** (Root Cause #8)

ADR-0012 requires a "single send queue" (§6.2) but the current implementation uses separate paths:

```
Notifications → channel → forwarder → BridgeConnection
Requests     → direct call → BridgePool → BridgeConnection
```

This creates race conditions:

```
T0: didChange(v2) → enqueued in notification channel
T1: completion request → direct call to BridgeConnection
T2: completion reaches downstream (BEFORE didChange!)
T3: didChange processed

Result: Completion computed on v1 (stale content)
```

**Why this is hidden now**: Content hash-based URIs (`file:///virtual/lua/HASH.lua`) create a new URI on every edit, accidentally avoiding the race. With stable URIs (PBI-200), this becomes catastrophic.

**3. Complex State Management**

Multiple pending queues per request type, timeout tracking, and cancellation management create complex state:

```rust
struct PendingIncrementalRequests {
    completion: Option<(i64, oneshot::Sender<...>)>,
    signature_help: Option<(i64, oneshot::Sender<...>)>,
    hover: Option<(i64, oneshot::Sender<...>)>,
}
```

Each request type needs:
- Separate timeout task
- Cancellation on supersede
- Cleanup on completion/timeout

### Insight from Python Prototype (handler2.py)

The `handler2.py` prototype demonstrates a simpler, event-driven approach using the **actor pattern**:

1. **Single-writer loop**: One task consumes from a coalescing mailbox, serializing all writes
2. **Generation counter**: Superseding mechanism based on monotonic generation numbers
3. **Stale check at write time**: Late decision when dequeuing, not when enqueuing
4. **Immediate cancellation**: REQUEST_CANCELLED response when superseded
5. **No timeout needed**: Generation counter provides event-based bounded wait

**Key insight**: Superseding itself provides the bounded wait. If a request is superseded, it receives immediate feedback (REQUEST_CANCELLED). The latest request always proceeds when initialization completes—no arbitrary timeout needed.

## Decision

**Adopt the actor-based message ordering pattern from handler2.py** for the bridge architecture, replacing timeout-based control with event-driven superseding.

### Core Design Principles

#### 1. Actor Pattern: Single-Writer Loop per Server

Each `BridgeConnection` has exactly one writer task consuming from a unified operation queue:

```rust
// Unified operation type
enum BridgeOperation {
    Notification { method: String, params: Value },
    Request { id: i64, method: String, params: Value, response_tx: oneshot::Sender<...> },
}

// Single operation queue (FIFO)
struct BridgeConnection {
    operation_queue: mpsc::UnboundedSender<Envelope>,
    // ... other fields
}

// Single writer loop
async fn writer_loop() {
    while let Some(envelope) = operation_rx.recv().await {
        if is_stale_or_cancelled(&envelope).await {
            continue; // Drop stale operations
        }

        match envelope.operation {
            Notification { method, params } => {
                write_notification(method, params).await;
            }
            Request { id, method, params, response_tx } => {
                write_request(id, method, params).await;
                pending_requests.insert(id, response_tx);
            }
        }
    }
}
```

**Benefits**:
- **Ordering guarantee**: Operations processed in enqueue order (FIFO)
- **No interleaving**: Single writer prevents byte-level corruption
- **Unified path**: Notifications and requests use same queue

#### 2. Generation-Based Coalescing

Each superseding key (virtual_uri, method/incremental_type) has a monotonic generation counter:

```rust
// Generation counter per superseding key
type SupersedeKey = (String, String); // (virtual_uri, method/incremental_type)

struct BridgeConnection {
    gen_by_key: Arc<Mutex<HashMap<SupersedeKey, u64>>>,
    last_request_id_by_key: Arc<Mutex<HashMap<SupersedeKey, i64>>>,
    cancelled_request_ids: Arc<Mutex<HashSet<i64>>>,
}

// Envelope with generation snapshot
struct Envelope {
    operation: BridgeOperation,
    key: SupersedeKey,
    gen: u64, // generation at enqueue time
}
```

**How superseding works**:

```rust
async fn send(&self, operation: BridgeOperation) {
    let key = supersede_key_of(&operation);
    let mut gen_map = self.gen_by_key.lock().await;
    let mut last_req_map = self.last_request_id_by_key.lock().await;
    let mut cancelled = self.cancelled_request_ids.lock().await;

    // Bump generation (newest wins)
    let old_gen = gen_map.get(&key).copied().unwrap_or(0);
    let new_gen = old_gen + 1;
    gen_map.insert(key.clone(), new_gen);

    // If this supersedes a previous request, cancel it
    let mut cancel_id = None;
    if should_supersede(&operation) {
        if let Some(prev_id) = last_req_map.get(&key).copied() {
            if let BridgeOperation::Request { id, .. } = &operation {
                if prev_id != *id {
                    cancel_id = Some(prev_id);
                    cancelled.insert(prev_id);
                }
            }
        }
    }

    // Update last request ID for this key
    if let BridgeOperation::Request { id, .. } = &operation {
        last_req_map.insert(key.clone(), *id);
    }

    drop(gen_map);
    drop(last_req_map);
    drop(cancelled);

    // Send immediate cancellation response (outside lock)
    if let Some(id) = cancel_id {
        // Return REQUEST_CANCELLED for superseded request
        self.send_error_response(id, ErrorCodes::REQUEST_CANCELLED,
            "Request cancelled (superseded before dispatch)");
    }

    // Enqueue with generation snapshot
    self.operation_queue.send(Envelope {
        operation,
        key,
        gen: new_gen
    }).await;
}
```

#### 3. Stale Check at Write Time (Late Decision)

The writer loop checks staleness when dequeuing, not when enqueuing:

```rust
async fn is_stale_or_cancelled(&self, env: &Envelope) -> bool {
    let gen_map = self.gen_by_key.lock().await;
    let cancelled = self.cancelled_request_ids.lock().await;

    // Check if request was explicitly cancelled
    if let BridgeOperation::Request { id, .. } = &env.operation {
        if cancelled.contains(id) {
            return true;
        }
    }

    // Check if generation is stale (newer operation enqueued)
    let cur_gen = gen_map.get(&env.key).copied().unwrap_or(0);
    env.gen != cur_gen
}
```

**Why late decision matters**:

```
Scenario: User types "pri" → "print" → "printf" rapidly

T0: send(completion, "pri")   → gen=1 enqueued
T1: send(completion, "print") → gen=2 enqueued, gen=1 cancelled
T2: send(completion, "printf")→ gen=3 enqueued, gen=2 cancelled
T3: writer dequeues gen=1 → stale check: cur_gen=3, drop
T4: writer dequeues gen=2 → stale check: cur_gen=3, drop
T5: writer dequeues gen=3 → fresh, write to server

Result: Only latest request ("printf") reaches server
```

Early decision (check at enqueue time) would miss rapid superseding. Late decision ensures only the absolutely latest operation proceeds.

#### 4. Immediate Cancellation Response

When a request is superseded, send immediate feedback to the client:

```rust
// ADR-0012 § LSP error codes
pub const REQUEST_CANCELLED: i32 = -32800; // LSP 3.17

if let Some(prev_id) = cancel_id {
    self.send_error_response(
        prev_id,
        ErrorCodes::REQUEST_CANCELLED,
        "Request cancelled (superseded before dispatch)"
    );
}
```

**LSP compliance**:
- Every request receives a response (LSP 3.x § Request Message)
- Uses standard `RequestCancelled` error code (-32800)
- Client sees explicit cancellation, not timeout

#### 5. Per-Virtual-URI Granularity

Superseding operates at per-virtual-URI granularity, not connection-level:

```rust
fn supersede_key_of(op: &BridgeOperation) -> SupersedeKey {
    let method = op.method();
    let uri = extract_uri_from_params(op.params());

    // Different virtual URIs = independent superseding
    // file:///virtual/lua/example.md#block-0.lua → completion
    // file:///virtual/lua/example.md#block-1.lua → completion
    // These do NOT supersede each other

    let incremental_type = match method {
        "textDocument/completion" => "completion",
        "textDocument/hover" => "hover",
        "textDocument/signatureHelp" => "signature",
        _ => return (uri.to_string(), method.to_string()),
    };

    (uri.to_string(), incremental_type.to_string())
}
```

**Why URI granularity matters**: Different code blocks are independent. Completion in block-0 should not supersede completion in block-1.

### No Timeout Needed for Requests

**The generation counter provides event-based bounded wait**:

1. **During initialization**: Requests enqueue with generation snapshots
2. **User continues typing**: Newer requests supersede older ones, sending immediate REQUEST_CANCELLED
3. **Initialization completes**: Writer loop starts processing
4. **Only latest request proceeds**: Stale check drops all superseded operations
5. **User receives response**: Either result (latest) or REQUEST_CANCELLED (superseded)

**Bounded wait guarantee**: User never waits for stale requests. Either:
- Latest request completes when server ready (event-driven)
- Superseded requests receive immediate cancellation (no waiting)

**Timeout still used for notifications**: Notifications during initialization use 100ms timeout (can be dropped; see ADR-0012 §6.1 Phase 2 guard).

### Superseding Criteria

```rust
fn should_supersede(op: &BridgeOperation) -> bool {
    match op.method() {
        // Incremental requests: newer supersedes older
        "textDocument/completion" => true,
        "textDocument/hover" => true,
        "textDocument/signatureHelp" => true,

        // Notifications: newer supersedes older (same URI)
        "textDocument/didChange" => true,

        // Explicit actions: do NOT supersede (user explicitly requested each)
        "textDocument/definition" => false,
        "textDocument/references" => false,
        "textDocument/rename" => false,
        "textDocument/codeAction" => false,
        "textDocument/formatting" => false,

        _ => false,
    }
}
```

## Consequences

### Positive

**1. Event-Driven > Time-Based Control**
- No artificial timeout ceilings
- Adapts to server initialization time naturally
- Works regardless of system load
- User receives immediate feedback (REQUEST_CANCELLED) instead of waiting for timeout

**2. Guaranteed Message Ordering**
- Unified queue ensures notifications and requests maintain order
- Critical for stable URIs (PBI-200): didChange(v2) → completion always arrives in order
- Eliminates race condition (Root Cause #8)

**3. Simpler State Management**
- Single generation counter per key (no per-request-type pending maps)
- No timeout tasks per request
- Automatic cleanup (stale operations dropped by writer loop)

**4. Better User Experience**
- Immediate cancellation response (no waiting for timeout)
- Latest request always processes (event-driven)
- Predictable behavior (no timeout variability)

**5. LSP Compliance**
- Every request receives response (result or REQUEST_CANCELLED)
- Standard error codes (REQUEST_CANCELLED: -32800)
- Maintains protocol semantics

**6. Scalability**
- No timeout task overhead (O(1) state per supersede key, not per request)
- Late decision minimizes wasted work (only latest operation processed)
- Generation counter is lightweight (single u64 increment)

### Negative

**1. Per-Virtual-URI State Overhead**
- Each virtual URI + method combination needs generation counter
- Memory grows with number of active virtual documents
- Mitigation: Clean up generations when virtual documents close

**2. Complexity of Stale Check**
- Writer loop must check every operation before processing
- Lock contention on `gen_by_key` map
- Mitigation: Use concurrent hash map (DashMap) for lock-free reads

**3. Cancellation Response Overhead**
- Superseded requests receive explicit error response
- Network traffic for cancellations
- Mitigation: Better than timeout (faster feedback, no waiting)

**4. Requires Stable URIs**
- Generation counter assumes stable virtual document URIs
- Content hash-based URIs break superseding (each edit = new URI = new generation counter)
- **Dependency**: This ADR REQUIRES PBI-200 (stable virtual URI) to be effective

### Neutral

**1. Notification Timeout Remains**
- Notifications during initialization still use timeout (100ms)
- Can be dropped safely (ADR-0012 §6.1 Phase 2 guard)
- No change from ADR-0012 behavior

**2. Explicit Action Requests**
- Non-incremental requests (definition, references, rename) do NOT supersede
- Each explicit user action receives a response
- Same as ADR-0012 behavior

**3. Backward Compatibility**
- External interface unchanged (still returns standard LSP responses)
- Internal refactor only

## Implementation

### Phase 1: Unified Queue (PBI-201)

**Scope**: Replace separate notification/request paths with single operation queue

**Files**:
- `src/lsp/bridge/connection.rs`: Major refactor (lines 49-694)

**Changes**:
1. Define `BridgeOperation` enum (Notification | Request)
2. Replace `send_notification` and `send_request` with unified `send` method
3. Implement single writer loop consuming from operation queue
4. Remove separate notification forwarder path

**Exit Criteria**:
- All notifications and requests flow through same queue
- Writer loop processes operations in FIFO order
- Tests pass (no ordering violations)

### Phase 2: Generation-Based Superseding

**Scope**: Implement generation counter and stale check

**Files**:
- `src/lsp/bridge/connection.rs`: Add generation tracking

**Changes**:
1. Add `gen_by_key: DashMap<SupersedeKey, u64>`
2. Add `last_request_id_by_key: DashMap<SupersedeKey, i64>`
3. Add `cancelled_request_ids: DashMap<i64, ()>`
4. Implement `send` method with generation bump and cancellation
5. Implement `is_stale_or_cancelled` check in writer loop
6. Remove timeout-based pending request tracking

**Exit Criteria**:
- Superseded requests receive REQUEST_CANCELLED immediately
- Only latest request per key proceeds to server
- No timeout tasks (event-driven)

### Phase 3: Integration with Stable URIs (PBI-200)

**Scope**: Verify superseding works with stable virtual URIs

**Dependencies**: PBI-200 (Stable Virtual Document Identity)

**Changes**:
1. Update `supersede_key_of` to use stable URIs (not content hash)
2. Add per-URI lifecycle tracking (didOpen/didClose)
3. Clean up generation counters when virtual documents close

**Exit Criteria**:
- didChange + completion ordering maintained with stable URIs
- Generation counters cleaned up on didClose
- No resource leaks (stale generations)

### Migration Strategy

**From ADR-0012 timeout-based control to ADR-0014 actor pattern**:

| Component | ADR-0012 (Before) | ADR-0014 (After) |
|-----------|-------------------|------------------|
| **Operation path** | Notifications → channel, Requests → direct call | Unified queue (single path) |
| **Superseding** | Timeout-based bounded wait | Generation counter (event-based) |
| **Cancellation** | REQUEST_FAILED after timeout | REQUEST_CANCELLED immediately |
| **State tracking** | `PendingIncrementalRequests` per type | Generation counter per (URI, method) |
| **Writer** | Mixed (channel reader + direct write) | Single writer loop (actor pattern) |

**Backward compatibility**: External LSP interface unchanged. Internal refactor only.

## Related ADRs

- **[ADR-0012](0012-multi-ls-async-bridge-architecture.md)**: Multi-LS async bridge architecture
  - Establishes timeout-based control (§7.3 Request Superseding Pattern)
  - Defines single send queue requirement (§6.2 Document Notification Order)
  - **Relationship**: ADR-0014 supersedes timeout-based control while maintaining LSP compliance and ordering guarantees

- **[ADR-0009](0009-async-bridge-architecture.md)**: Original async architecture **(Superseded)**
  - Established tokio-based async I/O
  - **Relationship**: ADR-0012 replaced this; ADR-0014 further refines ADR-0012

- **[ADR-0007](0007-language-server-bridge-virtual-document-model.md)**: Virtual document model
  - Discusses stable identity across edits (§Virtual Document Identity)
  - Content hash-based URIs vs. index-based stable URIs
  - **Relationship**: ADR-0014 REQUIRES stable URIs (PBI-200) for effective superseding

## References

**Source Prototype**: `__ignored/handler2.py` (lines 69-216)

**Root Cause Analysis**: `__ignored/plan-fix-hang.md` (Root Cause #8: Notification/Request Ordering Violation)

**Critical Dependency**: PBI-200 (Stable Virtual Document Identity) - Without stable URIs, generation counters reset on every edit, breaking superseding.
