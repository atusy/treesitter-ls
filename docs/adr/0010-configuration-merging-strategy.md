# ADR-0010: Configuration Merging Strategy

## Status

Accepted (Implemented across Sprints 118, 119, 120)

## Context

treesitter-ls needs to support multiple configuration sources to accommodate different use cases:

1. **Programmed defaults**: Built-in defaults for zero-config usage
2. **User-wide defaults**: Settings that apply across all projects for a user
3. **Project-specific settings**: Configuration local to a specific project/directory
4. **Session-specific overrides**: Settings passed directly from the LSP client at initialization

The limitations of the current system are:

- Missing **User-wide defaults**
- **Project-specific settings** are the only based on `./treesitter-ls.toml`
- Complex `captureMappings` overrides must be duplicated in each project's `treesitter-ls.toml`

The standard pattern in many language servers and CLI tools is layered configuration with clear precedence rules. This ADR proposes adding a **user configuration layer** between programmed defaults and project config.

## Decision

**Implement a four-layer configuration system with "later sources override earlier ones" semantics.**

### Query Configuration Schema

treesitter-ls introduces a unified `queries` field to simplify query file configuration:

```toml
[languages.python]
queries = [
    { path = "/usr/share/python/highlights.scm" },
    { path = "/usr/share/python-locals.scm", kind = "locals" },
    { path = "./custom-injections.scm", kind = "injections" }
]
```

**QueryItem structure:**

| Field  | Type   | Required | Description                                      |
|--------|--------|----------|--------------------------------------------------|
| `path` | string | Yes      | Path to the `.scm` query file                    |
| `kind` | string | No       | Query type: `"highlights"`, `"locals"`, `"injections"` |

**Type inference rules (when `kind` is omitted):**

1. If the filename contains a recognized query type, use it:
   - `*highlights*.scm` → `"highlights"`
   - `*locals*.scm` → `"locals"`
   - `*injections*.scm` → `"injections"`
2. Otherwise, default to `"highlights"`

**Examples of type inference:**

| Path                              | Inferred `kind` |
|-----------------------------------|-----------------|
| `/usr/share/python/highlights.scm` | `highlights`    |
| `./queries/python-highlights.scm`  | `highlights`    |
| `/usr/share/python-locals.scm`     | `locals`        |
| `./my-custom-injections.scm`       | `injections`    |
| `./python.scm`                     | `highlights`    |
| `./custom-queries.scm`             | `highlights`    |

**Relationship with legacy fields:**

The `queries` field coexists with the legacy `highlights`, `locals`, and `injections` fields during a transition period:

- **Legacy format** (still supported):
  ```toml
  [languages.python]
  highlights = ["./highlights.scm"]
  locals = ["./locals.scm"]
  injections = ["./injections.scm"]
  ```

- **New unified format**:
  ```toml
  [languages.python]
  queries = [
      { path = "./highlights.scm" },
      { path = "./locals.scm", kind = "locals" },
      { path = "./injections.scm", kind = "injections" }
  ]
  ```

- **Merge behavior**: When both formats are present, `queries` entries are processed first, then legacy fields append to their respective types

### Configuration Sources (Lowest to Highest Precedence)

1. **Programmed defaults** (lowest precedence)
   - Source: `src/config.rs` (`default_search_paths()`, implicit `autoInstall: true`)
   - Purpose: Sensible out-of-the-box behavior; enables zero-config experience

2. **User configuration file**
   - Location: `$XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml`
   - Falls back to `~/.config/treesitter-ls/treesitter-ls.toml` on most Unix systems
   - Purpose: User-wide defaults (e.g., default `searchPaths`, global `captureMappings` overrides)

3. **Project configuration file**
   - Location: `./treesitter-ls.toml` in workspace root (loaded via `load_toml_settings()`)
   - Future: `--config` CLI option to specify alternative path
   - Purpose: Project-specific settings, version-controlled with the project

4. **Session-specific overrides** (highest precedence)
   - Sources:
     - `initializationOptions` in the LSP `initialize` request (at startup)
     - `workspace/didChangeConfiguration` notification (at runtime)
   - Purpose: Per-session overrides from the editor/client configuration
   - Note: Runtime changes via `didChangeConfiguration` re-trigger the merge process

### Merge Algorithm

The merge function should accept a slice of configs for flexibility:

```rust
fn merge_all(configs: &[Option<TreeSitterSettings>]) -> Option<TreeSitterSettings>
```

Configs are applied in order (earlier = lower precedence, later = higher precedence):

```
final_config = merge_all(&[defaults, user_config, project_config, init_options])
```

This design allows adding new layers (e.g., workspace-level config) without changing the function signature.

**Scalar values and Option types** (`searchPaths`, `autoInstall`):
- Later sources completely replace earlier values (via `primary.or(fallback)`)
- Example: `autoInstall: false` in init_options overrides `autoInstall: true` from project config

