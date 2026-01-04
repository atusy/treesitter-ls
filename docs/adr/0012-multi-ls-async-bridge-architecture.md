# ADR-0012: Multi-Language Server Async Bridge Architecture

## Status

Accepted

**Supersedes**:
- [ADR-0009](0009-async-bridge-architecture.md): Single-LS async architecture (**completely replaced** due to unfixable hang issues)
- [ADR-0008](0008-language-server-bridge-request-strategies.md): Per-method strategies (**partially replaced** for multi-LS aspects; single-LS strategies remain valid)

## Context

ADR-0009 established the tokio-based async bridge architecture for concurrent LSP request handling with a single downstream language server. However, the current implementation suffers from **severe hang issues** due to waker/channel race conditions. Multiple fix attempts (yield_now, mpsc, Notify) have not resolved the root cause.

Additionally, real-world usage requires bridging to **multiple downstream language servers** simultaneously:
- Python code in markdown may need both **pyright** (type checking, completion) and **ruff** (linting, formatting)
- Embedded SQL may need both a SQL language server and the host language server
- Future polyglot scenarios (e.g., TypeScript + CSS in Vue files)

### Problems with Current Implementation (ADR-0009)

The existing async bridge implementation has fundamental issues that cannot be resolved through incremental fixes:

**1. Waker/Channel Race Conditions:**
- Async tasks occasionally hang indefinitely waiting for responses
- Root cause: Complex interaction between tokio wakers and channel notification timing
- Attempted fixes (yield_now, mpsc channels, Notify) provide partial relief but don't eliminate hangs

**2. LSP Ordering Violations:**
- Notifications and requests can arrive out of order at downstream servers
- Can violate LSP spec requirement: `didOpen` must precede other document notifications
- Current mutex-based serialization insufficient for multi-document scenarios

**3. Limited to Single LS per Language:**
- No support for multiple servers handling same language (e.g., pyright + ruff)
- No aggregation or routing strategies for overlapping capabilities
- Cannot leverage complementary strengths of different servers

## Decision

**Re-implement from scratch** with simpler, proven patterns. The new architecture uses a **routing-first, aggregation-optional** approach that supports 1:N communication patterns with proper:

1. **Fan-out/Scatter-Gather** — Send requests to multiple LSes, aggregate responses
2. **Ordering Guarantees** — Notifications must maintain order per (downstream, document)
3. **Cancellation Propagation** — `$/cancelRequest` from upstream flows to all downstream
4. **Resilience** — Circuit breaker and bulkhead patterns for fault isolation
5. **LSP Compliance** — All error handling uses standard LSP error codes and response structures

### 1. LSP Error Codes and Response Structures

All error responses must use standard LSP error codes to maintain protocol compliance:

```rust
/// LSP-compliant error codes (LSP 3.17+)
pub struct ErrorCodes;

impl ErrorCodes {
    /// Request failed but was syntactically correct (LSP 3.17)
    /// Use for: downstream server failures, timeouts, circuit breaker open
    pub const REQUEST_FAILED: i32 = -32803;

    /// Server cancelled the request (LSP 3.17)
    /// Only for requests that explicitly support server cancellation
    pub const SERVER_CANCELLED: i32 = -32802;

    /// Server not initialized (JSON-RPC reserved)
    /// Use for: requests/notifications sent before `initialized`
    pub const SERVER_NOT_INITIALIZED: i32 = -32002;
}

/// LSP-compliant error response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResponseError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}
```

**Critical LSP requirement:** Every request MUST receive a response. Never return `None`, drop requests, or leave them hanging. Use `ResponseError` with appropriate error codes for all failure scenarios.

**Usage guidelines:**
- Use `REQUEST_FAILED` (-32803) for most error scenarios: timeouts, downstream failures, circuit breaker open
- Include human-readable `message` describing the specific error context
- Optional `data` field can provide additional debug information (e.g., which downstream server failed)

### 2. Design Principle: Routing First

**Most requests should be routed to a single downstream LS based on capabilities.** Aggregation is only needed when multiple LSes provide overlapping functionality that must be combined.

