<!-- Focus on providing info for users. Avoid technical details -->

# treesitter-ls

A fast and flexible Language Server Protocol (LSP) server that leverages Tree-sitter for accurate parsing and language-aware features across multiple programming languages.

## Features

- **🎨 Semantic Highlighting** - Full, range, and delta semantic tokens with customizable mappings
- **🌐 Language Injection** - Syntax highlighting for embedded languages (e.g., Lua in Markdown code blocks)
- **🔍 Go to Definition** - Language-agnostic navigation using Tree-sitter locals queries
- **📝 Smart Selection** - Expand selection based on AST structure with injection awareness
- **🔧 Code Actions** - Refactoring support (e.g., parameter reordering)

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
