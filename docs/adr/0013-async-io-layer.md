# ADR-0013: Async I/O Layer for Bridge Architecture

| | |
|---|---|
| **Status** | Proposed |
| **Date** | 2026-01-05 |
| **Decision-makers** | atusy |
| **Consulted** | - |
| **Informed** | - |

## Context

The LSP bridge connects injection regions to external language servers via stdio (ADR-0006). Language servers are spawned as child processes with stdin/stdout streams for LSP JSON-RPC communication. The bridge must handle I/O from multiple concurrent requests without blocking, while efficiently managing system resources.

The fundamental I/O infrastructure decision impacts:
- Thread overhead: How many OS threads are needed to manage N language servers?
- Cancellation: How to cleanly interrupt I/O operations on shutdown/timeout?
- Reliability: How to detect dead or hung servers?
- Code maintainability: Sync vs async boundaries, idiomatic patterns

Three critical requirements drive this decision:
1. **Zero extra OS threads**: Language servers are long-lived; spawning one OS thread per server doesn't scale
2. **Clean cancellation**: Shutdown and timeout must interrupt blocked I/O without hanging
3. **Idiomatic async**: Pure async codebase integrates cleanly with tower-lsp's async handlers

## Decision

**Use `tokio::process` with pure async I/O and `select!` macro for all language server communication.**

Specifically:
- Spawn language servers using `tokio::process::Command` (not `std::process::Command`)
- Use `tokio::io::AsyncBufReadExt` and `tokio::io::AsyncWriteExt` for async stdin/stdout operations
- Run a dedicated async reader task per server using `select!` to multiplex:
  - Reading responses from server stdout
  - Shutdown signals
  - Idle timeout detection
- Write requests to server stdin using async mutex-protected writer

### Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    tokio runtime                        │
│                                                         │
│  Per-server async tasks (green threads):                │
│                                                         │
│  ┌─────────────────────────────────────────────────┐    │
│  │              AsyncBridgeConnection              │    │
│  │                                                 │    │
│  │  ┌──────────────┐    ┌────────────────────────┐ │    │
│  │  │ send_request │    │     reader task        │ │    │
│  │  │   (async)    │    │                        │ │    │
│  │  └──────┬───────┘    │  select! {             │ │    │
│  │         │            │    line = read =>      │ │    │
│  │         ▼            │    _ = shutdown =>     │ │    │
│  │  ┌──────────────┐    │    _ = timeout =>      │ │    │
│  │  │ AsyncWrite   │    │  }                     │ │    │
│  │  │ (stdin)      │    │                        │ │    │
│  │  └──────────────┘    └────────────────────────┘ │    │
│  │         │                       │               │    │
│  └─────────┼───────────────────────┼───────────────┘    │
│            │                       │                    │
│            ▼                       ▼                    │
│     ┌─────────────────────────────────────┐             │
│     │           tokio reactor             │             │
│     │      (epoll/kqueue — no threads)    │             │
│     └─────────────────────────────────────┘             │
└─────────────────────────────────────────────────────────┘
              │                       │
              ▼                       ▼
       ┌──────────────────────────────────┐
       │         rust-analyzer            │
       │          (subprocess)            │
       └──────────────────────────────────┘
```

### Key Implementation Pattern

```rust
use tokio::process::{Command, ChildStdin, ChildStdout};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn reader_task(
    stdout: ChildStdout,
    pending: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
    mut shutdown_rx: oneshot::Receiver<()>,
) {
    let mut reader = BufReader::new(stdout);

    loop {
        select! {
            result = read_message(&mut reader) => {
                match result {
                    Ok(Some(msg)) => route_response(msg, &pending),

                    Ok(None) => {
                        // EOF - server died
                        log::error!("Server process exited unexpectedly (EOF on stdout)");
                        cleanup_and_fail_pending(&pending, "Server process exited");
                        break;
                    }

                    Err(e) => {
                        log::error!("Read error on server stdout: {}", e);
                        cleanup_and_fail_pending(&pending, &format!("Connection error: {}", e));
                        break;
                    }
                }
            }
            _ = &mut shutdown_rx => {
                log::debug!("Shutdown signal received");
                cleanup_and_fail_pending(&pending, "Connection shutting down");
                break;
            }
            _ = tokio::time::sleep(idle_timeout) => {
                log::warn!("Server idle timeout - no response to pending requests");
                cleanup_and_fail_pending(&pending, "Server idle timeout");
                break;
            }
        }
    }
}