```
┌─────────────────────────────────────────────────────────────────────────┐
│                           Request Routing                                │
│                                                                          │
│   Incoming Request                                                       │
│         │                                                                │
│         ▼                                                                │
│   ┌─────────────────────────────────────────────────────────────────┐   │
│   │ Which LSes have this capability for this languageId?            │   │
│   └─────────────────────────────────────────────────────────────────┘   │
│         │                                                                │
│         ├── 0 LSes → Return null/empty                                  │
│         ├── 1 LS   → Route to single LS (no aggregation needed)         │
│         └── N LSes → Check routing strategy:                            │
│                        ├── SingleByCapability → Pick alphabetically first│
│                        └── FanOut → Send to all, aggregate responses    │
└─────────────────────────────────────────────────────────────────────────┘

Priority order: Servers are sorted alphabetically by name.
  - pyright vs ruff → pyright wins (p < r)
  - lua-ls vs pyright → lua-ls wins (l < p)
```

**Example: pyright + ruff for Python**

| Method | pyright | ruff | Routing Strategy |
|--------|---------|------|------------------|
| `hover` | ✅ | ❌ | → pyright only (no aggregation) |
| `definition` | ✅ | ❌ | → pyright only |
| `completion` | ✅ | ✅ | → FanOut + merge_all |
| `formatting` | ❌ | ✅ | → ruff only |
| `codeAction` | ✅ | ✅ | → FanOut + merge_all |
| `diagnostics` | ✅ | ✅ | → Both (notification pass-through, no aggregation) |

**When aggregation IS needed:**
- `completion`: Both LSes return completion items → merge into single list
- `codeAction`: pyright refactorings + ruff lint fixes → merge into single list

**When aggregation is NOT needed:**
- Single capable LS → route directly
- Diagnostics → notification pass-through (client aggregates)
- Capabilities don't overlap → route to respective LS

### 3. Routing Strategies

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

### 4. Multiplexed Request-Reply (when fan-out is needed)

```
┌─────────────────────────────────────────────────────────────────────────┐
│                         treesitter-ls (Host LS)                          │
│  ┌────────────────────────────────────────────────────────────────────┐ │
│  │                    TokioAsyncLanguageServerPool                     │ │
│  │                                                                     │ │
│  │   ┌─────────────────┐                                              │ │
│  │   │  RequestRouter  │ ─── routes by (method, languageId, caps)     │ │
│  │   └────────┬────────┘                                              │ │
│  │            │                                                        │ │
│  │   ┌────────┴────────┐    Fan-out: scatter to multiple LSes         │ │
│  │   │                 │                                               │ │
│  │   ▼                 ▼                                               │ │
│  │ ┌───────────┐  ┌───────────┐  ┌───────────┐                        │ │
│  │ │  pyright  │  │   ruff    │  │ lua-ls    │  ... per-LS connection │ │
│  │ │(conn + Q) │  │(conn + Q) │  │(conn + Q) │                        │ │
│  │ └─────┬─────┘  └─────┬─────┘  └─────┬─────┘                        │ │
│  │       │              │              │                               │ │
│  │   ┌───┴──────────────┴──────────────┴───┐                          │ │
│  │   │         ResponseAggregator          │  Fan-in: merge/rank      │ │
│  │   └─────────────────────────────────────┘                          │ │
│  └────────────────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────────────────┘
```

**Key Design Points:**

- **ID Namespace Isolation**: Each downstream connection maintains its own `next_request_id`. The pool maps `(upstream_id, downstream_key)` → `downstream_id` for correlation.
- **Per-Connection Send Queue**: Each `TokioAsyncBridgeConnection` serializes writes via `Mutex<ChildStdin>`, ensuring no byte-level corruption.
- **Aggregation Strategies**: Configurable per method:
  - `first_wins`: Return first successful response (hedged request)
  - `merge_all`: Collect all responses, merge arrays (completion items)
  - `ranked`: Apply priority ranking, return highest-priority response

### 5. Ordered Notification Stream

Notifications (`didOpen`, `didChange`, `didClose`) have no response and no `id`. Order is critical:

#### 5.1 Initialization Order

**LSP mandates that `didOpen` and other notifications must be sent AFTER `initialized` notification:**

```
initialize (request)      →  Server
                          ←  initialize (response)
initialized (notification)→  Server
────────────────────────────────────────
didOpen (notification)    →  Server     ← NOW allowed
hover (request)           →  Server     ← NOW allowed
```

**Multi-server parallel initialization:**

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

