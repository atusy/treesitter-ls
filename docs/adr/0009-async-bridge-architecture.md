# ADR-0009: Async Bridge Architecture for Concurrent LSP Requests

| | |
|---|---|
| **Status** | accepted |
| **Date** | 2026-01-01 |
| **Decision-makers** | atusy |
| **Consulted** | - |
| **Informed** | - |

## Context and Problem Statement

The LSP bridge (ADR-0006) connects injection regions to external language servers via stdio. Tower-lsp processes requests concurrently, meaning multiple LSP requests (hover, completion, signatureHelp, documentHighlight) can arrive simultaneously for the same document. The original synchronous take/return pool pattern causes blocking when multiple requests fight over the same connection's stdin/stdout streams.

The question is: what async architecture should we adopt for bridging concurrent requests to external language servers?

## Decision Drivers

* **Concurrency**: Tower-lsp dispatches requests concurrently; bridge must not serialize high-frequency requests
* **Latency**: Users expect responsive hover, completion, and signature help
* **Resource efficiency**: Spawning language servers is expensive; connections must be reused
* **Cancellation**: Requests must be cancellable on timeout or user action
* **Reliability**: Handle timeouts, server crashes, and dead server detection correctly
* **Idiomatic async**: Follow tokio best practices, avoid sync-in-async anti-patterns

## Considered Options

1. **Synchronous Take/Return Pool** — Current pattern for low-frequency handlers
2. **Async Request/Response Queue with Background Reader Thread** — Interim implementation (commit 7a10bcd)
3. **Multiple Connection Instances per Language** — Spawn multiple servers, round-robin requests
4. **Full Async I/O with tokio::process** — Use tokio's async process I/O with `select!`

## Decision Outcome

**Chosen option**: "Full Async I/O with tokio::process" (Option 4), because it provides clean cancellation via `select!`, uses no extra threads (reactor-based I/O), follows idiomatic tokio patterns, and avoids the shutdown bug present in the background thread approach.

### Consequences

**Positive:**
* Zero extra OS threads — reactor monitors all connections efficiently
* Clean cancellation via `select!` — timeout, shutdown, and read unified in one construct
* Dead server detection built-in — timeout branch detects hung servers
* Pure async codebase — no sync/async boundary crossing
* Standard error propagation via `Result`
* Scales to many connections without thread overhead

**Negative:**
* Requires rewrite of `AsyncBridgeConnection` and `AsyncLanguageServerPool`
* `tokio::process` API differs from `std::process` — some refactoring needed
* Less familiar pattern for developers used to blocking I/O

**Neutral:**
* Notification forwarding ($/progress) continues via mpsc channel
* Request ID routing via DashMap remains unchanged

### Confirmation

* Unit test: Verify `select!` correctly handles concurrent read + shutdown + timeout
* Integration test: Shutdown while server is silent → connection closes cleanly (no hang)
* E2E test: Open document, trigger concurrent hover + completion + signatureHelp → all complete or timeout gracefully
* Resource test: 10 language servers active → thread count does not increase proportionally

## Pros and Cons of the Options

### Option 1: Synchronous Take/Return Pool

Keep the current pattern where callers take exclusive ownership of a connection, use it, then return it.

* Good, because simple mental model (exclusive access)
* Good, because no background threads or tasks needed
* Good, because already implemented and working for low-frequency requests
* Bad, because **serializes all requests** to the same language → unacceptable latency for concurrent requests
* Bad, because caller must remember to return connection (easy to leak on early return/panic)

### Option 2: Async Request/Response Queue with Background Reader Thread

Interim implementation from commit 7a10bcd. Background OS thread reads responses and routes by request ID.

* Good, because enables concurrent requests on shared connection
* Good, because simple request ID routing (atomic increment, DashMap lookup)
* Bad, because **shutdown bug**: blocked `read_line()` ignores shutdown flag
  ```rust
  loop {
      if shutdown.load(SeqCst) { break; }  // Never reached if...
      reader.read_line(&mut buf);          // ...blocked here forever
  }
  ```
* Bad, because one OS thread wasted per connection (blocked on I/O)
* Bad, because mixed sync/async model (`blocking_send`, `spawn_blocking`)
* Bad, because manual timeout logic, error-prone

### Option 3: Multiple Connection Instances per Language

Spawn N instances of each language server, distribute requests round-robin.

* Good, because true parallelism (N concurrent requests)
* Good, because no response routing needed (each connection handles one request at a time)
* Bad, because **N× resource usage** (memory, CPU) for each language server
* Bad, because language servers may not handle multiple instances well (file locking, port conflicts)
* Bad, because complex load balancing logic
* Bad, because rust-analyzer alone uses 500MB+ RAM; N instances unacceptable

### Option 4: Full Async I/O with tokio::process

Use `tokio::process::Command` with async stdin/stdout and `select!` for multiplexing.