/// Fail all pending requests with INTERNAL_ERROR and clear the pending map.
/// This is called when the reader task exits abnormally (EOF, read error, timeout, or shutdown).
fn cleanup_and_fail_pending(
    pending: &Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
    reason: &str,
) {
    let count = pending.len();

    // Fail all pending requests
    for entry in pending.iter() {
        let request_id = *entry.key();
        if let Some((_, response_tx)) = pending.remove(entry.key()) {
            let error = ResponseError {
                code: ErrorCode::InternalError,  // -32603
                message: reason.to_string(),
                data: None,
            };

            // Send error response (ignore send failures - client may have disconnected)
            let _ = response_tx.send(Err(error));
        }
    }

    log::warn!("Failed {} pending requests due to: {}", count, reason);
}
```

**Cleanup Guarantees**:
- ✅ Every pending request receives exactly one error response (LSP protocol compliance)
- ✅ All responses sent before reader task exits (atomic from client perspective)
- ✅ Failed send attempts logged but don't block cleanup (client may have disconnected)
- ✅ Pending map cleared to prevent memory leaks

### Idle Timeout: Dead Server Detection

**Purpose**: Detect zombie servers (process alive but unresponsive to pending requests).

**Duration**: Implementation-defined timeout without response to pending requests (typically 30-120 seconds, balancing responsiveness vs false positives).

**Idle Timeout Gating by Connection State** (ADR-0014 integration):

- **Initializing State**: Timer DISABLED
  - Rationale: Initialization has its own timeout mechanism (see § Initialization Timeout below)
  - Initialize request does NOT trigger idle timer
  - Server silence is expected (initialization is slow)

- **Ready State**: Timer operates normally (see below)
  - Quiescent: STOPPED
  - Active: RUNNING

- **Failed State**: Timer STOPPED
  - Connection unusable, no pending operations allowed

- **Closing State**: Timer STOPPED
  - Shutdown in progress, pending operations being failed (per ADR-0016)

- **Closed State**: Timer N/A
  - Connection terminated

**State-Based Timer Management** (when in Ready state):

Idle timeout operates based on request activity:

- **Quiescent State** (no pending requests):
  - Timer: STOPPED
  - Server silence: Healthy (no activity expected)
  - Unsolicited notifications: Processed normally, no timer impact

- **Active State** (pending requests > 0):
  - Timer: RUNNING
  - Additional requests: Keep same timer running (don't restart)
  - Response received (pending > 1): Reset timer (fresh timeout window, stay active)
  - Response received (pending becomes 0): **STOP timer** (transition to Quiescent)
  - Notification received: Reset timer (fresh timeout window, stay active)
  - Timeout expires without stdout: Timeout fires (hung server)

**Timer Lifecycle**:
1. **Start**: First request sent when quiescent (pending count: 0→1)
2. **Keep running**: Additional requests sent (pending count increases)
3. **Reset**: Any stdout activity (response or notification) while active
4. **Stop**: Last response received (pending count returns to 0)

**Example Trace**:
```
T0: Quiescent state (no timer)
    req1 sent → START timer (pending: 0→1)

T1: req2 sent → KEEP timer running (pending: 1→2)

T2: resp1 received → RESET timer (pending: 2→1, still active)

T3: Server notification → RESET timer (fresh timeout window)

T4: resp2 received → STOP timer (pending: 1→0, quiescent)

T5: Extended silence → ✅ No timeout (timer stopped, healthy idle)
```

**Timeout Behavior**:
1. Close connection
2. Mark connection state as `Failed` (per ADR-0014)
3. Connection pool spawns new instance (per ADR-0015)

**Why State-Based**:
- Servers with minimal functionality (e.g., hover-only) never send unsolicited notifications
- Healthy servers can be silent indefinitely when idle
- Timeout should only fire when expecting activity but not receiving it

**Independence from Other Timeouts**:
- **Per-request timeout** (ADR-0015 aggregation layer): Bounds user-facing latency in multi-server scenarios, operates at router level
- **Generation-based superseding** (ADR-0014 coalescing): Event-driven cancellation, no time limit

Idle timeout is a **server health monitor** for pending requests, not a request latency bound or idle connection killer.

### Initialization Timeout: Separate Mechanism

**Purpose**: Bound initialization time to prevent indefinite hangs during server startup.

**Duration**: Implementation-defined (typically 30-60 seconds)
- Longer than idle timeout (initialization is legitimately slow)
- Accounts for slow servers: rust-analyzer (5-10s), pyright on large projects (3-8s)
- Fails fast on broken servers: missing dependencies, bad config (<1s to crash)

**Timer Management**:
1. **Start**: When initialize request sent (Connection state: Initializing)
2. **Stop**: When initialize response received (transition to Ready)
3. **Timeout**: If no response within duration

**Timeout Behavior**:
1. Log error: "Server initialization timeout after {duration}s"
2. Transition connection state: Initializing → Failed
3. Fail initialization request with REQUEST_FAILED
4. Trigger circuit breaker (per ADR-0015)
5. Connection pool schedules retry with backoff

**Independence from Idle Timeout**:
- Initialization timeout: Longer duration, single operation (initialize)
- Idle timeout: Shorter duration, any pending request (once Ready)
- Both are per-connection health monitors
- No overlap: Initialization timeout disabled once Ready; idle timeout disabled during Initializing

**Example Timeline**:
```
T0: Spawn server, send initialize
    → Initialization timeout: 60s timer STARTS
    → Idle timeout: DISABLED (state = Initializing)

T1: Server processing (8 seconds)
    → Initialization timeout: 52s remaining
    → Idle timeout: still DISABLED

T2: Initialize response received
    → Initialization timeout: STOPPED
    → Idle timeout: ENABLED (state = Ready)
    → Connection ready for normal operations
