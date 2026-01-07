# ADR-0015: Multi-Server Coordination for Bridge Architecture

## Status

Proposed

**Extracted from**: [ADR-0012](0012-multi-ls-async-bridge-architecture.md) (focusing on multi-server aspects)

**Related**:
- [ADR-0013](0013-async-io-layer.md): Async I/O patterns and concurrency primitives
- [ADR-0014](0014-actor-based-message-ordering.md): Single-server message ordering and request superseding

## Context

### The Multi-Server Problem

Real-world usage requires bridging to **multiple downstream language servers** simultaneously for the same language:
- Python code may need both **pyright** (type checking, completion) and **ruff** (linting, formatting)
- Embedded SQL may need both a SQL language server and the host language server
- Future polyglot scenarios (e.g., TypeScript + CSS in Vue files)

Traditional LSP bridges support only **one server per language**. This limitation forces users to choose between complementary tools instead of leveraging their combined strengths.

### Key Challenges

1. **Server Discovery**: How do we identify which servers handle which languages?
2. **Request Routing**: Given a request for a language, which server(s) should receive it?
3. **Lifecycle Management**: How do we spawn, initialize, and shut down multiple servers?
4. **Capability Overlap**: When multiple servers support the same LSP method, how do we decide which to use?
5. **Partial Failures**: What happens when some servers initialize successfully but others fail?

## Decision

Adopt a **routing-first, aggregation-optional** multi-server coordination model that supports 1:N communication patterns (one client → multiple language servers per language).

### 1. Design Principle: Routing First

**Most requests should be routed to a single downstream LS based on capabilities.** Aggregation is only needed when multiple LSes provide overlapping functionality that must be combined.

```
┌──────────────────────────────────────────────────────────────────────────┐
│                           Request Routing                                │
│                                                                          │
│   Incoming Request                                                       │
│         │                                                                │
│         ▼                                                                │
│   ┌─────────────────────────────────────────────────────────────────┐    │
│   │ Which LSes have this capability for this languageId?            │    │
│   └─────────────────────────────────────────────────────────────────┘    │
│         │                                                                │
│         ├── 0 LSes → Return REQUEST_FAILED (-32803) w/ "no provider"     │
│         ├── 1 LS   → Route to single LS (no aggregation needed)          │
│         └── N LSes → Check routing strategy:                             │
│                        ├── SingleByCapability → Pick alphabetically first│
│                        └── FanOut → Send to all, aggregate responses     │
└──────────────────────────────────────────────────────────────────────────┘
```

**Priority order**: Users can explicitly define a `priority` list in the bridge configuration. If not defined, the bridge falls back to a deterministic order based on server names (sorted alphabetically).
- Explicit: `priority: ["ruff", "pyright"]` → `ruff` is checked first
- Default: `pyright` vs `ruff` → `pyright` wins (alphabetical)

**No-provider handling**: Returning `REQUEST_FAILED` with a clear message ("no downstream language server provides hover for python") keeps misconfiguration visible instead of silently returning `null`.

**Example: pyright + ruff for Python**

| Method | pyright | ruff | Routing Strategy |
|--------|---------|------|------------------|
| `hover` | ✅ | ❌ | → pyright only (no aggregation) |
| `definition` | ✅ | ❌ | → pyright only |
| `completion` | ✅ | ✅ | → FanOut + merge_all |
| `formatting` | ❌ | ✅ | → ruff only |
| `codeAction` | ✅ | ✅ | → FanOut + merge_all |
| `diagnostics` | ✅ | ✅ | → Both (notification pass-through, no aggregation) |

### 2. Routing Strategies

```rust
enum RoutingStrategy {
    /// Route to single LS with highest priority (default)
    /// No aggregation needed - fast path
    SingleByCapability {
        priority: Vec<String>,  // e.g., ["pyright", "ruff"]
    },

    /// Fan-out to multiple LSes, aggregate responses
    /// Only for methods where overlapping results must be combined
    FanOut {
        aggregation: AggregationStrategy,
    },
}
```

**When aggregation IS needed (candidate-based methods):**
- `completion`: Both LSes return completion item candidates → merge into single list
- `codeAction`: pyright refactoring candidates + ruff lint fix candidates → merge candidate lists (user selects one for execution)

**When aggregation is NOT needed:**
- Single capable LS → route directly
- Diagnostics → notification pass-through (client aggregates per LSP spec)
- Capabilities don't overlap → route to respective LS

