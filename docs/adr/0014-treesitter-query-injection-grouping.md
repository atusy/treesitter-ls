# ADR-0014: Tree-sitter Query-Based Injection Grouping

## Status

Proposed

## Context

ADR-0007 defines separate vs. merged modes for injection regions at the **(host, injection language) pair** level. However, this granularity is insufficient for some real-world use cases:

### The R Markdown Problem

R Markdown (`.Rmd`) files contain two types of R code:

1. **R chunks** (executable): `` ```{r} `` — these should merge into a single virtual document for cross-chunk references
2. **R code blocks** (display-only): `` ```r `` — these are independent examples that should stay separate

Both produce injections with the same language (`r`), but require different merge behavior. The `(host, injection)` pair model cannot distinguish them.

### Similar Cases

| Host | Injection Type | Syntax | Desired Behavior |
|------|---------------|--------|------------------|
| Rmd | Executable chunk | `` ```{r} `` | Merge |
| Rmd | Display code block | `` ```r `` | Separate |
| Quarto | Labeled chunks | `` ```{python} #| label: setup `` | Merge by label |
| Org-mode | Tangled blocks | `:tangle yes` | Merge |
| Org-mode | Example blocks | `#+BEGIN_EXAMPLE` | Separate |
| Jupyter-like | Cell type | Code vs. scratch | Merge code, separate scratch |

### Why (Host, Injection) Pairs Are Insufficient

The current model in ADR-0007 treats all injections of the same language identically:

```
markdown + r → separate (default)
```

But we need:

```
markdown + r[chunk]      → merge
markdown + r[code_block] → separate
```

The **AST node type** or **metadata** determines the correct behavior, not just the language pair.

## Decision

**Use Tree-sitter query metadata to control injection grouping behavior at the capture level.**

### Leveraging `#set!` Directives

Tree-sitter queries support the `#set!` directive to attach metadata to captures. We extend this mechanism with custom properties for injection grouping:

```scheme
; injections.scm for markdown (Rmd variant)

; Executable R chunks - merge into single virtual document
((fenced_code_block
  (info_string) @_lang
  (code_fence_content) @injection.content)
 (#match? @_lang "^\\{r")
 (#set! injection.language "r")
 (#set! injection.merge true))

; Display R code blocks - keep separate (default)
((fenced_code_block
  (info_string) @_lang
  (code_fence_content) @injection.content)
 (#eq? @_lang "r")
 (#set! injection.language "r"))
; injection.merge defaults to false
```

### New `#set!` Properties

| Property | Type | Default | Description |
|----------|------|---------|-------------|
| `injection.merge` | boolean | `false` | Whether this injection should merge with others |
| `injection.group` | string | `"default"` | Group identifier for merging (same group → same virtual doc) |

### Grouping Semantics

Injections merge into the same virtual document when **all** conditions are true:

1. Same `injection.language`
2. Same `injection.group` (or both using default)
3. `injection.merge` is `true` for all injections in the group

```
Virtual Document Identity = (host_uri, injection_language, group_id)
```

### Example: Multiple Groups

```scheme
; Quarto with labeled chunks
((fenced_code_block
  (info_string) @_lang
  (directive) @_directive
  (code_fence_content) @injection.content)
 (#match? @_lang "^\\{python")
 (#match? @_directive "label:\\s*(\\w+)")
 (#set! injection.language "python")
 (#set! injection.merge true)
 (#set! injection.group @_directive))  ; group by label value
```

This allows:
- `#| label: data-prep` chunks → merge into one virtual document
- `#| label: visualization` chunks → merge into another
- Unlabeled chunks → merge into `"default"` group

### Fallback to Configuration

When query metadata is absent, fall back to ADR-0007's `(host, injection)` pair configuration:

```
Effective merge mode:
1. Check injection capture for `injection.merge` property
2. If absent, check config: languages.<host>.bridge.<injection>.mode
3. If absent, check wildcard: languages.<host>.bridge._.mode
4. If absent, default to "separate"
```

