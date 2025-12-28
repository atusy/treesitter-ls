# ADR-0007: Virtual Document Model for Injection Regions

## Status

Proposed

## Context

When redirecting LSP requests for injection regions (see [ADR-0006](0006-language-server-redirection.md)), we need to represent injection content to language servers. A host document may contain multiple injection regions of the same language (e.g., multiple Rust code blocks in Markdown).

The key question: **Should multiple injections of the same language be merged into a single virtual document, or kept separate?**

This decision affects:
- Symbol conflict handling (multiple `fn main()` definitions)
- Cross-block reference resolution
- Offset translation complexity
- Compatibility with different use cases (documentation vs. literate programming)

## Decision

**Use separate virtual documents by default, with configurable merged mode for literate programming.**

### Separate Mode (Default)

Each injection region becomes its own virtual document:

```
Host: file:///docs/tutorial.md
  │
  ├─▶ treesitter-ls:///docs/tutorial.md#injection-0.rs  (lines 5-10)
  ├─▶ treesitter-ls:///docs/tutorial.md#injection-1.rs  (lines 20-25)
  └─▶ treesitter-ls:///docs/tutorial.md#injection-2.rs  (lines 40-50)
```

#### Why Separate by Default

| Consideration | Separate | Merged |
|---------------|----------|--------|
| **Conflicts** | ✅ No conflicts—each block can have `fn main()` | ❌ Duplicate symbols cause errors |
| **Documentation patterns** | ✅ Matches reality—examples are standalone | ❌ Assumes literate programming |
| **Offset mapping** | ✅ Simple—contiguous region → contiguous virtual | ❌ Complex—non-contiguous gaps |
| **Cross-block refs** | ❌ Cannot resolve `foo()` from another block | ✅ Would work if merged |

Real-world documentation code blocks are typically **independent examples** that would conflict if merged.

### Merged Mode (Configurable)

For literate programming workflows, merged mode concatenates all injections of the same language.

#### Fine-Grained Control: (Host, Injection) Pairs

The appropriate mode depends on **both** the host document and the injection language:

| Host | Injection | Use Case | Mode |
|------|-----------|----------|------|
| Markdown | Python | Documentation examples | separate |
| Markdown | Rust | Documentation examples | separate |
| Org-mode | Python | Literate programming (`:tangle`) | merged |
| `.lhs` | Haskell | Literate Haskell | merged |

The same injection language (e.g., Python) may need different modes in different host contexts. Configuration should allow specifying mode per `(host, injection)` pair:

```json
{
  "treesitter-ls": {
    "injectionMode": {
      "_": { "_": "separate" },
      "markdown": { "python": "separate", "rust": "separate" },
      "org": { "python": "merged" }
    }
  }
}
```

The `_` wildcard matches any host or injection language, enabling layered defaults:

| Pattern | Meaning |
|---------|---------|
| `"_": { "_": "separate" }` | Global default for all pairs |
| `"_": { "haskell": "merged" }` | Default for Haskell in any host |
| `"org": { "_": "merged" }` | Default for any injection in org-mode |
| `"org": { "python": "merged" }` | Specific (host, injection) pair |

Precedence: **specific pair > host default > injection default > global default**

Note: `injectionMode` is configured per **host/injection pair**, not per server. The same server handles both modes—only the virtual document structure differs.

| Mode | Use Case | Behavior |
|------|----------|----------|
| `separate` (default) | Documentation, tutorials | Each injection → independent virtual document |
| `merged` | Literate programming (`.lhs`, org-mode tangling) | All injections of same language → single virtual document |

Merged mode considerations for future implementation:
- Insert placeholder lines (comments/whitespace) to preserve line numbers for diagnostics
- Handle conflicting symbols gracefully (report as diagnostics from treesitter-ls, not language server)
- Consider block ordering annotations for explicit concatenation order

#### Feature-Specific Mode Overrides

Even when merged mode is requested, some features may internally use separated virtual documents for performance or error isolation (e.g., `semanticTokens` benefits from smaller documents and tolerates syntax errors in individual blocks). Features requiring cross-block context (e.g., `diagnostics`, `goToDefinition`) should respect the user's mode selection.