**When aggregation is UNSAFE (direct-edit methods):**
- `formatting`, `rangeFormatting`: Returns text edits directly (no user selection step)
  - **MUST use SingleByCapability routing** — multiple servers would produce conflicting edits
  - Example: If both pyright and ruff could format, their edits would conflict (different indentation, quote styles, etc.)
  - NOT safe to merge: No way to reconcile conflicting text edits for the same range
- `rename`: Returns workspace edits directly across multiple files
  - **MUST use SingleByCapability routing** — multiple rename strategies would corrupt the workspace
- **Rule**: Methods that return direct edits (not proposals) MUST route to single server only

### 3. Server Pool Architecture

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         treesitter-ls (Host LS)                         │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                      LanguageServerPool                            │ │
│  │                                                                    │ │
│  │   ┌─────────────────┐                                              │ │
│  │   │  RequestRouter  │ ─── routes by (method, languageId, caps)     │ │
│  │   └────────┬────────┘                                              │ │
│  │            │                                                       │ │
│  │   ┌────────┴────────┐    Fan-out: scatter to multiple LSes         │ │
│  │   │                 │                                              │ │
│  │   ▼                 ▼                                              │ │
│  │ ┌───────────┐  ┌───────────┐  ┌───────────┐                        │ │
│  │ │  pyright  │  │   ruff    │  │ lua-ls    │  ... per-LS connection │ │
│  │ │(conn + Q) │  │(conn + Q) │  │(conn + Q) │                        │ │
│  │ └─────┬─────┘  └─────┬─────┘  └─────┬─────┘                        │ │
│  │       │              │              │                              │ │
│  │   ┌───┴──────────────┴──────────────┴───┐                          │ │
│  │   │         ResponseAggregator          │  Fan-in: merge/rank      │ │
│  │   └─────────────────────────────────────┘                          │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Design Points:**
- **RequestRouter**: New component that determines which server(s) receive a request
- **Per-Connection Isolation**: Each downstream connection maintains its own request ID namespace and send queue
- **ResponseAggregator**: New component that combines responses when fan-out is used
- **ID Namespace Isolation**: Each downstream connection maintains its own `next_request_id`. The pool maps `(upstream_id, downstream_key)` → `downstream_id` for correlation

### 4. Server Lifecycle Management

#### 4.1 Parallel Multi-Server Initialization

When connecting to multiple downstream language servers, `initialize` requests can be sent in parallel since each server is an independent process with no inter-server dependencies.

```
┌─────────┐     ┌──────────┐     ┌──────────┐
│ Bridge  │     │ pyright  │     │   ruff   │
└────┬────┘     └────┬─────┘     └────┬─────┘
     │               │                │
     │──initialize──▶│                │
     │──initialize───────────────────▶│  (parallel, no wait)
     │               │                │
     │◀──result──────│                │  (pyright responds first)
     │──initialized─▶│                │
     │──didOpen─────▶│                │  (pyright ready, can use)
     │               │                │
     │◀──result───────────────────────│  (ruff responds later)
     │──initialized──────────────────▶│
     │──didOpen──────────────────────▶│  (ruff now ready)
```

Key points:
- **Parallel `initialize`**: Send to all servers concurrently without waiting
- **Independent lifecycle**: Each server's `initialized` → `didOpen` proceeds as soon as that server responds
- **No global barrier**: Servers that initialize faster can start handling requests immediately

#### 4.2 Partial Initialization Failure Policy

When initializing multiple downstream servers in parallel, some may succeed while others fail. The bridge handles this gracefully:

| Scenario | Behavior | Rationale |
|----------|----------|-----------|
| All servers initialize successfully | Normal operation with all servers | Expected case |
| Some servers fail initialization | Continue with working servers, failed servers enter circuit breaker open state | Graceful degradation - pyright failures shouldn't block ruff usage |
| All servers fail initialization | Bridge reports errors but remains alive, circuit breakers prevent request routing | Allow recovery without full bridge restart |

**Error propagation:**
- Failed `initialize` requests trigger circuit breaker for that specific server
- Requests routed to failed servers receive `REQUEST_FAILED` with circuit breaker message
- Client sees degraded functionality (e.g., "pyright unavailable") rather than total failure
- **Fan-out awareness**: If a method is configured for aggregation (e.g., completion merge_all) and one server is in circuit breaker/open or still uninitialized, the router skips it and proceeds with the available servers. The aggregator marks the response as partial in `data` so UX continues instead of blocking on an unhealthy peer.

