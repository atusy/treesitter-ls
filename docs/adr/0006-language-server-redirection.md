# ADR-0006: LSP Redirection Architecture for Injection Regions

## Status

Proposed

## Context

Markdown code blocks and other injection regions (e.g., JavaScript inside HTML `<script>` tags, SQL in string literals) currently only receive Tree-sitter-based features from treesitter-ls. While Tree-sitter provides excellent syntax highlighting via semantic tokens, injection regions lack access to full LSP capabilities such as:

- Go-to-definition with cross-file resolution
- Completion with type information
- Hover documentation
- Diagnostics from language-specific analyzers

Modern editors can only attach one LSP server per buffer, meaning users must choose between treesitter-ls (fast semantic tokens for the host document) and a language-specific server (full features but only for the primary language).

The key insight is: **treesitter-ls already knows where injection regions are and what languages they contain**. It can act as an LSP proxy, forwarding requests for injection regions to appropriate language servers.

## Decision

**Implement LSP Client capability in treesitter-ls to redirect requests for injection regions to configured language servers.**

### Architecture Overview

```
Editor                   treesitter-ls                    Language Servers
  │                           │                                  │
  │ ── textDocument/xxx ───▶  │                                  │
  │                           │ (Is cursor in injection?)        │
  │                           │                                  │
  │                           │── YES ─▶ forward request ───────▶│ (e.g., rust-analyzer)
  │                           │          with offset adjustment  │
  │                           │                                  │
  │ ◀─── response ────────────│◀──────── response ───────────────│
  │                           │          with offset adjustment  │
  │                           │                                  │
  │                           │── NO ──▶ handle locally          │
  │                           │          (treesitter-ls logic)   │
```

### Configuration

Redirection is configured separately from Tree-sitter language settings, using a **server-centric** model:

```json
{
  "treesitter-ls": {
    "redirections": {
      "rust-analyzer": { "languages": ["rust"], ... },
      "pyright": { "languages": ["python"], ... },
      "typos-lsp": { "languages": ["markdown", "asciidoc", "text"], ... }
    }
  }
}
```

#### Why Separate from `languages`

| Concern | `languages` field | `redirections` field |
|---------|-------------------|---------------------|
| **Purpose** | Tree-sitter parser/query config | LSP server forwarding |
| **Primary key** | Language name | Server name |
| **Scope** | One language per entry | One server → multiple filetypes |
| **Example** | Parser paths, query sources | `typos-lsp` for markdown + asciidoc |

This separation allows:
- **Cross-cutting servers**: `typos-lsp` provides diagnostics for multiple languages
- **Multiple servers per language**: `pyright` + `ruff` for Python (both in `redirections`)
- **Independent lifecycle**: Tree-sitter config doesn't affect server spawning

#### Design Considerations

- **Server lifecycle**: Servers are spawned on-demand when an injection region of a matching language is first encountered
- **Process management**: Servers are kept alive for reuse; shutdown when no documents reference that language
- **Multiple servers per language**: When multiple servers match, see [ADR-0008](0008-redirection-request-strategies.md) for merging rules

### Offset Translation

Injection regions exist at specific byte offsets within the host document. Redirected requests must translate positions:

```
Host Document (Markdown)          Virtual Document (Rust)
┌─────────────────────────┐       ┌─────────────────────────┐
│ # Title                 │       │                         │
│                         │       │                         │
│ ```rust                 │       │fn main() {              │
│ fn main() {             │ ────▶ │    println!("hi");      │
│     println!("hi");     │       │}                        │
│ }                       │       │                         │
│ ```                     │       └─────────────────────────┘
│                         │
│ More text...            │
└─────────────────────────┘

Cursor at line 4, col 5 in host ──▶ line 1, col 5 in virtual
```

The existing `OffsetCalculator` in `src/analysis/offset_calculator.rs` provides this translation capability.

## Consequences

### Positive

- **Full LSP in injections**: Users get completion, hover, diagnostics in code blocks
- **No editor configuration**: Works transparently; editor only talks to treesitter-ls
- **Leverages existing detection**: Reuses injection detection from Tree-sitter queries
- **Progressive enhancement**: Falls back gracefully to Tree-sitter when servers unavailable

### Negative

- **Resource overhead**: Multiple language server processes consume memory
- **Complexity**: treesitter-ls becomes both server and client; protocol translation adds complexity
- **Initialization latency**: Spawning language servers on first use adds delay
- **Debugging difficulty**: Multi-hop request/response makes troubleshooting harder

### Neutral

- **Configuration burden**: Users must configure which servers to use per language
- **Partial feature support**: Not all LSP methods will be redirected initially (incremental implementation)
- **Server compatibility**: Some language servers may not handle virtual documents well

## Implementation Phases

### Phase 1: Infrastructure
- LSP client implementation in treesitter-ls
- Server lifecycle management (spawn, shutdown, health check)
- Configuration parsing for server definitions

### Phase 2-5: Feature Implementation
See [ADR-0008](0008-redirection-request-strategies.md) for per-method implementation details.

## Related Decisions

- [ADR-0005](0005-language-detection-fallback-chain.md): Language detection applies to both host documents and injection regions
- [ADR-0007](0007-virtual-document-model.md): How multiple injections are represented as virtual documents
- [ADR-0008](0008-redirection-request-strategies.md): Per-method redirection strategies