### Virtual Document Identity

Stable identity across edits is desirable—it allows language servers to maintain state (diagnostics, symbol caches) for each injection. However, Tree-sitter node IDs may persist across incremental parses only if reused, and reuse is not guaranteed. Simple URI schemes have limitations:

| Scheme | Problem |
|--------|---------|
| Index-based (`#injection-2`) | Shifts when blocks inserted/deleted above |
| Byte-offset (`#@500-650`) | Shifts when content changes above |
| Content hash | Changes on any edit to the block |

Possible approaches:
- **User-provided labels**: Use `#| label: my-block` (Quarto/Rmd) as stable ID when present
- **Heuristic matching**: Match "similar" injections across parses (same language, overlapping range) and assign persistent IDs
- **Accept instability**: URI changes trigger close/reopen; simple but loses server-side state

The choice affects implementation complexity vs. user experience. This ADR does not prescribe a specific scheme.

### Virtual Document Materialization

Virtual documents may be **logical** (in-memory only) or **materialized** (written to disk), depending on language server requirements:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Virtual Document Model                        │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Injection Content ──┬──▶ Logical Virtual Document              │
│                      │    (in-memory, didOpen only)             │
│                      │    For: pyright, typescript-ls           │
│                      │                                          │
│                      └──▶ Materialized Virtual Document         │
│                           (temp file on disk + project files)   │
│                           For: rust-analyzer, gopls             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

**Why materialization is sometimes required**: Some language servers (notably rust-analyzer) index from the filesystem rather than relying solely on `didOpen` content. They return `null` for queries when files don't exist on disk or lack project context.

For materialized documents:
- Create temporary project structure (see [ADR-0006](0006-language-server-redirection.md#server-specific-workspace-provisioning))
- Write injection content to real file
- Use real file URI in LSP communication
- Clean up on document close or server shutdown

### Virtual Document Lifecycle

1. **Creation**: When injection region is first parsed, create virtual document
2. **Materialization** (if required): Write to temp file with project structure
3. **URI**: Unique identifier—either virtual scheme or real temp file path
4. **Registration**: Send `textDocument/didOpen` to language server
5. **Wait for indexing**: For materialized documents, wait for `publishDiagnostics`
6. **Sync**: On host document change, send `textDocument/didChange` (or rewrite temp file)
7. **Cleanup**: Send `textDocument/didClose` and delete temp files when injection is removed or host closes

### Server Process Sharing

One language server process handles **all virtual documents** for that language, minimizing resource usage while maintaining isolation between code blocks.

## Consequences

### Positive

- **No symbol conflicts**: Independent blocks can have duplicate symbols
- **Simple offset translation**: One-to-one mapping between injection and virtual document
- **Matches common patterns**: Documentation examples work out of the box
- **Future flexibility**: Merged mode can be added without breaking existing behavior
- **Server compatibility**: Materialization handles servers requiring real files

### Negative

- **No cross-block navigation**: Cannot go-to-definition across blocks in separate mode
- **Many virtual URIs**: Large documents with many injections create many virtual documents
- **Disk overhead**: Materialized documents use temp disk space
- **Merged mode complexity**: Future implementation requires line number preservation logic

### Neutral

- **Configuration required for literate programming**: Users must opt-in to merged mode
- **Different behavior per language**: Some languages may use separate, others merged

## Implementation Phases

### Phase 1: Separate Mode Only (Done)
- Virtual document creation and lifecycle
- Materialization for rust-analyzer
- Server process sharing

### Phase 2: Merged Mode (Future)
- `injectionMode: "merged"` configuration option
- Concatenation of same-language injections into single virtual document
- Placeholder line insertion for line number preservation
- Conflict detection and reporting

## Related Decisions

- [ADR-0006](0006-language-server-redirection.md): Core LSP redirection architecture
- [ADR-0008](0008-redirection-request-strategies.md): Per-method redirection strategies