**Implementation requirement:**
- `send_notification` must check `initialized` flag (same as `send_request`)
- Exception: `initialized` notification itself is allowed before flag is set

```rust
pub async fn send_notification(&self, method: &str, params: Value) -> Result<(), ResponseError> {
    // Guard: block notifications before initialization (except "initialized" itself)
    if !self.initialized.load(Ordering::SeqCst) && method != "initialized" {
        return Err(ResponseError {
            code: ErrorCodes::SERVER_NOT_INITIALIZED,
            message: "Cannot send notification: downstream language server not initialized".to_string(),
            data: None,
        });
    }
    // ...
}
```

**Partial Initialization Failure Policy:**

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
- Health monitoring logs warn about failed servers for debugging

**Example: pyright fails, ruff succeeds:**
```
┌─────────┐     ┌──────────┐     ┌──────────┐
│ Bridge  │     │ pyright  │     │   ruff   │
└────┬────┘     └────┬─────┘     └────┬─────┘
     │──initialize──▶│                │
     │──initialize───────────────────▶│  (parallel)
     │               │                │
     │◀──error───────│                │  (pyright crashes)
     │  (circuit breaker opens)       │
     │               │                │
     │◀──result───────────────────────│  (ruff succeeds)
     │──initialized──────────────────▶│
     │──didOpen──────────────────────▶│  (ruff ready)
     │               │                │
     │──hover(Python)────────────────▶│  (routes to ruff only)
     │               X                │  (pyright unavailable)
```

This approach maximizes availability while maintaining LSP compliance.

#### 5.1.1 Three-Layer Race Condition

**Problem: Client sends requests before bridge downstream is ready**

treesitter-ls responds to client's `initialize` immediately, independent of downstream bridge connections. This creates a race condition.

**Bridge spawn timing options:**

| Option | When to spawn | Tradeoff |
|--------|---------------|----------|
| 1. Eager | treesitter-ls startup | Wastes resources if language never used |
| **2. On language detection** | When embedded language detected (open or edit) | ✓ Balanced: spawn early, but only when needed |
| 3. On request | When request targets embedded language block | Late spawn, longer race window |

**We use Option 2:** When treesitter-ls detects embedded language blocks (e.g., Python in markdown), it spawns the corresponding downstream LS (e.g., pyright). Detection can occur:
- On `didOpen`: Document opened with existing embedded blocks
- On `didChange`: User adds a new embedded block while editing

This means the downstream LS starts initializing as soon as the embedded language is detected, before the user requests hover/completion in that block.

```
┌────────┐     ┌──────────────┐     ┌──────────┐
│ Client │     │ treesitter-ls│     │ pyright  │
└───┬────┘     └──────┬───────┘     └────┬─────┘
    │                 │                  │
    │──initialize────▶│                  │
    │◀──result────────│                  │  (immediate response)
    │──initialized───▶│                  │
    │                 │                  │
    │──didOpen(md)───▶│                  │
    │                 │ (parse, find Python block)
    │                 │──initialize─────▶│  (spawn pyright)
    │──hover(Python)─▶│                  │
    │                 │──hover──────────▶│  ← RACE: pyright not ready!
    │                 │                  │
    │                 │◀──init result────│
    │                 │──initialized────▶│
    │                 │──didOpen────────▶│  (treesitter-ls sends, not client)
    │                 │                  │
    ════════════════════════════════════════════ didOpen sent
    │                 │                  │
    │──hover(Python)─▶│──hover──────────▶│  ← Normal: just forward
    │◀──result────────│◀──result─────────│
```

**Key insight: The client never sends `didOpen` to the downstream LS.**
treesitter-ls is responsible for sending `didOpen` to the bridge downstream. This defines two distinct phases:

| Phase | Duration | Request Handling |
|-------|----------|-----------------|
| **Initialization Window** | spawn → didOpen sent | Special handling required (Request Superseding, Wait with Timeout) |
| **Normal Operation** | after didOpen sent | Simple pass-through, no queuing needed |

Once `didOpen` is sent to the downstream LS, the bridge enters "Normal Operation" mode where all client requests are simply forwarded without any special handling.

**Design Decision: Wait with Timeout (during Initialization Window)**

Three options were considered:

