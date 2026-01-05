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

// Unified order channel + coalescing map for supersede-able operations
struct BridgeConnection {
    // Coalescing map for supersede-able operations only
    // Stores LATEST operation per key to minimize memory during initialization
    coalescing_map: Arc<DashMap<SupersedeKey, Envelope>>,

    // Single order channel for ALL operations (preserves FIFO)
    // Enqueues either SupersedeKey (for coalesced ops) or Envelope (for direct ops)
    order_queue: mpsc::Sender<OperationHandle>,  // Bounded (capacity: 256)

    // ... other fields
}

// Discriminated union for order channel
enum OperationHandle {
    Coalesced(SupersedeKey),  // Retrieve from coalescing_map
    Direct(Envelope),          // Use directly
}

// Single writer loop consuming from unified order queue
async fn writer_loop(
    coalescing_map: Arc<DashMap<SupersedeKey, Envelope>>,
    mut order_queue_rx: mpsc::Receiver<OperationHandle>,
    pending_requests: Arc<DashMap<i64, oneshot::Sender<...>>>,
) {
    while let Some(handle) = order_queue_rx.recv().await {
        let envelope = match handle {
            OperationHandle::Coalesced(key) => {
                // Retrieve latest envelope from map
                match coalescing_map.remove(&key) {
                    Some((_, envelope)) => envelope,
                    None => continue, // Superseded and cleaned up early
                }
            }
            OperationHandle::Direct(envelope) => {
                // Use envelope directly
                envelope
            }
        };

        // Write operation to stdin (ordering preserved!)
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
- **Ordering guarantee**: Single order channel ensures FIFO for ALL operations (supersede-able + non-supersede-able)
- **No interleaving**: Single writer loop prevents byte-level corruption
- **Memory efficiency**: Coalescing map stores only latest operation per key (supersede-able ops only)
- **Bounded memory**: Hard capacity limits prevent OOM during slow initialization
- **Early cleanup**: Superseded operations removed from map immediately, not at dequeue time

**Why unified order channel is critical**:

Dual separate channels would break ordering:
```rust
// BROKEN: Two separate channels
tokio::select! {
    Some(key) = coalescing_rx.recv() => { ... }
    Some(envelope) = direct_rx.recv() => { ... }
}
// select! can arbitrarily choose branch → ordering violated!
```

Scenario with broken ordering:
```
T0: didChange (supersede-able) → coalescing channel
T1: definition (non-supersede-able) → direct channel
T2: select! chooses direct branch → definition processed FIRST
Result: Definition sees stale content (race condition!)
```

With unified channel:
```
T0: didChange → order_queue.send(Coalesced(key))
T1: definition → order_queue.send(Direct(envelope))
T2: Writer dequeues in FIFO: didChange, then definition
Result: Definition sees fresh content ✓
```

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

    // Bump generation (newest wins)
    let old_gen = gen_map.get(&key).copied().unwrap_or(0);
    let new_gen = old_gen + 1;
    gen_map.insert(key.clone(), new_gen);

    drop(gen_map);

    let envelope = Envelope {
        operation,
        key: key.clone(),
        gen: new_gen,
    };

    // Route based on superseding behavior
    if should_supersede(&envelope.operation) {
        // Supersede-able: Use coalescing map for early cleanup
        // This replaces old operation with new one, freeing memory immediately
        if let Some(old_envelope) = self.coalescing_map.insert(key.clone(), envelope) {
            // Cancel superseded request immediately
            if let BridgeOperation::Request { response_tx, .. } = old_envelope.operation {
                let _ = response_tx.send(Err(ResponseError {
                    code: ErrorCodes::REQUEST_CANCELLED,
                    message: "Request cancelled (superseded before dispatch)".into(),
                    data: None,
                }));
            }
            // Don't enqueue again - key already in order queue
        } else {
            // New key - enqueue handle for ordering
            // If queue full, this blocks (backpressure)
            self.order_queue.send(OperationHandle::Coalesced(key)).await.ok();
        }
    } else {
        // Non-supersede-able: Enqueue envelope directly
        // If queue full, this blocks (backpressure)
        self.order_queue.send(OperationHandle::Direct(envelope)).await.ok();
    }
}
```

#### 3. Coalescing Map for Memory-Bounded Superseding

**Design choice: Why coalescing map?**

Superseding can be implemented with generation counter + staleness check alone (see Alternative 1), but this accumulates stale envelopes in the queue. The coalescing map is an **optimization** that provides:
- **Early cleanup**: Stale envelopes freed at enqueue time (not dequeue)
- **Memory efficiency**: O(unique keys) instead of O(total requests)
- **No wasted processing**: Writer doesn't check staleness for every envelope

Each supersede-able operation is stored in a `DashMap<SupersedeKey, Envelope>`, where inserting a new envelope for an existing key automatically replaces (and frees) the old one.

**Critical mechanism: Queue deduplication via conditional enqueue**

```rust
// When new operation arrives
if should_supersede(&envelope.operation) {
    // Try to insert into map
    if let Some(old_envelope) = self.coalescing_map.insert(key.clone(), envelope) {
        // ↑ Map already had this key (returns old envelope)

        // Old envelope freed immediately (early cleanup)
        if let BridgeOperation::Request { response_tx, .. } = old_envelope.operation {
            let _ = response_tx.send(Err(REQUEST_CANCELLED));
        }

        // CRITICAL: Don't enqueue again!
        // Key is already in order_queue from first send
        // This prevents duplicate entries in queue
    } else {
        // ↑ New key (map.insert returned None)

        // First time seeing this key - enqueue for FIFO ordering
        self.order_queue.send(OperationHandle::Coalesced(key)).await.ok();
    }
}
```

**Why this works - Queue deduplication guarantee**:

```
T0: First send for key → map.insert returns None → ENQUEUE key
T1: Second send for key → map.insert returns Some → DON'T enqueue
T2: Third send for key → map.insert returns Some → DON'T enqueue
...
TN: Nth send for key → map.insert returns Some → DON'T enqueue

Queue state: [Coalesced(key)] ← Only ONE entry, regardless of N sends
Map state: {key => envelope_N} ← Only LATEST envelope

Writer dequeues key once → retrieves envelope_N → writes latest
```

**Deduplication invariant**: For any SupersedeKey, the order_queue contains **at most one** `Coalesced(key)` entry, enqueued on the first send.

**Memory guarantee with queue deduplication**:

```
Scenario: User types "pri" → "print" → "printf" rapidly

T0: send(completion, "pri")   → map[key] = envelope(gen=1)
                               → map.insert returns None (new key)
                               → order_queue.send(Coalesced(key))
                               Queue: [Coalesced(key)] (1 entry)
                               Map: {key => env1} (1 envelope, ~1KB)

T1: send(completion, "print") → map[key] = envelope(gen=2)
                               → map.insert returns Some(env1) (existing key!)
                               → DON'T enqueue (key already in queue)
                               → env1 freed immediately
                               Queue: [Coalesced(key)] (still 1 entry!)
                               Map: {key => env2} (1 envelope, ~1KB)

T2: send(completion, "printf")→ map[key] = envelope(gen=3)
                               → map.insert returns Some(env2)
                               → DON'T enqueue
                               → env2 freed immediately
                               Queue: [Coalesced(key)] (still 1 entry!)
                               Map: {key => env3} (1 envelope, ~1KB)

T3: Writer dequeues key → map.remove(key) returns env3 → write to server

Result: Only latest request reaches server
Queue memory: O(unique keys) — 1 entry per key, ~100 bytes each
Map memory: O(unique keys) — 1 envelope per key, ~1KB each
Total: ~1.1KB per unique (URI, method) pair
```

**Scaling analysis**:

| Scenario | Queue Entries | Map Entries | Total Memory |
|----------|--------------|-------------|--------------|
| 1 doc, 100 edits (same key) | 1 entry | 1 envelope | ~1.1KB |
| 10 docs, 10 edits each (10 keys) | 10 entries | 10 envelopes | ~11KB |
| 100 docs × 5 methods (500 keys) | 500 entries | 500 envelopes | ~550KB |

Compare to unbounded queue without deduplication:
- 100 edits same doc: 100 envelopes = ~100KB (vs ~1.1KB)
- 10 docs × 10 edits: 1000 envelopes = ~1MB (vs ~11KB)

**Why unified order channel is critical**:

- Coalescing map (DashMap) doesn't preserve insertion order
- Single order channel `mpsc::Sender<OperationHandle>` maintains FIFO for ALL operations
- Handles both coalesced (retrieve from map) and direct (use envelope) operations
- When coalesced key appears multiple times, only first enqueue matters (later are supersedes with map already updated)
- Writer processes all operations in strict FIFO order

**Ordering guarantee preserved**:

```
Scenario: didChange followed by definition request

T0: send(didChange) → coalescing_map[key] = envelope
                   → order_queue.send(Coalesced(key))
T1: send(definition) → order_queue.send(Direct(envelope))

Writer loop:
1. Dequeue Coalesced(key) → retrieve from map → write didChange
2. Dequeue Direct(envelope) → use envelope → write definition

Result: Server receives didChange BEFORE definition ✓
```

**Edge case: Document closed before writer processes**:

```
Scenario: Operations enqueued, then document closed

T0: send(completion, uri1) → map[key1]=env1, queue: [Coalesced(key1)]
T1: didClose(uri1) → map.retain(|k,_| k.uri != uri1)
                   → Map now empty (key1 removed)
                   → Queue still: [Coalesced(key1)]  ← Key remains!

Writer loop:
1. Dequeue Coalesced(key1)
2. map.remove(key1) → returns None (already removed)
3. Continue (skip) ← No write, no error

Result: Stale key in queue is safely skipped
```

**Queue contains phantom entries after cleanup**: This is acceptable because:
1. Phantom entries are small (~100 bytes per key)
2. Writer safely skips them (map lookup returns None)
3. Queue eventually drains during normal operation
4. Alternative (scanning queue to remove) would be expensive and complex

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
- Early cleanup minimizes wasted work (stale operations freed immediately)
- Bounded memory footprint during initialization (coalescing map + bounded queues)
- Generation counter is lightweight (single u64 increment)

**7. Memory Efficiency During Slow Initialization**

Critical improvement over naive queue approach:

| Scenario | Naive Queue (Late Decision) | Coalescing Map (Early Cleanup) |
|----------|---------------------------|-------------------------------|
| **User types 100 chars during 5s init** | 100 envelopes queued (~100KB) | 1 envelope in map (~1KB) |
| **10 documents, rapid editing** | 1000+ envelopes (~1MB+) | 10-50 entries (~50KB) |
| **Cleanup timing** | After dequeue (late) | On supersede (immediate) |
| **Memory bound** | Unbounded (grows with requests) | O(documents × methods) |

**Prevents OOM**: Bounded memory even when initialization takes 10+ seconds (e.g., rust-analyzer on large projects).

### Negative

**1. Per-Virtual-URI State Overhead**
- Each virtual URI + method combination needs entry in coalescing map
- Memory grows with number of active virtual documents (bounded by O(documents × methods))
- Typical: 3-50 entries; Max realistic: ~500 entries
- Mitigation: Clean up map entries when virtual documents close

**2. Dual Queue Complexity**
- Two separate queues (coalescing + direct) increase implementation complexity
- Requires routing logic in `send()` to choose correct queue
- Mitigation: Clear routing based on `should_supersede()` predicate

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

### Phase 1: Unified Order Queue with Coalescing Map (PBI-201)

**Scope**: Replace separate notification/request paths with unified order queue + coalescing map

**Files**:
- `src/lsp/bridge/connection.rs`: Major refactor (lines 49-694)

**Changes**:
1. Define `BridgeOperation` enum (Notification | Request)
2. Define `OperationHandle` enum (Coalesced | Direct)
3. Implement coalescing map for supersede-able operations:
   - `coalescing_map: Arc<DashMap<SupersedeKey, Envelope>>`
4. Implement unified order channel for ALL operations:
   - `order_queue: mpsc::Sender<OperationHandle>` (bounded, capacity 256)
   - Routes coalesced keys and direct envelopes through SAME channel
5. Replace `send_notification` and `send_request` with unified `send` method
6. Implement single writer loop consuming from unified order queue
7. Remove separate notification forwarder path

**Exit Criteria**:
- All operations (supersede-able + non-supersede-able) flow through single order channel
- Writer loop processes operations in strict FIFO order (no interleaving)
- Superseded operations cleaned up immediately (early cleanup)
- Memory bounded during initialization (verify with load test)
- Tests pass (no ordering violations, including didChange → request sequences)

### Phase 2: Generation-Based Superseding

**Scope**: Integrate generation counter with coalescing map

**Files**:
- `src/lsp/bridge/connection.rs`: Add generation tracking

**Changes**:
1. Add `gen_by_key: DashMap<SupersedeKey, u64>` for generation tracking
2. Update `send` method:
   - Bump generation counter on each send
   - Use `coalescing_map.insert()` to replace old envelope (early cleanup)
   - Send immediate `REQUEST_CANCELLED` when superseding
   - Enqueue key only if new (not if replacing)
3. Remove stale check from writer loop (no longer needed - map contains only latest)
4. Remove timeout-based pending request tracking

**Exit Criteria**:
- Superseded requests receive REQUEST_CANCELLED immediately
- Only latest request per key proceeds to server
- Coalescing map never contains stale envelopes
- No timeout tasks (event-driven)

### Phase 3: Integration with Stable URIs (PBI-200)

**Scope**: Verify superseding works with stable virtual URIs

**Dependencies**: PBI-200 (Stable Virtual Document Identity)

**Changes**:
1. Update `supersede_key_of` to use stable URIs (not content hash)
2. Add per-URI lifecycle tracking (didOpen/didClose)
3. Clean up coalescing map entries when virtual documents close:
   ```rust
   async fn on_did_close(&self, uri: &str) {
       // Remove all entries for this URI
       self.coalescing_map.retain(|key, _| key.0 != uri);
       self.gen_by_key.retain(|key, _| key.0 != uri);
   }
   ```

**Exit Criteria**:
- didChange + completion ordering maintained with stable URIs
- Coalescing map entries cleaned up on didClose
- No resource leaks (memory stays bounded as documents open/close)

### Migration Strategy

**From ADR-0012 timeout-based control to ADR-0014 actor pattern**:

| Component | ADR-0012 (Before) | ADR-0014 (After) |
|-----------|-------------------|------------------|
| **Operation path** | Notifications → channel, Requests → direct call | Unified order queue (single FIFO path) |
| **Superseding** | Timeout-based bounded wait | Generation counter + coalescing map (event-based) |
| **Cancellation** | REQUEST_FAILED after timeout | REQUEST_CANCELLED immediately |
| **State tracking** | `PendingIncrementalRequests` per type | Coalescing map per (URI, method) |
| **Writer** | Mixed (channel reader + direct write) | Single writer loop (unified order queue) |
| **Ordering** | Race between notification channel and request call | Guaranteed FIFO (all ops through same queue) |
| **Memory** | Unbounded during init | Bounded by O(unique URIs × methods) |
| **Cleanup** | Late (at dequeue) | Early (at enqueue via map replacement) |

**Backward compatibility**: External LSP interface unchanged. Internal refactor only.

## Alternatives Considered

### Alternative 1: Unbounded Queue with Late Staleness Check

Use `mpsc::UnboundedSender` with staleness checking when dequeuing from the writer loop.

**Design**:

```rust
struct BridgeConnection {
    operation_queue: mpsc::UnboundedSender<Envelope>,  // Unbounded
    gen_by_key: Arc<Mutex<HashMap<SupersedeKey, u64>>>,
    cancelled_request_ids: Arc<Mutex<HashSet<i64>>>,
}

async fn writer_loop() {
    while let Some(envelope) = operation_rx.recv().await {
        // Late decision: Check staleness when dequeuing
        if is_stale_or_cancelled(&envelope).await {
            continue; // Drop stale operations
        }
        write_to_stdin(envelope).await;
    }
}

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

**How it works**:

```
Scenario: User types "pri" → "print" → "printf" rapidly during initialization

T0: send(completion, "pri")   → gen=1 enqueued
T1: send(completion, "print") → gen=2 enqueued, gen=1 marked cancelled
T2: send(completion, "printf")→ gen=3 enqueued, gen=2 marked cancelled
    [Queue now contains: envelope(gen=1), envelope(gen=2), envelope(gen=3)]

T3: Initialization completes, writer loop starts
T4: Dequeue envelope(gen=1) → is_stale? cur_gen=3, yes → drop
T5: Dequeue envelope(gen=2) → is_stale? cur_gen=3, yes → drop
T6: Dequeue envelope(gen=3) → is_stale? cur_gen=3, no → write to server

Result: Only latest request reaches server (correct behavior)
```

**Why rejected**:

**1. Unbounded memory growth during slow initialization**

```
Scenario: User types 100 characters during 5s rust-analyzer initialization

With late decision:
- 100 completion requests enqueued
- All 100 envelopes remain in queue (each ~1KB)
- Total memory: ~100KB for this one document
- If 10 documents actively edited: ~1MB
- If user types 500 chars: ~5MB for completions alone

Risk: OOM crash if initialization takes 10+ seconds under heavy editing
```

**2. Wasted processing cycles**

Writer loop must:
- Dequeue all stale envelopes (99 out of 100)
- Acquire locks to check staleness for each
- Drop each stale envelope
- Only 1 out of 100 envelopes actually written

**3. Lock contention on staleness check**

Every dequeued operation requires:
```rust
let gen_map = self.gen_by_key.lock().await;      // Lock acquisition
let cancelled = self.cancelled_request_ids.lock().await; // Another lock
```

Under high throughput (100+ ops/sec), lock contention becomes bottleneck.

**4. No hard memory bound**

Queue size is `O(total requests sent)`, not `O(unique documents × methods)`:
- 1 document, 100 edits → 100 envelopes queued
- 100 documents, 10 edits each → 1000 envelopes queued
- Scales poorly with user activity during initialization

**Comparison to chosen design**:

| Aspect | Late Decision (Rejected) | Coalescing Map (Chosen) |
|--------|-------------------------|------------------------|
| **Memory during init** | O(total requests) — unbounded | O(unique keys) — bounded |
| **Queue size (100 edits)** | 100 envelopes (~100KB) | 1 envelope (~1KB) |
| **Staleness check** | At dequeue (all items) | Not needed (map has only latest) |
| **Lock contention** | High (every dequeue) | Low (only on send) |
| **OOM risk** | Yes (long init + heavy editing) | No (bounded by documents) |

### Alternative 2: Bounded Queue Only (No Coalescing)

Use `mpsc::Sender<Envelope>` with bounded capacity but no coalescing.

**Design**:

```rust
struct BridgeConnection {
    operation_queue: mpsc::Sender<Envelope>,  // Bounded (capacity: 128)
}

async fn send(&self, operation: BridgeOperation) {
    let envelope = Envelope { operation, key, gen };
    // Blocks when queue full (backpressure)
    self.operation_queue.send(envelope).await.ok();
}
```

**Why rejected**:

**1. Still accumulates stale items (just bounded)**

```
Scenario: User types rapidly, queue capacity = 128

- User types 200 chars during initialization
- First 128 enqueued
- 129th send blocks (backpressure)
- Of the 128 in queue, only 1-2 are fresh, 126+ are stale
- Still wasting 99% of queue capacity on stale items
```

**2. Backpressure blocks user actions**

When queue full:
- `send().await` blocks the LSP request handler
- User's typing appears frozen (completions don't trigger)
- Bad UX during initialization window

