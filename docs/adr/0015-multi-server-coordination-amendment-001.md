# ADR-0015 Amendment 001: LSP Protocol Compliance Corrections

**Date**: 2026-01-06
**Status**: Proposed
**Amends**: [ADR-0015](0015-multi-server-coordination.md) § Cancellation Propagation (lines 348-557)
**Related**: [ADR-0014](0014-actor-based-message-ordering.md) § Generation-Based Coalescing

## Issues Addressed

This amendment resolves three critical LSP protocol compliance issues:

1. **Missing Response Guarantee for Cancelled Requests** (CRITICAL)
2. **$/cancelRequest Response Confusion** (MODERATE)
3. **Partial Results Format May Violate LSP Spec** (MODERATE)

---

## Issue 1: Missing Response Guarantee for Cancelled Requests

**LSP Requirement**: Every request with an `id` MUST receive exactly one response.

> LSP Spec § Request: "Every processed request must send a response back to the sender of the request."

**ADR-0015 § 6.4** (lines 496-526) shows cancellation scenarios, but doesn't specify **when** and **who** sends the response for cancelled requests.

**Scenario 1a** (line 498-510):
```
Request Cancelled While Still Enqueued
T1: Upstream sends $/cancelRequest for ID=42
    └─ Forward to connection actor:
        └─ Send REQUEST_CANCELLED response to upstream
```

**Problem**: "Send REQUEST_CANCELLED response" but:
- When exactly? (Synchronously? Asynchronously?)
- Who sends it? (Connection actor? Router? Both?)
- What if send fails? (Guarantee mechanism?)

**Impact**: CRITICAL - Clients may hang waiting for response that may never arrive.

---

## Issue 2: $/cancelRequest Response Confusion

**ADR-0015 line 556-557**:
```
Upstream response to cancellation: Always return a response to the client
after propagating cancellation. Use the standard LSP `RequestCancelled` code
(-32800) when the method is server-cancellable; otherwise use `REQUEST_FAILED`
with a `"cancelled"` message. Never leave the upstream request pending—
cancellation must still round-trip a response per LSP.
```

**LSP Spec Conflict**: `$/cancelRequest` is a **notification** (no `id`, no response expected):

> LSP Spec § $/cancelRequest: "The cancel notification is sent from the client to the server."

