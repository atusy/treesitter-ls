# ADR-0015: Multi-Server Coordination for Bridge Architecture

| | |
|---|---|
| **Status** | Draft |
| **Date** | 2026-01-07 |

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

**Adopt a routing-first, aggregation-optional multi-server coordination model that supports 1:N communication patterns (one client → multiple language servers per language).**

### Design Principle: Routing First

Most requests should be routed to a single downstream server based on capabilities. Aggregation is only needed when multiple servers provide overlapping functionality that must be combined.

**Priority Order:**
- Users can explicitly define a `priority` list in bridge configuration
- If not defined, fall back to deterministic alphabetical order of server names
- Example: `priority: ["ruff", "pyright"]` → ruff checked first; default: pyright wins (alphabetical)

**No-Provider Handling:** Return `REQUEST_FAILED` with clear message ("no downstream language server provides hover for python") to keep misconfiguration visible.

**Example: pyright + ruff for Python**

| Method | pyright | ruff | Routing Strategy |
|--------|---------|------|------------------|
| `hover` | ✅ | ❌ | → pyright only (no aggregation) |
| `definition` | ✅ | ❌ | → pyright only |
| `completion` | ✅ | ✅ | → FanOut + merge_all |
| `formatting` | ❌ | ✅ | → ruff only |
| `codeAction` | ✅ | ✅ | → FanOut + merge_all |
| `diagnostics` | ✅ | ✅ | → Both (notification pass-through) |

## Architecture

### Server Pool Architecture

```
┌─────────────────────────────────────────────────────────┐
│                   treesitter-ls (Host LS)               │
│  ┌────────────────────────────────────────────────────┐ │
│  │              LanguageServerPool                    │ │
│  │                                                    │ │
│  │   ┌─────────────────┐                              │ │
│  │   │  RequestRouter  │ ── routes by (method,        │ │
│  │   │                 │     languageId, caps)        │ │
│  │   └────────┬────────┘                              │ │
│  │            │                                       │ │
│  │   ┌────────┴────────┐    Fan-out to multiple LSes  │ │
│  │   │                 │                              │ │
│  │   ▼                 ▼                              │ │
│  │ ┌───────────┐  ┌───────────┐  ┌───────────┐        │ │
│  │ │  pyright  │  │   ruff    │  │ lua-ls    │        │ │
│  │ │(conn + Q) │  │(conn + Q) │  │(conn + Q) │        │ │
│  │ └─────┬─────┘  └─────┬─────┘  └─────┬─────┘        │ │
│  │       │              │              │              │ │
│  │   ┌───┴──────────────┴──────────────┴───┐          │ │
│  │   │         ResponseAggregator          │          │ │
│  │   │            (Fan-in)                 │          │ │
│  │   └─────────────────────────────────────┘          │ │
│  └────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────┘
```

**Key Design Points:**
- **RequestRouter**: Determines which server(s) receive a request
- **Per-Connection Isolation**: Each downstream connection maintains its own send queue (ADR-0014)
- **ResponseAggregator**: Combines responses when fan-out is used
- **Request ID Semantics**: Upstream request IDs used directly for downstream servers (no transformation)

### Request ID Semantics

**Decision**: Use upstream request IDs directly for downstream servers.

**ID Flow:**
```
Client (editor)          treesitter-ls           Downstream Servers
     ├─ completion ID=42 ─→ Router
                             ├─ pyright (ID=42)
                             └─ ruff (ID=42)
```

**Tracking Structure:**
```rust
/// Maps request ID to response handlers for all servers handling that request
/// Request ID type per LSP spec: integer | string
pending_responses: DashMap<RequestId, Vec<(String, oneshot::Sender<ResponseResult>)>>
                   //       ↑               ↑                    ↑
                   //   Request ID      Server Key          Response sender
                   //   (int | string)

// Example entry:
// 42 → [("pyright", tx1), ("ruff", tx2)]
```

**Benefits:**
- Single map lookup for cancellation (no correlation indirection)
- Request ID consistent across client → bridge → servers
- Simpler state management (one entry per request)

