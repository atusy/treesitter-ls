---
name: tidy
description: Check for dead code and thin wrappers after refactoring. Use automatically during TDD refactor phase or after any structural code changes.
---

# INSTRUCTIONS

Run the `/tidy` command to check for dead code and tidying opportunities.

This skill auto-triggers during:
- TDD refactor phase (`/tdd:refactor`)
- After renaming or moving functions
- After adding parameters to existing functions
- After any structural refactoring

The `/tidy` command will:
1. Run dead code detection (`RUSTFLAGS="-W dead_code -W unused" cargo build`)
2. Check for thin wrapper functions (e.g., `foo()` that just calls `foo_with_options(..., None)`)
3. Check for unused imports
4. Verify backward compatibility if removing public APIs
