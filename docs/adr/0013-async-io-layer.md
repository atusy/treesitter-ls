# ADR-0013: Async I/O Layer for Bridge Architecture

| | |
|---|---|
| **Status** | Draft |
| **Date** | 2026-01-05 |
| **Decision-makers** | atusy |

## Context

The LSP bridge connects injection regions to external language servers via stdio (ADR-0006). Language servers are spawned as child processes with stdin/stdout streams for LSP JSON-RPC communication. The bridge must handle I/O from multiple concurrent requests without blocking, while efficiently managing system resources.

### Key Requirements

The fundamental I/O infrastructure decision impacts:

1. **Scalability**: How many OS threads are needed to manage N language servers?
2. **Cancellation**: How to cleanly interrupt I/O operations on shutdown/timeout?
3. **Reliability**: How to detect dead or hung servers?
4. **Maintainability**: Sync vs async boundaries, idiomatic patterns

Three critical requirements drive this decision:
- **Zero extra OS threads**: Language servers are long-lived; one thread per server doesn't scale
- **Clean cancellation**: Shutdown and timeout must interrupt blocked I/O without hanging
- **Idiomatic async**: Pure async codebase integrates cleanly with tower-lsp's async handlers

## Decision

**Use `tokio::process` with pure async I/O for all language server communication.**

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

### Key Components

**Process Management:**
- Spawn language servers using `tokio::process::Command`
- Use `tokio::io::AsyncBufReadExt` and `tokio::io::AsyncWriteExt` for async stdin/stdout operations

**Reader Task Pattern:**
- Run a dedicated async reader task per server using `select!` to multiplex:
  - Reading responses from server stdout
  - Shutdown signals
  - Timeout detection
- Route responses to pending request handlers via shared map

**Writer Pattern:**
- Write requests to server stdin using async mutex-protected writer
- Single writer task ensures no byte-level corruption

**Pending Request Cleanup:**
When the reader task exits abnormally (EOF, read error, timeout, or shutdown), all pending requests must receive error responses:
- Fail all pending requests with `INTERNAL_ERROR` (-32603)
- Clear the pending map to prevent memory leaks
- Log failures for observability

**Race Prevention (Request Registration vs Reader Exit):**

A critical race condition exists between request registration and reader task cleanup. The **check-insert-check pattern** prevents orphaned requests:

```rust
async fn send_request(&self, request: Request) -> Result<Response> {
    // FIRST CHECK: Fail fast if connection not ready
    if self.connection_state.load() != ConnectionState::Ready {
        return Err(Error::ConnectionNotReady);
    }

    let request_id = request.id;  // LSP spec: integer | string
    let (response_tx, response_rx) = oneshot::channel();
    self.pending.insert(request_id, response_tx);

    // SECOND CHECK: Detect race with cleanup
    if self.connection_state.load() != ConnectionState::Ready {
        self.pending.remove(&request_id);
        return Err(Error::ConnectionNotReady);
    }

    // Safe: cleanup either hasn't started or will include our entry
    self.write_request(request_id, request).await?;
    response_rx.await
}
```

### Timeout Architecture

The system uses two distinct timeout mechanisms with different purposes:

**1. Idle Timeout (Server Health Monitor)**

- **Purpose**: Detect zombie servers (process alive but unresponsive to pending requests)
- **Scope**: Connection-level health monitoring
- **State-Based Gating**:
  - **Disabled** during: Initializing, Quiescent (no pending), Closing, Failed, Closed states
  - **Enabled** during: Ready state with pending requests > 0
- **Timer Lifecycle**:
  - **Start**: First request sent when quiescent (pending count: 0→1)
  - **Keep running**: Additional requests sent (pending count increases)
  - **Reset**: Any stdout activity (response or notification) while active
  - **Stop**: Last response received (pending count returns to 0)
- **Behavior on Timeout**: Connection marked as Failed, circuit breaker triggered, pool spawns new instance

**2. Initialization Timeout (Startup Bound)**

- **Purpose**: Bound initialization time to prevent indefinite hangs during server startup
- **Scope**: Single operation during connection startup
- **Duration**: Longer than idle timeout (typically 30-60 seconds)
- **Timer Management**:
  - **Start**: When initialize request sent (Connection state: Initializing)
  - **Stop**: When initialize response received (transition to Ready)
- **Behavior on Timeout**: Connection transitions to Failed, circuit breaker triggered, pool retries with backoff

