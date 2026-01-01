# ADR-0012: Lua-Based Configuration Filter

## Status

Proposed

## Context

treesitter-ls needs the ability to dynamically modify configuration at startup based on runtime context. Static configuration files cannot address several important use cases:

1. **Root project directory detection**: Identifying the actual project root (e.g., monorepo root vs. subdirectory) requires inspecting the filesystem at runtime
2. **Language server selection**: Choosing between conflicting servers (e.g., `denols` vs. `tsserver`/`tsgo` for TypeScript) based on project markers (presence of `deno.json`, `package.json`, etc.)
3. **Conditional configuration**: Enabling/disabling features based on environment variables, workspace structure, or other runtime factors
4. **Path resolution**: Computing absolute paths from relative ones based on actual working directory

Currently, users must either:
- Maintain separate configuration files for different project types
- Use editor-specific mechanisms (e.g., Neovim's `exrc`) which don't integrate with LSP init_options
- Accept suboptimal default configurations

## Decision

**Implement a Lua-based configuration filter that processes configuration at startup.**

### Filter Mechanism

The Lua filter receives the merged configuration (after ADR-0010 cross-layer merging) and can modify it before the LSP server uses it.

```lua
-- Example: ~/.config/treesitter-ls/filter.lua
return function(config, context)
  -- context provides runtime information
  -- context.root_path: workspace root from LSP initialization
  -- context.workspaceFolders: all workspace folders
  -- context.clientInfo: editor name/version

  -- Detect Deno projects
  local is_deno = file_exists(context.root_path .. "/deno.json")
                or file_exists(context.root_path .. "/deno.jsonc")

  if is_deno then
    -- Disable TypeScript bridge for Deno projects
    if config.languages and config.languages.typescript then
      config.languages.typescript.bridge = { enabled = false }
    end
    -- Or configure denols instead of tsserver
    if config.bridge and config.bridge.servers then
      config.bridge.servers.typescript = { command = { "deno", "lsp" } }
    end
  end

  return config
end
```

### Configuration Loading Order

```
1. Load user config (~/.config/treesitter-ls/config.toml)
2. Load project config (.treesitter-ls.toml)
3. Receive init_options from editor
4. Merge all layers (ADR-0010)
5. Apply Lua filter (this ADR) ← NEW STEP
6. Resolve wildcards (ADR-0011)
7. Use final configuration
```

### Lua Runtime

We will embed **mlua** (LuaJIT or Lua 5.4 bindings for Rust) to provide:
- Sandboxed execution (no `io`, `os.execute`, limited `os`)
- Read-only filesystem access helpers (`file_exists`, `read_file`)
- JSON/TOML parsing utilities
- Configuration table manipulation

### Filter Configuration

Filters are specified as an array using `[[filters]]`. One element = single filter, multiple elements = chain executed in order:

```toml
# config.toml

# Single filter (path-based)
[[filters]]
path = "~/.config/treesitter-ls/filters/deno-detect.lua"

# Or inline Lua code for simple filters
[[filters]]
inline = """
return function(config, context)
  if file_exists(context.root_path .. "/deno.json") then
    config.languages.typescript.bridge.enabled = false
  end
  return config
end
"""
```

```toml
# Multiple filters chained (executed in order)
# Each filter receives the output of the previous one

[[filters]]
path = "~/.config/treesitter-ls/filters/detect-project-type.lua"

[[filters]]
path = "~/.config/treesitter-ls/filters/configure-bridge.lua"

[[filters]]
inline = """
return function(config, context)
  -- Final adjustments
  return config
end
"""
```

### Filter Specification Options

| Option | Type | Description |
|--------|------|-------------|
| `[[filters]]` | array | Array of filters (single or chained) |
| `filters[].path` | string | Path to a Lua filter file |
| `filters[].inline` | string | Inline Lua code |
| `filters[].enabled` | bool | Enable/disable this filter (default: true) |

Each `[[filters]]` entry must have exactly one of `path` or `inline`.

### Filter Discovery (Auto-detection)

When no `[[filters]]` is configured, filters are auto-discovered in order:

1. `.treesitter-ls/filter.lua` (project-specific)
2. `~/.config/treesitter-ls/filter.lua` (user default)

The first existing file is used as a single-element filter array.

### Filter in init_options

Editors can pass filter configuration via LSP initialization:

```lua
-- Neovim example
vim.lsp.start({
  name = "treesitter_ls",
  cmd = { "treesitter-ls" },
  init_options = {
    filters = {
      { path = vim.fn.stdpath("config") .. "/treesitter-ls/filter.lua" },
      -- Or inline for simple cases
      -- { inline = "return function(c, ctx) return c end" }
    }
  }
})
```

### Filter Chain Execution

When multiple `[[filters]]` entries exist, they execute in sequence:

```
config₀ (merged from all layers)
    │
    ▼
┌───────────────┐
│  filters[0]   │
└───────────────┘
    │
    ▼
config₁
    │
    ▼
┌───────────────┐
│  filters[1]   │
└───────────────┘
    │
    ▼
config₂
    │
    ▼
   ...
    │
    ▼
configₙ (final filtered config)
```

Each filter in the chain:
- Receives the config output from the previous filter
- Receives the same context object (read-only)
- Must return a config table
- Filters with `enabled = false` are skipped
- If any filter returns `nil` or errors, the chain stops and uses the last valid config

### Context Object

The filter receives a read-only context object:

```lua
context = {
  root_path = "/path/to/project",       -- Workspace root
  workspaceFolders = { ... },           -- All workspace folders
  clientInfo = {                        -- Editor information
    name = "Neovim",
    version = "0.10.0"
  },
  platform = "macos",                   -- "linux", "macos", "windows"
  env = {                               -- Selected environment variables
    HOME = "/Users/...",
    XDG_CONFIG_HOME = "...",
  }
}
```

### Helper Functions

Built-in helpers available in the Lua environment:

```lua
-- Filesystem (read-only, sandboxed to workspace + config dirs)
file_exists(path) -> boolean
read_file(path) -> string | nil
glob(pattern) -> table<string>

-- Path utilities
join_path(...) -> string
dirname(path) -> string
basename(path) -> string

-- Data formats
parse_json(str) -> table
parse_toml(str) -> table

-- Logging
log.info(msg), log.warn(msg), log.debug(msg)
```

## Consequences

### Positive

- **Dynamic configuration**: Users can adapt settings based on actual project structure
- **Language server coexistence**: Clean solution for TypeScript/Deno and similar conflicts
- **Powerful customization**: Lua provides full programming capabilities within sandbox
- **Familiar to Neovim users**: Lua is already the standard configuration language for Neovim
- **Portable**: Configuration logic travels with the project (project-level filter.lua)

### Negative

- **Added complexity**: Another layer in configuration resolution
- **Dependency**: Requires embedding Lua runtime (~2-3MB binary size increase)
- **Learning curve**: Users must learn Lua for advanced customization
- **Debugging difficulty**: Filter errors may be harder to diagnose than static config errors
- **Startup overhead**: Filter execution adds latency to LSP initialization

### Neutral

- **Optional feature**: Static configuration continues to work without any filter
- **Sandboxed execution**: Security-conscious users may still be wary of running code
- **No LSP standard**: This is a treesitter-ls specific extension

## Alternatives Considered

### Alternative 1: JSON-based Conditionals

```json
{
  "languages": {
    "typescript": {
      "$if": { "fileExists": "deno.json" },
      "$then": { "bridge": { "enabled": false } },
      "$else": { "bridge": { "enabled": true } }
    }
  }
}
```

**Rejected**: Limited expressiveness, awkward syntax, creates a custom DSL.

### Alternative 2: External Script

Run an external script that outputs JSON configuration.

**Rejected**: Security concerns, platform-specific scripts, no standard interface.

### Alternative 3: WASM Filters

Use WASM instead of Lua for the filter.

**Rejected for now**: Higher complexity, less familiar to users, harder to debug. Could be reconsidered if WASM is adopted for plugins (ADR-0013).

## Implementation Phases

### Phase 1: Basic Filter Support
- [ ] Add mlua dependency
- [ ] Implement filter loading from standard locations
- [ ] Provide basic context object (root_path, clientInfo)
- [ ] Add filter execution to configuration loading pipeline
- [ ] Error handling and logging for filter failures

### Phase 2: Helper Functions
- [ ] Implement sandboxed filesystem helpers
- [ ] Add path utility functions
- [ ] Implement JSON/TOML parsing helpers
- [ ] Add logging functions

### Phase 3: Documentation and Examples
- [ ] Document filter API
- [ ] Provide example filters for common use cases (Deno detection, monorepo handling)
- [ ] Add filter debugging guide

## Related Decisions

- [ADR-0010](0010-configuration-merging-strategy.md): Cross-layer configuration merging (filter runs after merge)
- [ADR-0011](0011-wildcard-config-inheritance.md): Wildcard inheritance (filter runs before wildcard resolution)
- [ADR-0013](0013-wasm-plugin-codeaction.md): WASM plugins for code actions (related extensibility feature)