**Safety:**
- No ID collision risk (single upstream client)
- Each request ID is unique per client connection

### Routing Strategies

```rust
enum RoutingStrategy {
    /// Route to single LS with highest priority (default)
    SingleByCapability {
        priority: Vec<String>,  // e.g., ["pyright", "ruff"]
    },

    /// Fan-out to multiple LSes, aggregate responses
    FanOut {
        aggregation: AggregationStrategy,
    },
}
```

**When Aggregation IS Needed (Candidate-Based Methods):**
- `completion`: Both servers return candidates → merge into single list
- `codeAction`: pyright refactoring + ruff lint fixes → merge candidates (user selects one for execution)

**When Aggregation is NOT Needed:**
- Single capable server → route directly
- Diagnostics → notification pass-through (client aggregates per LSP spec)
- Capabilities don't overlap → route to respective server

**When Aggregation is UNSAFE (Direct-Edit Methods):**
- `formatting`, `rangeFormatting`: Returns text edits directly (no user selection)
  - **MUST use SingleByCapability** — multiple servers would produce conflicting edits
- `rename`: Returns workspace edits directly across files
  - **MUST use SingleByCapability** — multiple rename strategies would corrupt workspace

**Rule**: Methods that return direct edits (not proposals) MUST route to single server only.

### Server Lifecycle Management

**Parallel Multi-Server Initialization:**

When connecting to multiple downstream servers, `initialize` requests sent in parallel since each server is independent.

```
┌─────────┐     ┌──────────┐     ┌──────────┐
│ Bridge  │     │ pyright  │     │   ruff   │
└────┬────┘     └────┬─────┘     └────┬─────┘
     │──initialize──▶│                │
     │──initialize───────────────────▶│  (parallel, no wait)
     │◀──result──────│                │  (pyright responds first)
     │──initialized─▶│                │
     │──didOpen─────▶│                │  (pyright ready)
     │◀──result───────────────────────│  (ruff responds later)
     │──initialized──────────────────▶│
     │──didOpen──────────────────────▶│  (ruff now ready)
```

**Key Points:**
- **Parallel `initialize`**: Send to all servers concurrently
- **Independent lifecycle**: Each server's `initialized` → `didOpen` proceeds as soon as that server responds
- **No global barrier**: Servers that initialize faster can start handling requests immediately

**Partial Initialization Failure Policy:**

| Scenario | Behavior | Rationale |
|----------|----------|-----------|
| All servers initialize successfully | Normal operation | Expected case |
| Some servers fail | Continue with working servers, failed enter circuit breaker open state | Graceful degradation |
| All servers fail | Bridge reports errors but remains alive, circuit breakers prevent routing | Allow recovery without restart |

**Fan-out awareness**: If a method is configured for aggregation and one server is unhealthy/uninitialized, router skips it and proceeds with available servers. Aggregator marks response as partial so UX continues instead of blocking.

**Per-Downstream Document Lifecycle:**

Maintain the latest host-document snapshot per downstream. When a slower server reaches `didOpen`, send the full text as of "now", not as of when the first downstream opened.

**Document Lifecycle States** (per downstream, per URI):

```
States: NotOpened | Opened | Closed

Transitions:
- NotOpened → Opened      (didOpen sent to downstream)
- Opened → Closed         (didClose sent to downstream)
- NotOpened → Closed      (didClose before didOpen - suppress didOpen)
```

**Notification Handling by State:**

| Notification | NotOpened State | Opened State | Closed State |
|--------------|----------------|--------------|--------------|
| `didChange` | **DROP** (didOpen contains current state) | **FORWARD** | **SUPPRESS** |
| `didSave` | **DROP** | **FORWARD** | **SUPPRESS** |
| `willSave` | **DROP** | **FORWARD** | **SUPPRESS** |
| `didClose` | Transition to **Closed**, suppress pending didOpen | **FORWARD**, transition to **Closed** | Already closed |

**Why drop instead of queue**: The `didOpen` notification contains the complete document text at send time. Accumulated client edits are included. Dropping `didChange` before `didOpen` avoids duplicate state updates.

**Multi-Server Backpressure Coordination:**