**Independence**: The two timeouts serve different purposes and never overlap (idle disabled during Initializing; initialization timeout disabled once Ready).

## Consequences

### Positive

**Zero Extra OS Threads:**
- tokio reactor monitors all file descriptors in a single event loop
- N language servers = N async tasks (green threads) multiplexed on tokio runtime's thread pool
- Thread count scales with CPU cores, not with number of language servers

**Clean Cancellation:**
- `select!` macro unifies shutdown, timeout, and read in one construct
- No blocked system calls that ignore cancellation signals
- Immediate shutdown even when server is silent

**Dead Server Detection:**
- Idle timeout detects hung servers without separate monitoring
- EOF on stdout automatically detected and propagated

**Idiomatic Async Patterns:**
- Pure async codebase with no sync/async boundary crossing
- Compatible with tower-lsp's async request handlers

**Scalability:**
- Concurrent requests on same connection supported
- Efficient resource usage even with many language servers

### Negative

**API Differences:**
- `tokio::process::Command` has different API than `std::process::Command`
- Requires understanding of tokio's async I/O primitives

**Runtime Dependency:**
- Requires tokio runtime with multi-threaded executor
- Already a dependency via tower-lsp, so minimal impact

### Neutral

**Async Task Overhead:**
- Two async tasks per language server (reader + shared writer access)
- Green threads are lightweight (~2KB stack), not OS threads

## Alternatives Considered

### Alternative 1: std::process with Background OS Threads

Use standard library's `std::process` with one blocking OS thread per server reading stdout.

**Rejected Reasons:**

1. **Shutdown Bug**: Blocked `read_line()` call ignores shutdown flag
   ```rust
   loop {
       if shutdown.load(SeqCst) { break; }  // Never reached if...
       reader.read_line(&mut buf);          // ...blocked here forever
   }
   ```

2. **Thread Overhead**: One OS thread per connection wasted blocked on I/O

3. **Mixed Sync/Async**: Requires `blocking_send`, `spawn_blocking` for boundary crossing

4. **Manual Timeout Logic**: Complex and error-prone to implement correctly

**Thread Comparison:**

| Scenario | Background Threads | tokio async |
|----------|-------------------|-------------|
| 1 language server | 1 extra OS thread | 0 extra OS threads |
| 5 language servers | 5 extra OS threads | 0 extra OS threads |
| 20 language servers | 20 extra OS threads | 0 extra OS threads |
| Shutdown while idle | ❌ Hangs on read | ✅ Clean exit via `select!` |

### Alternative 2: Multiple Connection Instances per Language

Spawn N instances of each language server, distribute requests round-robin.

**Rejected Reasons:**

1. **N× Resource Usage**: Each instance consumes memory and CPU
2. **Unacceptable Memory Cost**: rust-analyzer alone uses 500MB+ RAM; multiple instances infeasible
3. **Server Compatibility**: Language servers may not handle multiple instances (file locking, port conflicts)
4. **Complexity**: Requires load balancing logic without solving the fundamental I/O cancellation problem

## Related Decisions

- **ADR-0006**: Core LSP bridge architecture (pooling, spawn strategy)
- **ADR-0014**: Message ordering and request superseding (built on this I/O layer)
- **ADR-0015**: Multi-server coordination (uses this I/O foundation for N servers)
- **ADR-0016**: Graceful shutdown (uses shutdown signal from `select!`, adds LSP handshake and process cleanup)
- **ADR-0017**: Timeout precedence hierarchy (coordinates idle timeout with other timeout systems)

## Notes

**Clarification on "Zero Extra Threads":**
- Refers to zero extra **OS threads**, not zero async tasks
- Multiple async tasks (reader + writer per server) are expected and desirable
- All async tasks are green threads multiplexed by tokio runtime
- Multiple language servers = multiple async tasks, but thread count remains constant

**Verification:**
- Unit test: `select!` correctly handles concurrent read + shutdown + timeout
- Integration test: Shutdown while server silent → connection closes cleanly
- Resource test: 10+ language servers active → OS thread count does not increase proportionally

## Amendment History

- **2026-01-06**: Merged [Amendment 001](0013-async-io-layer-amendment-001.md) - Added pending request cleanup requirements and race prevention pattern to prevent indefinite client hangs on reader task exit
- **2026-01-06**: Merged [Amendment 002](0013-async-io-layer-amendment-002.md) - Added state-based idle timeout gating and separate initialization timeout mechanism to prevent idle timeout from firing during slow initialization