#### 4.3 Per-Downstream Document Lifecycle

Maintain the latest host-document snapshot per downstream. When a slower server reaches its `didOpen`, send the full text as of "now", not as of when the first downstream opened.

**Document Lifecycle State** (per downstream server, per document URI):

```
States: NotOpened | Opened | Closed

Transitions:
- NotOpened → Opened      (didOpen sent to downstream)
- Opened → Closed         (didClose sent to downstream)
- NotOpened → Closed      (didClose received before didOpen sent - suppress didOpen)
```

**Notification Handling by State**:

| Notification | NotOpened State | Opened State | Closed State |
|--------------|----------------|--------------|--------------|
| `didChange` | **DROP** (didOpen will contain current state) | **FORWARD** | **SUPPRESS** |
| `didSave` | **DROP** | **FORWARD** | **SUPPRESS** |
| `willSave` | **DROP** | **FORWARD** | **SUPPRESS** |
| `didClose` | Transition to **Closed**, suppress pending didOpen | **FORWARD**, transition to **Closed** | Already closed |

**Example - Multi-server parallel initialization**:

```
┌────────┐     ┌──────────────┐     ┌──────────┐     ┌──────────┐
│ Client │     │ treesitter-ls│     │ pyright  │     │   ruff   │
└───┬────┘     └──────┬───────┘     └────┬─────┘     └────┬─────┘
    │──didOpen(md)───▶│                  │                │
    │                 │ (spawn both servers)              │
    │                 │                  │                │
    │──didChange(md)─▶│ ❌ DROP both     │                │  ← Both NotOpened
    │                 │ (initializing...)│                │
    │                 │◀──init result────│                │
    │                 │──initialized────▶│                │
    │                 │──didOpen(virt)──▶│                │  ← pyright: NotOpened→Opened
    │                 │                  │                │
    │──didChange(md)─▶│──didChange(virt)▶│                │  ← pyright: FORWARD
    │                 │ ❌ DROP ruff      │                │  ← ruff: still NotOpened
    │                 │                  │                │
    │                 │◀──────────init result─────────────│
    │                 │──initialized─────────────────────▶│
    │                 │──didOpen(virt)───────────────────▶│  ← ruff: NotOpened→Opened
    │                 │                  │                │     (includes ALL changes)
    │──didChange(md)─▶│──didChange(virt)▶│                │
    │                 │──didChange(virt)─────────────────▶│  ← Both: FORWARD
```

**Edge Cases**:

1. **didClose before didOpen sent**:
   - Transition: `NotOpened → Closed`
   - Suppress pending didOpen (prevent ghost document)
   - Example: User closes file while server still initializing

2. **didClose after didOpen sent, during initialization**:
   - Server state: `Initializing`, document state: `Opened`
   - Forward didClose immediately (preserve LSP protocol pairing)
   - Document transitions: `Opened → Closed`

**Why drop instead of queue**: The `didOpen` notification contains the complete document text at send time. Accumulated client edits are included. Dropping `didChange` before `didOpen` avoids duplicate state updates and simplifies state management.

#### 4.4 Multi-Server Backpressure Coordination

**Decision: Accept state divergence under extreme backpressure** (non-atomic broadcast)

When routing notifications to multiple downstream servers, if one server's queue is full (per ADR-0014 § Non-Blocking Backpressure), notifications are handled independently per server.

**Strategy**:

```
Router sends didSave to 3 servers:
├─ pyright: queue full → DROP (per ADR-0014)
├─ ruff: queue OK → FORWARD
└─ lua-ls: queue OK → FORWARD

Result: pyright doesn't see didSave, ruff and lua-ls do (STATE DIVERGENCE)
```

**Why accept divergence**: This is equivalent to attaching language servers to a real file at different times.

**Real-world analogy**:
```
User opens Python file:
  ├─ pyright attached immediately
  ├─ User makes edits
  └─ ruff attached 5 seconds later (user starts ruff-server manually)

Result: pyright sees edit history, ruff sees current snapshot (STATE DIVERGENCE)
```

Language servers already handle being attached at arbitrary points in a document's lifetime. Each server receives its own stream of notifications and builds its own view of document state. Temporary divergence under backpressure is architecturally equivalent to staggered attachment timing.

**Characteristics**:

| Aspect | Decision | Rationale |
|--------|----------|-----------|
| **Atomic broadcast** | ❌ Rejected | Requires distributed transaction; blocks healthy servers on slowest server |
| **Independent delivery** | ✅ Accepted | Keeps healthy servers synchronized; matches real-world attachment timing |
| **Recovery mechanism** | Automatic | Next coalescable notification (didChange) re-synchronizes state |
| **Divergence window** | Temporary | Only affects non-coalescable notifications (didSave, willSave); didChange is never dropped (stored in coalescing map per ADR-0014) |

**Trade-offs**:

- **Advantage**: System remains responsive under load; healthy servers continue working
- **Disadvantage**: Servers may temporarily have inconsistent view of save state
- **Mitigation**: Coalescable notifications (didChange) are never dropped, ensuring content synchronization
- **LSP compliance**: Each server receives a valid notification stream; no protocol violations

**Example scenario - Extreme backpressure**:

```
T0: User saves file (didSave notification)
T1: Route to pyright, ruff, lua-ls
    ├─ pyright: queue full (256 operations queued, slow initialization)
    │   └─ DROP didSave (log warning)
    ├─ ruff: queue OK → FORWARD didSave
    └─ lua-ls: queue OK → FORWARD didSave

T2: User edits file (didChange notification - coalescable)
T3: Route to all servers
    ├─ pyright: queue still full → Store in coalescing map (NOT dropped)
    ├─ ruff: queue OK → FORWARD didChange
    └─ lua-ls: queue OK → FORWARD didChange

T4: pyright initialization completes, queue drains
T5: pyright processes didChange from coalescing map
    └─ State synchronized (content matches ruff and lua-ls)
    └─ didSave notification was missed (non-critical for content sync)
```

**Conclusion**: State divergence is an acceptable trade-off. Prefer **availability** (healthy servers continue working) over **consistency** (all servers see identical notification sequence) under extreme backpressure.

### 5. Notification Pass-through

**Diagnostics and other server-initiated notifications do NOT require aggregation.**

`textDocument/publishDiagnostics` is a notification (no `id`, no response expected). Each downstream server can publish its own diagnostics independently:

```
pyright  ──publishDiagnostics──►  bridge  ──publishDiagnostics──►  upstream
ruff     ──publishDiagnostics──►  bridge  ──publishDiagnostics──►  upstream
                                  (pass-through, no merge)
```

The bridge simply:
1. Receives notification from downstream
2. Transforms URI (virtual → host document URI)
3. Forwards to upstream client

The client (e.g., VSCode) automatically aggregates diagnostics from multiple sources. This is the standard LSP behavior and requires no special handling.

**Other pass-through notifications:**
- `$/progress` — Already forwarded via `notification_sender` channel
- `window/logMessage` — Can be forwarded as-is
- `window/showMessage` — Can be forwarded as-is

### 6. Cancellation Propagation and Coalescing Handoff

#### 6.1 Request Lifecycle Phases

Requests transition through distinct phases, each managed by different ADR components:

```
┌─────────────────────────────────────────────────────────────────────┐
│ Phase 1: Enqueued (ADR-0014 domain)                                │
│ - Request in unified order queue or coalescing map                 │
│ - Not yet sent to downstream server                                │
│ - Managed by: Connection actor (ADR-0014)                          │
│ - Cancellation: Remove from map/queue if present, ignore if        │
│               already superseded                                    │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ Writer loop dequeues
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│ TRANSITION POINT: Register in pending_correlations                 │
│ - Responsibility: Connection writer loop (ADR-0014)                │
│ - Timing: BEFORE writing to server stdin                           │
└─────────────────────────────────────────────────────────────────────┘
                              │
                              │ Write to stdin
                              ▼
┌─────────────────────────────────────────────────────────────────────┐
│ Phase 2: Pending (ADR-0015 domain)                                 │
│ - Request sent to downstream server stdin                          │
│ - Awaiting response from server                                    │
│ - Managed by: Router/pool (ADR-0015)                               │
│ - Cancellation: Propagate $/cancelRequest to downstream            │
└─────────────────────────────────────────────────────────────────────┘
```

#### 6.2 Cancellation Handling by Phase

**Phase 1 (Enqueued) - Request in Queue or Coalescing Map**:

Cancellation in Phase 1 has **two sub-cases**:

**Sub-case 1a: Request still enqueued (not yet superseded)**:

```
Upstream sends $/cancelRequest for request ID=42:
├─ Check pending_correlations for ID=42
├─ NOT FOUND (request not yet sent to downstream)
├─ Forward to connection actor (ADR-0014):
│   ├─ Remove from coalescing map (if coalescable and present)
│   ├─ Mark in order queue for skipping (if not yet dequeued)
│   └─ Send REQUEST_CANCELLED response to upstream
└─ Request never reaches downstream server
```

