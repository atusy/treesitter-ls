# Sprint 135 Backlog Refinement Summary

**Date**: 2026-01-04
**Event**: Backlog Refinement for Sprint 135
**Participants**: Product Owner (AI-agentic), Development Team
**Duration**: Full refinement session

## Context

- **Sprint 133**: Completed PBI-178 (fakeit bridge infrastructure)
- **Sprint 134**: Completed PBI-179 (real lua-language-server initialization)
- **Current Foundation**:
  - BridgeConnection spawns real lua-language-server process
  - JSON-RPC framing with Content-Length headers working
  - Initialize → Initialized → didOpen handshake complete
  - Phase 1 notification guard (SERVER_NOT_INITIALIZED)
  - Bounded 5s timeout on initialization
  - 384 unit tests passing, E2E tests with real lua-ls working

## Refinement Outcomes

### PBI-180: Real Completion from lua-language-server

**Status Before**: ready
**Status After**: refining
**Complexity Identified**: 8 acceptance criteria mixing infrastructure with feature

**Key Findings**:
- **Infrastructure Components** (5 AC): send_request, response correlation, request superseding, timeout, Phase 2 guard
- **Feature Components** (3 AC): Virtual document URI, position/range translation, E2E tests
- **Risk**: High complexity may cause sprint failure or incomplete delivery
- **Concern**: Request superseding adds significant complexity (PendingIncrementalRequests, race condition handling)

**Recommendation**: **Split into PBI-180a (MVP) and PBI-180b (Robustness)**

**Detailed Analysis**: See `/Users/atusy/ghq/github.com/atusy/treesitter-ls___async-bridge/docs/sprint-135-pbi-180-split-analysis.md`

**Action Required**: Sprint Planning must decide split vs. keep before selecting Sprint 135 scope

---

### PBI-181: Hover Information for Lua

**Status Before**: draft
**Status After**: ready

**Refinement Actions**:
1. **Clarified acceptance criteria** (6 AC total):
   - LanguageServerPool.hover() wired to BridgeConnection.send_request
   - Virtual document URI and position translation
   - Hover response returned (no range translation needed - ranges are optional in LSP)
   - Request superseding reuses PendingIncrementalRequests from PBI-180
   - E2E test with real lua-ls hover (e.g., for built-in `print` function)
   - All unit tests pass

2. **Added dependency notes**:
   - Depends on PBI-180 (or PBI-180a if split) for send_request infrastructure
   - Reuses request superseding pattern - only adds hover-specific wiring
   - Lower complexity than PBI-180 (infrastructure exists)

3. **Added verification commands**: All acceptance criteria now have specific grep/test commands for verification

4. **Sizing estimate**: Smaller than PBI-180; could potentially pair with another small PBI in same sprint

**Readiness**: PBI-181 is now **ready** for selection in Sprint 135 or later (contingent on PBI-180/180a completion)

---

### PBI-182: Definition and Signature Help

**Status**: draft (not refined this session)
**Rationale**: Focus on getting PBI-180/181 ready first; PBI-182 can be refined once PBI-180/181 pattern is established

**Recommendation**: Defer refinement to Sprint 136 or later when request pattern is proven

---

### PBI-183: Request Cancellation and Superseding

**Status**: draft (not refined this session)
**Note**: If PBI-180 is split, PBI-183 may become redundant (superseding covered by PBI-180b)

**Recommendation**: Re-evaluate PBI-183 after Sprint Planning decision on PBI-180 split

## Backlog State Summary

| PBI | Status | AC Count | Complexity | Notes |
|-----|--------|----------|------------|-------|
| PBI-178 | done | 7 | - | Sprint 133: Fakeit infrastructure |
| PBI-179 | done | 7 | - | Sprint 134: Real LSP initialization |
| PBI-180 | **refining** | 8 | **High** | Decision needed: split vs. keep |
| PBI-181 | **ready** | 6 | Medium | Depends on PBI-180/180a |
| PBI-182 | draft | 4 | TBD | Defer refinement |
| PBI-183 | draft | 4 | TBD | May be redundant if PBI-180 splits |

## Sprint 135 Planning Preparation

