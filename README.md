<!-- Focus on providing info for users. Avoid technical details -->

# treesitter-ls

Tree-sitter-based language server for accurate parsing and language-aware features across multiple programming languages.

- **🚀 Multi-language Support** - Works with any language that has a Tree-sitter grammar (e.g., Python, JavaScript, Rust, Lua, etc.)
- **🔍 Injection Regions** - Detect and handle embedded languages (e.g., Rust in Markdown code blocks)
- **⚙️ LSP Bridge** - Redirect LSP requests (go-to-definition, hover, etc.) from injection regions to external language servers (e.g., rust-analyzer)

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
