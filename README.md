<!-- Focus on providing info for users. Avoid technical details -->

# treesitter-ls

A fast and flexible Language Server Protocol (LSP) server that leverages Tree-sitter for accurate parsing and language-aware features across multiple programming languages.

## Features

- **Semantic Highlighting** - Full, range, and delta semantic tokens with customizable mappings
- **Language Injection** - Syntax highlighting for embedded languages (e.g., Lua in Markdown code blocks)
- **Smart Selection** - Expand selection based on AST structure with injection awareness
- **Code Actions** - Refactoring support (e.g., parameter reordering)
- **LSP Bridge** - Full LSP features in injection regions by bridging to language-specific servers

## Installation

### Pre-built Binaries

Download the latest release for your platform from [GitHub Releases](https://github.com/atusy/treesitter-ls/releases)

### Build from Source

Requirements:
- Rust (latest stable)
- Cargo

```bash
# Clone the repository
git clone https://github.com/atusy/treesitter-ls.git
cd treesitter-ls

# Build release binary
cargo build --release

# Binary location: target/release/treesitter-ls
```

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

## LSP Bridge

LSP Bridge enables full language server features in injection regions (e.g., Rust code blocks in Markdown). Instead of implementing each language feature natively, treesitter-ls bridges requests to language-specific servers like rust-analyzer or pyright.

### Supported Features

- Completion
- Signature Help
- Go to Definition
- Hover
- Find References
- Rename
- Code Actions
- Formatting

### Configuration

Configure bridge servers and enable bridging per host language:

```json
{
  "bridge": {
    "servers": {
      "rust-analyzer": {
        "cmd": ["rust-analyzer"],
        "languages": ["rust"],
        "workspaceType": "cargo"
      },
      "pyright": {
        "cmd": ["pyright-langserver", "--stdio"],
        "languages": ["python"]
      }
    }
  },
  "languages": {
    "markdown": { "bridge": ["rust", "python"] },
    "quarto": { "bridge": ["python", "r"] },
    "rmd": { "bridge": ["r"] }
  }
}
```

### Bridge Filter Semantics

The `bridge` array in language configuration controls which injection languages are bridged:

- `bridge: ["rust", "python"]` - Only bridge these specific languages
- `bridge: []` - Disable bridging entirely for this host language
- `bridge: null` or omitted - Bridge all configured languages (default)

### Neovim Example

```lua
vim.lsp.config.treesitter_ls = {
  cmd = { "treesitter-ls" },
  init_options = {
    autoInstall = true,
    bridge = {
      servers = {
        ["rust-analyzer"] = {
          cmd = { "rust-analyzer" },
          languages = { "rust" },
          workspaceType = "cargo",
        },
        pyright = {
          cmd = { "pyright-langserver", "--stdio" },
          languages = { "python" },
        },
      },
    },
    languages = {
      markdown = { bridge = { "rust", "python" } },
    },
  },
}
vim.lsp.enable("treesitter_ls")
```