| Option | Behavior | Tradeoff |
|--------|----------|----------|
| Return Error | Fail immediately | Poor UX: user sees error, must retry |
| Buffer & Resend | Queue requests | Complex: ordering, memory management |
| **Wait with Timeout** | Block until ready | ✓ Best UX: transparent to user |

**Rationale for Wait with Timeout:**

1. **Initialization is fast** — Most language servers initialize in <1 second
2. **Async waiting is cheap** — Does not block other requests in the event loop
3. **Timeout ensures safety** — Returns error if initialization fails/hangs
4. **Better UX** — Users prefer 500ms delay over error messages

**During Initialization Window, not all requests should wait the same way.** Request handling strategy depends on request semantics:

| Category | Requests | Behavior during Init Window | Rationale |
|----------|----------|----------------------------|-----------|
| **Incremental** | completion, signatureHelp, hover | Request superseding (newer cancels older) | Stale results are useless, but user may be waiting |
| **Explicit action** | definition, references, rename, codeAction, formatting | Wait with timeout | User explicitly requested, waiting is expected |

**Note:** After `didOpen` is sent (Normal Operation), all requests are simply forwarded. No special handling needed.

**Request Superseding pattern for incremental requests (during Initialization Window):**

Incremental requests (completion, signatureHelp, hover) should NOT immediately return empty. The user might be waiting for the result. Instead, use a **request superseding** pattern:

1. If not initialized, keep the request pending
2. If a **new request of the same type** arrives, send `REQUEST_FAILED` error for the old request
3. When initialization completes, process the most recent pending request

**LSP compliance rationale:**
- **Every request gets a response** (LSP requirement) - no dropped requests
- Uses `REQUEST_FAILED` (-32803) for superseded requests (LSP 3.17+)
- The request failed due to changed client state (newer request arrived), making the result obsolete

```
Scenario A: User is waiting
  "pri" → completion① (pending)
  ... user waits ...
  (initialization complete)
  → Process completion①, return result ✓  User gets their result

Scenario B: User continues typing (request superseding)
  "pri" → completion① (pending)
  "print" → completion② arrives
  → Send REQUEST_FAILED error for completion①
  → Keep completion② pending
  (initialization complete)
  → Process completion②, return result ✓  Only latest result matters
```

**Error response for superseded requests:**
```rust
ResponseError {
    code: ErrorCodes::REQUEST_FAILED,  // -32803 (LSP 3.17)
    message: "Request superseded by newer request of same type".to_string(),
    data: Some(json!({"reason": "incremental_request_superseded"})),
}
```

This provides optimal UX while maintaining LSP compliance:
- If user pauses typing, they get completion when server is ready
- If user continues typing, stale requests receive proper failure errors
- Clients can handle `REQUEST_FAILED` appropriately (typically silently for superseded requests)

**Timeout considerations:**

| Category | Timeout needed? | Rationale |
|----------|-----------------|-----------|
| **Incremental** (request superseding) | No | Natural cleanup via newer requests canceling older ones; if initialization hangs, explicit actions will reveal it |
| **Explicit action** | Yes (5s) | User is explicitly waiting; need feedback if server is broken |

For incremental requests with superseding, timeout is unnecessary because:
1. User behavior naturally triggers new requests (typing, cursor movement)
2. New requests automatically discard stale pending ones
3. Memory usage is bounded (only 1 pending request per type)
4. If initialization is truly broken, explicit actions (definition, etc.) will timeout and alert user

**Implementation:**

