# Sprint 37 Implementation Plan - PBI-041

## Goal
Consolidate QueryLoader API to a single `resolve_query` entry point.

## Current State Analysis

**File:** `src/language/query_loader.rs`

**Current API (lines 24-31, 206-214):**
- `pub fn resolve_query_with_inheritance` - public entry point (creates HashSet, calls recursive)
- `fn resolve_query_recursive` - private helper with visited parameter
- `pub fn load_query_with_inheritance` - uses resolve_query_with_inheritance internally

**Callers of `resolve_query_with_inheritance`:**
1. `load_query_with_inheritance` (line 212) - internal use
2. Tests (7 test functions) - all in same file

**Target State:**
- Single `pub fn resolve_query` method as entry point
- `resolve_query_with_inheritance` removed (replaced by `resolve_query`)
- All tests updated to use new name
- No REVIEW comments in file

## Acceptance Criteria Verification Commands

```bash
# AC1: Single resolve_query method exists
grep -c 'pub fn resolve_query\b' src/language/query_loader.rs  # Expected: 1

# AC2: resolve_query_with_inheritance removed or made private
grep -c 'pub fn resolve_query_with_inheritance' src/language/query_loader.rs  # Expected: 0

# AC3: All tests pass
cargo test --lib -- query_loader

# AC4: No REVIEW comments remain in file
grep -c '// REVIEW:' src/language/query_loader.rs  # Expected: 0
```

## Implementation Plan

This is a pure refactoring task (structural changes only, no behavioral changes).
Since there are no behavioral changes, we use REFACTOR commits only.

### Subtask S37-1: Rename resolve_query_with_inheritance to resolve_query

**Type:** refactor (structural change)

**Changes:**
1. Rename `pub fn resolve_query_with_inheritance` to `pub fn resolve_query` (line 24)
2. Update docstring to reflect new name

**Verification:**
```bash
cargo test --lib -- query_loader
cargo clippy -- -D warnings
```

**Commit:** `refactor(query_loader): rename resolve_query_with_inheritance to resolve_query`

---

### Subtask S37-2: Update test calls to use new method name

**Type:** refactor (structural change)

**Changes:**
Update all test method calls from `resolve_query_with_inheritance` to `resolve_query`:
- Line 383: `test_resolve_query_with_inheritance_no_inheritance`
- Line 409: `test_resolve_query_with_inheritance_single_parent`
- Line 447: `test_resolve_query_with_inheritance_removes_directive`
- Line 473: `test_resolve_query_with_real_typescript`
- Line 520: `test_resolve_query_with_inheritance_circular_detection`
- Line 545: `test_resolve_query_with_real_javascript_multiple_inheritance`

Also rename test functions to match:
- `test_resolve_query_with_inheritance_no_inheritance` -> `test_resolve_query_no_inheritance`
- `test_resolve_query_with_inheritance_single_parent` -> `test_resolve_query_single_parent`
- `test_resolve_query_with_inheritance_removes_directive` -> `test_resolve_query_removes_directive`
- `test_resolve_query_with_inheritance_circular_detection` -> `test_resolve_query_circular_detection`

**Verification:**
```bash
cargo test --lib -- query_loader
cargo clippy -- -D warnings
```

**Commit:** `refactor(query_loader): update tests to use resolve_query`

---

### Subtask S37-3: Update load_query_with_inheritance caller

**Type:** refactor (structural change)

**Changes:**
Update line 212 from:
```rust
let query_str = Self::resolve_query_with_inheritance(runtime_bases, lang_name, file_name)?;
```
to:
```rust
let query_str = Self::resolve_query(runtime_bases, lang_name, file_name)?;
```

**Verification:**
```bash
cargo test --lib -- query_loader
cargo clippy -- -D warnings
```

**Commit:** `refactor(query_loader): update load_query_with_inheritance to use resolve_query`

---

### Subtask S37-4: Verify and remove any REVIEW comments

**Type:** refactor (structural change)

**Changes:**
Check for and remove any `// REVIEW:` comments in the file.

**Verification:**
```bash
grep -c '// REVIEW:' src/language/query_loader.rs  # Expected: 0
```

**Commit:** (if needed) `refactor(query_loader): remove REVIEW comments`

---

### Subtask S37-5: Verify all acceptance criteria pass

**Type:** verification

**Commands:**
```bash
# AC1: Single resolve_query method exists (expected: 1)
grep -c 'pub fn resolve_query\b' src/language/query_loader.rs

# AC2: resolve_query_with_inheritance removed (expected: 0)
grep -c 'pub fn resolve_query_with_inheritance' src/language/query_loader.rs

# AC3: All tests pass
cargo test --lib -- query_loader

# AC4: No REVIEW comments (expected: 0)
grep -c '// REVIEW:' src/language/query_loader.rs

# Full test suite
make test
make check
```

**No commit needed - verification only**

---

## Execution Order

Since all subtasks are structural changes with no dependencies between them,
they can be combined into fewer commits:

**Option A: Single refactor commit** (recommended for this small change)
- [ ] S37-1: Rename function
- [ ] S37-2: Update tests
- [ ] S37-3: Update internal caller
- [ ] S37-4: Remove REVIEW comments (if any)
- [ ] S37-5: Verify ACs

**Commit:** `refactor(query_loader): consolidate to single resolve_query entry point`

## Notes

- This is a pure rename refactoring with no behavioral changes
- All existing tests continue to verify the same behavior
- No new tests needed since behavior is unchanged
