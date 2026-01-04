# PBI-180 Split Analysis for Sprint 135 Planning

## Context

During Sprint 135 Backlog Refinement, PBI-180 was identified as having high complexity with 8 acceptance criteria mixing foundational infrastructure (send_request, response correlation, request superseding) with feature delivery (real completions).

## Current PBI-180 Scope

**User Story**: As a developer editing Lua files, I can receive real completion suggestions from lua-language-server for embedded Lua, so that I can write Lua code efficiently with accurate, context-aware completions.

**8 Acceptance Criteria**:
1. BridgeConnection.send_request implements textDocument/completion with request ID tracking and response correlation
2. Completion request uses virtual document URI with translated position from host coordinates
3. Completion response ranges translated back to host document coordinates before returning
4. **Request superseding**: newer completion cancels older with REQUEST_FAILED during init window
5. Bounded timeout (5s default, configurable) returns REQUEST_FAILED if lua-ls unresponsive
6. Phase 2 guard allows completion after initialized but before didOpen with wait pattern
7. E2E test sends completion to Lua block and receives real CompletionItems from lua-ls
8. E2E test verifies rapid completion requests trigger superseding with only latest processed

## Complexity Analysis

### Infrastructure Components (High Risk)
- **send_request method**: Core async request/response mechanism (#1)
- **Response correlation**: Mapping request IDs to responses (#1)
- **Request superseding**: PendingIncrementalRequests tracking (#4, #8)
- **Bounded timeout**: tokio::select! pattern (#5)
- **Phase 2 guard**: wait_for_initialized pattern (#6)

### Feature Components (Lower Risk)
- **Virtual document URI**: Build from injection region (#2)
- **Position translation**: Reuses existing CacheableInjectionRegion (#2)
- **Range translation**: Transform CompletionItem ranges back (#3)
- **E2E test**: Verify real lua-ls completions (#7)

## Risk Assessment

**Combining in Single Sprint**:
- Risk: High complexity may cause sprint failure or incomplete delivery
- Infrastructure bugs (superseding, timeout) could block feature delivery
- 8 acceptance criteria = high chance of discovering edge cases mid-sprint
- Request superseding is complex: needs careful handling of race conditions

**Splitting into Two Sprints**:
- Risk: Lower per-sprint complexity
- MVP (basic completions) delivered faster
- Robustness (superseding) added incrementally
- Each sprint has clearer success criteria

## Proposed Split

### Option A: Split by Infrastructure vs. Feature

**PBI-180a: Basic Completion Request/Response (MVP)**

**User Story**: As a developer editing Lua files, I can receive real completion suggestions from lua-language-server for embedded Lua, so that I can write Lua code efficiently with accurate, context-aware completions.

**5 Acceptance Criteria**:
1. BridgeConnection.send_request implements textDocument/completion with request ID tracking and response correlation
2. Completion request uses virtual document URI with translated position from host coordinates
3. Completion response ranges translated back to host document coordinates before returning
4. Bounded timeout (5s default) returns REQUEST_FAILED if lua-ls unresponsive (simple timeout, no superseding)
5. E2E test sends completion to Lua block and receives real CompletionItems from lua-ls

**Scope**: Deliver working completions for Lua code blocks. User gets results (or timeout error) for every completion request. No optimization for rapid typing.

**Exit Criteria**: Completions work when typing slowly or pausing. Rapid typing may show multiple results or timeouts, but system is stable.

---

**PBI-180b: Request Superseding Pattern (Robustness)**

**User Story**: As a developer editing Lua files rapidly, I only see relevant suggestions for my current code, not outdated results from earlier requests.

**Benefit**: Improves UX during rapid typing by cancelling stale completion requests.

**4 Acceptance Criteria**:
1. PendingIncrementalRequests struct tracks latest completion request per connection
2. Newer completion requests send REQUEST_FAILED to older pending requests with "superseded" reason
3. Phase 2 guard (wait_for_initialized) combined with superseding during initialization window
4. E2E test verifies rapid completion requests trigger superseding with only latest processed

**Scope**: Add request superseding optimization. Reuses send_request infrastructure from PBI-180a.

**Exit Criteria**: Rapid typing (typing "print" char-by-char) only processes latest completion, earlier ones receive REQUEST_FAILED.

**Dependency**: PBI-180a must complete first (provides send_request infrastructure).

### Option B: Keep as Single PBI

**Rationale for Keeping Together**:
- Request superseding is essential for MVP user experience
- Without it, rapid typing causes confusing UX (multiple outdated completions)
- ADR-0012 §7.3 specifies superseding as part of Phase 1 foundation
- Splitting delays core robustness feature

**Mitigation**:
- Strong TDD discipline: implement send_request first, then add superseding incrementally
- Treat acceptance criteria #1-6 as infrastructure subtasks, #7-8 as feature validation
- Daily progress checkpoints to detect if scope needs adjustment mid-sprint

## Recommendation

**Recommended**: **Option A - Split into PBI-180a and PBI-180b**

**Reasoning**:
1. **Deliver Value Faster**: PBI-180a delivers working completions (core user value) in Sprint 135
2. **Reduce Risk**: Each PBI has clearer scope and lower complexity
3. **Incremental Robustness**: PBI-180b adds optimization on stable foundation
4. **Alignment with TDD**: Natural test-first boundaries (basic request/response → superseding)
5. **Learning from Sprint 134**: Sprint 134 delivered 7 subtasks successfully; PBI-180a has 5 AC (comparable), PBI-180b has 4 AC (smaller follow-up)

**Counter to "Essential for MVP"**:
- Basic timeout (PBI-180a AC #4) provides acceptable UX: user sees timeout error if lua-ls is slow
- Request superseding (PBI-180b) is **optimization**, not **blocker** for completing Phase 1
- PBI-181 (hover) could proceed after PBI-180a, getting more features live sooner

**Alternative**: If Product Owner decides superseding is truly essential for MVP, keep as single PBI but:
- Plan for 2-sprint duration if needed
- Have clear "Phase 2 guard + basic timeout" checkpoint at mid-sprint
- Be ready to defer E2E superseding test (#8) to follow-up PBI if time runs short

## Decision Needed at Sprint Planning

Sprint Planning for Sprint 135 should:
1. Review this analysis
2. Decide: Split (PBI-180a/180b) or Keep (PBI-180 as-is)
3. If split: Adjust backlog accordingly before selecting Sprint 135 PBI
4. If keep: Acknowledge higher complexity and plan mitigation strategies

## ADR-0012 Alignment

Both options align with ADR-0012 Phase 1:
- **§7.3 Request Superseding Pattern**: Specified for incremental requests (completion, hover, signatureHelp)
- **§6.1 Two-Phase Notification Handling**: Phase 2 guard (wait_for_initialized) included in both options
- **§1 LSP Compliance**: Both options use REQUEST_FAILED error codes correctly

**Phase 1 Exit Criteria** (per ADR-0012):
> "All existing single-LS tests pass without hangs"
> "Can handle Python, Lua, SQL blocks simultaneously in markdown"
> "No initialization race failures under normal conditions"

PBI-180a satisfies "normal conditions" (single completion request, user pauses).
PBI-180b addresses "initialization race" with rapid requests.

**Interpretation**: Phase 1 can be satisfied with PBI-180a, making PBI-180b a Phase 1 refinement rather than blocker.

## Implementation Notes

If split is chosen:

**PBI-180a Implementation Path** (simpler):
1. Implement send_request with request ID tracking (no superseding map yet)
2. Add simple bounded timeout using tokio::time::timeout
3. Wire completion.rs to call send_request with virtual document URI
4. Translate position to/from virtual document
5. E2E test: Single completion request receives real CompletionItems

**PBI-180b Implementation Path** (builds on 180a):
1. Add PendingIncrementalRequests struct to BridgeConnection
2. Modify send_request to register incremental requests and supersede older ones
3. Combine wait_for_initialized with superseding check
4. E2E test: Rapid typing scenario verifies superseding behavior

## Related Documentation

- ADR-0012 §7.3: Request Superseding Pattern
- ADR-0012 §6.1: Two-Phase Notification Handling
- Sprint 134 Retrospective: Document two-pass (fakeit → real) strategy
- docs/json-rpc-framing-checklist.md: LSP Base Protocol patterns