**Decision**: Accept state divergence under extreme backpressure (non-atomic broadcast).

When routing notifications to multiple downstream servers, if one server's queue is full (per ADR-0014), notifications are handled independently per server.

**Strategy:**
```
Router sends didSave to 3 servers:
├─ pyright: queue full → DROP (per ADR-0014)
├─ ruff: queue OK → FORWARD
└─ lua-ls: queue OK → FORWARD

Result: pyright doesn't see didSave, ruff and lua-ls do (STATE DIVERGENCE)
```

**Why Accept Divergence**: This is equivalent to attaching language servers to a real file at different times. Servers already handle being attached at arbitrary points in a document's lifetime.

**Recovery**: Next coalescable notification (didChange) re-synchronizes state.

### Notification Pass-Through

**Diagnostics and other server-initiated notifications do NOT require aggregation.**

```
pyright  ──publishDiagnostics──►  bridge  ──publishDiagnostics──►  upstream
ruff     ──publishDiagnostics──►  bridge  ──publishDiagnostics──►  upstream
                                  (pass-through, no merge)
```

The bridge:
1. Receives notification from downstream
2. Transforms URI (virtual → host document URI)
3. Forwards to upstream client

The client (e.g., VSCode) automatically aggregates diagnostics from multiple sources per LSP standard behavior.

**Other Pass-Through Notifications:**
- `$/progress` — Already forwarded via notification channel
- `window/logMessage` — Forwarded as-is
- `window/showMessage` — Forwarded as-is

### Cancellation Propagation

**Request Lifecycle Phases:**

```
┌─────────────────────────────────────────────────────────┐
│ Phase 1: Enqueued (ADR-0014 domain)                    │
│ - Request in order queue or coalescing map             │
│ - Not yet sent to downstream server                    │
│ - Cancellation: Remove from map/queue if present       │
└─────────────────────────────────────────────────────────┘
                      │
                      │ Writer loop dequeues
                      ▼
┌─────────────────────────────────────────────────────────┐
│ TRANSITION: Register in pending_responses              │
│ - Connection writer loop (ADR-0014)                    │
│ - BEFORE writing to server stdin                       │
└─────────────────────────────────────────────────────────┘
                      │
                      │ Write to stdin
                      ▼
┌─────────────────────────────────────────────────────────┐
│ Phase 2: Pending (ADR-0015 domain)                     │
│ - Request sent to downstream server stdin              │
│ - Awaiting response                                    │
│ - Cancellation: Propagate $/cancelRequest to downstream│
└─────────────────────────────────────────────────────────┘
```

**Cancellation Handling by Phase:**

**Phase 1 (Enqueued):**
- **Sub-case 1a**: Request still enqueued → Connection actor removes from map/queue, sends REQUEST_CANCELLED
- **Sub-case 1b**: Request already superseded → Ignore (already got REQUEST_CANCELLED via superseding)

**Phase 2 (Pending):**
- Propagate `$/cancelRequest` to all downstream servers
- Keep entry in `pending_responses` (responses still expected)

**Cancellation Response Handling:**

treesitter-ls operates as transparent proxy for cancellation:

**For requests already sent:**
- Forward `$/cancelRequest` to downstream server(s)
- Downstream decides: send `result` (too late) or `REQUEST_CANCELLED` error
- Forward downstream response to client

**For requests NOT yet sent (in coalescing map/queue):**
- Remove from local tracking structures
- Bridge sends `REQUEST_CANCELLED` error to client immediately
- Never forward to downstream (request never sent)

### Response Aggregation Strategies

For fan-out **requests** (with `id`), configure aggregation per method:

```rust
enum AggregationStrategy {
    /// Return first successful response, cancel others
    FirstWins,

    /// Wait for all, merge array results (candidate lists only)
    MergeAll {
        dedup_key: Option<String>,  // e.g., 'label' for completions
        max_items: Option<usize>,
    },

    /// Wait for all, return highest priority non-null result
    Ranked {
        priority: Vec<String>,
    },
}
```