**Sub-case 1b: Request already superseded**:

```
Upstream sends $/cancelRequest for request ID=1:
├─ Check pending_correlations for ID=1
├─ NOT FOUND (request not yet sent to downstream)
├─ Forward to connection actor (ADR-0014):
│   ├─ NOT in coalescing map (was replaced by ID=2)
│   ├─ Already received REQUEST_CANCELLED response (via superseding)
│   └─ IGNORE cancellation (already processed)
└─ No action needed
```

**Rationale**:
- **Sub-case 1a**: Request is still enqueued, waiting to be sent. Removing it from the queue prevents unnecessary downstream processing. Connection actor responds with `REQUEST_CANCELLED` to satisfy LSP protocol requirement.
- **Sub-case 1b**: Coalescable request (e.g., completion) was superseded by a newer request and already received `REQUEST_CANCELLED` response (ADR-0014 § Generation-Based Coalescing). Subsequent `$/cancelRequest` from upstream is redundant.

**Phase 2 (Pending) - Request Sent to Downstream**:

```
Upstream sends $/cancelRequest for request ID=42:
├─ Check pending_correlations for ID=42
├─ FOUND: [(pyright, 101), (ruff, 205)]
├─ Propagate $/cancelRequest to all downstream servers:
│   ├─ pyright: send $/cancelRequest {id: 101}
│   └─ ruff: send $/cancelRequest {id: 205}
└─ Remove ID=42 from pending_correlations
```

**Rationale**: Request was sent to downstream servers and is awaiting response. Propagate cancellation to allow servers to abort processing (though they may legally ignore it per LSP spec).

#### 6.3 Handoff Protocol (ADR-0014 ↔ ADR-0015 Coordination)

**Writer loop responsibilities** (ADR-0014):

```rust
// In connection writer loop (ADR-0014 domain)
loop {
    let operation = order_queue.recv().await;

    match operation {
        Request { id, method, params, response_tx } => {
            // 1. Remove from coalescing map (if coalescable)
            coalescing_map.remove((uri, method));

            // 2. HANDOFF: Register in pending_correlations (ADR-0015 domain)
            //    This MUST happen BEFORE writing to stdin
            pool.register_pending_request(upstream_id: id, downstream_id: next_id);

            // 3. Write request to server stdin
            write_request(next_id, method, params).await?;

            // 4. Store response waiter
            response_waiters.insert(next_id, response_tx);
        }
    }
}
```

**Router/pool responsibilities** (ADR-0015):

```rust
// In LanguageServerPool (ADR-0015 domain)
async fn handle_cancel_request(&self, upstream_id: i64) {
    // Check if request is in pending_correlations (Phase 2)
    if let Some(downstream_requests) = self.pending_correlations.get(&upstream_id) {
        // Request was sent to downstream - propagate cancellation
        for (downstream_key, downstream_id) in downstream_requests {
            if let Some(conn) = self.connections.get(&downstream_key) {
                let _ = conn.send_notification("$/cancelRequest", json!({
                    "id": downstream_id
                })).await;
            }
        }
        self.pending_correlations.remove(&upstream_id);
    } else {
        // Request not in pending_correlations
        // Either: (1) Already superseded and responded with REQUEST_CANCELLED
        //         (2) Already completed and responded
        //         (3) Never reached writer loop (gated by connection state)
        // Action: Ignore cancellation (already processed)
    }
}
```

**Tracking Structure:**

```rust
/// Maps upstream request ID to downstream request IDs for cancellation propagation
/// Managed by: LanguageServerPool (ADR-0015)
/// Updated by: Connection writer loops (ADR-0014) via handoff protocol
pending_correlations: DashMap<i64, Vec<(String, i64)>>, // upstream_id → [(downstream_key, downstream_id)]
```

#### 6.4 Cancellation Scenarios

**Scenario 1a: Request Cancelled While Still Enqueued**

```
T0: User requests hover ID=42 → enqueued in order queue
T1: Upstream sends $/cancelRequest for ID=42
    └─ pending_correlations check: NOT FOUND (not yet sent)
    └─ Forward to connection actor:
        ├─ Remove from coalescing map (if present)
        ├─ Mark in queue for skipping
        └─ Send REQUEST_CANCELLED response to upstream
T2: Writer loop dequeues ID=42
    └─ Skip (marked as cancelled)
    └─ Request never sent to downstream server
```

