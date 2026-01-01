# ADR-0013: WASM Plugin for Code Actions

## Status

Proposed

## Context

treesitter-ls provides code actions through static, built-in implementations (e.g., parameter reordering in `src/analysis/refactor.rs`). However, users need custom code actions for:

1. **Language-specific transformations**: Converting between syntax patterns (e.g., `if-else` to ternary, callback to async/await)
2. **Injected language handling**: Code actions within embedded languages (SQL in strings, HTML in templates, markdown code blocks)
3. **Project-specific refactorings**: Custom transformations aligned with team conventions
4. **Ecosystem integration**: Actions that understand framework-specific patterns (React hooks, Vue composition API)

Currently, adding new code actions requires:
- Modifying the Rust source code
- Recompiling the binary
- Contributing upstream and waiting for release

This creates a high barrier to customization and slows iteration on experimental features.

## Decision

**Implement a WASM-based plugin system for extending code action functionality.**

### Plugin Interface

WASM plugins implement a standardized interface for providing code actions:

```wit
// Plugin interface (wit definition)
interface codeaction-plugin {
  // Called to check if plugin provides actions for this context
  record action-context {
    uri: string,
    language: string,
    range: range,
    tree-sitter-node-kind: string,
    parent-language: option<string>,  // For injected languages
    diagnostics: list<diagnostic>,
  }

  record code-action {
    title: string,
    kind: string,  // "refactor", "quickfix", "source", etc.
    edit: workspace-edit,
    is-preferred: bool,
  }

  // Main plugin entry points
  get-actions: func(ctx: action-context, source: string) -> list<code-action>
  apply-action: func(action-id: string, source: string) -> workspace-edit
}
```

### Plugin Discovery

Plugins are loaded from configured paths:

```toml
# config.toml
[plugins]
searchPaths = [
  "~/.config/treesitter-ls/plugins",
  ".treesitter-ls/plugins"
]

# Or explicit plugin registration
[[plugins.codeActions]]
name = "react-refactorings"
path = "~/.config/treesitter-ls/plugins/react-refactorings.wasm"
languages = ["javascript", "typescript", "javascriptreact", "typescriptreact"]

[[plugins.codeActions]]
name = "sql-formatter"
path = "~/.config/treesitter-ls/plugins/sql-formatter.wasm"
injectedLanguages = ["sql"]  # Triggers for SQL injected into other languages
```

### Tree-sitter Integration

Plugins receive Tree-sitter context to make intelligent decisions:

```wit
record tree-context {
  // Current node information
  node-kind: string,
  node-text: string,
  node-range: range,

  // Parent chain for context
  ancestors: list<ancestor-info>,

  // Sibling information
  prev-sibling: option<sibling-info>,
  next-sibling: option<sibling-info>,

  // For injected languages
  injection-info: option<injection-info>,
}

record injection-info {
  host-language: string,
  host-node-kind: string,  // e.g., "string" for SQL in JS template literal
  injection-range: range,  // Range within host document
}
```

### WASM Runtime

We will use **wasmtime** as the WASM runtime:
- **Memory isolation**: Each plugin runs in its own sandbox
- **Resource limits**: CPU and memory limits prevent runaway plugins
- **Capability-based security**: Plugins only access what they're granted
- **WASI support**: Standard I/O for debugging (but sandboxed)

### Plugin Execution Flow

```
1. LSP receives textDocument/codeAction request
2. treesitter-ls identifies relevant language(s) at cursor position
3. Built-in code actions are collected
4. For each registered plugin matching the language:
   a. Construct action-context from current state
   b. Call plugin's get-actions() with source text
   c. Collect returned code actions
5. Merge and deduplicate all actions
6. Return combined list to client
```

### Example Plugin (Conceptual)

```rust
// sql-extract.wasm - Extract SQL from string to constant
// Written in Rust, compiled to WASM

#[export]
fn get_actions(ctx: ActionContext, source: &str) -> Vec<CodeAction> {
    // Only trigger for SQL injected in JS/TS template literals
    if ctx.parent_language != Some("javascript")
       && ctx.parent_language != Some("typescript") {
        return vec![];
    }

    // Check if it's a string containing SQL
    if !looks_like_sql(ctx.tree_sitter_node_kind, source) {
        return vec![];
    }

    vec![CodeAction {
        title: "Extract SQL to constant".into(),
        kind: "refactor.extract".into(),
        // ... edit details
    }]
}
```

