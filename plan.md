# Sprint 109 Plan: PBI-132 - Bridge I/O Timeout

## Sprint Goal
Add timeout mechanism to bridge I/O operations to prevent server hangs when bridged language servers are slow or unresponsive.

## Product Backlog Item
- **ID**: PBI-132
- **Story**: As a developer editing Lua files, I want treesitter-ls to remain responsive when bridged language servers are slow or unresponsive, so that the LSP does not hang indefinitely

## Acceptance Criteria
1. Bridge I/O operations have configurable timeout (read_response_for_id_with_notifications accepts timeout parameter; defaults to 30 seconds)
2. Timeout triggers graceful error handling (returns None response with empty notifications; no infinite loop)
3. All bridge request methods use timeout
4. Unit test verifies timeout behavior

## Technical Analysis

### Current State
The `read_response_for_id_with_notifications()` method in `/Users/atusy/ghq/github.com/atusy/treesitter-ls/src/lsp/bridge/connection.rs` (lines 259-339) has an infinite loop that:
- Uses blocking I/O via `BufReader::read_line()`
- Waits indefinitely for responses
- Has no timeout mechanism

### Design Decision
Use `std::time::Duration` and `std::time::Instant` to implement timeout:
- Track start time at beginning of read operation
- Check elapsed time after each read attempt
- Return None response when timeout expires

### Methods to Update
The following methods call `read_response_for_id_with_notifications` and need timeout support:
- `spawn_with_notifications` (initialization)
- `goto_definition_with_notifications`
- `type_definition_with_notifications`
- `implementation_with_notifications`
- `declaration_with_notifications`
- `document_highlight_with_notifications`
- `document_link_with_notifications`
- `folding_range_with_notifications`
- `hover_with_notifications`
- `completion_with_notifications`
- `signature_help_with_notifications`
- `references_with_notifications`
- `rename_with_notifications`
- `code_action_with_notifications`
- `formatting_with_notifications`
- `inlay_hint_with_notifications`
- `prepare_call_hierarchy_with_notifications`
- `incoming_calls_with_notifications`
- `outgoing_calls_with_notifications`
- `prepare_type_hierarchy_with_notifications`
- `supertypes_with_notifications`
- `subtypes_with_notifications`
- `shutdown` (via `read_response_for_id`)

## Subtasks (TDD Order)

### Subtask 1: Add timeout parameter to read_response_for_id_with_notifications
- [ ] **RED**: Write test that `read_response_for_id_with_notifications` accepts a `Duration` timeout parameter
- [ ] **GREEN**: Add timeout parameter to method signature; existing callers compile with the new signature
- [ ] **REFACTOR**: None needed

**File**: `src/lsp/bridge/connection.rs`
**Test location**: `src/lsp/bridge/connection.rs` (tests module)

### Subtask 2: Implement timeout check in read loop
- [ ] **RED**: Write test that verifies timeout returns None response when no data arrives within timeout period
- [ ] **GREEN**: Add `Instant::now()` at start, check `elapsed() > timeout` in loop, return `ResponseWithNotifications { response: None, notifications }` on timeout
- [ ] **REFACTOR**: Extract timeout check into helper if needed

**File**: `src/lsp/bridge/connection.rs`
**Implementation notes**:
- Use `std::time::Instant` and `std::time::Duration`
- Check timeout after each `read_line` call
- Log timeout occurrence for debugging

### Subtask 3: Add DEFAULT_TIMEOUT constant
- [ ] **RED**: Write test that DEFAULT_TIMEOUT constant equals 30 seconds
- [ ] **GREEN**: Add `const DEFAULT_TIMEOUT: Duration = Duration::from_secs(30);`
- [ ] **REFACTOR**: None needed

**File**: `src/lsp/bridge/connection.rs`

### Subtask 4: Update read_response_for_id to use timeout
- [ ] **RED**: Write test verifying read_response_for_id uses DEFAULT_TIMEOUT
- [ ] **GREEN**: Update `read_response_for_id` to pass `DEFAULT_TIMEOUT` to `read_response_for_id_with_notifications`
- [ ] **REFACTOR**: None needed

**File**: `src/lsp/bridge/connection.rs`

### Subtask 5: Update spawn_with_notifications to use timeout
- [ ] **RED**: Verify spawn uses timeout (compile check)
- [ ] **GREEN**: Update spawn to pass `DEFAULT_TIMEOUT` when calling `read_response_for_id_with_notifications`
- [ ] **REFACTOR**: None needed

**File**: `src/lsp/bridge/connection.rs`

### Subtask 6: Update all bridge request methods to use timeout
- [ ] **RED**: Compile check - all methods must pass timeout
- [ ] **GREEN**: Update all 22 `*_with_notifications` bridge methods to pass `DEFAULT_TIMEOUT`
- [ ] **REFACTOR**: Consider extracting common timeout handling pattern if repetitive

**File**: `src/lsp/bridge/connection.rs`
**Methods to update**:
- `goto_definition_with_notifications`
- `type_definition_with_notifications`
- `implementation_with_notifications`
- `declaration_with_notifications`
- `document_highlight_with_notifications`
- `document_link_with_notifications`
- `folding_range_with_notifications`
- `hover_with_notifications`
- `completion_with_notifications`
- `signature_help_with_notifications`
- `references_with_notifications`
- `rename_with_notifications`
- `code_action_with_notifications`
- `formatting_with_notifications`
- `inlay_hint_with_notifications`
- `prepare_call_hierarchy_with_notifications`
- `incoming_calls_with_notifications`
- `outgoing_calls_with_notifications`
- `prepare_type_hierarchy_with_notifications`
- `supertypes_with_notifications`
- `subtypes_with_notifications`

### Subtask 7: Update wait_for_indexing_with_notifications to use timeout
- [ ] **RED**: Verify method has timeout protection
- [ ] **GREEN**: Add timeout check to the indexing wait loop (uses similar pattern)
- [ ] **REFACTOR**: None needed

**File**: `src/lsp/bridge/connection.rs`
**Notes**: This method has its own read loop (lines 418-468) that also needs timeout protection

### Subtask 8: Unit test for timeout behavior
- [ ] **RED**: Write comprehensive test that simulates slow/unresponsive server
- [ ] **GREEN**: Test passes with timeout implementation
- [ ] **REFACTOR**: None needed

**File**: `src/lsp/bridge/connection.rs` (tests module)
**Test approach**:
- Create a mock scenario where no response arrives
- Verify function returns within timeout period
- Verify response is None and notifications are preserved

## Definition of Done
- [ ] All subtasks completed
- [ ] All tests pass (`cargo test`)
- [ ] No clippy warnings (`cargo clippy -- -D warnings`)
- [ ] Code formatted (`cargo fmt`)
- [ ] Each TDD phase committed separately
- [ ] Acceptance criteria verified:
  - [ ] AC1: read_response_for_id_with_notifications accepts timeout parameter; defaults to 30 seconds
  - [ ] AC2: Timeout returns None response with empty notifications; no infinite loop
  - [ ] AC3: All bridge request methods pass timeout
  - [ ] AC4: Unit test verifies timeout behavior

## Notes
- The blocking I/O nature of `BufReader::read_line()` means we cannot interrupt mid-read
- Timeout check happens between read operations, not during
- For truly responsive timeout, would need non-blocking I/O or async, but that's out of scope for this PBI
- 30 second default is generous to allow for slow servers while preventing indefinite hangs
