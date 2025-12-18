<!-- README.md should not contain technical details or developer info. They should go to CONTRIBUTING.md -->

# treesitter-ls

A fast and flexible Language Server Protocol (LSP) server that leverages Tree-sitter for accurate parsing and language-aware features across multiple programming languages.

## Features

### Core Capabilities
- **🚀 Zero Configuration** - Works out of the box with automatic parser/query installation
- **🎨 Semantic Highlighting** - Full, range, and delta semantic tokens with customizable mappings
- **🌐 Language Injection** - Syntax highlighting for embedded languages (e.g., Lua in Markdown code blocks)
- **🔍 Go to Definition** - Language-agnostic navigation using Tree-sitter locals queries
- **📝 Smart Selection** - Expand selection based on AST structure with injection awareness
- **🔧 Code Actions** - Refactoring support (e.g., parameter reordering)
- **📦 Dynamic Parser Loading** - Load Tree-sitter parsers at runtime from shared libraries

## Installation

### Pre-built Binaries
*Coming soon*

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

## Quick Start

### Zero-Configuration Mode (Recommended)

treesitter-ls works out of the box with no configuration:

1. Start your editor with treesitter-ls configured as the LSP server
2. Open any file with a supported language
3. The parser and queries are automatically downloaded and installed

That's it! Syntax highlighting and other features work immediately.

### Example: Neovim Setup

Using Neovim's built-in LSP client (0.11+):

```lua
-- ~/.config/nvim/init.lua
vim.lsp.config.treesitter_ls = {
  cmd = { "treesitter-ls" },
}
vim.lsp.enable("treesitter_ls")

-- Disable built-in treesitter highlighting to avoid conflicts
vim.api.nvim_create_autocmd("FileType", {
  callback = function()
    vim.treesitter.stop()
  end,
})
```

Or with nvim-lspconfig:

```lua
require("lspconfig").treesitter_ls.setup({})
```

## CLI Commands

treesitter-ls provides a command-line interface for managing languages:

```bash
# Install a language (parser + queries)
treesitter-ls language install lua

# List all supported languages
treesitter-ls language list

# Show installed languages and their status
treesitter-ls language status

# Uninstall a language
treesitter-ls language uninstall lua

# Generate a default configuration file
treesitter-ls config init
```

See `treesitter-ls --help` for all options.

## Configuration

Configuration is optional. When provided via LSP `initializationOptions`, you can customize behavior:

```jsonc
{
  "autoInstall": true,              // Auto-install missing parsers (default: true)
  "searchPaths": ["/custom/path"],  // Custom paths for parsers/queries
  "languages": {
    "rust": {
      "filetypes": ["rs"],          // File extensions
      "library": "/path/to/rust.so" // Explicit parser path
    }
  },
  "captureMappings": {              // Customize token types
    "_": {
      "highlights": {
        "variable.builtin": "variable.defaultLibrary"
      }
    }
  }
}
```

### Default Data Directories

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/treesitter-ls/` |
| macOS | `~/Library/Application Support/treesitter-ls/` |
| Windows | `%APPDATA%/treesitter-ls/` |

See [docs/README.md](docs/README.md) for complete configuration reference.

## Prerequisites (for Auto-Install)

Auto-install compiles parsers from source, requiring:

| Dependency | Purpose | Installation |
|------------|---------|--------------|
| **tree-sitter CLI** | Compiles parser grammars | `cargo install tree-sitter-cli` |
| **Git** | Clones parser repositories | Usually pre-installed |
| **C Compiler** | Required for compilation | Platform-specific (see below) |

### C Compiler Installation

| Platform | Command |
|----------|---------|
| **macOS** | `xcode-select --install` |
| **Debian/Ubuntu** | `sudo apt install build-essential` |
| **Fedora/RHEL** | `sudo dnf install gcc` |
| **Windows** | Install [Visual Studio Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) |

## Supported Languages

treesitter-ls supports any language with a Tree-sitter grammar available in nvim-treesitter:

- Lua, Python, Rust, Go, C, C++
- JavaScript, TypeScript, TSX, JSX
- HTML, CSS, JSON, YAML, TOML
- Markdown, LaTeX
- Bash, Fish, Zsh
- SQL, GraphQL
- And many more...

Run `treesitter-ls language list` for the complete list.

## Troubleshooting

### Parser Not Loading
```bash
# Check if parser exists
treesitter-ls language status

# Reinstall the language
treesitter-ls language install <language> --force
```

### No Syntax Highlighting
1. Verify queries exist: `treesitter-ls language status --verbose`
2. Check LSP logs for errors
3. Ensure your editor has semantic tokens enabled

### Missing Prerequisites
```bash
# Verify dependencies
tree-sitter --version
git --version
cc --version  # or gcc --version
```

## Contributing

We welcome contributions! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for:
- Development setup and workflow
- Architecture overview
- Testing guidelines
- Code style and commit conventions

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

## Acknowledgments

- [Tree-sitter](https://tree-sitter.github.io/) - The incremental parsing library
- [tower-lsp](https://github.com/ebkalderon/tower-lsp) - Async LSP framework for Rust
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) - Query files and parser metadata
- The Tree-sitter grammar authors for their excellent work

## Related Projects

- [tree-sitter](https://github.com/tree-sitter/tree-sitter) - The parsing library
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) - Neovim's Tree-sitter integration
- Individual grammar repositories (e.g., [tree-sitter-rust](https://github.com/tree-sitter/tree-sitter-rust))