**3. No memory improvement over late decision**

Bounded queue prevents unbounded growth, but:
- Max memory still: `capacity × envelope_size` (e.g., 128KB)
- Coalescing map: typically 3-50 envelopes (3-50KB)
- **Coalescing is 3-40× more memory efficient**

**4. Doesn't solve the fundamental problem**

The issue isn't queue size per se — it's storing **obsolete data**:
- Bounded queue: limits obsolete data
- Coalescing map: **eliminates** obsolete data

**Comparison**:

| Aspect | Bounded Queue Only | Coalescing Map |
|--------|-------------------|----------------|
| **Memory bound** | Hard limit (128 envelopes) | Dynamic (unique keys) |
| **Typical memory** | ~128KB (full queue) | ~3-50KB (active docs) |
| **Stale items** | Up to capacity-1 | Zero (replaced immediately) |
| **Backpressure** | Blocks on full queue | Rarely blocks (small map) |
| **UX during init** | May freeze on heavy editing | Smooth (stale items dropped) |

### Why Coalescing Map is Superior

The chosen design (coalescing map + bounded order channel) combines best of both:

1. **Memory efficient**: O(unique keys) not O(requests)
2. **No stale accumulation**: Map replacement frees old envelopes immediately
3. **Smooth backpressure**: Order channel rarely fills (only unique keys)
4. **Clean semantics**: Map naturally represents "latest state per key"

**Key insight**: Superseding is fundamentally a **state synchronization** problem, not a **message queue** problem. Coalescing map is the semantically correct data structure.

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
