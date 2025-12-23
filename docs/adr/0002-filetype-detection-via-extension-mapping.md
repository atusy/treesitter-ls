# ADR-0002: Filetype Detection via Extension Mapping

## Status

Accepted (See [ADR-0005](0005-language-detection-fallback-chain.md) for proposed replacement)

## Context

treesitter-ls needs to select the appropriate Tree-sitter parser for opened files. While the LSP protocol allows clients to send a `language_id` via `textDocument/didOpen`, client behavior varies and can be inconsistent.

The following options were considered for language resolution:

1. **Fully rely on LSP client's language_id** - Leave it to the client
2. **Configuration-based mapping via file extension** - Server-side control
3. **Heuristic analysis of file content** - Shebang, magic comments, etc.

## Decision

**Prioritize configuration-based mapping via file extension, using the LSP client's language_id as a fallback.**

Implementation details:
- `FiletypeResolver` holds the extension → language mapping
- Each language defines its `filetypes` in the configuration
- On file open: resolve using extension mapping → LSP language_id (in order of priority)
- Once determined, the language is retained for the document's lifetime

```rust
let language_name = self
    .language
    .get_language_for_path(uri.path())             // Primary: file extension
    .or_else(|| language_id.map(|s| s.to_string())); // Fallback: LSP client
```

## Consequences

### Positive

- Predictable behavior unaffected by LSP client implementation differences
- Users can fully control language mapping via configuration
- Consistent behavior when opening the same file in different editors
- Simple implementation with fast language resolution

### Negative

- Files without extensions (e.g., Makefile) require explicit mapping in configuration
- Compound extensions like `file.tar.gz` only recognize the last part (`gz`)
- Shebang-based language detection (e.g., `#!/usr/bin/env python`) is not supported

### Neutral

- LSP client's language_id serves as a fallback for unconfigured extensions
- Language changes during editing are not expected (file must be closed and reopened)