**Scenario 1b: Superseded Request Cancelled**

```
T0: User types "foo" → completion request ID=1 enqueued
T1: User types "o" → completion request ID=2 enqueued (supersedes ID=1)
    └─ ID=1 immediately receives REQUEST_CANCELLED response (via superseding)
    └─ ID=1 removed from coalescing map (replaced by ID=2)
T2: Upstream sends $/cancelRequest for ID=1
    └─ pending_correlations check: NOT FOUND
    └─ Forward to connection actor:
        └─ NOT in coalescing map (already superseded)
        └─ IGNORE (already got REQUEST_CANCELLED response)
```

**Scenario 2: Sent Then Cancelled**

```
T0: User requests hover ID=42
T1: Writer loop dequeues, registers in pending_correlations
    └─ pending_correlations[42] = [(pyright, 101)]
T2: Writer loop sends to pyright stdin
T3: Upstream sends $/cancelRequest for ID=42
    └─ pending_correlations check: FOUND
    └─ Action: Send $/cancelRequest {id: 101} to pyright
T4: Clean up pending_correlations[42]
```

**Scenario 3: Multi-Server Fan-Out Cancellation**

```
T0: User requests completion ID=99 (fan-out to pyright + ruff)
T1: Router registers in pending_correlations
    └─ pending_correlations[99] = [(pyright, 201), (ruff, 305)]
T2: Both servers receive requests
T3: Upstream sends $/cancelRequest for ID=99
    └─ Propagate to both:
        ├─ pyright: $/cancelRequest {id: 201}
        └─ ruff: $/cancelRequest {id: 305}
T4: Clean up pending_correlations[99]
```

**Downstream non-compliance**: Servers may legally ignore `$/cancelRequest` (LSP § `$` notifications). Timeouts on fan-out aggregation remain the hard ceiling to guarantee the upstream request still completes.

**Upstream response to cancellation**: Always return a response to the client after propagating cancellation. Use the standard LSP `RequestCancelled` code (-32800) when the method is server-cancellable; otherwise use `REQUEST_FAILED` with a `"cancelled"` message. Never leave the upstream request pending—cancellation must still round-trip a response per LSP.

### 7. Response Aggregation Strategies

For fan-out **requests** (with `id`), configure aggregation per method:

```rust
enum AggregationStrategy {
    /// Return first successful response, cancel others
    FirstWins,

    /// Wait for all, merge array results (e.g., completion items, code action candidates).
    /// Note: This merges CANDIDATE LISTS, not execution results.
    /// - Completion: Merge completion item candidates from multiple servers
    /// - CodeAction: Merge code action candidates; user selects one, which is then executed individually
    /// Challenge: Deduplication is complex - servers may propose similar items with subtle differences.
    /// Safe by design: User selects one item for execution; no auto-execution conflicts.
    MergeAll {
        dedup_key: Option<String>,  // e.g., 'label' for completions
        max_items: Option<usize>,   // limit total items
    },

    /// Wait for all, return highest priority non-null result
    Ranked {
        priority: Vec<String>,  // server keys in priority order
    },
}
```

**Aggregation stability rules:**
- **Per-request timeout conditions**: Timeout applies **only when n ≥ 2 downstream servers participate in aggregation**
  - SingleByCapability: No per-request timeout (wait indefinitely for the single capable server, idle timeout per ADR-0013 protects against zombie)
  - FanOut with n=1: No per-request timeout (functionally equivalent to SingleByCapability)
  - FanOut with n≥2: Per-request timeout applies (default: 5s explicit, 2s incremental)
- **Per-request timeout behavior**: On timeout, aggregator returns whatever results are available **without sending $/cancelRequest to downstream servers**
  - Downstream servers continue processing and send responses
  - Late responses are **discarded** by router but **reset idle timeout** (serve as heartbeat for connection health)
  - Rationale: Server health independent of aggregation latency; responses act as natural heartbeat signal
- **Partial results**: If at least one downstream succeeds, respond with a successful `result` that contains merged items plus partial metadata in-band (e.g., `{ "items": [...], "partial": true, "missing": ["ruff"] }`)
- **Total failure**: If all downstreams fail or time out, respond with a single `ResponseError` (`REQUEST_FAILED`) describing the missing/unhealthy servers
- **Partial results are explicit**: Partial metadata must live inside the successful `result` payload (not an `error` field) to keep the wire response LSP-compliant while still surfacing degradation

### 8. Configuration Example

