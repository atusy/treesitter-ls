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

### Language Server Constraints

Language servers have requirements beyond the LSP protocol that affect this architecture:

**Project Context**: Many language servers require project structure to function. rust-analyzer returns `null` for go-to-definition on standalone `.rs` files—it needs `Cargo.toml` and proper workspace context to build its symbol index.

**Real Files on Disk**: Some servers index from the filesystem rather than relying solely on `didOpen` content. Virtual URIs (`file:///virtual/...`) are insufficient.

**Indexing Time**: Language servers need time to index after `didOpen` before responding to queries. The `publishDiagnostics` notification signals indexing completion.

These constraints mean redirection is not simply "forward request, return response"—it requires creating temporary project structures tailored to each language server.

## Decision

**Implement LSP Client capability in treesitter-ls to redirect requests for injection regions to configured language servers, with server-specific workspace provisioning and connection pooling.**

### Architecture Overview

```
┌─────────────────────────────────────────────────────────────────┐
│                        treesitter-ls                             │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌──────────────┐    ┌─────────────────┐    ┌────────────────┐  │
│  │ LSP Handler  │───▶│RedirectionRouter│───▶│ PositionMapper │  │
│  │ (lsp_impl)   │    │                 │    │                │  │
│  └──────────────┘    └────────┬────────┘    └────────────────┘  │
│                               │                                  │
│                               ▼                                  │
│                      ┌─────────────────┐                        │
│                      │   ServerPool    │                        │
│                      │ ┌─────────────┐ │                        │
│                      │ │rust-analyzer│ │  (on-demand spawn,    │
│                      │ ├─────────────┤ │   connection reuse)   │
│                      │ │  pyright    │ │                        │
│                      │ ├─────────────┤ │                        │
│                      │ │   gopls     │ │                        │
│                      │ └─────────────┘ │                        │
│                      └────────┬────────┘                        │
│                               │                                  │
│                               ▼                                  │
│                      ┌─────────────────┐                        │
│                      │WorkspaceManager │                        │
│                      │ (temp projects) │                        │
│                      └─────────────────┘                        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### Server Connection Pool

**Critical for production**: Spawning a language server per request is unacceptable (multi-second latency). Connections must be pooled and reused.

```
┌─────────────────────────────────────────────────────────┐
│                    ServerPool                            │
├─────────────────────────────────────────────────────────┤
│                                                          │
│  get_connection("rust")                                  │
│       │                                                  │
│       ▼                                                  │
│  ┌─────────────────┐    ┌─────────────────────────────┐ │
│  │ Connection      │ NO │ Spawn new server            │ │
│  │ exists?         │───▶│ Wait for initialization     │ │
│  └────────┬────────┘    │ Store in pool               │ │
│           │ YES         └─────────────────────────────┘ │
│           ▼                                              │
│  ┌─────────────────┐                                    │
│  │ Return existing │                                    │
│  │ connection      │                                    │
│  └─────────────────┘                                    │
│                                                          │
│  Idle timeout ───▶ Shutdown unused servers              │
│                                                          │
└─────────────────────────────────────────────────────────┘
```

Lifecycle:
- **Spawn on first use**: When injection region of a language is first encountered
- **Reuse**: Subsequent requests use existing connection
- **Idle shutdown**: After configurable timeout with no requests
- **Crash recovery**: Detect dead servers and respawn

### Server Registry and Configuration

Redirection requires knowing which server to use for each language:

```json
{
  "treesitter-ls": {
    "redirections": {
      "rust-analyzer": {
        "languages": ["rust"],
        "command": "rust-analyzer",
        "args": [],
        "workspaceType": "cargo"
      },
      "pyright": {
        "languages": ["python"],
        "command": "pyright-langserver",
        "args": ["--stdio"],
        "workspaceType": "none"
      },
      "gopls": {
        "languages": ["go"],
        "command": "gopls",
        "args": [],
        "workspaceType": "gomod"
      }
    }
  }
}
```

#### Why Server-Centric Configuration

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

### Workspace Provisioning

Different language servers require different project structures:

| Server | Workspace Type | Files Created |
|--------|----------------|---------------|
| rust-analyzer | `cargo` | `Cargo.toml`, `src/main.rs` |
| gopls | `gomod` | `go.mod`, `main.go` |
| pyright | `none` | (virtual document only) |
| typescript-language-server | `none` | (virtual document only) |

For servers requiring real files:
1. Create minimal temporary project structure
2. Write injection content to appropriate file
3. Initialize server with `rootUri` pointing to temp directory
4. Wait for `publishDiagnostics` before querying
5. Clean up temp directory on shutdown

### Async Communication

Language server communication uses synchronous stdio which blocks the thread. In treesitter-ls's async runtime (tokio), this requires `spawn_blocking` to avoid stalling other tasks.

```rust
let result = tokio::task::spawn_blocking(move || {
    let conn = pool.get_connection("rust")?;
    conn.request(method, params)
}).await.ok().flatten();
```

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

Translation is straightforward for positions within a single injection. See [ADR-0008](0008-redirection-request-strategies.md) for complex cases involving cross-file references.

## Consequences

### Positive

- **Full LSP in injections**: Users get completion, hover, diagnostics in code blocks
- **No editor configuration**: Works transparently; editor only talks to treesitter-ls
- **Leverages existing detection**: Reuses injection detection from Tree-sitter queries
- **Progressive enhancement**: Falls back gracefully to Tree-sitter when servers unavailable
- **Low latency**: Connection pooling enables fast responses after initial spawn

### Negative

- **Resource overhead**: Multiple language server processes consume memory; temp directories use disk space
- **Complexity**: treesitter-ls becomes both server and client; protocol translation adds complexity
- **Initial latency**: First request to a language incurs server spawn time
- **Debugging difficulty**: Multi-hop request/response makes troubleshooting harder
- **Server-specific logic**: Each language server may need custom workspace provisioning

### Neutral

- **Configuration burden**: Users must configure which servers to use per language
- **Partial feature support**: Not all LSP methods will be redirected (see [ADR-0008](0008-redirection-request-strategies.md))
- **Server availability**: Graceful degradation when servers not installed

## Implementation Phases

### Phase 1: Infrastructure (PoC Done)
- Basic LSP client implementation
- Workspace provisioning for rust-analyzer
- Offset translation
- Go-to-definition working

### Phase 2: Connection Pool
- Server connection pooling
- Idle timeout and cleanup
- Crash recovery

### Phase 3: Multi-Language Support
- Configuration system for server registry
- Workspace provisioner abstraction
- Support for pyright, gopls, typescript-language-server

### Phase 4+: Feature Expansion
See [ADR-0008](0008-redirection-request-strategies.md) for per-method implementation details.

## Related Decisions

- [ADR-0005](0005-language-detection-fallback-chain.md): Language detection applies to both host documents and injection regions
- [ADR-0007](0007-virtual-document-model.md): How multiple injections are represented as virtual documents
- [ADR-0008](0008-redirection-request-strategies.md): Per-method redirection strategies