**Confusion**: ADR conflates:
1. The **original request** (e.g., `textDocument/completion` ID=42) ← DOES receive response
2. The **$/cancelRequest notification** ← Does NOT receive response (it's a notification)

**Impact**: MODERATE - Implementation might send response to `$/cancelRequest` itself, violating LSP.

---

## Issue 3: Partial Results Format May Violate LSP Spec

**ADR-0015 line 594-596**:
```
Partial results: If at least one downstream succeeds, respond with a successful
`result` that contains merged items plus partial metadata in-band
(e.g., `{ "items": [...], "partial": true, "missing": ["ruff"] }`)
```

**LSP Spec Issue**: For `textDocument/completion`, LSP defines:
```typescript
interface CompletionList {
    isIncomplete: boolean;
    items: CompletionItem[];
}
```

**Problem**: Adding custom fields like `"partial": true` and `"missing": ["ruff"]`:
- Not defined in LSP spec
- Most clients ignore unknown fields (okay)
- Some strict parsers might reject (compliance risk)

**LSP-Compliant Alternative**: Use existing `isIncomplete: true` flag to signal partial results.

---

## Amendments

### Amendment 1: Explicit Response Guarantee for Cancelled Requests

**Replace ADR-0015 § 6.4 Scenario 1a** (lines 498-510) with:

```
**Scenario 1a: Request Cancelled While Still Enqueued**

```
T0: User requests hover ID=42 → enqueued in order queue
T1: Upstream sends $/cancelRequest for ID=42
T2: Router forwards to connection actor
T3: Connection actor processes cancellation:
    ├─ Check coalescing map: FOUND, remove
    ├─ OR check order queue: FOUND, mark for skipping
    └─ IMMEDIATELY send REQUEST_CANCELLED response:
        {
          "id": 42,
          "error": {
            "code": -32800,
            "message": "Request cancelled"
          }
        }
T4: Connection actor returns success to router
T5: Writer loop dequeues marked operation → skip (already cancelled)
```

**Response Guarantee**:
- Response sent SYNCHRONOUSLY by connection actor (Step T3)
- Response sent BEFORE removing from tracking structures
- If oneshot send fails, log warning (client may have disconnected)
- Response is sent exactly once (atomic remove-and-send)

**Implementation**:
```rust
// In connection actor (ADR-0014 domain)
async fn cancel_request(&self, request_id: i64) -> bool {
    // Try coalescing map first
    if let Some(operation) = self.coalescing_map.remove((uri, method)) {
        // Send response BEFORE returning
        let error = ResponseError {
            code: ErrorCode::RequestCancelled,  // -32800
            message: "Request cancelled".to_string(),
            data: None,
        };

        // Send via operation's response channel
        if let Err(_) = operation.response_tx.send(Err(error)) {
            log::warn!("Failed to send cancellation response for ID={}", request_id);
        }

        return true; // Cancelled successfully
    }

    // Try order queue
    if self.order_queue.mark_cancelled(request_id) {
        // Response will be sent when writer loop dequeues marked operation
        return true;
    }

    // Not found - already processed or doesn't exist
    false
}
```
```

**Update Scenario 1b** (line 511-525) to clarify superseding already sent response:

```
**Scenario 1b: Superseded Request Cancelled**

```
T0: User types "foo" → completion request ID=1 enqueued
T1: User types "o" → completion request ID=2 enqueued (supersedes ID=1)
    └─ ID=1 IMMEDIATELY receives REQUEST_CANCELLED (via superseding)
        Response sent synchronously when ID=2 replaces ID=1 in coalescing map
T2: Upstream sends $/cancelRequest for ID=1 (race condition)
    └─ Connection actor checks: NOT in coalescing map (already superseded)
    └─ Connection actor checks: NOT in order queue (never enqueued)
    └─ IGNORE cancellation (response already sent at T1)
```

**Key Point**: Superseding sends `REQUEST_CANCELLED` response **synchronously** when new operation replaces old in coalescing map (per ADR-0014 Amendment 001).
```

---

### Amendment 2: Clarify $/cancelRequest vs Original Request

**Replace ADR-0015 line 556-557** with:

```
### Upstream Response to Cancellation

**Critical Distinction**:
1. **Original request** (e.g., `textDocument/completion` ID=42): MUST receive response
2. **$/cancelRequest notification**: Does NOT receive response (it's a notification, not a request)

**Original Request Response**:

The request being cancelled MUST still receive a response:
- If server processed it before cancellation: Send `result`
- If cancelled before processing: Send `error` with `RequestCancelled` (-32800)
- If cancellation fails (request already sent): Server may still respond with `result`

**Example - Successful Cancellation**:
```json
// Client sends
{"jsonrpc":"2.0", "id":42, "method":"textDocument/completion", ...}

// Client sends (notification, no id)
{"jsonrpc":"2.0", "method":"$/cancelRequest", "params":{"id":42}}

// Bridge responds to ORIGINAL request (ID=42)
{
  "jsonrpc":"2.0",
  "id":42,  // ← Matches original request
  "error":{
    "code":-32800,
    "message":"Request cancelled"
  }
}

// Bridge does NOT respond to $/cancelRequest (it's a notification)
```

**Example - Late Cancellation**:
```json
// Client sends completion request
{"jsonrpc":"2.0", "id":42, "method":"textDocument/completion", ...}

// Server responds quickly (100ms)
{"jsonrpc":"2.0", "id":42, "result":{...}}

// Client sends cancellation (too late, 200ms)
{"jsonrpc":"2.0", "method":"$/cancelRequest", "params":{"id":42}}

// Bridge ignores $/cancelRequest (response already sent)
// Client already has the result - no action needed
```

**LSP Compliance**:
- ✅ Every request with `id` receives exactly one response
- ✅ Notifications (like `$/cancelRequest`) receive no response
- ✅ Cancellation is best-effort (server may complete before cancel)
```

**Add note about method-specific cancellability**:

```
**Method Cancellability**:

Not all LSP methods are cancellable:
- **Cancellable**: `textDocument/completion`, `textDocument/hover`, `textDocument/signatureHelp` (incremental, user-facing)
- **Non-cancellable**: `textDocument/didChange`, `textDocument/didSave` (notifications, no response anyway)
- **Side-effecting**: `workspace/executeCommand`, `textDocument/rename` (may have already executed)

For non-cancellable methods, bridge should still send `REQUEST_CANCELLED` to maintain protocol consistency, even though cancellation may not prevent execution.
```

---

### Amendment 3: LSP-Compliant Partial Results Format

**Replace ADR-0015 line 594-596** with:

```
**Partial Results** (one or more downstream servers failed/timed out):

Use LSP-native fields where available:

**For CompletionList responses**:
```json
{
  "isIncomplete": true,  // ← LSP-defined field for partial results
  "items": [/* merged items from successful servers */]
}
```
The `isIncomplete: true` flag signals to the client that results are partial and may be re-requested.

**For methods without native partial support** (e.g., `textDocument/hover`):

Use the most complete response available:
```rust
// If pyright succeeded but ruff timed out
return pyright_response;  // Return what we have

// Log degradation
log::warn!("Partial response for {}: ruff timed out, pyright succeeded", method);
```

Client sees a successful response (not an error), unaware that one server failed. This is acceptable because:
- User gets useful result (not an error)
- Missing server's contribution is often redundant (e.g., both provide hover docs)
- Alternative (returning error) is worse UX (blocks user entirely)

**For Total Failure** (all servers failed/timed out):
```json
{
  "id": 42,
  "error": {
    "code": -32803,  // REQUEST_FAILED
    "message": "All language servers unavailable for textDocument/completion (python)",
    "data": {
      "method": "textDocument/completion",
      "languageId": "python",
      "servers": [
        {"name": "pyright", "state": "ready", "reason": "timeout", "elapsed_ms": 5000},
        {"name": "ruff", "state": "failed", "reason": "initialization_failed"}
      ]
    }
  }
}
```

**Rationale**:
- Use LSP-native `isIncomplete` when available (standard, well-supported)
- Return best-effort result for methods without native partial support
- Provide detailed error data on total failure (debugging)
- Never invent custom LSP fields (avoids compatibility issues)
```

---

## Testing Requirements

### Response Guarantee Tests

1. **Test: Cancelled request receives response**
   ```rust
   #[tokio::test]
   async fn test_cancelled_request_gets_response() {
       // Setup: Send completion request ID=42
       // Action: Send $/cancelRequest for ID=42 immediately
       // Assert: Request ID=42 receives response (either result or error)
       // Assert: Response received within 100ms
       // Assert: No hang, no timeout
   }
   ```

2. **Test: Superseded request receives response synchronously**
   ```rust
   #[tokio::test]
   async fn test_superseded_request_immediate_response() {
       // Setup: Send completion request ID=1
       // Action: Send completion request ID=2 (supersedes ID=1)
       // Assert: ID=1 receives REQUEST_CANCELLED immediately
       // Assert: Response received before ID=2 is sent to server
   }
   ```

### $/cancelRequest Protocol Tests

3. **Test: $/cancelRequest is a notification (no response)**
   ```rust
   #[tokio::test]
   async fn test_cancel_request_no_response() {
       // Setup: Send completion request ID=42
       // Action: Send $/cancelRequest notification (no id field)
       // Assert: Original request ID=42 receives response
       // Assert: $/cancelRequest receives NO response
       // Assert: LSP message count = 2 sent, 1 received
   }
   ```

4. **Test: Late cancellation ignored (response already sent)**
   ```rust
   #[tokio::test]
   async fn test_late_cancellation_ignored() {
       // Setup: Send request ID=42, server responds immediately
       // Action: Client sends $/cancelRequest after response
       // Assert: Cancellation ignored (no error, no duplicate response)
       // Assert: Client has exactly one response for ID=42
   }
   ```

### Partial Results Tests

5. **Test: CompletionList uses isIncomplete for partial results**
   ```rust
   #[tokio::test]
   async fn test_partial_completion_uses_is_incomplete() {
       // Setup: Two servers (pyright, ruff) for Python
       // Action: Request completion, ruff times out
       // Assert: Response has isIncomplete=true
       // Assert: Response has items from pyright
       // Assert: No custom fields (partial, missing)
   }
   ```

6. **Test: Total failure returns error with details**
   ```rust
   #[tokio::test]
   async fn test_total_failure_detailed_error() {
       // Setup: Two servers, both fail
       // Action: Request completion
       // Assert: Response is error (REQUEST_FAILED)
       // Assert: Error data includes server states and reasons
       // Assert: Error message mentions "all servers unavailable"
   }
   ```

---

## LSP Spec References

### Request/Response Pairing
> LSP Spec § Request: "Every processed request must send a response back to the sender of the request."

**Compliance**: ✅ Every request receives exactly one response (result or error), even when cancelled.

### Notification Semantics
> LSP Spec § Notification: "A processed notification message must not send a response back."

**Compliance**: ✅ `$/cancelRequest` is a notification, receives no response.

### Cancellation Support
> LSP Spec § Cancellation: "A notification to ask the server to cancel a request. The request might still be performed."

**Compliance**: ✅ Best-effort cancellation, original request still receives response (result if completed, error if cancelled).

### CompletionList.isIncomplete
> LSP Spec § CompletionList: "`isIncomplete`: This list is not complete. Further typing should result in recomputing this list."

**Compliance**: ✅ Using `isIncomplete: true` for partial results is semantically correct and well-supported by clients.

---

## Coordination With Other ADRs

### ADR-0014 (Message Ordering)

- Superseding sends response synchronously (Amendment 0014-001 specifies this)
- Coalescing map removal includes response sending
- Connection actor handles cancellation for enqueued requests

### ADR-0013 (Async I/O)

- Reader task forwards responses from server
- Writer loop checks cancelled flag before sending
- Both ensure exactly-one response guarantee

### ADR-0016 (Shutdown)

- Shutdown sequence fails pending requests with REQUEST_FAILED
- Each request still receives exactly one response
- Cancellation during shutdown handled by connection state check

---

## Migration Notes

**For Existing Implementations**:

1. **Update cancellation handler** to send response synchronously
2. **Remove custom partial result fields** (`partial`, `missing`)
3. **Use LSP-native `isIncomplete`** for completion lists
4. **Never send response** to `$/cancelRequest` notification
5. **Test with strict LSP clients** (VSCode, Neovim) to verify compliance

**Backward Compatibility**: Fixes protocol violations, improves interoperability.

---

## Summary

**Changes**:
1. Explicit response guarantee for cancelled requests (synchronous send)
2. Clarified $/cancelRequest is notification (no response)
3. LSP-compliant partial results (use isIncomplete, not custom fields)

**Impact**:
- ✅ LSP protocol compliance (no hangs, correct notification semantics)
- ✅ Better client interoperability (standard fields only)
- ✅ Clear cancellation semantics (original request always responds)

**Effort**: Low - small changes to cancellation handler and partial result formatting

**Risk**: Low - fixes protocol violations, improves correctness

**Priority**: CRITICAL - LSP compliance is non-negotiable for editor integration

---

**Author**: Architecture Review Team
**Reviewers**: (pending)
**Implementation**: Required before Phase 1