```rust
/// Wait for initialization with timeout (for explicit actions only)
async fn wait_for_initialized(&self, timeout: Duration) -> Result<(), ResponseError> {
    if self.initialized.load(Ordering::SeqCst) {
        return Ok(());
    }

    tokio::select! {
        _ = self.initialized_notify.notified() => Ok(()),
        _ = tokio::time::sleep(timeout) => {
            Err(ResponseError {
                code: ErrorCodes::REQUEST_FAILED,
                message: "Timeout waiting for downstream language server initialization".to_string(),
                data: None,
            })
        }
    }
}

/// Wait indefinitely for initialization (for incremental requests with request superseding)
async fn wait_for_initialized_no_timeout(&self) {
    if self.initialized.load(Ordering::SeqCst) {
        return;
    }
    self.initialized_notify.notified().await;
}

/// Track pending incremental requests (only latest is kept)
struct PendingIncrementalRequests {
    completion: Option<(i64, oneshot::Sender<Result<Value, ResponseError>>)>,
    signature_help: Option<(i64, oneshot::Sender<Result<Value, ResponseError>>)>,
    hover: Option<(i64, oneshot::Sender<Result<Value, ResponseError>>)>,
}

impl PendingIncrementalRequests {
    /// Register new request, sending REQUEST_FAILED to any existing one of the same type
    fn register_completion(&mut self, id: i64, sender: oneshot::Sender<Result<Value, ResponseError>>) {
        if let Some((old_id, old_sender)) = self.completion.take() {
            // Send REQUEST_FAILED error to superseded request (LSP 3.17 compliant)
            let _ = old_sender.send(Err(ResponseError {
                code: ErrorCodes::REQUEST_FAILED,
                message: "Request superseded by newer request of same type".to_string(),
                data: Some(json!({"reason": "incremental_request_superseded"})),
            }));
            log::debug!("Sent REQUEST_FAILED for stale completion request {}", old_id);
        }
        self.completion = Some((id, sender));
    }
}

pub async fn send_request(&self, method: &str, params: Value) -> Result<Value, ResponseError> {
    match method {
        // initialize: no wait needed
        "initialize" => {}
        // Incremental: request superseding pattern, no timeout
        "textDocument/completion" | "textDocument/signatureHelp" | "textDocument/hover" => {
            // Register this request (sends REQUEST_FAILED to any pending one of same type)
            // Wait indefinitely - natural cleanup via request superseding pattern
            self.wait_for_initialized_no_timeout().await;
        }
        // Explicit actions: wait with timeout
        _ => {
            self.wait_for_initialized(Duration::from_secs(5)).await?;
        }
    }
    // ... proceed with request
}
```

**Timeout value (5 seconds) for explicit actions:**
- Covers slow language servers (e.g., pyright on large projects)
- Short enough to fail fast if server is broken (returns `REQUEST_FAILED` error)
- Configurable per-connection if needed in future

#### 5.2 Document Notification Order

```
Problem:
  upstream: didChange(v10) → completion
  If completion reaches downstream before didChange, downstream computes on stale state.

Solution:
  Per-downstream single send queue ensures:
    didChange(v10) → completion  (in downstream read order)
```

**Implementation:**

- Each `TokioAsyncBridgeConnection` already serializes writes via `Mutex<ChildStdin>`
- Notifications and requests share the same write path, preserving order
- For document-level parallelism (future optimization): separate queues per `(downstream, document_uri)`

### 6. Cancellation Propagation

When upstream sends `$/cancelRequest`, propagate to all downstream servers that have pending requests for that `upstream_id`:

```rust
// In TokioAsyncLanguageServerPool
async fn handle_cancel_request(&self, upstream_id: i64) {
    // Find all pending downstream requests for this upstream_id
    for (downstream_key, downstream_id) in self.pending_correlations.get(&upstream_id) {
        if let Some(conn) = self.connections.get(&downstream_key) {
            // Send $/cancelRequest to downstream
            let _ = conn.send_notification("$/cancelRequest", json!({
                "id": downstream_id
            })).await;
        }
    }
    // Clean up pending entries
    self.pending_correlations.remove(&upstream_id);
}
```

**Tracking Structure:**

```rust
/// Maps upstream request ID to downstream request IDs for cancellation
pending_correlations: DashMap<i64, Vec<(String, i64)>>, // upstream_id → [(downstream_key, downstream_id)]
```

### 7. Circuit Breaker Pattern

Prevent cascading failures when a downstream server is unhealthy:

```rust
struct CircuitBreaker {
    state: AtomicU8,           // 0=Closed, 1=Open, 2=HalfOpen
    failure_count: AtomicU32,
    last_failure: AtomicU64,   // timestamp
    config: CircuitBreakerConfig,
}

struct CircuitBreakerConfig {
    failure_threshold: u32,     // failures before opening (default: 5)
    reset_timeout_ms: u64,      // time before half-open (default: 30000)
    success_threshold: u32,     // successes in half-open to close (default: 2)
}
```

**State Transitions:**

```
     ┌─────────────────────────────────────┐
     │                                     │
     ▼                                     │
  Closed ──[failure_threshold]──► Open ────┘
     ▲                              │
     │                              │ [reset_timeout]
     │                              ▼
     └────[success_threshold]── HalfOpen
```

