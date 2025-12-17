---
description: Check for dead code and tidying opportunities after refactoring
---

# Tidy Check

Run this after completing a refactoring to ensure no dead code was left behind.

## Checklist

1. **Dead Code Detection**
   - Run `RUSTFLAGS="-W dead_code -W unused" cargo build 2>&1 | grep -i warning`
   - Check for unused functions, variables, imports

2. **Public API Wrappers**
   - Search for thin wrapper functions that delegate to `*_with_options` or similar patterns
   - Grep: `rg "pub fn \w+\(" --type rust | xargs -I{} basename {}`
   - For each public function, verify it's actually used: `rg "function_name[^_]" --type rust`

3. **Unused Imports**
   - Run `cargo clippy -- -W unused-imports`

4. **Backward Compatibility Check**
   - If removing public functions, check if they're part of the documented API
   - If external usage is possible, consider deprecation instead of removal

## Action Items

After running checks, either:
- **Remove** dead code if it's internal or unused
- **Deprecate** with `#[deprecated]` if it might have external users
- **Document** if keeping for API stability

## Example Grep Patterns

```bash
# Find potential thin wrappers
rg "pub fn (\w+)\(.*\) -> .* \{$" -A2 --type rust | grep -B1 "_with_"

# Find functions with _with_options pattern
rg "fn \w+_with_options" --type rust

# Check if base function is used anywhere
rg "function_name\(" --type rust | grep -v "pub fn"
```
