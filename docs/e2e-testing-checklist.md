# E2E Testing Anti-Pattern Checklist

**Created**: Sprint 136 Retrospective
**Purpose**: Prevent E2E tests from testing the wrong layer (library internals vs binary behavior)

## Core Principle: Test Through Binary, Not Library

E2E tests must verify user-facing behavior by testing the treesitter-ls binary, not internal library code.

**WRONG Pattern (Sprint 133-135 mistake)**:
```rust
// ❌ Importing and testing BridgeConnection directly
use treesitter_ls::lsp::bridge::connection::BridgeConnection;

#[tokio::test]
async fn test_completion() {
    let connection = BridgeConnection::new("lua-language-server").await?;
    connection.send_request("textDocument/completion", params).await?;
}
```

**RIGHT Pattern (Sprint 136 correction)**:
```rust
// ✅ Testing through treesitter-ls binary via LspClient
mod helpers;
use helpers::lsp_client::LspClient;

#[test]
fn test_completion() {
    let mut client = LspClient::new();  // Spawns treesitter-ls binary
    client.send_request("textDocument/completion", params);
}
```

## Verification Criteria for E2E Tests

### 1. Binary-First Test
- [ ] Test spawns treesitter-ls binary (not imports internal modules)
- [ ] Test uses LspClient helper or equivalent
- [ ] Test communicates via LSP protocol over stdin/stdout

### 2. User-Facing Behavior
- [ ] Test verifies behavior users/editors experience
- [ ] Test sends real LSP requests to real binary
- [ ] Test validates real LSP responses

### 3. No Library Imports
- [ ] No `use treesitter_ls::lsp::bridge::*` imports
- [ ] No direct BridgeConnection, LanguageServerPool usage
- [ ] Exception: Helper modules (LspClient) are OK

### 4. Test File Naming
- [ ] E2E tests named `e2e_lsp_*.rs` (testing LSP binary)
- [ ] NOT `e2e_bridge_*.rs` (suggests testing bridge library)
- [ ] Exception: `e2e_bridge_init.rs` acceptable as unit-ish test for connection initialization

## When Library-Level Tests Are Acceptable

**Unit-ish tests** that verify internal component behavior in isolation:
- `e2e_bridge_init.rs` - Tests BridgeConnection initialization protocol
- Tests requiring `#[cfg(feature = "e2e")]` for test infrastructure
- Tests that are clearly marked as testing implementation details

These should:
1. Be clearly documented as testing internal behavior
2. Not be primary verification for user-facing features
3. Supplement (not replace) true E2E tests via binary

## Sprint 136 Learning

**Problem Identified**: Mid-sprint user feedback revealed tests/e2e_bridge_completion.rs was testing wrong layer
- Test directly imported BridgeConnection library code
- Test bypassed treesitter-ls binary entirely
- Test couldn't catch integration issues in binary wiring

**Solution Applied**: Created tests/e2e_lsp_lua_completion.rs
- Spawns real treesitter-ls binary via LspClient
- Sends LSP requests through stdin/stdout
- Verifies real responses from binary

**Deprecation Action**: Existing wrong-layer tests marked deprecated
- Added clear deprecation comments explaining why wrong
- Pointed to correct pattern (e2e_lsp_lua_completion.rs)
- Kept e2e_bridge_init.rs as acceptable unit-ish test

## Integration with Acceptance Criteria

When writing PBI acceptance criteria, ensure E2E verification uses binary-first pattern:

**WRONG AC**:
```
E2E test uses BridgeConnection to send completion request
Verification: cargo test --test e2e_bridge_completion --features e2e
```

**RIGHT AC**:
```
E2E test using treesitter-ls binary receives real completion from lua-ls
Verification: cargo test --test e2e_lsp_lua_completion --features e2e
```

## References

- Sprint 136 PBI-184 AC5: "E2E tests use treesitter-ls binary (LspClient), NOT Bridge library directly"
- Pattern established: tests/e2e_lsp_protocol.rs, tests/e2e_lsp_lua_completion.rs
- Helper infrastructure: tests/helpers/lsp_client.rs