**Integration:**

```rust
impl TokioAsyncLanguageServerPool {
    async fn send_request_with_circuit_breaker(
        &self,
        key: &str,
        method: &str,
        params: Value,
    ) -> Result<Value, ResponseError> {
        let breaker = self.circuit_breakers.entry(key.to_string())
            .or_insert_with(|| CircuitBreaker::new(self.breaker_config.clone()));

        if !breaker.allow_request() {
            log::warn!("Circuit breaker open for {}, skipping request", key);
            // LSP compliance: Every request must receive a response
            return Err(ResponseError {
                code: ErrorCodes::REQUEST_FAILED,
                message: format!(
                    "Downstream server '{}' is unhealthy (circuit breaker open)",
                    key
                ),
                data: None,
            });
        }

        match self.send_request_inner(key, method, params).await {
            Ok(response) => {
                breaker.record_success();
                Ok(response)
            }
            Err(e) => {
                breaker.record_failure();
                log::warn!("Request to {} failed: {}", key, e);
                // LSP compliance: Return error response, not None
                Err(ResponseError {
                    code: ErrorCodes::REQUEST_FAILED,
                    message: format!("Request to downstream server '{}' failed: {}", key, e),
                    data: None,
                })
            }
        }
    }
}
```

### 8. Bulkhead Pattern

Isolate downstream servers so one slow/broken server doesn't exhaust shared resources:

```rust
struct BulkheadConfig {
    max_concurrent: usize,      // max concurrent requests per downstream (default: 10)
    queue_size: usize,          // max queued requests before rejection (default: 50)
}

// Per-connection semaphore
struct TokioAsyncBridgeConnection {
    // ... existing fields ...
    request_semaphore: Semaphore,  // limits concurrent requests
}
```

**Integration with existing `MAX_PENDING_REQUESTS`:**

The current `MAX_PENDING_REQUESTS = 100` acts as a global backpressure limit. Bulkhead adds per-connection limits for finer isolation.

### 9. Notification Pass-through (No Aggregation)

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

### 10. Response Aggregation Strategies (Request/Response Only)

For fan-out **requests** (with `id`), configure aggregation per method:

```rust
enum AggregationStrategy {
    /// Return first successful response, cancel others
    FirstWins,

    /// Wait for all, merge array results (completion items, diagnostics)
    MergeAll {
        dedup_key: Option<String>,  // field to deduplicate on
        max_items: Option<usize>,   // limit total items
    },

    /// Wait for all, return highest priority non-null result
    Ranked {
        priority: Vec<String>,  // server keys in priority order
    },
}
```