### Language Support for Plugin Development

Plugins can be written in any language that compiles to WASM:
- **Rust**: First-class support, used for examples and official plugins
- **Go**: Via TinyGo
- **AssemblyScript**: TypeScript-like syntax
- **C/C++**: Via Emscripten
- **Zig**: Native WASM target

### Plugin SDK

We will provide an SDK crate for Rust plugin development:

```rust
// treesitter-ls-plugin-sdk
use treesitter_ls_sdk::prelude::*;

#[treesitter_ls_plugin]
fn my_plugin() -> impl CodeActionPlugin {
    MyPlugin::new()
}

struct MyPlugin;

impl CodeActionPlugin for MyPlugin {
    fn get_actions(&self, ctx: &ActionContext, source: &str) -> Vec<CodeAction> {
        // Plugin implementation
    }
}
```

## Consequences

### Positive

- **Extensibility**: Users can add code actions without modifying core
- **Language ecosystem**: Plugins can target specific languages/frameworks
- **Injected language support**: Special handling for embedded SQL, HTML, etc.
- **Security**: WASM sandboxing provides strong isolation
- **Portability**: WASM plugins work across all platforms
- **Community ecosystem**: Third-party plugins can emerge

### Negative

- **Complexity**: WASM runtime adds significant implementation complexity
- **Binary size**: wasmtime dependency increases binary size (~10-20MB)
- **Performance overhead**: WASM execution is slower than native Rust
- **Development friction**: Plugin authors need WASM toolchain
- **Debugging challenges**: Debugging WASM plugins is harder than native code
- **API stability**: Plugin interface must be versioned and maintained

### Neutral

- **Optional feature**: Core functionality doesn't require plugins
- **Compilation target**: Users must compile their plugins to WASM
- **Interface standardization**: Plugin API becomes a compatibility contract
- **Multiple implementations**: Same plugin interface could support Lua (ADR-0012) in future

## Alternatives Considered

### Alternative 1: Lua Plugins

Use Lua (same as ADR-0012) for code action plugins.

**Pros**: Simpler, no compilation step, familiar to Neovim users
**Cons**: Slower execution, weaker sandboxing, less type safety

**Verdict**: Keep as potential future option. Lua for configuration (ADR-0012), WASM for compute-heavy plugins.

### Alternative 2: Native Dynamic Libraries

Load native .so/.dylib plugins.

**Rejected**: Security nightmare (full system access), platform-specific compilation, ABI compatibility issues.

### Alternative 3: Language Server Composition

Delegate to external language servers via bridge.

**Pros**: Leverage existing LSP implementations
**Cons**: Doesn't help for injected languages, high latency, complex coordination

**Verdict**: Already implemented in ADR-0006/0007/0008. WASM plugins complement this for custom transformations.

### Alternative 4: External Process Plugins

Spawn external processes for plugins.

**Rejected**: Security concerns, platform dependencies, high overhead.

## Implementation Phases

### Phase 1: Core WASM Runtime
- [ ] Add wasmtime dependency
- [ ] Define plugin interface (WIT definition)
- [ ] Implement plugin loading and instantiation
- [ ] Add basic sandboxing and resource limits

### Phase 2: Tree-sitter Integration
- [ ] Implement tree-context serialization for plugins
- [ ] Handle injected language context
- [ ] Integrate with existing code action flow

### Phase 3: Plugin SDK
- [ ] Create treesitter-ls-plugin-sdk crate
- [ ] Add procedural macros for plugin definition
- [ ] Provide helper utilities for common patterns

### Phase 4: Example Plugins
- [ ] SQL extraction plugin (for injected SQL)
- [ ] Import organization plugin
- [ ] Documentation examples

### Phase 5: Plugin Distribution
- [ ] Define plugin manifest format
- [ ] Consider plugin registry/repository
- [ ] Version compatibility checking

## Security Considerations

- **Capability-based access**: Plugins can only access explicitly granted resources
- **No filesystem access by default**: Source text is passed in, edits are passed out
- **Memory limits**: Prevent memory exhaustion attacks
- **CPU limits**: Timeout long-running plugins
- **Audit trail**: Log plugin loads and executions

## Related Decisions

- [ADR-0012](0012-lua-configuration-filter.md): Lua configuration filter (complementary extensibility)
- [ADR-0006](0006-language-server-bridge.md): Language server bridge (alternative extensibility via LSP delegation)