```yaml
# Configuration example (routing-first approach)
#
# Server discovery: languageServers with matching `languages` field are
# automatically used for that injection language. No explicit server list
# needed in bridges config.
#
# Priority order can be explicitly configured. If `priority` is omitted,
# it defaults to alphabetical order of server names.

languages:
  markdown:
    bridges:
      python:
        # Servers discovered from languageServers with languages: [python]
        priority: ["ruff", "pyright"] # Explicitly prioritize ruff
        # Default: single_by_capability routing (no aggregation config needed)
        #
        # Only configure methods that need non-default behavior:
        # IMPORTANT: Only candidate-based methods are safe for merge_all
        aggregations:
          textDocument/completion:
            strategy: merge_all      # Safe: candidates merged, user selects one
            dedup_key: label
          textDocument/codeAction:
            strategy: merge_all      # Safe: proposals merged, user selects one for execution
          # hover, definition: use default (single_by_capability, no config)
          # formatting, rename: MUST use single_by_capability (direct edits cannot be merged)

languageServers:
  pyright:
    cmd: [pyright-langserver, --stdio]
    languages: [python]              # ← auto-discovered for python bridges
  ruff:
    cmd: [ruff, server]
    languages: [python]              # ← auto-discovered for python bridges
```

## Consequences

### Positive

- **Complementary Tools**: Users can leverage multiple specialized tools for the same language (e.g., pyright + ruff)
- **Routing-First Simplicity**: Most requests go to a single LS — no aggregation overhead for common cases
- **Minimal Configuration**: Default capability-based routing works without per-method config
- **Graceful Degradation**: Partial initialization failures allow working servers to continue serving requests
- **Fault Isolation**: One crashed LS doesn't affect others (circuit breaker + bulkhead)
- **Parallel Initialization**: Multiple servers initialize concurrently without global barriers
- **Independent Lifecycles**: Faster servers can start handling requests immediately
- **Flexible Aggregation**: Per-method control over how responses are combined (when needed)
- **Cancellation Propagation**: Client cancellations propagated to all downstream servers
- **No Silent Failures**: Missing providers surface as explicit errors instead of `null` results
- **Backward Compatible**: Single-LS configurations continue to work unchanged

### Negative

- **Configuration Surface**: Users need to understand aggregation strategies and routing constraints
  - Must know which methods are safe for aggregation (candidate-based: completion, codeAction)
  - Must know which methods are UNSAFE for aggregation (direct-edit: formatting, rename)
  - Misconfiguration could cause data corruption (e.g., configuring formatting for FanOut would produce conflicting edits)
- **Aggregation Complexity**: Merging candidate lists (completion items, codeAction proposals) requires deduplication logic
  - Challenge: Different servers may propose similar candidates with subtle differences (labels, kinds, descriptions)
  - Making it hard to decide what counts as a "duplicate"
  - Note: Only safe for candidate-based methods where user selects ONE item; direct-edit methods MUST use SingleByCapability
- **Latency**: Fan-out with `merge_all` waits up to per-server timeouts; partial results may surface instead of complete lists
- **Memory**: Tracking pending correlations adds overhead
- **Coordination Complexity**: More state to manage (correlations, circuit breakers, aggregators)

### Neutral

- **Existing Tests**: Current single-LS tests remain valid
- **Incremental Adoption**: Routing-first means aggregation can be added later for specific methods
- **Diagnostics**: Pass-through by design — client handles aggregation

## Implementation Plan

### Phase 1: Single-LS-per-Language Foundation

**Scope**: Support **one language server per language** (multiple languages supported, but each language uses only one LS)

```
treesitter-ls (host)
  ├─→ pyright  (Python only)
  ├─→ lua-ls   (Lua only)
  └─→ sqlls    (SQL only)
```

**What works in Phase 1:**
- Multiple embedded languages in same document (Python, Lua, SQL blocks in markdown)
- Parallel initialization of multiple LSes: Each LS initializes independently with no global barrier
- Per-downstream snapshotting: Late initializers receive latest document state, not stale snapshot
- Simple routing: language → single LS (no aggregation needed)
- Routing errors surfaced: `REQUEST_FAILED` when no provider exists (no silent `null`)

**What Phase 1 does NOT support:**
- Multiple LSes for same language (e.g., Python → pyright + ruff)
- Fan-out / scatter-gather for requests
- Response aggregation/merging

**Exit Criteria:**
- All existing single-LS tests pass without hangs
- Can handle Python, Lua, SQL blocks simultaneously in markdown
- No initialization race failures under normal conditions