```yaml
# Configuration example (routing-first approach)
#
# Server discovery: languageServers with matching `languages` field are
# automatically used for that injection language. No explicit server list
# needed in bridges config.
#
# Priority order: alphabetical by server name (languageServers is a map, not array)
# Example: pyright comes before ruff alphabetically → pyright has higher priority

languages:
  markdown:
    bridges:
      python:
        # Servers discovered from languageServers with languages: [python]
        # Priority: pyright (alphabetically first) > ruff
        # Default: single_by_capability routing (no aggregation config needed)
        #
        # Only configure methods that need non-default behavior:
        aggregations:
          textDocument/completion:
            strategy: merge_all      # both LSes return items → merge
            dedup_key: label
          textDocument/codeAction:
            strategy: merge_all      # both LSes return actions → merge
          # hover, definition: use default (single_by_capability, no config)

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

- **LSP Compliance**: All error handling uses standard LSP error codes (REQUEST_FAILED, SERVER_NOT_INITIALIZED)
- **No Dropped Requests**: Every request receives a response, even during failures (circuit breaker open, timeout, etc.)
- **Request Superseding**: Stale incremental requests receive LSP-compliant REQUEST_FAILED errors when superseded by newer requests
- **Graceful Degradation**: Partial initialization failures allow working servers to continue serving requests
- **Routing-First Simplicity**: Most requests go to a single LS — no aggregation overhead for common cases
- **Minimal Configuration**: Default capability-based routing works without per-method config
- **Multi-LS Support**: Python files can use pyright + ruff simultaneously
- **Fault Isolation**: One crashed LS doesn't affect others (circuit breaker + bulkhead)
- **Cancellation Propagation**: Client cancellations propagated to all downstream servers
- **Flexible Aggregation**: Per-method control over how responses are combined (when needed)
- **Backward Compatible**: Single-LS configurations continue to work unchanged

### Negative

- **Complexity**: More state to manage (correlations, circuit breakers, aggregators)
- **Configuration Surface**: Users need to understand aggregation strategies for overlapping capabilities
- **Latency**: Fan-out with `merge_all` waits for slowest server (mitigated by per-server timeout)
- **Memory**: Tracking pending correlations adds overhead

### Neutral

- **Existing Tests**: Current single-LS tests remain valid
- **Incremental Adoption**: Routing-first means aggregation can be added later for specific methods
- **Diagnostics**: Pass-through by design — client handles aggregation

## Implementation Plan

The current async bridge has **hang issues** due to waker/channel race conditions.
Multiple fix attempts (yield_now, mpsc, Notify) have not resolved the root cause.
**Decision: Re-implement from scratch with simpler, proven patterns.**

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
- Parallel initialization of multiple LSes (each LS initializes independently)
- Initialization race handling (`initialized` flag, wait-with-timeout, request superseding pattern)
- Notification ordering guarantees (serialized writes per connection)
- Simple routing: language → single LS (no aggregation needed)

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
- **Circuit Breaker**: Prevent cascading failures when downstream LS is unhealthy
  - Open circuit after failure threshold (default: 5 failures)
  - Half-open state for recovery testing
  - Fast-fail instead of waiting for timeouts
- **Bulkhead Pattern**: Isolate downstream servers to prevent resource exhaustion
  - Per-connection semaphore (max concurrent requests)
  - Queue size limits before rejection
  - Prevent one slow LS from blocking others
- **Per-server timeout configuration**: Custom timeout per LS type
- **Health monitoring**: Track LS health metrics, log warnings for flaky servers

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
- Routing strategies (single-by-capability, fan-out)
- Response aggregation (merge_all, first_wins, ranked)
- Per-method aggregation configuration
- Cancellation propagation to multiple downstream LSes
- **Leverages Phase 2 resilience**: Each LS in multi-LS setup already has circuit breaker + bulkhead

**Exit Criteria:**
- Can use pyright + ruff simultaneously for Python
- Completion results merged from both LSes
- Routing config works (single-by-capability default, fan-out for configured methods)
- Resilience patterns work per-LS (pyright circuit breaker independent of ruff)

**Rationale for phased approach:**
- Phase 1 delivers immediate value (multi-language support) with minimal complexity
- Phase 2 adds stability/resilience to simple architecture (easier to debug)
- Phase 3 adds multi-LS complexity on top of stable, resilient foundation
- Routing-first principle means most requests still use Phase 1 fast path (single LS)

## Related ADRs

- **[ADR-0006](0006-language-server-bridge.md)**: Core LSP bridge architecture
  - Establishes the fundamental 1:1 bridge pattern (host document → single language server per language)
  - Defines eager spawn strategy and position translation
  - ADR-0012 extends this to 1:N (host document → multiple language servers per language)

- **[ADR-0008](0008-language-server-bridge-request-strategies.md)**: Per-method bridge strategies
  - Defines four strategies: Parallel Fetch, Full Delegation, Edit Filtering, Background Collection
  - Specifies how different LSP methods should be handled (completion, hover, diagnostics, etc.)
  - **Relationship to ADR-0012**: ADR-0008's per-method strategies remain valid for single-LS routing. However, ADR-0012 clarifies multi-LS aspects:
    - **Diagnostics**: ADR-0008 suggested "merge & dedupe"; ADR-0012 specifies "pass-through" (client aggregates)
    - **Initialization window**: ADR-0008 assumes servers are initialized; ADR-0012 adds "request superseding" and "wait-with-timeout" patterns for requests during initialization
    - **Multi-server merging**: ADR-0012 provides concrete routing strategies (SingleByCapability, FanOut) and aggregation options (merge_all, first_wins, ranked)

- **[ADR-0009](0009-async-bridge-architecture.md)**: Single-LS async architecture **(Superseded)**
  - Established tokio-based async I/O with request ID routing
  - Identified the need for async concurrency (tower-lsp dispatches concurrently)
  - **Why superseded**: Fundamental waker/channel race conditions proved unresolvable; ADR-0012 re-implements with simpler patterns
