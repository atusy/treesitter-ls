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

**Project Context**: Many language servers require project structure to function. rust-analyzer returns `null` for go-to-definition on standalone `.rs` files—it needs a project definition (via `Cargo.toml` or `rust-project.json`/`linkedProjects`) to build its symbol index.

**Real Files on Disk**: Some servers index from the filesystem rather than relying solely on `didOpen` content. Virtual URIs (`file:///virtual/...`) are insufficient.

**Indexing Time**: Language servers need time to index after `didOpen` before responding to queries. The `publishDiagnostics` notification signals indexing completion.

These constraints mean redirection is not simply "forward request, return response"—servers may need specific initialization options and real files on disk.

## Decision

**Implement LSP Client capability in treesitter-ls to redirect requests for injection regions to configured language servers, with user-provided initialization options and connection pooling.**

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
│                      │  TempFileStore  │                        │
│                      │ (injection.rs)  │                        │
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

#### Spawn Strategy

| Strategy | Trigger | First Request Latency | Resource Usage |
|----------|---------|----------------------|----------------|
| **Eager** | Injection detected during parse | Low (server pre-warmed) | Higher (may spawn unused) |
| **Lazy** | First LSP request to injection | High (spawn + index) | Lower (only when needed) |

**Recommended: Eager spawn** when injection is detected during document parsing or semantic token calculation. This eliminates user-perceived latency on first go-to-definition or hover.

```
Document Open/Edit
       │
       ▼
┌─────────────────┐
│ Parse document  │
│ Detect injects  │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────────────────┐
│ For each new    │────▶│ Background: spawn server    │
│ injection lang  │     │ Write temp workspace        │
└─────────────────┘     │ Wait for publishDiagnostics │
                        └─────────────────────────────┘
         │
         ▼
  (User makes request)
         │
         ▼
┌─────────────────┐
│ Server ready    │ ──▶ Immediate response
│ (already warm)  │
└─────────────────┘
```

Injection detection already happens during:
- `textDocument/semanticTokens` (we scan all injections for highlighting)
- Incremental parsing on `textDocument/didChange`

Spawning can piggyback on these existing code paths.

Lifecycle:
- **Spawn on injection detection**: Background spawn when new language injection is found
- **Reuse**: All subsequent requests use warm connection
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
        "initializationOptions": {
          "linkedProjects": ["~/.config/treesitter-ls/rust-project.json"]
        }
      },
      "pyright": {
        "languages": ["python"],
        "command": "pyright-langserver",
        "args": ["--stdio"]
      },
      "gopls": {
        "languages": ["go"],
        "command": "gopls"
      }
    }
  }
}
```

| Field | Required | Description |
|-------|----------|-------------|
| `languages` | Yes | Languages this server handles |
| `command` | Yes | Server executable |
| `args` | No | Command-line arguments |
| `initializationOptions` | No | Passed to server's `initialize` request |

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

Different language servers have different project structure requirements:

| Server | Requirement | Solution |
|--------|-------------|----------|
| rust-analyzer | Project context | `linkedProjects` pointing to user's `rust-project.json` |
| gopls | Module context | v0.15.0+ has improved standalone file support |
| pyright | None | Works with virtual documents via `didOpen` |
| typescript-language-server | None | Works with virtual documents via `didOpen` |

#### Design: User-Configured LSP + Minimal File Creation

treesitter-ls should be as simple as possible:
1. **Create only the source file** with injection content
2. **Pass user-provided settings** to the language server via `initializationOptions`
3. **Let the language server** use its own configuration mechanisms

For rust-analyzer, users maintain a `rust-project.json` that defines a virtual crate:

```json
// ~/.config/treesitter-ls/rust-project.json
{
  "sysroot_src": "~/.rustup/toolchains/stable-x86_64-apple-darwin/lib/rustlib/src/rust/library",
  "crates": [{
    "root_module": "/tmp/treesitter-ls/injection.rs",
    "edition": "2021",
    "deps": []
  }]
}
```

treesitter-ls configuration points rust-analyzer to this file:

```json
{
  "redirections": {
    "rust-analyzer": {
      "languages": ["rust"],
      "command": "rust-analyzer",
      "initializationOptions": {
        "linkedProjects": ["~/.config/treesitter-ls/rust-project.json"]
      }
    }
  }
}
```

**Benefits**:
- treesitter-ls has zero language-specific knowledge
- Users leverage familiar LSP configuration patterns
- Full flexibility for any language server
- Simpler implementation (just write one file)

#### Provisioning Flow

1. Create temporary file with injection content (e.g., `/tmp/treesitter-ls/injection.rs`)
2. Initialize server with user-provided `initializationOptions`
3. Send `didOpen` notification
4. Wait for `publishDiagnostics` before querying
5. Clean up temp file on shutdown

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

- **Resource overhead**: Multiple language server processes consume memory
- **Complexity**: treesitter-ls becomes both server and client; protocol translation adds complexity
- **Initial latency**: First request to a language incurs server spawn time
- **Debugging difficulty**: Multi-hop request/response makes troubleshooting harder

### Neutral

- **Configuration optional**: Some servers (pyright) work out-of-the-box; others (rust-analyzer) benefit from `initializationOptions` for full functionality
- **Partial feature support**: Not all LSP methods will be redirected (see [ADR-0008](0008-redirection-request-strategies.md))
- **Server availability**: Graceful degradation when servers not installed

## Implementation Phases

### Phase 1: Infrastructure (PoC Done)
- Basic LSP client implementation
- Temporary source file creation
- Offset translation
- Go-to-definition working

### Phase 2: Connection Pool
- Server connection pooling
- Idle timeout and cleanup
- Crash recovery

### Phase 3: Configuration System
- `initializationOptions` passthrough
- Support for multiple language servers

### Phase 4+: Feature Expansion
See [ADR-0008](0008-redirection-request-strategies.md) for per-method implementation details.

## Related Decisions

- [ADR-0005](0005-language-detection-fallback-chain.md): Language detection applies to both host documents and injection regions
- [ADR-0007](0007-virtual-document-model.md): How multiple injections are represented as virtual documents
- [ADR-0008](0008-redirection-request-strategies.md): Per-method redirection strategies