```

**Failure Case**:
```
T0: Spawn server, send initialize
    → Initialization timeout: 60s timer STARTS

T1: Server crashes (bad config, exit code 1)
    → Initialization timeout: fires immediately (no response)
    → Transition: Initializing → Failed
    → Circuit breaker: record failure, backoff 500ms
    → Pool: spawn new instance after backoff
```

## Consequences

### Positive

**Zero extra OS threads:**
- tokio reactor (epoll on Linux, kqueue on macOS) monitors all file descriptors in a single event loop
- N language servers = N async tasks (green threads) multiplexed on tokio runtime's thread pool
- Thread count scales with CPU cores, not with number of language servers
- Resource test: 20 language servers active → thread count remains constant

**Clean cancellation via `select!`:**
- Shutdown, timeout, and read unified in one construct
- No blocked system calls that ignore cancellation signals
- Integration test verified: shutdown while server silent → connection closes immediately (no hang)

**Dead server detection built-in:**
- Timeout branch in `select!` detects hung servers without separate monitoring
- EOF on stdout automatically detected and propagated

**Idiomatic tokio patterns:**
- Pure async codebase with no sync/async boundary crossing
- Standard error propagation via `Result`
- Compatible with tower-lsp's async request handlers

**Scalability:**
- Supports concurrent requests on same connection (routing handled at message layer - see ADR-0014)
- Efficient resource usage even with many language servers

### Negative

**API differences from std::process:**
- `tokio::process::Command` has slightly different API than `std::process::Command`
- Requires understanding of tokio's async I/O primitives
- Less familiar pattern for developers used to blocking I/O

**Refactoring required:**
- Existing synchronous take/return pool pattern must be rewritten
- Background thread implementation (commit 7a10bcd) must be replaced

### Neutral

**Async task overhead:**
- Two async tasks per language server (reader + shared writer access)
- Green threads are lightweight (~2KB stack), not OS threads
- Task creation/destruction handled by tokio runtime

**Runtime dependency:**
- Requires tokio runtime with multi-threaded executor
- Already a dependency via tower-lsp

## Alternatives Considered

### std::process::Command with background OS threads

Use standard library's `std::process` with one blocking OS thread per server reading stdout.

**Rejected because:**
- **Shutdown bug**: Blocked `read_line()` call ignores shutdown flag
  ```rust
  loop {
      if shutdown.load(SeqCst) { break; }  // Never reached if...
      reader.read_line(&mut buf);          // ...blocked here forever
  }
  ```
- **Thread overhead**: One OS thread per connection wasted blocked on I/O
- **Mixed sync/async**: Requires `blocking_send`, `spawn_blocking` for boundary crossing
- **Manual timeout logic**: Complex and error-prone to implement correctly

**Thread comparison:**

| Scenario | Background threads | tokio async |
|----------|-------------------|-------------|
| 1 language server | 1 extra OS thread | 0 extra OS threads |
| 5 language servers | 5 extra OS threads | 0 extra OS threads |
| 20 language servers | 20 extra OS threads | 0 extra OS threads |
| Shutdown while idle | ❌ Hangs on read | ✅ Clean exit via `select!` |

### Multiple connection instances per language

Spawn N instances of each language server, distribute requests round-robin.

**Rejected because:**
- **N× resource usage**: Each instance consumes memory and CPU
- **Unacceptable memory cost**: rust-analyzer alone uses 500MB+ RAM; multiple instances infeasible
- **Server compatibility**: Language servers may not handle multiple instances (file locking, port conflicts)
- **Complexity**: Requires load balancing logic without solving the fundamental I/O cancellation problem

## Related Decisions

- **ADR-0006**: Core LSP bridge architecture (pooling, spawn strategy)
- **ADR-0014**: Message ordering and request superseding (built on this I/O layer)
- **ADR-0015**: Multi-server coordination (uses this I/O foundation for N servers)
- **ADR-0016**: Graceful shutdown (uses shutdown signal from `select!`, adds LSP handshake and process cleanup)

## Notes

**Clarification on "zero extra threads":**
- Refers to zero extra **OS threads**, not zero async tasks
- Multiple async tasks (reader + writer per server) are expected and desirable
- All async tasks are green threads multiplexed by tokio runtime
- Multiple language servers = multiple async tasks, but thread count remains constant

**Confirmation:**
- Unit test: Verify `select!` correctly handles concurrent read + shutdown + timeout
- Integration test: Shutdown while server silent → connection closes cleanly (no hang)
- Resource test: 10+ language servers active → OS thread count does not increase proportionally

## Amendment History

- **2026-01-06**: Merged [Amendment 001](0013-async-io-layer-amendment-001.md) - Added pending request cleanup requirements to prevent indefinite client hangs on reader task exit (addresses Critical Issue C2: Pending Request Hang on Reader Task Abnormal Exit)
- **2026-01-06**: Merged [Amendment 002](0013-async-io-layer-amendment-002.md) - Added state-based idle timeout gating and separate initialization timeout mechanism to prevent idle timeout from firing during slow initialization (addresses Critical Issue C3: Idle Timeout Firing During Initialization)
