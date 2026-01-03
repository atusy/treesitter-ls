# Sprint Review - Sprint 126

**Date**: 2026-01-03
**PBI**: PBI-157 - Deep merge for initialization_options
**Sprint Goal**: Fix initialization_options shallow merge bug to comply with ADR-0010 deep merge semantics

## Sprint Status: BLOCKED - Cannot Accept Increment

### Definition of Done Results

#### 1. Unit Tests (`make test`): FAILED
**Status**: 4 test failures (unrelated to Sprint 126 deliverables)

```
FAILED tests (all in analysis::semantic module):
- test_indented_injection_semantic_tokens
- test_injection_semantic_tokens_basic
- test_nested_injection_semantic_tokens
- test_semantic_tokens_delta_with_injection

Error: "Should load markdown language" panic
```

**Analysis**: These failures are in `/Users/atusy/ghq/github.com/atusy/treesitter-ls___config-merge/src/analysis/semantic.rs` and are **NOT related to PBI-157 work**. PBI-157 only touched `/Users/atusy/ghq/github.com/atusy/treesitter-ls___config-merge/src/config.rs`. However, per Definition of Done, **all unit tests must pass**.

#### 2. Code Quality (`make check`): PASSED
- cargo check: PASSED
- cargo clippy: PASSED
- cargo fmt: PASSED

### What Was Delivered

**Commit**: `be5872e fix(config): deep merge initialization_options (ADR-0010)`

#### Implementation
1. **deep_merge_json helper function** (lines 35-58 in src/config.rs)
   - Recursive JSON object merging
   - Handles nested objects correctly
   - Non-objects: overlay replaces base

2. **resolve_language_server_with_wildcard updated** (line 94)
   - Deep merges initialization_options from wildcard and specific configs
   - Changed from shallow replace to deep merge

3. **merge_language_servers updated** (line 453)
   - Deep merges initialization_options across config layers
   - Changed from shallow replace to deep merge

#### Tests Added/Updated
**4 New Tests** (all in src/config.rs):
1. `test_resolve_language_server_deep_merges_initialization_options` (lines 2323-2371)
2. `test_merge_language_servers_deep_merges_initialization_options` (lines 2373-2422)
3. `test_resolve_language_server_specific_overrides_wildcard_same_key` (lines 2424-2464)
4. `test_resolve_language_server_nested_objects_deep_merge` (lines 2466-2512)

**1 Updated Test**:
- `test_resolve_language_server_with_wildcard_specific_overrides_wildcard` (lines 2189-2259)
  - Updated expectation to verify deep merge behavior
  - Added assertion for inherited `defaultOption` from wildcard

### Acceptance Criteria Verification

#### AC1: resolve_language_server_with_wildcard deep merges initialization_options JSON objects
**Status**: VERIFIED
- Test: `test_resolve_language_server_deep_merges_initialization_options`
- Verifies: wildcard `{feature1: true}` + specific `{feature2: true}` = merged `{feature1: true, feature2: true}`
- Implementation: Line 94 in src/config.rs uses `deep_merge_json`

#### AC2: merge_language_servers deep merges initialization_options across config layers
**Status**: VERIFIED
- Test: `test_merge_language_servers_deep_merges_initialization_options`
- Verifies: base layer `{baseOpt: 1}` + overlay `{overlayOpt: 2}` = merged `{baseOpt: 1, overlayOpt: 2}`
- Implementation: Line 453 in src/config.rs uses `deep_merge_json`

#### AC3: Deep merge preserves specific values over wildcard values for same keys
**Status**: VERIFIED
- Test: `test_resolve_language_server_specific_overrides_wildcard_same_key`
- Verifies: wildcard `{opt: 1}` + specific `{opt: 2}` = `{opt: 2}`
- Implementation: `deep_merge_json` overlay replaces base for same keys

#### AC4: Nested JSON objects are merged recursively
**Status**: VERIFIED
- Test: `test_resolve_language_server_nested_objects_deep_merge`
- Verifies: wildcard `{a: {b: 1}}` + specific `{a: {c: 2}}` = `{a: {b: 1, c: 2}}`
- Implementation: `deep_merge_json` recursive logic (line 49)

### Sprint Review Decision

**Increment Status**: NOT ACCEPTED

**Rationale**:
1. All acceptance criteria for PBI-157 are met
2. Code quality checks pass
3. **However**: 4 unrelated unit tests are failing (markdown language loading)
4. Definition of Done requires "All unit tests pass"
5. Cannot mark PBI-157 as "done" until all tests pass

### Next Steps

**Required Actions Before Acceptance**:
1. Fix 4 failing markdown language loading tests in `src/analysis/semantic.rs`
2. Verify all 438 tests pass
3. Re-run Sprint Review

**Alternative**:
If markdown tests are known flaky/environmental issues, consider:
- Documenting exclusion rationale
- Updating Definition of Done to specify "all tests related to current PBI"

### Files Modified
- `/Users/atusy/ghq/github.com/atusy/treesitter-ls___config-merge/src/config.rs` (+252 lines, comprehensive test coverage)

### Retrospective Notes
- PBI-157 implementation is complete and correct
- Pre-existing test failures blocking acceptance
- Consider CI/CD to catch test failures earlier
- Consider test categorization (unit/integration/e2e) to allow partial acceptance