### Configuration Override

Users can override query-derived behavior via configuration:

```toml
[languages.markdown.bridge.r]
# Force all R injections to merge, regardless of query metadata
mode = "merged"
forceMode = true  # ignores query-level injection.merge
```

| Field | Description |
|-------|-------------|
| `mode` | Default mode when query doesn't specify |
| `forceMode` | When `true`, ignore query metadata and use `mode` |

### Virtual Document URI Scheme

Extend the URI scheme from ADR-0007 to include group:

```
Separate mode:  treesitter-ls:///path/file.md#injection-0.r
Merged mode:    treesitter-ls:///path/file.md#group-default.r
Named group:    treesitter-ls:///path/file.md#group-data_prep.py
```

### Interaction with `injection.combined`

Tree-sitter's built-in `injection.combined` property (used in syntax highlighting) has similar semantics but different purpose:

| Property | Purpose | Scope |
|----------|---------|-------|
| `injection.combined` | Syntax highlighting efficiency | Tree-sitter internal |
| `injection.merge` | Virtual document LSP bridging | treesitter-ls |

We keep these separate because:
- Combined highlighting doesn't imply wanting LSP cross-references
- Merge for LSP has materialization/lifecycle implications
- Users may want different behavior for highlighting vs. LSP

However, `injection.combined` can serve as a hint:

```toml
[languages.markdown.bridge._]
# Treat injection.combined as injection.merge when injection.merge is absent
inheritCombined = true  # default: false
```

## Consequences

### Positive

- **Fine-grained control**: Merge behavior per injection instance, not just language
- **Query-driven**: Leverages existing Tree-sitter query infrastructure
- **Composable**: Multiple groups allow partial merging
- **Backwards compatible**: Absent metadata falls back to ADR-0007 behavior
- **Declarative**: Behavior defined alongside injection patterns

### Negative

- **Query complexity**: Custom queries required for advanced use cases
- **Multiple query files**: May need host-specific injection queries (e.g., `injections-rmd.scm`)
- **Learning curve**: Users must understand `#set!` directives
- **Two control planes**: Query metadata and config can conflict (resolved by `forceMode`)

### Neutral

- **Property naming**: `injection.merge` parallels `injection.combined` but differs in purpose
- **Group naming**: Users choose group identifiers; no standardization required
- **Query distribution**: Custom queries could be shared via community repositories

## Implementation Phases

### Phase 1: Basic Merge Property
- [ ] Parse `injection.merge` property from query captures
- [ ] Implement merge grouping by `(language, "default")` when `merge=true`
- [ ] Update virtual document URI scheme for merged documents
- [ ] Update ADR-0007 to reference this ADR

### Phase 2: Named Groups
- [ ] Parse `injection.group` property from query captures
- [ ] Implement grouping by `(language, group_id)`
- [ ] Support capture references in group (e.g., `@_directive`)

### Phase 3: Configuration Integration
- [ ] Implement `forceMode` to override query metadata
- [ ] Implement `inheritCombined` option
- [ ] Document query customization patterns

### Phase 4: Query Distribution
- [ ] Ship default `injections.scm` for common literate programming formats
- [ ] Document how users can provide custom queries

## Related Decisions

- [ADR-0007](0007-language-server-bridge-virtual-document-model.md): Virtual document model (base for this extension)
- [ADR-0010](0010-configuration-merging-strategy.md): Configuration merging strategy
- [ADR-0011](0011-wildcard-config-inheritance.md): Wildcard config inheritance

## References

- [Tree-sitter Predicates and Directives](https://tree-sitter.github.io/tree-sitter/using-parsers/queries/3-predicates-and-directives.html)
- [Tree-sitter Syntax Highlighting](https://tree-sitter.github.io/tree-sitter/3-syntax-highlighting.html) — `injection.combined` documentation
