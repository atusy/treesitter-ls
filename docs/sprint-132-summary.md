# Sprint 132 Summary: PBI-163 Foundation Work

## Goal
"Users never experience editor freezes from LSP request hangs, receiving either success or clear error responses within bounded time"

## Achievements

### 1. LSP-Compliant Error Types (COMPLETED)
**Files Created:**
- `src/lsp/bridge/error_types.rs` - Full LSP 3.17+ compliant error types

**What Was Delivered:**
- `ErrorCodes` struct with standard LSP error codes:
  - `REQUEST_FAILED` (-32803): For timeouts, downstream failures
  - `SERVER_NOT_INITIALIZED` (-32002): For requests before initialization
  - `SERVER_CANCELLED` (-32802): For server-side cancellations

- `ResponseError` struct matching LSP 3.x Response Message spec:
  - `code: i32` - Error code
  - `message: String` - Human-readable message
  - `data: Option<Value>` - Optional additional data
  - `#[serde(skip_serializing_if = "Option::is_none")]` for clean JSON

- Helper methods for common scenarios:
  - `ResponseError::timeout()` - Timeout errors with "reason": "timeout"
  - `ResponseError::not_initialized()` - Server not ready errors
  - `ResponseError::request_failed()` - General failure errors

**Test Coverage:**
- 8 unit tests, all passing
- Tests verify:
  - LSP JSON-RPC error response structure
  - Serialization with and without data field
  - Error code constants match LSP spec
  - Helper methods create correct structures

**Commits:**
- `b0232e6`: feat(bridge): add LSP-compliant error types (GREEN)
- `c8a1520`: refactor(bridge): add ResponseError helper methods (REFACTOR)

### 2. Codebase Exploration (COMPLETED)
**Findings Documented:**
- Current architecture uses `TokioAsyncLanguageServerPool` and `TokioAsyncBridgeConnection`
- Async patterns: oneshot channels, DashMap for pending requests, tokio::select! in reader
- Identified limitations:
  - No bounded timeouts (can hang indefinitely)
  - No request superseding for incremental requests
  - Initialization guard returns String errors, not ResponseError
  - No circuit breaker or bulkhead patterns

**Decision:**
Per ADR-0012, complete rewrite needed with simpler patterns.

## Deferred to ADR-0012 Phase 1

The following work requires a complete architectural rewrite and is planned for the next sprint:

### 1. Bounded Timeouts (ADR-0012 § 7)
- `wait_for_initialized()` with `tokio::select!` and timeout
- All request paths wrapped with bounded timeouts
- Ensures every request receives a response within 5s (configurable)

### 2. Request Superseding (ADR-0012 § 7.3)
- `PendingIncrementalRequests` struct for tracking latest request
- Automatic `REQUEST_FAILED` for superseded requests
- Applies to: completion, signatureHelp, hover

### 3. New Architecture (ADR-0012 § 3)
- `LanguageServerPool` (replaces TokioAsyncLanguageServerPool)
- `BridgeConnection` (replaces TokioAsyncBridgeConnection)
- Simpler patterns, no complex waker/channel race conditions

### 4. Multi-Language E2E Tests (ADR-0012 § 5.2)
- Markdown with Python, Lua, SQL blocks simultaneously
- Rapid requests during parallel initialization
- Verify no hangs, all complete within bounded time

## Verification Results

### Unit Tests
```
make test
Result: ✓ 461 passed, 0 failed
```

### Code Quality
```
make check
Result: ✓ All checks pass (after cargo fmt)
```

### E2E Tests
```
make test_e2e
Result: 20/21 pass
Note: 1 snapshot test failure (test_semantic_tokens_snapshot) is pre-existing and unrelated to error types work
```

## Sprint Status: REVIEW

**Foundation work complete.** This sprint delivered the LSP-compliant error types that will be used throughout the Phase 1 rewrite. The error types are tested, documented, and ready for integration into the new `LanguageServerPool` and `BridgeConnection` implementations.

## Next Sprint: ADR-0012 Phase 1 Full Implementation

**Goal:** Implement complete `LanguageServerPool` and `BridgeConnection` rewrite with:
1. Bounded timeouts on all request paths
2. Request superseding for incremental requests
3. Simplified async patterns using `tokio::select!`
4. Multi-language initialization support
5. No hangs guarantee - every request gets a response

**Estimated Scope:** Large (5-7 days) - complete rewrite of core bridge architecture

**Dependencies:**
- Foundation work from Sprint 132 (error types) ✓
- ADR-0012 detailed specification ✓
- Existing E2E test infrastructure ✓

## References
- ADR-0012: Multi-Language Server Async Bridge Architecture
- PBI-163: Bounded error responses to prevent hangs
- Files: `src/lsp/bridge/error_types.rs`, `src/lsp/bridge/tokio_*`