**Aggregation Stability Rules:**
- **Per-request timeout conditions**: Timeout applies **only when n ≥ 2 downstream servers participate**
  - SingleByCapability: No per-request timeout (wait for single server, idle timeout protects)
  - FanOut with n=1: No per-request timeout (functionally equivalent to single)
  - FanOut with n≥2: Per-request timeout applies (default: 5s explicit, 2s incremental)
- **Per-request timeout behavior**: On timeout, return whatever results available **without sending $/cancelRequest**
  - Downstream servers continue processing and send responses
  - Late responses **discarded** by router but **reset idle timeout** (heartbeat for connection health)
  - **Memory management**: Request entry removed from `pending_responses` after returning partial results
- **Partial results**: If at least one downstream succeeds, respond with successful `result` using LSP-native fields (e.g., for CompletionList: `{ "isIncomplete": true, "items": [...] }`)
- **Total failure**: If all downstreams fail or time out, respond with `ResponseError` (`REQUEST_FAILED`)

## Consequences

### Positive

**Complementary Tools:**
- Users can leverage multiple specialized tools for same language (pyright + ruff)

**Routing-First Simplicity:**
- Most requests go to single server — no aggregation overhead for common cases

**Minimal Configuration:**
- Default capability-based routing works without per-method config

**Graceful Degradation:**
- Partial initialization failures allow working servers to continue
- Fault isolation: One crashed server doesn't affect others

**Parallel Initialization:**
- Multiple servers initialize concurrently without global barriers
- Faster servers start handling requests immediately

**Flexible Aggregation:**
- Per-method control over response combination (when needed)

**Cancellation Propagation:**
- Client cancellations propagated to all downstream servers

**No Silent Failures:**
- Missing providers surface as explicit errors instead of `null` results

**Backward Compatible:**
- Single-server configurations continue to work unchanged

### Negative

**Configuration Surface:**
- Users must understand aggregation strategies and routing constraints
- Must know which methods are safe for aggregation (candidate-based vs direct-edit)
- Misconfiguration could cause data corruption

**Aggregation Complexity:**
- Merging candidate lists requires deduplication logic
- Different servers may propose similar candidates with subtle differences
- Safe only for candidate-based methods where user selects ONE item

**Latency:**
- Fan-out with `merge_all` waits up to per-server timeouts
- Partial results may surface instead of complete lists

**Memory:**
- Tracking pending responses adds overhead

**Coordination Complexity:**
- More state to manage (response tracking, circuit breakers, aggregators)

### Neutral

**Existing Tests:**
- Current single-server tests remain valid

**Incremental Adoption:**
- Routing-first means aggregation can be added later for specific methods

**Diagnostics:**
- Pass-through by design — client handles aggregation

## Alternatives Considered

### Alternative 1: Single Server Per Language (Status Quo)

Maintain the limitation of one server per language.

**Rejected Reasons:**

1. **Forced choice**: Users must choose between complementary tools instead of using both
2. **Limited functionality**: Can't combine pyright's type checking with ruff's linting in single session
3. **User workarounds**: Users resort to running multiple editors or manual tool switching
4. **Industry trend**: Modern development benefits from specialized, composable tools

### Alternative 2: Merge All Servers into Single Process

Create monolithic language servers that combine all capabilities.

**Rejected Reasons:**

1. **Maintenance burden**: Would require forking and merging upstream language servers
2. **Update lag**: Can't track upstream updates without constant merging
3. **Resource waste**: Combined server loads all capabilities even if user needs subset
4. **Binary compatibility**: Different servers may have conflicting dependencies

### Alternative 3: Always Aggregate (No Routing Priority)

Always fan out to all servers and merge results.

**Rejected Reasons:**

1. **Unnecessary latency**: Most requests have single capable server (no aggregation needed)
2. **Unsafe for direct-edit methods**: Formatting/rename would produce conflicting edits
3. **Memory overhead**: Tracking all responses for all requests even when unnecessary
4. **Complexity without benefit**: Aggregation logic for methods that don't need it

**Why routing-first is better**: Fast path for common case (single capable server), aggregation only when actually needed.

## Configuration Example