**Languages HashMap** (`languages`):
- **Deep merge at language level**: Keys from later sources override same keys from earlier sources
- **Deep merge within each language**: Individual fields (`parser`, `queries`, `bridge`, etc.) are merged
- The `queries` array is **replaced entirely**, not concatenated (same for legacy `highlights`, `locals`, `injections` fields)
- Example:
  ```toml
  # user config
  [languages.python]
  parser = "/usr/lib/python.so"
  queries = [
      { path = "/usr/share/python/highlights.scm" },
      { path = "/usr/share/python-locals.scm", kind = "locals" }
  ]
  bridge = { rust = { enabled = true }, javascript = { enabled = true } }

  # project config
  [languages.python]
  queries = [
      { path = "./queries/python-highlights.scm" }  # replaces user's queries entirely
  ]
  # bridge not specified

  # final (deep merge)
  [languages.python]
  parser = "/usr/lib/python.so"                           # inherited from user
  queries = [{ path = "./queries/python-highlights.scm" }] # replaced by project (user's locals lost!)
  bridge = { rust = { enabled = true }, javascript = { enabled = true } }  # inherited from user
  ```

  **Note**: If the project only wants to override highlights while keeping the user's locals, it must include both:
  ```toml
  # project config (preserving user's locals)
  [languages.python]
  queries = [
      { path = "./queries/python-highlights.scm" },
      { path = "/usr/share/python-locals.scm", kind = "locals" }  # must repeat user's locals
  ]
  ```

**Bridge servers HashMap** (`languageServers`):
- **Deep merge at server level**: Keys (server names) from later sources override same keys from earlier sources
- **Deep merge within each server**: Individual fields (`cmd`, `languages`, `workspaceType`, `initializationOptions`) are merged
- Example:
  ```toml
  # user config
  [languageServers.rust-analyzer]
  cmd = ["rust-analyzer"]
  languages = ["rust"]
  workspaceType = "cargo"

  # project config
  [languageServers.rust-analyzer]
  initializationOptions = { linkedProjects = ["./Cargo.toml"] }

  # final (deep merge)
  [languageServers.rust-analyzer]
  cmd = ["rust-analyzer"]                                        # inherited
  languages = ["rust"]                                           # inherited
  workspaceType = "cargo"                                        # inherited
  initializationOptions = { linkedProjects = ["./Cargo.toml"] }  # added by project
  ```

**Capture mappings** (`captureMappings`):
- **Deep merge**: Individual capture mappings are merged per-language, per-query-type
- Later sources override specific keys while preserving unmentioned keys from earlier sources
- Example:
  ```toml
  # user config
  [captureMappings._.highlights]
  "variable.builtin" = "fallback.variable"
  "function.builtin" = "fallback.function"

  # project config
  [captureMappings._.highlights]
  "variable.builtin" = "project.variable"

  # final (deep merge)
  [captureMappings._.highlights]
  "variable.builtin" = "project.variable"  # overridden
  "function.builtin" = "fallback.function" # inherited
  ```

### File Loading Behavior

1. **Missing files are silently ignored**
   - User config doesn't exist: proceed with empty user config
   - Project config doesn't exist (and `--config` not specified): proceed with empty project config
   - No error, no warning—this enables zero-config startup

2. **Invalid files cause startup failure**
   - Parse errors in any config file should fail fast with a clear error message
   - Users should know immediately if their config is malformed

3. **`--config` option with missing file**
   - If user explicitly specifies `--config /path/to/config.toml` and file doesn't exist: error
   - Explicit paths should be validated; implicit defaults can be missing

### Implementation Notes

**Config loading order:**
```rust
fn load_configuration(cli_config_path: Option<&Path>) -> Option<TreeSitterSettings> {
    let defaults = Some(default_settings());  // from src/config/defaults.rs
    let user_config = load_optional(xdg_config_path());
    let project_config = load_optional_project_config(cli_config_path);
    // init_options applied later in LSP initialize handler

    merge_all(&[defaults, user_config, project_config])
}
```

**XDG Base Directory compliance:**
- Use `$XDG_CONFIG_HOME` if set
- Fall back to `$HOME/.config` otherwise
- Consider using the `dirs` or `directories` crate for cross-platform support

## Consequences

### Positive

- **Layered flexibility**: Users can set sensible defaults globally while projects customize as needed
- **Editor-agnostic defaults**: User config works regardless of which editor/client is used
- **Version control friendly**: Project configs can be committed to repos
- **Zero-config still works**: All layers are optional; empty config results in auto-install behavior
- **Precedence is intuitive**: "Closer to the action" = higher priority (session > project > user)
- **Unified queries format**: Single `queries` field with type inference reduces config verbosity
- **Self-documenting paths**: Filenames like `highlights.scm` convey intent without explicit `kind`

### Negative

- **Complexity increase**: Four config sources to understand and debug
- **Arrays replace, not merge**: `queries` arrays are replaced entirely, not concatenated; overriding one query type requires repeating all
- **No "unset" mechanism**: Cannot explicitly remove a field inherited from earlier layers (would need `null` support)
- **File I/O at startup**: Reading up to two config files adds latency (minimal in practice)
- **Transition period**: Both `queries` and legacy fields (`highlights`, `locals`, `injections`) must be supported during migration

### Neutral

