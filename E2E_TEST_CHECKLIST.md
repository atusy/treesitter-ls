# E2E Test Checklist for LSP Features

This checklist ensures E2E tests follow established resilience patterns and prevent recurring issues identified in Sprint 119-120 retrospectives.

## Pre-Implementation Review

Before writing new E2E test code:

- [ ] Review `scripts/minimal_init.lua` for available test helpers
- [ ] Check existing similar tests (hover, definition, completion) for established patterns
- [ ] Identify if the LSP operation requires async language server indexing

## Mandatory Test Patterns

### 1. LSP Indexing Resilience

For operations requiring language server indexing (hover, completion, definition, etc.):

**REQUIRED**: Use `helper.retry_for_lsp_indexing()` for retry logic

```lua
local success = _G.helper.retry_for_lsp_indexing({
  child = child,
  lsp_request = function()
    -- Execute LSP request (e.g., vim.lsp.buf.hover())
  end,
  check = function()
    -- Return true when expected result is available
  end,
  max_retries = 20,      -- optional, default: 20
  wait_ms = 3000,        -- optional, default: 3000
  retry_delay_ms = 500,  -- optional, default: 500
})

MiniTest.expect.equality(success, true, "Should eventually get result")
```

**DO NOT**: Implement manual retry loops with `for _ = 1, 20 do ... end`

### 2. Async Path Verification

For PBIs implementing async I/O (PBI-141+):

- [ ] Create dedicated test case with `_async` suffix (e.g., `markdown_rust_async`)
- [ ] Use realistic scenario that exercises full request/response cycle
- [ ] Verify the async path returns expected results (not just "got response")

### 3. Coordinate Transformation Verification

For LSP responses with position ranges (textEdit, Location, etc.):

- [ ] Verify ranges are in host document coordinates (not virtual document)
- [ ] Test position-based assertions (e.g., `range.start.line >= expected_host_line`)
- [ ] Document expected coordinate systems in test comments

## Test Structure Template

```lua
T["feature_name"] = create_file_test_set(".md", {
  -- Test document content with clear markers
  "# Example",
  "",
  "```rust",
  "// Test code here",
  "```",
})

T["feature_name"]["operation_succeeds_with_expected_result"] = function()
  -- Position cursor with clear line reference
  child.cmd([[normal! 4G4|]])  -- line 4, column 4

  -- Use retry helper for LSP operations
  local success = _G.helper.retry_for_lsp_indexing({
    child = child,
    lsp_request = function()
      child.lua([[vim.lsp.buf.operation()]])
    end,
    check = function()
      -- Check for expected result
      return child.lua_get([[_G.result ~= nil]])
    end,
  })

  MiniTest.expect.equality(success, true, "Should get result")

  -- Verify result details
  local result = child.lua_get([[_G.result]])
  -- Add specific assertions
end
```

## Definition of Done for E2E Tests

- [ ] Test uses `helper.retry_for_lsp_indexing()` for LSP operations
- [ ] Test passes consistently (no flaky failures)
- [ ] Test verifies specific expected behavior (not just "got response")
- [ ] Test includes clear comments explaining cursor positions and expected results
- [ ] For async features: dedicated async path test exists
- [ ] For range-based responses: coordinate transformation verified

## Common Pitfalls

1. **Manual retry loops**: Always use `helper.retry_for_lsp_indexing()` instead
2. **Insufficient retry time**: Language servers may need 10+ seconds for complex indexing
3. **Weak assertions**: Verify specific expected results, not just response presence
4. **Missing async verification**: Async PBIs need dedicated tests exercising async path

## References

- Sprint 119 Retrospective: Created `helper.retry_for_lsp_indexing()` to eliminate duplication
- Sprint 120 Retrospective: Identified need for checklist to enforce pattern adoption
- Test Helper: `/Users/atusy/ghq/github.com/atusy/kakehashi/scripts/minimal_init.lua`
- Example Tests: `tests/test_lsp_hover.lua`, `tests/test_lsp_goto_definition.lua`
