# Async Bridge Removal - Preparation for ADR-0012 Re-implementation

This document tracks what was removed from the codebase to prepare for the re-implementation of the async bridge according to [ADR-0012: Multi-Language Server Async Bridge Architecture](docs/adr/0012-multi-ls-async-bridge-architecture.md).

## Why Remove Instead of Refactor?

ADR-0012 explicitly states: **"Re-implement from scratch with simpler, proven patterns."**

The current implementation (`TokioAsyncLanguageServerPool` and `TokioAsyncBridgeConnection`) suffers from:
- **Severe hang issues** - Async tasks occasionally hang indefinitely waiting for responses
- **Root cause** - Complex interaction between tokio wakers and channel notification timing
- **Multiple fix attempts failed** - yield_now, mpsc channels, Notify provide partial relief but don't eliminate hangs

## What Was Removed

### 1. `src/lsp/bridge/tokio_connection.rs` (1,271 lines)

**Removed:** The entire `TokioAsyncBridgeConnection` implementation

**Key components removed:**
- `TokioAsyncBridgeConnection` struct and implementation
- Request/response handling with waker/channel coordination
- Notification handling
- All unit tests

**Why it was problematic:**
- Complex waker coordination led to race conditions
- Channel notification timing issues caused hangs
- Async task coordination was fragile

### 2. `src/lsp/bridge/tokio_async_pool.rs` (2,409 lines)

**Removed:** The entire `TokioAsyncLanguageServerPool` implementation

**Key components removed:**
- `TokioAsyncLanguageServerPool` struct and implementation
- Connection pooling and lifecycle management
- Request routing and correlation
- Initialization protocol handling
- All unit tests

**Why it was removed:**
- Built on top of the flawed `TokioAsyncBridgeConnection`
- Would need complete redesign anyway
- ADR-0012 introduces new architecture (RequestRouter, ResponseAggregator)

### 3. Entire Bridge Module (2,532 lines)

**Removed:** The entire `src/lsp/bridge/` directory in subsequent cleanup

**Files removed:**
- `cleanup.rs` (349 lines) - Temporary directory cleanup (only used by bridge)
- `connection.rs` (1,659 lines) - Connection traits and types
- `error_types.rs` (176 lines) - LSP error code constants
- `workspace.rs` (266 lines) - Workspace setup utilities
- `text_document.rs` and subdirectory (80 lines) - Response wrapper types

**Module declaration removed:**
- Removed `pub mod bridge;` from `src/lsp.rs`
- Removed `startup_cleanup` call from `src/lsp/lsp_impl.rs`

**Why removed:**
- Only one function (`startup_cleanup`) was still being called
- That function only cleaned up temp directories for the removed bridge
- The new implementation (ADR-0012) will likely need different patterns:
  - Different directory structure (possibly no temp dirs)
  - Different connection patterns (simpler, no tokio waker issues)
  - Different response handling (direct translation, no wrappers)
- Keeping unused infrastructure creates confusion about which patterns to follow
- All code preserved in git history (commit `ab7a2d3`) if needed as reference

## What Was Preserved

**Nothing.** The entire bridge implementation and infrastructure were removed.

**Rationale (YAGNI principle):**
- The old infrastructure was designed for specific patterns (temp directories, workspace isolation, response wrappers) that may not apply to the new design
- ADR-0012 introduces new architecture (RequestRouter, ResponseAggregator) that will have its own infrastructure needs
- Git history preserves everything if we need reference material during re-implementation
- Clean slate makes it clearer where to start with Phase 1

## No Stub Implementation

**Decision:** No stub was created. Bridge call sites were modified to return `None` directly.

**Rationale (YAGNI principle):**
- The stub would only be called once (for cleanup in `did_close`)
- That single call did nothing (no-op `close_documents_for_host`)
- Simpler to just add TODO comments at call sites than maintain 150 lines of stub code
- Clean slate makes it clearer where to wire in the new `LanguageServerPool`

## Code Updated

### 1. `src/lsp/lsp_impl.rs`
- Removed `stub_pool` field from `TreeSitterLs`
- Removed import of `StubLanguageServerPool`
- Updated notification channel comment to clarify it's for future bridge re-implementation
- Added TODO comment in `did_close` where bridge cleanup will be needed

### 2. Text Document Implementations
Modified bridge call sites to return `Ok(None)` with TODO comments:
- `src/lsp/lsp_impl/text_document/completion.rs` - Returns `None` instead of calling bridge
- `src/lsp/lsp_impl/text_document/definition.rs` - Returns `None` instead of calling bridge
- `src/lsp/lsp_impl/text_document/hover.rs` - Returns `None` instead of calling bridge
- `src/lsp/lsp_impl/text_document/signature_help.rs` - Returns `None` instead of calling bridge

Each file contains clear TODO comments showing what needs to be implemented when `LanguageServerPool` is ready.

## Impact on Functionality

### What Stopped Working
- **All async bridge functionality** - Embedded language support is currently disabled
- Python, Lua, SQL code blocks in markdown will not get LSP features until re-implementation

### What Still Works
- **Host language server** - treesitter-ls still provides LSP features for the host document
- **Configuration parsing** - Bridge configuration is still parsed and validated
- **Workspace setup** - Temporary directories and workspace isolation logic intact

