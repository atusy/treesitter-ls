<!-- Focus on providing info for users. Avoid technical details -->

# treesitter-ls

Tree-sitter-based language server for accurate parsing and language-aware features across multiple programming languages.

- **🚀 Multi-language Support** - Works with any language that has a Tree-sitter grammar (e.g., Python, JavaScript, Rust, Lua, etc.)
- **🔍 Injection Regions** - Detect and handle embedded languages (e.g., Rust in Markdown code blocks)
- **⚙️ LSP Bridge** - Redirect LSP requests (go-to-definition, hover, etc.) from injection regions to external language servers (e.g., rust-analyzer)

## Installation

### Pre-built Binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/atusy/treesitter-ls/releases)

### Enable Automatic Parser/Query Installation

Prepare the following, and treesitter-ls will auto-install parsers/queries as needed:

- tree-sitter CLI
- Git
- C compiler

## Setup

See docs/README.md for detailed setup instructions for various editors.

A quick start with Neovim:

```bash
make deps/nvim
nvim -u scripts/minimal_init.lua
```

## Supported LSP Features

treesitter-ls supports LSP features via three mechanisms:

- **Host**: Direct support for the main document language
- **Injection**: Embedded language regions (e.g., code blocks in Markdown)
- **Bridge**: Injection regions delegated to external language servers

| Feature | Host | Injection | Bridge |
|---------|:----:|:---------:|:------:|
| Semantic Tokens | ✅ | ✅ | ❌ |
| Selection Range | ✅ | ✅ | ❌ |
| Code Actions | ✅ | ✅ | ✅ |
| Go-to Definition | ❌ | ❌ | ✅ |
| Hover | ❌ | ❌ | ✅ |
| Completion | ❌ | ❌ | ✅ |
| Signature Help | ❌ | ❌ | ✅ |
| Find References | ❌ | ❌ | ✅ |
| Rename | ❌ | ❌ | ✅ |
| Formatting | ❌ | ❌ | ✅ |

## Configuration

### Language Servers (Bridge)

Configure external language servers for bridging injection regions.

```json
{
  "languageServers": {
    "rust-analyzer": {
      "cmd": ["rust-analyzer"],
      "languages": ["rust"]
    },
    "pyright": {
      "cmd": ["pyright-langserver", "--stdio"],
      "languages": ["python"]
    }
  }
}
```

### Migration from bridge.servers (Deprecated)

The `bridge.servers` field is deprecated. Migrate to the top-level `languageServers` field:

**Before (deprecated):**
```json
{
  "bridge": {
    "servers": {
      "rust-analyzer": {
        "cmd": ["rust-analyzer"],
        "languages": ["rust"]
      }
    }
  }
}
```

**After (recommended):**
```json
{
  "languageServers": {
    "rust-analyzer": {
      "cmd": ["rust-analyzer"],
      "languages": ["rust"]
    }
  }
}
```

Both formats work during the transition period, with `languageServers` taking precedence if both are specified.