### Ready for Selection
- **If PBI-180 kept as-is**: PBI-180 (8 AC, high complexity)
- **If PBI-180 split**: PBI-180a (5 AC, medium complexity)
- **Future sprint**: PBI-181 (6 AC, medium complexity, depends on PBI-180/180a)

### Not Ready for Selection
- PBI-182: draft status, needs refinement
- PBI-183: draft status, may be redundant

### Decision Points for Sprint Planning

1. **PBI-180 Split Decision**:
   - Review `/Users/atusy/ghq/github.com/atusy/treesitter-ls___async-bridge/docs/sprint-135-pbi-180-split-analysis.md`
   - Decide: Split (PBI-180a/180b) or Keep (PBI-180 as-is)
   - If split: Adjust backlog before sprint selection

2. **Sprint Scope**:
   - Option A: PBI-180a only (focus on MVP delivery)
   - Option B: PBI-180 as-is (higher risk, more features)
   - Option C: PBI-180a + PBI-181 (if estimates allow)

3. **Risk Mitigation**:
   - If keeping PBI-180: Plan checkpoints, consider 2-sprint buffer
   - If splitting: Commit to PBI-180b in Sprint 136 to complete robustness

## ADR-0012 Phase 1 Progress

### Completed (PBI-178, PBI-179)
- ✅ Bridge module structure (pool.rs, connection.rs)
- ✅ BridgeConnection spawns real language server
- ✅ JSON-RPC framing (Content-Length headers)
- ✅ Initialize → Initialized → didOpen handshake
- ✅ Phase 1 notification guard (SERVER_NOT_INITIALIZED)
- ✅ Bounded timeout (5s) on initialization
- ✅ E2E tests with real lua-language-server

### Next (PBI-180/180a)
- ⏳ send_request with request ID tracking and response correlation
- ⏳ Virtual document URI and position translation
- ⏳ Real completion requests to lua-language-server
- ⏳ Bounded timeout on requests (5s default)
- ⏳ Phase 2 guard (allow requests after initialized, before didOpen)
- ⏳ Request superseding pattern (PBI-180 full, or deferred to PBI-180b)

### Future (PBI-181+)
- ⏹️ Hover, definition, signatureHelp requests
- ⏹️ Multiple embedded languages (Python, Lua, SQL simultaneously)
- ⏹️ Parallel initialization of multiple language servers

## Lessons Applied

### From Sprint 134 Retrospective
1. **"Add performance budgets to ADR-0012 Phase 2/3"**: Not addressed in this refinement (Phase 2 scope)
2. **"Document E2E testing patterns in ADR-0013"**: Not addressed (waiting for more patterns to emerge)
3. **JSON-RPC framing checklist**: Referenced in PBI-180 analysis (framing patterns proven in Sprint 134)

### From Sprint 133 Retrospective
1. **"Document two-pass strategy"**: Applied - PBI-180 analysis references fakeit→real progression
2. **"Test baseline hygiene"**: Current baseline is clean (384 unit tests, 3 E2E tests passing)
3. **"Dead code convention"**: Applied - existing code uses `#[allow(dead_code)]` with phase comments

## Retrospective Items for Follow-up

### Active Improvements Affected by This Refinement
- **"Document two-pass (fakeit → real) strategy in ADR-0012"** (Sprint 133): PBI-180 split analysis documents progression from infrastructure to feature
- **"Add performance budgets to ADR-0012 Phase 2/3"** (Sprint 134): Deferred - Phase 1 not complete yet

### New Retrospective Candidates
- **Backlog refinement patterns**: This refinement session created detailed split analysis - consider template for future complex PBIs
- **Complexity signals**: 8+ acceptance criteria may indicate split opportunity
- **Infrastructure vs. feature**: Clear separation helps with incremental delivery

## References

- ADR-0012: Multi-Language Server Async Bridge Architecture
- docs/sprint-135-pbi-180-split-analysis.md: Detailed PBI-180 split recommendation
- docs/json-rpc-framing-checklist.md: LSP Base Protocol patterns from Sprint 134
- ASYNC_BRIDGE_REMOVAL.md: Context on re-implementation from scratch