### Phase 2: Resilience Patterns (Stability Before Complexity)

**Scope**: Add fault isolation and recovery patterns to **single-LS-per-language** setup before adding multi-LS complexity

**Why Phase 2 before Multi-LS:**
- Resilience patterns work with simple single-LS architecture
- Stabilize foundation before adding aggregation complexity
- Circuit breaker and bulkhead become MORE critical with multi-LS (Phase 3)
- Better to debug resilience issues without aggregation layer

**What Phase 2 adds:**
- Circuit Breaker: Prevent cascading failures when downstream LS is unhealthy
- Bulkhead Pattern: Isolate downstream servers to prevent resource exhaustion
- Per-server timeout configuration: Custom timeout per LS type
- Health monitoring: Track LS health metrics
- Partial-result metadata: Flag degraded responses

**Exit Criteria:**
- Circuit breaker opens/closes correctly when LS crashes/recovers
- Bulkhead prevents slow LS from blocking other languages
- System remains responsive even when one LS is unhealthy

### Phase 3: Multi-LS-per-Language with Aggregation

**Scope**: Extend to support **multiple language servers per language** with routing and aggregation

```
treesitter-ls (host)
  └─→ Python blocks
        ├─→ pyright  (type checking, completion) ← Circuit breaker from Phase 2
        └─→ ruff     (linting, formatting)       ← Bulkhead from Phase 2
             ↓
        [RequestRouter + ResponseAggregator] ← New in Phase 3
```

**What Phase 3 adds:**
- Routing strategies: single-by-capability (default) and fan-out
- Response aggregation: merge_all, first_wins, ranked strategies
- Per-method aggregation configuration: Configure only methods that need non-default behavior
- Cancellation propagation: Propagate `$/cancelRequest` to all downstream LSes with pending requests
- Fan-out skip/partial: Unhealthy or uninitialized servers skipped in aggregation
- Leverages Phase 2 resilience: Each LS in multi-LS setup already has circuit breaker + bulkhead

**Exit Criteria:**
- Can use pyright + ruff simultaneously for Python
- Completion item candidates merged from both LSes with deduplication working correctly
- CodeAction candidate lists merged without duplicate proposals in UI
- User can select any candidate; execution goes to specific server individually
- Routing config works (single-by-capability default, fan-out for configured methods)
- Resilience patterns work per-LS (pyright circuit breaker independent of ruff)
- Partial results surfaced when one LS times out or is unhealthy

**Rationale for phased approach:**
- Phase 1 delivers immediate value (multi-language support) with minimal complexity
- Phase 2 adds stability/resilience to simple architecture (easier to debug)
- Phase 3 adds multi-LS complexity on top of stable, resilient foundation
- Routing-first principle means most requests still use Phase 1 fast path (single LS)

## Related ADRs

- **[ADR-0006](0006-language-server-bridge.md)**: Core LSP bridge architecture
  - Establishes the fundamental 1:1 bridge pattern (host document → single language server per language)
  - ADR-0015 extends this to 1:N (host document → multiple language servers per language)

- **[ADR-0008](0008-language-server-bridge-request-strategies.md)**: Per-method bridge strategies
  - ADR-0008's per-method strategies remain valid for single-LS routing
  - ADR-0015 clarifies multi-LS aspects: diagnostics pass-through, fan-out routing, aggregation strategies

- **[ADR-0012](0012-multi-ls-async-bridge-architecture.md)**: Multi-LS async bridge architecture **(Parent ADR)**
  - This ADR extracts multi-server coordination decisions from ADR-0012
  - ADR-0012 will be superseded by ADR-0013 (async I/O), ADR-0014 (message ordering), and ADR-0015 (multi-server coordination)

- **[ADR-0013](0013-async-io-layer.md)**: Async I/O infrastructure
  - Provides the async I/O patterns and concurrency primitives that enable parallel server management

- **[ADR-0014](0014-actor-based-message-ordering.md)**: Message ordering and request superseding
  - Handles single-server message ordering concerns (didOpen before didChange)
  - ADR-0015 coordinates multiple servers; ADR-0014 ensures correct ordering within each server connection

- **[ADR-0016](0016-graceful-shutdown.md)**: Graceful shutdown and connection lifecycle
  - Defines shutdown coordination for multiple concurrent connections
  - ADR-0015 router broadcasts shutdown; ADR-0016 specifies per-connection shutdown sequence and multi-server timeout policy