```yaml
# Routing-first approach: minimal configuration needed
languages:
  markdown:
    bridges:
      python:
        # Servers auto-discovered from languageServers with languages: [python]
        priority: ["ruff", "pyright"]  # Explicitly prioritize ruff

        # Only configure methods that need non-default behavior:
        aggregations:
          textDocument/completion:
            strategy: merge_all      # Safe: candidates, user selects one
            dedup_key: label
          textDocument/codeAction:
            strategy: merge_all      # Safe: proposals, user executes one
          # hover, definition: use default (single_by_capability)
          # formatting, rename: MUST use single_by_capability

languageServers:
  pyright:
    cmd: [pyright-langserver, --stdio]
    languages: [python]  # ← auto-discovered
  ruff:
    cmd: [ruff, server]
    languages: [python]  # ← auto-discovered
```

## Implementation Plan

### Phase 1: Single-LS-per-Language Foundation

**Scope**: Support **one language server per language** (multiple languages, each uses only one server)

**What Works:**
- Multiple embedded languages in same document (Python, Lua, SQL blocks)
- Parallel initialization: Each server initializes independently
- Per-downstream snapshotting: Late initializers receive latest state
- Simple routing: language → single server
- Routing errors surfaced: `REQUEST_FAILED` when no provider

**What Phase 1 Does NOT Support:**
- Multiple servers for same language (Python → pyright + ruff)
- Fan-out / scatter-gather
- Response aggregation/merging

### Phase 2: Resilience Patterns

**Scope**: Add fault isolation and recovery to single-server-per-language setup

**What Phase 2 Adds:**
- Circuit Breaker: Prevent cascading failures
- Bulkhead Pattern: Isolate downstream servers
- Per-server timeout configuration
- Health monitoring
- Partial-result metadata

**Why Before Multi-LS**: Stabilize foundation before adding aggregation complexity.

### Phase 3: Multi-LS-per-Language with Aggregation

**Scope**: Extend to support **multiple language servers per language**

**What Phase 3 Adds:**
- Routing strategies: single-by-capability (default) and fan-out
- Response aggregation: merge_all, first_wins, ranked strategies
- Per-method aggregation configuration
- Cancellation propagation to all downstream servers
- Fan-out skip/partial for unhealthy servers
- Leverages Phase 2 resilience per-server

**Exit Criteria:**
- Can use pyright + ruff simultaneously for Python
- Completion candidates merged with deduplication
- CodeAction candidates merged without duplicates
- Routing config works (defaults + overrides)
- Resilience patterns work per-server
- Partial results surfaced when one server times out

## Related ADRs

- **[ADR-0006](0006-language-server-bridge.md)**: Core LSP bridge architecture (1:1 pattern)
  - ADR-0015 extends to 1:N (one client → multiple servers per language)
- **[ADR-0008](0008-language-server-bridge-request-strategies.md)**: Per-method bridge strategies
  - Per-method strategies remain valid for single-server routing
- **[ADR-0012](0012-multi-ls-async-bridge-architecture.md)**: Multi-LS async bridge **(Parent ADR)**
  - This ADR extracts multi-server coordination from ADR-0012
- **[ADR-0013](0013-async-io-layer.md)**: Async I/O infrastructure
  - Provides async I/O patterns enabling parallel server management
- **[ADR-0014](0014-actor-based-message-ordering.md)**: Message ordering and superseding
  - Handles single-server ordering; ADR-0015 coordinates multiple servers
- **[ADR-0016](0016-graceful-shutdown.md)**: Graceful shutdown
  - Defines shutdown coordination for multiple concurrent connections
  - Router broadcasts shutdown; ADR-0016 specifies per-connection sequence

## Amendment History

- **2026-01-07**: Merged [Amendment 002](0015-multi-server-coordination-amendment-002.md) - Simplified ID namespace by using upstream request IDs directly (no transformation), replaced `pending_correlations` with `pending_responses`
- **2026-01-06**: Merged [Amendment 001](0015-multi-server-coordination-amendment-001.md) - Updated partial results to use LSP-native fields (isIncomplete), clarified $/cancelRequest semantics, added response guarantees for cancelled requests