```rust
loop {
    select! {
        result = reader.read_line(&mut buf) => {
            let msg = result?;
            route_response(msg, &pending_requests);
        }
        _ = shutdown_rx.recv() => {
            break;  // Clean exit, no blocked thread
        }
        _ = tokio::time::sleep(idle_timeout) => {
            log::warn!("Server idle timeout, closing connection");
            break;
        }
    }
}
```

* Good, because **zero extra threads** — tokio reactor (epoll/kqueue) monitors all fds
* Good, because **clean cancellation** — `select!` unifies read, shutdown, and timeout
* Good, because **dead server detection** — idle timeout branch built into the loop
* Good, because **idiomatic tokio** — pure async, standard patterns
* Good, because **correct shutdown** — no blocked read to ignore signals
* Neutral, because requires refactoring existing code
* Bad, because slightly more complex initial setup with `tokio::process::Command`

## More Information

### Architecture Diagram

```
┌─────────────────────────────────────────────────────────┐
│                    tokio runtime                         │
│                                                          │
│  ┌─────────────────────────────────────────────────┐    │
│  │              AsyncBridgeConnection               │    │
│  │                                                  │    │
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
│            │                       │                     │
│            ▼                       ▼                     │
│     ┌─────────────────────────────────────┐             │
│     │           tokio reactor              │             │
│     │      (epoll/kqueue — no threads)     │             │
│     └─────────────────────────────────────┘             │
└─────────────────────────────────────────────────────────┘
              │                       │
              ▼                       ▼
       ┌──────────────────────────────────┐
       │         rust-analyzer            │
       │          (subprocess)            │
       └──────────────────────────────────┘
```

### Related ADRs

* [ADR-0006](0006-language-server-bridge.md): Core LSP bridge architecture (pooling, spawn strategy)
* [ADR-0008](0008-language-server-bridge-request-strategies.md): Per-method bridge strategies

### Implementation Plan

1. **Phase 1**: Implement `TokioAsyncBridgeConnection` using `tokio::process`
   - Spawn language server with `tokio::process::Command`
   - Reader task with `select!` for read/shutdown/timeout
   - Request ID routing via existing DashMap pattern

2. **Phase 2**: Implement `TokioAsyncLanguageServerPool`
   - Connection management with `Arc<TokioAsyncBridgeConnection>`
   - Lazy spawn on first request

3. **Phase 3**: Wire to high-frequency LSP handlers
   - hover, completion, signatureHelp, documentHighlight

4. **Phase 4**: Migrate remaining handlers, remove old implementations
   - definition, references, rename, codeAction, formatting

5. **Phase 5**: Remove deprecated code
   - Delete `AsyncBridgeConnection` (thread-based)
   - Delete `LanguageServerPool` (sync take/return)

### Key Implementation Details

```rust
use tokio::process::{Command, ChildStdin, ChildStdout};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

pub struct TokioAsyncBridgeConnection {
    stdin: tokio::sync::Mutex<ChildStdin>,
    pending_requests: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
    next_request_id: AtomicI64,
    shutdown_tx: Option<oneshot::Sender<()>>,
    reader_handle: tokio::task::JoinHandle<()>,
}

impl TokioAsyncBridgeConnection {
    pub async fn spawn(cmd: &[String]) -> Result<Self, Error> {
        let mut child = Command::new(&cmd[0])
            .args(&cmd[1..])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?;

        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();

        let (shutdown_tx, shutdown_rx) = oneshot::channel();
        let pending = Arc::new(DashMap::new());

        let reader_handle = tokio::spawn(
            Self::reader_task(stdout, pending.clone(), shutdown_rx)
        );

        Ok(Self { stdin: tokio::sync::Mutex::new(stdin), pending_requests: pending, ... })
    }

    async fn reader_task(
        stdout: ChildStdout,
        pending: Arc<DashMap<i64, oneshot::Sender<ResponseResult>>>,
        mut shutdown_rx: oneshot::Receiver<()>,
    ) {
        let mut reader = BufReader::new(stdout);

        loop {
            select! {
                result = Self::read_message(&mut reader) => {
                    match result {
                        Ok(Some(msg)) => Self::route_response(msg, &pending),
                        Ok(None) => break,  // EOF
                        Err(e) => { log::error!("Read error: {}", e); break; }
                    }
                }
                _ = &mut shutdown_rx => {
                    log::debug!("Shutdown received");
                    break;
                }
            }
        }
    }
}
```

### Thread Comparison

| Scenario | Option 2 (Thread) | Option 4 (tokio) |
|----------|-------------------|------------------|
| 1 language server | 1 extra thread | 0 extra threads |
| 5 language servers | 5 extra threads | 0 extra threads |
| 20 language servers | 20 extra threads | 0 extra threads |
| Shutdown while idle | ❌ Hangs on read | ✅ Clean exit |
| Server dies | Detects on next read | Detects via timeout |