## Re-implementation Plan (ADR-0012)

The new implementation will be built in three phases:

### Phase 1: Single-LS-per-Language Foundation
- Support one language server per language (multiple languages, but no overlapping servers)
- LSP compliance with proper error codes
- Two-phase notification handling (before initialized, before didOpen)
- Request superseding pattern for incremental requests
- Parallel initialization of multiple LSes

**Target:** Python, Lua, SQL blocks simultaneously in markdown (one LS per language)

### Phase 2: Resilience Patterns
- Circuit breaker pattern (prevent cascading failures)
- Bulkhead pattern (resource isolation per server)
- Per-server timeout configuration
- Health monitoring

**Target:** Stability and fault isolation before adding complexity

### Phase 3: Multi-LS-per-Language
- Routing strategies (single-by-capability, fan-out)
- Response aggregation (merge_all, first_wins, ranked)
- Cancellation propagation
- Support multiple servers for same language (e.g., pyright + ruff)

**Target:** Full ADR-0012 functionality

## New Implementation Naming

Per ADR-0012 naming decision:
- **`LanguageServerPool`** (was `TokioAsyncLanguageServerPool`)
- **`BridgeConnection`** (was `TokioAsyncBridgeConnection`)

**Rationale:** Implementation techniques (tokio, async) are internal details that shouldn't leak into class names. Domain names remain accurate even if async runtime changes.

## Migration Checklist for Re-implementation

When implementing the new `LanguageServerPool`:

- [ ] Create `src/lsp/bridge/` module structure per ADR-0012
- [ ] Create `src/lsp/bridge/pool.rs` with `LanguageServerPool`
- [ ] Create `src/lsp/bridge/connection.rs` with `BridgeConnection`
- [ ] Implement Phase 1 functionality (single-LS-per-language)
- [ ] Design new error handling approach (reference old `error_types.rs` from git)
- [ ] Design workspace setup if needed (reference old `workspace.rs` from git)
- [ ] Design notification handling (reference old `WithNotifications` wrappers from git)
- [ ] Update `src/lsp/lsp_impl.rs` to use new `LanguageServerPool`
- [ ] Add `pub mod bridge;` back to `src/lsp.rs`
- [ ] Write tests for new implementation
- [ ] Verify no hangs under concurrent load
- [ ] Add startup cleanup if the new design needs temp directories

## Files Changed Summary

### Deleted (Total: ~7,600 lines across 5 commits)

**Commit 1: Remove async implementation (ab7a2d3)**
- `src/lsp/bridge/tokio_connection.rs` (1,271 lines)
- `src/lsp/bridge/tokio_async_pool.rs` (2,409 lines)

**Commit 2: Remove bridge e2e tests (958fd51)**
- `tests/e2e_completion.rs` (240 lines)
- `tests/e2e_hover.rs` (223 lines)
- `tests/e2e_signature_help.rs` (261 lines)
- `tests/e2e_notification.rs` (312 lines)
- `tests/e2e_definition.rs` - 4 bridge test functions (231 lines)

**Commit 3: Rename test file (1aeb13a)**
- Renamed `tests/e2e_definition.rs` → `tests/e2e_lsp_protocol.rs`

**Commit 4: Remove unused helpers (72e4495)**
- `NO_RESULT_MESSAGE` constant from hover.rs
- `create_no_result_hover()` function from hover.rs
- Unused imports and variables (27 lines)

**Commit 5: Remove bridge infrastructure (7ca60e7)**
- `src/lsp/bridge/cleanup.rs` (349 lines)
- `src/lsp/bridge/connection.rs` (1,659 lines)
- `src/lsp/bridge/error_types.rs` (176 lines)
- `src/lsp/bridge/workspace.rs` (266 lines)
- `src/lsp/bridge/text_document.rs` (14 lines)
- `src/lsp/bridge/text_document/completion.rs` (17 lines)
- `src/lsp/bridge/text_document/definition.rs` (17 lines)
- `src/lsp/bridge/text_document/hover.rs` (17 lines)
- `src/lsp/bridge/text_document/signature_help.rs` (17 lines)

### Modified

**Commit 1:**
- `src/lsp/lsp_impl.rs` (removed bridge pool usage, added TODO comments)
- `src/lsp/lsp_impl/text_document/completion.rs` (return `Ok(None)` with TODO)
- `src/lsp/lsp_impl/text_document/definition.rs` (return `Ok(None)` with TODO)
- `src/lsp/lsp_impl/text_document/hover.rs` (return `Ok(None)` with TODO)
- `src/lsp/lsp_impl/text_document/signature_help.rs` (return `Ok(None)` with TODO)

**Commit 3:**
- `tests/e2e_lsp_protocol.rs` (updated documentation to reflect protocol tests only)

**Commit 4:**
- `src/lsp/lsp_impl/text_document/hover.rs` (removed unused code)
- `src/lsp/lsp_impl/text_document/definition.rs` (removed unused variable)

**Commit 5:**
- `src/lsp.rs` (removed `pub mod bridge;` declaration)
- `src/lsp/lsp_impl.rs` (removed `startup_cleanup` call, added comment)

## Compilation Status

✅ Code compiles successfully (`cargo check` passes)

The codebase is ready for re-implementation according to ADR-0012 with a clean foundation and all problematic code removed.
