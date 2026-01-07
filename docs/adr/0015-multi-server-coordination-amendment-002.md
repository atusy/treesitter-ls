# ADR-0015 Amendment 002: Simplified ID Namespace (No Transformation)

**Date**: 2026-01-06
**Status**: Proposed
**Amends**: [ADR-0015](0015-multi-server-coordination.md) § Server Pool Architecture (lines 113-145), § Cancellation Propagation (lines 348-557)
**Supersedes**: Partial - simplifies correlation tracking from original ADR

## Issue

ADR-0015 line 142-145 specifies ID transformation:
```
ID Namespace Isolation: Each downstream connection maintains its own
`next_request_id`. The pool maps `(upstream_id, downstream_key)` →
`downstream_id` for correlation
```

**Problems with ID Transformation**:

1. **Complexity**: Requires bidirectional mapping (upstream ↔ downstream IDs)
2. **Cancellation Race** (CRITICAL #5): Cancellation can arrive during ID mapping registration
3. **ID Mismatch**: `$/cancelRequest {id: 42}` from client uses upstream ID, but server expects downstream ID
4. **State Overhead**: `pending_correlations` map grows with every in-flight request
5. **Debugging Difficulty**: IDs change across bridge boundary, hard to trace requests

**Why Transformation Was Considered**: Avoid ID collision if multiple upstreams send to same downstream. But treesitter-ls is a **single-upstream bridge** - only one client (the editor).

**Impact**: Unnecessary complexity for no actual benefit in single-upstream scenario.

---

## Amendment: Use Upstream IDs Directly

### Architectural Decision

**REMOVE ID transformation**: Use client's request ID directly when forwarding to downstream servers.

```rust
// BEFORE (complex):
upstream_id: 42
  ├─ pyright: downstream_id: 201   // Transform ID
  └─ ruff: downstream_id: 305      // Different ID per server

pending_correlations: Map<UpstreamID, Vec<(ServerKey, DownstreamID)>>
                      //  42 → [(pyright, 201), (ruff, 305)]

// AFTER (simple):
upstream_id: 42
  ├─ pyright: downstream_id: 42    // SAME ID
  └─ ruff: downstream_id: 42       // SAME ID

pending_responses: Map<RequestID, Vec<(ServerKey, ResponseSender)>>
                   //  42 → [(pyright, tx1), (ruff, tx2)]
```

### Benefits

✅ **Eliminates correlation mapping**: No complex (upstream_id, downstream_key) → downstream_id bookkeeping

✅ **Simplifies cancellation**: `$/cancelRequest {id: 42}` works directly on all servers

✅ **Removes CRITICAL #5 race**: No registration step between dequeue and send

✅ **Improves debugging**: Request ID consistent across entire system (client → bridge → servers)

✅ **Reduces memory**: One map entry per request, not per (request × server)

✅ **Stateless routing**: Router doesn't need to track ID mappings

### Trade-offs

**Limitation**: Cannot support multiple upstream clients to same downstream server (not a use case for treesitter-ls).

**Example of non-issue**:
```
Scenario: Editor A (upstream 1) and Editor B (upstream 2) both connected to same rust-analyzer
- Editor A sends request ID=42
- Editor B sends request ID=42
- Collision on rust-analyzer!

Reality: treesitter-ls doesn't support this. One editor ↔ one treesitter-ls ↔ N downstream servers.
```

---

## Updated Architecture

### Replace ADR-0015 § 3 Server Pool Architecture

**Remove lines 142-145** and replace with:

```
### Request ID Semantics

**Decision**: Use upstream request IDs directly for downstream servers.

**ID Flow**:
```
Client (editor)          treesitter-ls           Downstream Servers
     ├─ completion ID=42 ─→ Router
                             ├─ pyright (ID=42)
                             └─ ruff (ID=42)
```

**Tracking Structure**:
```rust
/// Maps request ID to response handlers for all servers handling that request
/// Single source of truth for in-flight requests
pending_responses: DashMap<i64, Vec<(String, oneshot::Sender<ResponseResult>)>>
                   //      ↑            ↑                    ↑
                   //   Request ID  Server Key          Response sender

// Example entry:
// 42 → [("pyright", tx1), ("ruff", tx2)]
//
// When aggregating:
// - Wait for both tx1 and tx2 to receive responses
// - Merge results according to aggregation strategy
// - Send single response to client with ID=42
```

**Benefits**:
- Single map lookup for cancellation (no correlation indirection)
- Request ID consistent across client → bridge → servers
- Simpler state management (one entry per request)

**Safety**:
- No ID collision risk (single upstream client)
- Each request ID is unique per client connection
- Downstream servers never see conflicting IDs from different upstreams
```

---

## Simplified Cancellation Handling

### Replace ADR-0015 § 6.2 (Cancellation Handling by Phase)

**Remove complex "Phase 1/Phase 2" distinction** and replace with:

```
### Cancellation Handling: Always Forward to Downstream

**Principle**: When client sends `$/cancelRequest`, always forward to downstream servers. Let servers handle cancellation as best-effort per LSP spec.

**Handler Implementation**:
```rust
async fn handle_cancel_request(&self, request_id: i64) {
    log::debug!("Handling $/cancelRequest for ID={}", request_id);

    // Lookup which servers are handling this request
    if let Some(handlers) = self.pending_responses.get(&request_id) {
        // Forward to all downstream servers
        for (server_key, _) in handlers.value() {
            if let Some(conn) = self.connections.get(server_key) {
                log::debug!("Forwarding $/cancelRequest to {}", server_key);

                // Send with SAME ID (no transformation)
                let _ = conn.send_notification("$/cancelRequest", json!({
                    "id": request_id
                })).await;
            }
        }
    } else {
        // Request not in pending_responses
        // Might be: (a) already completed, (b) never existed, (c) coalescing map
        log::debug!("Request ID={} not in pending, checking coalescing map", request_id);

        // Try connection-level cancellation (coalescing map, order queue)
        for conn in self.connections.values() {
            if conn.cancel_request(request_id).await {
                log::debug!("Request ID={} cancelled in connection {}", request_id, conn.name());
                return; // Cancelled before being sent
            }
        }

        // Still not found - already completed or invalid ID
        // Forward to all servers anyway (belt-and-suspenders)
        log::debug!("Request ID={} not found anywhere, forwarding $/cancelRequest as fallback", request_id);

        for conn in self.connections.values() {
            let _ = conn.send_notification("$/cancelRequest", json!({
                "id": request_id
            })).await;
        }
    }
}
```

**Why "Always Forward" Works**:
1. **LSP spec allows it**: Servers MAY ignore `$/cancelRequest` (best-effort)
2. **No harm if already completed**: Server ignores cancellation for unknown ID
3. **No harm if never existed**: Server ignores cancellation for unknown ID
4. **Catches race conditions**: If request in-flight during cancellation check, server still receives cancel

**Response Guarantee** (from Amendment 0015-001):
The original request (e.g., completion ID=42) still receives exactly one response:
- `RequestCancelled` error if cancelled before completion
- `result` if server completed before cancellation arrived

The `$/cancelRequest` notification receives NO response (it's a notification).
```

---

## Connection-Level Cancellation (ADR-0014 Integration)

### Simplified Connection Cancellation

**Update ADR-0014 cancellation handler**:

```rust
// Connection actor handles local cancellation (before sending to server)
async fn cancel_request(&self, request_id: i64) -> bool {
    // Try coalescing map
    if let Some(operation) = self.coalescing_map.remove((uri, method)) {
        // Send response BEFORE forwarding (from Amendment 0015-001)
        let _ = operation.response_tx.send(Err(ResponseError {
            code: RequestCancelled,
            message: "Request cancelled",
        }));
        return true; // Cancelled locally (never sent to server)
    }

    // Try order queue
    if self.order_queue.mark_cancelled(request_id) {
        return true; // Will be skipped when dequeued
    }

    // Not found locally
    false
}
```

**Key Difference from Before**: No need to check `pending_correlations` at connection level. Router handles that with `pending_responses`.

---

## Request Sending Flow (Simplified)

```rust
// Router sends request to connection(s)
async fn send_request(
    &self,
    request_id: i64,  // Use client's ID directly
    method: &str,
    params: Value,
) -> Result<Value> {
    // Determine which server(s) to send to
    let servers = self.route_request(method, language_id);

    // Create response handlers
    let mut handlers = Vec::new();

    for server_key in servers {
        let conn = self.connections.get(&server_key)?;

        // Create oneshot for response
        let (tx, rx) = oneshot::channel();

        // Send to server with SAME ID
        conn.send_request(request_id, method, params.clone(), tx).await?;

        handlers.push((server_key.clone(), rx));
    }

    // Register handlers (for cancellation and aggregation)
    self.pending_responses.insert(request_id, handlers.clone());

    // Aggregate responses
    let result = self.aggregate_responses(request_id, handlers, method).await?;

    // Remove from pending
    self.pending_responses.remove(&request_id);

    Ok(result)
}

// Connection sends request
async fn send_request(
    &self,
    request_id: i64,  // Use upstream ID directly
    method: &str,
    params: Value,
    response_tx: oneshot::Sender<ResponseResult>,
) -> Result<()> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": request_id,  // NO transformation
        "method": method,
        "params": params,
    });

    // Store response handler
    self.response_waiters.insert(request_id, response_tx);

    // Write to server stdin
    self.write_json(request).await?;

    Ok(())
}
```

---

## Resolves CRITICAL #5: Registration Race Eliminated

**Original Problem** (from architecture review):
```
T1: Writer loop dequeues operation from order queue
T2: User cancels ($/cancelRequest for ID=42)
T3: Router checks pending_correlations: NOT FOUND (not registered yet)
T5: Connection handler checks order queue: NOT FOUND (already dequeued at T1)
T7: Cancellation ignored (ADR-0015 line 482: "Not found... already processed")
T8: Writer loop registers in pending_correlations (line 449)
T9: Writer loop sends request to server stdin
    └─ REQUEST SENT TO SERVER DESPITE CANCELLATION
```

**How This Amendment Fixes It**:

```
T1: Writer loop dequeues operation
T2: User cancels ($/cancelRequest for ID=42)
T3: Router checks pending_responses: FOUND (registered at send time)
    OR: Not found → forwards to all servers anyway (belt-and-suspenders)
T4: Router sends $/cancelRequest to server(s)
T5: Writer loop sends request to server stdin
T6: Server receives both request and cancellation
T7: Server handles cancellation (may cancel or complete)
```

**Key Improvements**:
1. ✅ No separate registration step (registered when `send_request` called)
2. ✅ Cancellation always forwarded (even if not in `pending_responses`)
3. ✅ Server decides outcome (bridge doesn't try to prevent sending)
4. ✅ No race window (registration happens at router level, before connection sends)

---

## Testing Requirements

### ID Consistency Tests

1. **Test: Request ID unchanged across bridge**
   ```rust
   #[tokio::test]
   async fn test_request_id_passthrough() {
       // Setup: Client sends completion request ID=99
       // Assert: pyright receives request with ID=99
       // Assert: ruff receives request with ID=99
       // Assert: Client receives response with ID=99
   }
   ```

2. **Test: Cancellation uses same ID**
   ```rust
   #[tokio::test]
   async fn test_cancel_same_id() {
       // Setup: Client sends request ID=42
       // Action: Client sends $/cancelRequest {id: 42}
       // Assert: pyright receives $/cancelRequest {id: 42}
       // Assert: ruff receives $/cancelRequest {id: 42}
   }
   ```

### Cancellation Robustness Tests

3. **Test: Always-forward cancellation (not found scenario)**
   ```rust
   #[tokio::test]
   async fn test_cancel_not_found_forwards_anyway() {
       // Setup: Request ID=99 already completed (not in pending_responses)
       // Action: Client sends $/cancelRequest {id: 99}
       // Assert: $/cancelRequest forwarded to all servers
       // Assert: Servers ignore (unknown ID)
       // Assert: No errors, no hangs
   }
   ```

4. **Test: Cancellation during request send (race)**
   ```rust
   #[tokio::test]
   async fn test_cancel_during_send() {
       // Setup: Request ID=42 being sent to slow server
       // Action: Client sends $/cancelRequest during send
       // Assert: Both request and cancellation reach server
       // Assert: Server handles appropriately (may cancel or complete)
       // Assert: Client receives exactly one response
   }
   ```

### Multi-Server Fan-Out Tests

5. **Test: Fan-out uses same ID for all servers**
   ```rust
   #[tokio::test]
   async fn test_fanout_same_id() {
       // Setup: Completion aggregation (pyright + ruff)
       // Action: Send request ID=100
       // Assert: pyright receives ID=100
       // Assert: ruff receives ID=100
       // Assert: Both responses aggregated under ID=100
   }
   ```

---

## Migration Notes

**For Existing Implementations**:

1. **Remove `next_request_id` counter from connections**: Use upstream ID directly
2. **Remove `pending_correlations` map**: Replace with `pending_responses`
3. **Simplify cancellation handler**: Always forward, don't check phases
4. **Update tests**: Verify ID consistency across bridge

**Backward Compatibility**: Internal change only, no external API impact.

---

## Updated Data Structures Summary

### Before (Complex)
```rust
// Router level
pending_correlations: DashMap<i64, Vec<(String, i64)>>  // upstream → downstream IDs

// Connection level
next_request_id: AtomicI64                               // ID generator per connection
response_waiters: DashMap<i64, oneshot::Sender>         // downstream ID → response
```

### After (Simple)
```rust
// Router level
pending_responses: DashMap<i64, Vec<(String, oneshot::Sender)>>  // request ID → handlers

// Connection level
response_waiters: DashMap<i64, oneshot::Sender>                   // request ID → response
```

**Memory Saved**: Eliminated one map (`pending_correlations`) and one atomic counter per connection (`next_request_id`).

---

## Coordination With Other ADRs

### ADR-0013 (Async I/O)

- Reader task routes responses by request ID (unchanged)
- Same ID across bridge simplifies response routing

### ADR-0014 (Message Ordering)

- Coalescing map uses request ID directly
- Order queue uses request ID directly
- No correlation bookkeeping needed

### ADR-0016 (Shutdown)

- Pending responses failed with same ID
- Simpler cleanup (one map instead of two)

---

## Summary

**Change**: Remove ID transformation, use upstream request IDs directly for downstream servers.

**Resolves**:
- CRITICAL #5: Registration race eliminated
- Complexity: ID correlation mapping removed
- Cancellation: Always-forward policy (robust, simple)

**Benefits**:
- ✅ Simpler architecture (one map instead of two)
- ✅ Easier debugging (consistent IDs)
- ✅ Robust cancellation (always forwards)
- ✅ No race conditions (no registration step)
- ✅ Less memory (fewer tracking structures)

**Trade-off**: Cannot support multiple upstreams to same downstream (not a use case).

**Effort**: Low - removes code rather than adding it

**Risk**: Very low - simplification reduces bugs

**Priority**: HIGH - Resolves CRITICAL #5, simplifies architecture significantly

---

**Author**: Architecture Review Team
**Reviewers**: (pending)
**Implementation**: Required before Phase 1
