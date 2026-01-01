# ADR-0015: Dependency Lock File for External Resources

## Status

Proposed

## Context

treesitter-ls dynamically fetches resources from external repositories at runtime:

### Current Dependency: nvim-treesitter

| Resource | Source | Usage |
|----------|--------|-------|
| `parsers.lua` | nvim-treesitter `main` branch | Parser metadata (URL, revision, location) |
| Query files (`*.scm`) | nvim-treesitter `main` branch | Syntax highlighting, injections, locals |

Currently, the code fetches from the `main` branch directly:

```rust
// src/install/metadata.rs
const PARSERS_LUA_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/main/lua/nvim-treesitter/parsers.lua";

// src/install/queries.rs
const NVIM_TREESITTER_QUERIES_URL: &str =
    "https://raw.githubusercontent.com/nvim-treesitter/nvim-treesitter/main/runtime/queries";
```

### Problems with Always Fetching `main`

1. **Non-reproducible builds**: Running `treesitter-ls install lua` today vs. tomorrow may produce different results if nvim-treesitter updates parser revisions or query files.

2. **Silent breakage**: nvim-treesitter may introduce breaking changes to query syntax, parser metadata format, or file locations that break treesitter-ls without warning.

3. **Testing instability**: CI tests may pass one day and fail the next due to upstream changes.

4. **No audit trail**: Users cannot determine which version of external resources was used for a specific treesitter-ls version.

### Why Not Just Pin in Code?

Hardcoding a commit hash in the source code would work, but:
- Requires code changes to update dependencies
- No separation between "which version to use" (configuration) vs. "how to use it" (code)
- Difficult for users to override when needed

## Decision

**Introduce `treesitter-ls.lock` as a machine-generated lock file that pins external resource versions.**

### Lock File Format

```toml
# treesitter-ls.lock
# Auto-generated. Do not edit manually. Run `treesitter-ls lock update` to refresh.

[metadata]
generated_at = "2025-01-15T10:30:00Z"
treesitter_ls_version = "0.1.0"

[nvim-treesitter]
# Git commit hash from nvim-treesitter main branch
revision = "abc123def456789..."
# When this revision was recorded
locked_at = "2025-01-15T10:30:00Z"
```

### Resolution Order

When fetching external resources, treesitter-ls resolves the version as follows:

```
1. CLI flag: `--nvim-treesitter-rev <commit>`  (highest priority)
2. Environment variable: `TREESITTER_LS_NVIM_TREESITTER_REV`
3. Lock file: `treesitter-ls.lock` in current directory or ancestors
4. Project lock file: `.treesitter-ls/lock` in project root
5. Default: `main` branch (lowest priority)
```

### CLI Commands

```bash
# Create/update lock file with current main branch HEAD
treesitter-ls lock update

# Create/update lock file pinning to specific revision
treesitter-ls lock update --rev abc123

# Show current lock status
treesitter-ls lock status

# Remove lock file (revert to following main)
treesitter-ls lock remove
```

### Lock File Location

The lock file follows a standard discovery pattern:

1. **Project-local** (recommended): `./treesitter-ls.lock` in repository root
2. **Directory-specific**: `.treesitter-ls/lock` in any ancestor directory
3. **User-global**: `~/.config/treesitter-ls/lock`

This allows:
- Per-project pinning (commit `treesitter-ls.lock` to version control)
- User-wide defaults (for consistent local development)
- Workspace overrides (for monorepos)

### Interaction with Caching

The existing metadata cache (`MetadataCache`) already caches `parsers.lua` content for 24 hours. The lock file changes *which* content to fetch, not caching behavior:

```
Without lock file:
  fetch(main branch) → cache for 24h

With lock file (revision=abc123):
  fetch(abc123) → cache for 24h
```

Since locked revisions are immutable, the cache TTL could be extended or made permanent for locked resources (future optimization).

### Schema Evolution

The lock file includes a schema version for forward compatibility:

```toml
[metadata]
schema_version = 1
# ... other fields
```

Future versions may add new locked dependencies or fields while maintaining backward compatibility.

## Consequences

### Positive

- **Reproducible installations**: Same lock file = same parser/query versions
- **Controlled updates**: Explicitly decide when to update via `lock update`
- **Audit trail**: Lock file in version control documents exact dependency state
- **CI stability**: Tests run against known-good dependency versions
- **Rollback capability**: Revert lock file to restore previous dependency state
- **User flexibility**: CLI flags and env vars allow temporary overrides

### Negative

- **Maintenance overhead**: Must periodically run `lock update` to get upstream improvements
- **Staleness risk**: Old lock files may miss important bug fixes or new language support
- **Additional file**: One more file to commit and manage in projects
- **Discovery complexity**: Multiple lock file locations could cause confusion

### Neutral

- **Not a full package manager**: Only locks metadata sources, not parser binaries themselves (parser binaries are already pinned by nvim-treesitter's `revision` field in `parsers.lua`)
- **Single dependency for now**: Initially only nvim-treesitter, but schema supports future additions
- **Optional feature**: Users who prefer following `main` simply don't create a lock file

## Implementation Phases

### Phase 1: Lock File Basics
- [ ] Define lock file schema and parser
- [ ] Implement lock file discovery (cwd → ancestors → user config)
- [ ] Add `lock update` command to generate lock file
- [ ] Modify resource fetchers to respect locked revision

### Phase 2: CLI Integration
- [ ] Add `lock status` command
- [ ] Add `lock remove` command
- [ ] Add `--nvim-treesitter-rev` flag to relevant commands
- [ ] Add `TREESITTER_LS_NVIM_TREESITTER_REV` environment variable support

### Phase 3: User Experience
- [ ] Warn when lock file is stale (> 30 days old)
- [ ] Add `--update-lock` flag to installation commands
- [ ] Document lock file workflow in user guide

### Phase 4: Future Considerations
- [ ] Extended cache TTL for locked revisions
- [ ] Lock file validation against treesitter-ls compatibility
- [ ] Support for additional external dependencies

## Related Decisions

- [ADR-0004](0004-keep-tree-sitter-cli-dependency.md): External tool dependencies
- [ADR-0003](0003-parser-compilation-fallback-strategy.md): Parser installation strategy

## References

- [Cargo.lock](https://doc.rust-lang.org/cargo/guide/cargo-toml-vs-cargo-lock.html) - Rust's dependency lock file
- [package-lock.json](https://docs.npmjs.com/cli/v9/configuring-npm/package-lock-json) - npm's lock file
- [nvim-treesitter parsers.lua](https://github.com/nvim-treesitter/nvim-treesitter/blob/main/lua/nvim-treesitter/parsers.lua) - Current metadata source
