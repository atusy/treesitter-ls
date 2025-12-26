# ADR-0008: Request-Specific Redirection Strategies

## Status

Proposed

## Context

When redirecting LSP requests for injection regions (see [ADR-0006](0006-language-server-redirection.md)), different LSP methods have different characteristics:

| Method | Latency Sensitivity | treesitter-ls Capability | Language Server Value |
|--------|---------------------|--------------------------|----------------------|
| Semantic Tokens | High (visual feedback) | Good (Tree-sitter highlights) | Better (type-aware) |
| Go-to-Definition | Medium | Local only (locals.scm) | Cross-file resolution |
| Completion | High (typing flow) | None | Full |
| Hover | Low | None | Full |
| Diagnostics | Low (background) | None | Full |

A single redirection strategy doesn't fit all methods. We need per-method strategies that balance latency, correctness, and user experience.

## Decision

**Implement different redirection strategies based on LSP method characteristics.**

### Strategy 1: Parallel Fetch with Progressive Refinement

**Applies to**: `textDocument/semanticTokens/full`, `textDocument/semanticTokens/range`

```
                    ┌─────────────────────────────┐
 Request ──────────▶│      treesitter-ls          │
                    │  ┌─────────────────────┐    │
                    │  │ Tree-sitter tokens  │────│───▶ Immediate response
                    │  │ (local, fast)       │    │     (use if redirect slow)
                    │  └─────────────────────┘    │
                    │           ▼                 │
                    │  ┌─────────────────────┐    │
                    │  │ Redirect to server  │────│───▶ rust-analyzer
                    │  │ (async)             │    │
                    │  └─────────────────────┘    │
                    │           │                 │
                    │           ▼                 │
                    │  ┌─────────────────────┐    │
                    │  │ Merge results       │────│───▶ Final response
                    │  │ (prefer redirected) │    │     (replaces initial)
                    │  └─────────────────────┘    │
                    └─────────────────────────────┘
```

**Behavior**:
1. Fetch Tree-sitter tokens and redirected tokens **in parallel**
2. If redirected response arrives first → use it directly
3. If Tree-sitter response arrives first → return it immediately as provisional response
4. When redirected response arrives → send updated tokens (via `textDocument/semanticTokens/full` refresh mechanism)

**Rationale**: Users see instant syntax highlighting from Tree-sitter while richer type-aware tokens arrive asynchronously. This provides the best perceived performance.

### Strategy 2: Full Delegation

**Applies to**: `textDocument/definition`, `textDocument/references`, `textDocument/completion`, `textDocument/hover`, `textDocument/signatureHelp`

```
Request (cursor in injection) ──▶ Forward to language server
                                  (treesitter-ls result ignored)
```

**Behavior**:
- Redirect entirely to the language server
- treesitter-ls does not attempt to provide a local result
- If no language server configured → fall back to treesitter-ls capability (if any)

**Rationale**: These features require deep language understanding that Tree-sitter cannot provide. Local fallbacks would be misleading or incomplete.

**Fallback behavior**:

| Method | treesitter-ls Fallback |
|--------|----------------------|
| Go-to-Definition | locals.scm (local scope only) |
| Find References | locals.scm (local scope only) |
| Completion | None |
| Hover | None |
| Signature Help | None |

### Strategy 3: Background Collection

**Applies to**: `textDocument/publishDiagnostics`

```
                    ┌─────────────────────────────┐
 (No request)       │      treesitter-ls          │
                    │                             │
 Document Change ──▶│  Notify language servers    │
                    │           │                 │
                    │           ▼                 │
                    │  ┌─────────────────────┐    │
                    │  │ Collect diagnostics │◀───│──── rust-analyzer
                    │  │ from all servers    │    │
                    │  └─────────────────────┘    │
                    │           │                 │
                    │           ▼                 │
                    │  ┌─────────────────────┐    │
                    │  │ Translate offsets   │────│───▶ publishDiagnostics
                    │  │ Merge & dedupe      │    │     to editor
                    │  └─────────────────────┘    │
                    └─────────────────────────────┘
```

**Behavior**:
- Language servers push diagnostics asynchronously
- treesitter-ls collects diagnostics from all configured servers
- Translate positions from virtual document coordinates to host document coordinates
- Merge and deduplicate diagnostics from multiple servers
- Forward combined diagnostics to the editor

**Rationale**: Diagnostics are push-based, not request-based. treesitter-ls acts as an aggregator.

### Multi-Server Merging Rules

When multiple servers are configured for a language:

| Method | Merging Strategy |
|--------|------------------|
| Semantic Tokens | Later server wins for overlapping ranges |
| Go-to-Definition | Return first non-empty result (query in order) |
| Find References | Concatenate all results, dedupe by location |
| Completion | Merge completion lists from all servers |
| Hover | Concatenate hover content with separator |
| Diagnostics | Merge all, dedupe by range + message |

## Consequences

### Positive

- **Optimized UX per feature**: Each method gets the strategy that best fits its characteristics
- **Fast visual feedback**: Semantic tokens appear instantly via parallel fetch
- **Accurate navigation**: Go-to-definition uses authoritative language server
- **Comprehensive diagnostics**: Aggregated from multiple sources

### Negative

- **Implementation complexity**: Three different strategies to implement and maintain
- **Inconsistent latency**: Some features instant (semantic tokens), others have server latency
- **Refresh mechanism dependency**: Progressive refinement requires editor support for token refresh

### Neutral

- **Per-method configuration possible**: Future enhancement could allow users to override strategies
- **Server capability detection**: Some servers may not support all methods; need graceful degradation

## Implementation Phases

### Phase 2: Semantic Tokens (Parallel Fetch)
- Implement parallel fetch with timeout
- Progressive refinement via refresh notification
- Token merging for multi-server scenarios

### Phase 3: Navigation Features (Full Delegation)
- Go-to-definition redirection
- Find references redirection
- Document symbol redirection

### Phase 4: Editing Features (Full Delegation)
- Completion redirection with merge
- Hover redirection with concatenation
- Signature help redirection

### Phase 5: Diagnostics (Background Collection)
- Diagnostic collection from redirected servers
- Offset translation for diagnostic positions
- Diagnostic merging and deduplication

## Related Decisions

- [ADR-0006](0006-language-server-redirection.md): Core LSP redirection architecture
- [ADR-0007](0007-virtual-document-model.md): How injections are represented as virtual documents