- **TOML format**: Consistent with project config; JSON would work but TOML is more readable for humans
- **XDG compliance**: Standard for Unix tools; Windows path handling needs separate consideration
- **Future extensibility**: Additional layers (e.g., workspace-level) could be added with same merge rules
- **Deprecation warnings**: `log_deprecation_warnings()` will fire for all layers that use deprecated fields

## Implementation Phases

**Overall Progress**: Phases 1-3 completed. Core configuration loading infrastructure is in place. Remaining work: CLI options and end-to-end testing.

### Phase 1: Query Configuration Schema (Completed - Sprint 118, PBI-151)
- [x] Add `QueryItem` struct with `path` (required) and `kind` (optional) fields
- [x] Add `queries: Option<Vec<QueryItem>>` field to `LanguageConfig`
- [x] Implement `QueryKind` enum (`Highlights`, `Locals`, `Injections`) with default `Highlights`
- [x] Implement type inference from filename (e.g., `*highlights*.scm` → `Highlights`)
- [x] Normalize `queries` + legacy fields into unified internal representation
- [x] Emit deprecation warning when legacy `highlights`/`locals`/`injections` fields are used

### Phase 2: Core Merging (Completed - Sprint 119, PBI-150)
- [x] Implement `merge_all()` function for layered config merging
- [x] Deep merge for `languages` HashMap
- [x] Deep merge for `languageServers` HashMap
- [x] Deep merge for `captureMappings`

### Phase 3: User Configuration File (Completed - Sprint 120, PBI-149)
- [x] XDG Base Directory compliance for config path
- [x] Load user config from `$XDG_CONFIG_HOME/treesitter-ls/treesitter-ls.toml`
- [x] Silent ignore for missing user config file

### Phase 4: Project Configuration (Partial - existing `./treesitter-ls.toml`)
- [x] Load project config from `./treesitter-ls.toml`
- [ ] `--config` CLI option for alternative path
- [ ] Error on missing file when explicitly specified

### Phase 5: Testing
- [ ] Unit tests for `QueryItem` parsing and type inference
- [ ] Unit tests for `merge()` function covering all value types
- [ ] Integration tests loading actual files from XDG and project paths
- [ ] E2E Neovim tests verifying init_options override file-based config

## Alternatives Considered

### 1. Shallow merge for `languages` HashMap (current implementation)
- Pro: Simple to implement and understand
- Con: Users must repeat all fields when overriding a single field (e.g., must specify `parser` again just to change `queries`)
- Con: Less intuitive — users expect inheritance
- Decision: **Change to deep merge** for `languages` to match `captureMappings` behavior; arrays within language config (e.g., `queries`) are replaced, not merged

### 2. Prepend arrays instead of replace
- Pro: Allow extending `searchPaths` from earlier layers
- Con: Current `primary.or(fallback)` is simpler and predictable
- Con: Users can manually include default paths if they want extension
- Decision: Keep current replace behavior for simplicity

### 3. Single config file with includes
- Pro: Simpler loading logic
- Con: Requires inventing include syntax; less conventional
- Decision: Rejected; layered files are standard in the ecosystem

### 4. Environment variable overrides
- Pro: Easy CI/CD integration
- Con: Not useful for complex settings like `languages` config
- Decision: Deferred; could be added later for specific scalar settings like `autoInstall`

### 5. Keep separate `highlights`, `locals`, `injections` fields (current implementation)
- Pro: Explicit, no type inference needed
- Pro: No new data structure to learn
- Con: Verbose configuration—three separate arrays to manage
- Con: Adding new query types (e.g., `folds`, `indents`) requires schema changes
- Decision: **Introduce unified `queries` field** with type inference; legacy fields remain for backward compatibility during transition

### 6. Merge queries per-kind instead of replacing entire array
- Pro: Override only highlights while inheriting locals from user config
- Con: Significantly more complex merge logic
- Con: Unintuitive when mixing `queries` field with legacy fields across layers
- Decision: Keep simple array replacement; users can use ADR-0011 wildcard inheritance for shared queries

## Migration Guide

### From legacy fields to `queries`

**Before (legacy):**
```toml
[languages.python]
highlights = ["/path/to/highlights.scm", "/path/to/custom.scm"]
locals = ["/path/to/locals.scm"]
injections = ["/path/to/injections.scm"]
```

**After (unified):**
```toml
[languages.python]
queries = [
    { path = "/path/to/highlights.scm" },
    { path = "/path/to/custom.scm" },           # kind inferred as "highlights"
    { path = "/path/to/locals.scm", kind = "locals" },
    { path = "/path/to/injections.scm", kind = "injections" }
]
```

**Migration steps:**
1. Replace each legacy array with `queries` entries
2. Add explicit `kind` for `locals` and `injections` (highlights is the default)
3. If filenames follow the `*highlights*.scm`, `*locals*.scm` pattern, `kind` can be omitted
4. Test configuration with `treesitter-ls --check-config` (future feature)
5. Remove legacy fields once satisfied

## Related Decisions

- [ADR-0011](0011-wildcard-config-inheritance.md): Wildcard inheritance within a single config layer
