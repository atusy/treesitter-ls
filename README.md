<!-- README.md should not contain technical details or developer info. They should go to CONTRIBUTING.md -->

# treesitter-ls

A fast and flexible Language Server Protocol (LSP) server that leverages Tree-sitter for accurate parsing and language-aware features across multiple programming languages.

## Features

### Core Capabilities
- **🚀 Dynamic Parser Loading** - Load Tree-sitter parsers at runtime from shared libraries
- **🎨 Semantic Highlighting** - Full, range, and delta semantic tokens with customizable mappings
- **🌐 Language Injection** - Syntax highlighting for embedded languages (e.g., Lua in Markdown code blocks)
- **🔍 Go to Definition** - Language-agnostic navigation using Tree-sitter locals queries
- **📝 Smart Selection** - Expand selection based on AST structure with injection awareness
- **🔧 Code Actions** - Refactoring support (e.g., parameter reordering)

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

### 1. Obtain Parser Libraries

You need Tree-sitter parser shared libraries for the languages you want to support:

- **Linux**: `<language>.so`
- **macOS**: `<language>.dylib`
- **Windows**: `<language>.dll` *(experimental)*

Example: Building the Rust parser
```bash
git clone https://github.com/tree-sitter/tree-sitter-rust.git
cd tree-sitter-rust
npm install
npm run build
# Creates rust.so (Linux) or rust.dylib (macOS)
```

### 2. Configure Your Editor

#### Neovim (Native LSP)

```lua
-- ~/.config/nvim/init.lua
vim.api.nvim_create_autocmd("FileType", {
  pattern = { "rust", "lua", "markdown" },
  callback = function()
    vim.lsp.start({
      name = "treesitter-ls",
      cmd = { "/path/to/treesitter-ls" },
      root_dir = vim.fs.dirname(vim.fs.find({ ".git" }, { upward = true })[1]),
      init_options = {
        searchPaths = { "/path/to/parsers" },
        languages = {
          rust = { filetypes = { "rs" } },
          lua = { filetypes = { "lua" } },
          markdown = { filetypes = { "md", "markdown" } },
        }
      }
    })
  end
})
```

#### VS Code
*Extension coming soon*

#### Other Editors
Any editor with LSP support can use treesitter-ls. See [Editor Setup](docs/editors.md) for more examples.

## Configuration

The server is configured via LSP initialization options:

```jsonc
{
  "searchPaths": [
    "/path/to/parsers",      // Directory containing parser/<lang>.so files
    "/path/to/queries"       // Directory containing queries/<lang>/*.scm files
  ],
  "languages": {
    "rust": {
      "library": "/explicit/path/to/rust.so",  // Optional: override searchPaths
      "filetypes": ["rs"],                     // Required: file extensions
      "highlight": [                           // Optional: custom highlighting queries
        { "path": "/path/to/highlights.scm" },
        { "query": "(identifier) @variable" }
      ],
      "locals": [                              // Optional: for go-to-definition
        { "path": "/path/to/locals.scm" }
      ]
    },
    "markdown": {
      "filetypes": ["md", "markdown"],
      "injections": [                          // Optional: for embedded language support
        { "path": "/path/to/injections.scm" }
      ]
    }
  },
  "captureMappings": {                         // Optional: customize token types
    "_": {                                     // "_" applies to all languages
      "constant": "variable.readonly",
      "keyword.return": "keyword"
    },
    "rust": {                                  // Language-specific mappings
      "lifetime": "label"
    }
  }
}
```

### Configuration Options

| Option | Description | Default |
|--------|-------------|---------|
| `searchPaths` | Directories to search for parsers and queries | `[]` |
| `languages.<lang>.library` | Explicit parser library path | Auto-detect from searchPaths |
| `languages.<lang>.filetypes` | File extensions to associate | Required |
| `languages.<lang>.highlight` | Highlighting query sources | Auto-detect from searchPaths |
| `languages.<lang>.locals` | Locals query sources for navigation | Auto-detect from searchPaths |
| `languages.<lang>.injections` | Injection query sources for embedded languages | Auto-detect from searchPaths |
| `captureMappings` | Map Tree-sitter captures to LSP token types | Built-in mappings |

## Query Files

Tree-sitter queries power the language features:

### Highlights Query (`highlights.scm`)
Defines syntax highlighting:
```scheme
(function_item name: (identifier) @function)
(string_literal) @string
```

### Locals Query (`locals.scm`)
Enables go-to-definition:
```scheme
(function_item name: (identifier) @local.definition.function)
(call_expression function: (identifier) @local.reference.function)
```

### Injections Query (`injections.scm`)
Defines embedded language regions for syntax highlighting:
```scheme
; Markdown fenced code blocks with language identifier
(fenced_code_block
  (info_string (language) @injection.language)
  (code_fence_content) @injection.content)

; HTML script tags
(script_element
  (raw_text) @injection.content
  (#set! injection.language "javascript"))
```

When an injection query matches, treesitter-ls will:
1. Parse the injected content with the appropriate language parser
2. Generate semantic tokens for the embedded code
3. Merge tokens with the host document (with proper position adjustment)

This enables syntax highlighting for code blocks in Markdown, embedded SQL in strings, and similar scenarios.

### Query Locations
Queries are searched in this order:
1. Explicit paths in configuration
2. `<searchPath>/queries/<language>/highlights.scm`
3. `<searchPath>/queries/<language>/locals.scm`
4. `<searchPath>/queries/<language>/injections.scm`

Example directory structure:
```
/path/to/resources/
├── parser/
│   ├── rust.so
│   ├── lua.so
│   └── markdown.so
└── queries/
    ├── rust/
    │   ├── highlights.scm
    │   └── locals.scm
    ├── lua/
    │   ├── highlights.scm
    │   └── locals.scm
    └── markdown/
        ├── highlights.scm
        └── injections.scm    # Enables Lua/Python/etc. highlighting in code blocks
```

## Supported LSP Features

### Text Synchronization
- `textDocument/didOpen`
- `textDocument/didChange` (full sync)
- `textDocument/didClose`

### Language Features
- `textDocument/semanticTokens/full` - Full document highlighting
- `textDocument/semanticTokens/range` - Partial highlighting
- `textDocument/semanticTokens/full/delta` - Incremental updates
- `textDocument/definition` - Go to definition
- `textDocument/selectionRange` - Expand/shrink selection

### Code Actions
- `textDocument/codeAction` - Parameter reordering (for supported grammars)

## Troubleshooting

### Parser Not Found
```
Error: Could not load parser for language 'rust'
```
**Solution**: Ensure the parser library exists in one of the `searchPaths` directories or specify an explicit `library` path.

### No Syntax Highlighting
**Check**:
1. Parser loaded successfully (check server logs)
2. `highlights.scm` query file exists and is valid
3. File extension matches configured `filetypes`

### Go to Definition Not Working
**Requirements**:
- `locals.scm` query file with `local.definition.*` and `local.reference.*` captures
- Properly structured Tree-sitter grammar with scope information

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
- The Tree-sitter grammar authors for their excellent work

## Related Projects

- [tree-sitter](https://github.com/tree-sitter/tree-sitter) - The parsing library
- [nvim-treesitter](https://github.com/nvim-treesitter/nvim-treesitter) - Neovim's Tree-sitter integration
- Individual grammar repositories (e.g., [tree-sitter-rust](https://github.com/tree-sitter/tree-sitter-rust))
