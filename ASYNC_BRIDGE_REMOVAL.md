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

### 3. Module Structure Changes

**Updated `src/lsp/bridge.rs`:**
- Removed `mod tokio_connection;`
- Removed `mod tokio_async_pool;`
- Removed `pub use tokio_async_pool::TokioAsyncLanguageServerPool;`
- Added `mod stub_pool;`
- Added `pub use stub_pool::StubLanguageServerPool;`

## What Was Preserved

These components are **NOT** part of the problematic implementation and remain for re-use:

### 1. Bridge Infrastructure
- **`cleanup.rs`** - Temporary directory cleanup utilities
- **`connection.rs`** - Connection traits and types (interface definitions)
- **`error_types.rs`** - LSP-compliant error codes (will be reused per ADR-0012 §1)
- **`workspace.rs`** - Workspace setup utilities

### 2. Response Wrapper Types
Located in `src/lsp/bridge/text_document/`:
- `CompletionWithNotifications`
- `HoverWithNotifications`
- `SignatureHelpWithNotifications`
- `GotoDefinitionWithNotifications`

**Why preserved:** These types capture both LSP responses and `$/progress` notifications. They're part of the **interface**, not the flawed implementation. The new `LanguageServerPool` will continue to return these types.

### 3. Configuration Structures
- `BridgeServerConfig` - Defines server command, args, environment
- `WorkspaceType` - Defines workspace isolation strategy

**Why preserved:** These define **what** to bridge, not **how**. Configuration schema is stable.

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

- [ ] Create `src/lsp/bridge/pool.rs` with `LanguageServerPool`
- [ ] Create `src/lsp/bridge/connection.rs` (replace the trait-only version) with `BridgeConnection`
- [ ] Implement Phase 1 functionality (single-LS-per-language)
- [ ] Reuse `error_types.rs` for LSP-compliant error codes
- [ ] Reuse `workspace.rs` for workspace setup
- [ ] Continue returning `WithNotifications` wrapper types
- [ ] Update `src/lsp/lsp_impl.rs` to use new `LanguageServerPool` instead of stub
- [ ] Write tests for new implementation
- [ ] Verify no hangs under concurrent load
- [ ] Remove `stub_pool.rs` once new implementation is complete

## Files Changed Summary

### Deleted
- `src/lsp/bridge/tokio_connection.rs` (1,271 lines)
- `src/lsp/bridge/tokio_async_pool.rs` (2,409 lines)


### Modified
- `src/lsp/bridge.rs` (module structure)
- `src/lsp/lsp_impl.rs` (use stub instead of async pool)
- `src/lsp/lsp_impl/text_document/completion.rs` (extract `.response`)
- `src/lsp/lsp_impl/text_document/definition.rs` (extract `.response`)
- `src/lsp/lsp_impl/text_document/hover.rs` (extract `.response`)
- `src/lsp/lsp_impl/text_document/signature_help.rs` (extract `.response`)

### Preserved
- `src/lsp/bridge/cleanup.rs`
- `src/lsp/bridge/connection.rs` (trait definitions)
- `src/lsp/bridge/error_types.rs`
- `src/lsp/bridge/workspace.rs`
- `src/lsp/bridge/text_document/*.rs` (response wrappers)

## Compilation Status

✅ Code compiles successfully (`cargo check` passes)

The codebase is ready for re-implementation according to ADR-0012 with a clean foundation and all problematic code removed.
