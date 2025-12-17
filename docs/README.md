# treesitter-ls Documentation

treesitter-ls is a Language Server Protocol (LSP) server that uses Tree-sitter for fast, accurate parsing. It provides semantic highlighting, go-to-definition, selection ranges, and code actions for any language with a Tree-sitter grammar.

## Features

### Semantic Tokens (Syntax Highlighting)

Provides LSP semantic tokens based on Tree-sitter `highlights.scm` queries. Works with any editor that supports LSP semantic tokens.

- Supports language injection (e.g., SQL in JavaScript template strings, code blocks in Markdown)
- Uses nvim-treesitter query files for compatibility
- Supports query inheritance (e.g., TypeScript inherits from `ecma`)

### Go-to-Definition

Jump to definitions using Tree-sitter `locals.scm` queries. Works for:

- Variables
- Functions
- Imports
- Parameters

### Selection Range

Expand/shrink selection based on AST structure. Select increasingly larger syntax nodes with each invocation.

### Code Actions

- **Swap Parameters**: Reorder function parameters

## Zero-Configuration Usage

treesitter-ls works out of the box with no configuration required:

1. Start the LSP server
2. Open any file with a supported language
3. The parser and queries are automatically downloaded and installed

### Default Data Directories

| Platform | Path |
|----------|------|
| Linux | `~/.local/share/treesitter-ls/` |
| macOS | `~/Library/Application Support/treesitter-ls/` |
| Windows | `%APPDATA%/treesitter-ls/` |

Parsers are stored in `{data_dir}/parser/` and queries in `{data_dir}/queries/`.

## Configuration

Configuration is provided via LSP `initializationOptions`. All options are optional.

### Configuration Options

```json
{
  "searchPaths": ["/custom/path/to/parsers", "/another/path"],
  "autoInstall": true,
  "languages": {
    "lua": {
      "library": "/path/to/lua.so",
      "filetypes": ["lua"],
      "highlight": [
        {"path": "/path/to/highlights.scm"},
        {"query": "(identifier) @variable"}
      ],
      "locals": [
        {"path": "/path/to/locals.scm"}
      ]
    }
  },
  "captureMappings": {
    "_": {
      "highlights": {
        "variable.builtin": "variable.defaultLibrary"
      }
    }
  }
}
```

### Option Reference

#### `searchPaths`

Array of directories to search for parsers and queries. If not specified, uses platform-specific defaults.

Parsers are searched as `{searchPath}/{language}.{so,dylib,dll}`.
Queries are searched as `{searchPath}/{language}/{query_type}.scm`.

#### `autoInstall`

- `true` (default): Automatically download and install missing parsers/queries when a file is opened
- `false`: Require manual installation via CLI

#### `languages`

Per-language configuration. Usually not needed as treesitter-ls auto-detects languages.

| Field | Description |
|-------|-------------|
| `library` | Explicit path to the parser library (`.so`, `.dylib`, `.dll`) |
| `filetypes` | File extensions that map to this language |
| `highlight` | Array of highlight query sources |
| `locals` | Array of locals query sources (for go-to-definition) |

Query sources can be:
- `{"path": "/path/to/file.scm"}` - Load from file
- `{"query": "(node) @capture"}` - Inline query

#### `captureMappings`

Remap Tree-sitter capture names to LSP semantic token types. Use `_` as a wildcard for all languages.

```json
{
  "captureMappings": {
    "_": {
      "highlights": {
        "variable.builtin": "variable.defaultLibrary",
        "function.builtin": "function.defaultLibrary"
      }
    },
    "rust": {
      "highlights": {
        "type.builtin": "type.defaultLibrary"
      }
    }
  }
}
```

### Project Configuration File

You can also use a `treesitter-ls.toml` file in your project root:

```toml
[captureMappings._.highlights]
"variable.builtin" = "variable.defaultLibrary"

[languages.custom_lang]
filetypes = ["ext"]
highlight = [
  { path = "./queries/highlights.scm" }
]
```

Project configuration is merged with LSP initialization options.

## CLI Commands

The CLI uses a hierarchical subcommand structure: `treesitter-ls <resource> <action>`.

### Language Management

```bash
# Install a language (parser + queries)
treesitter-ls language install lua

# Install with verbose output
treesitter-ls language install rust --verbose

# Force reinstall
treesitter-ls language install python --force

# Custom data directory
treesitter-ls language install go --data-dir /custom/path

# Bypass metadata cache
treesitter-ls language install ruby --no-cache

# List supported languages
treesitter-ls language list
```

## Editor Integration

### Neovim

Using Neovim's built-in LSP client (0.11+):

```lua
vim.lsp.config.treesitter_ls = {
  cmd = { "treesitter-ls" },
  init_options = {
    -- Optional: customize settings
    autoInstall = true,
  },
}
vim.lsp.enable("treesitter_ls")

-- Disable built-in treesitter highlighting to avoid conflicts
vim.api.nvim_create_autocmd("FileType", {
  callback = function()
    vim.treesitter.stop()
  end,
})
```

With nvim-lspconfig:

```lua
require("lspconfig").treesitter_ls.setup({
  init_options = {
    autoInstall = true,
  },
})
```

### VS Code

(Configuration for VS Code LSP clients would go here)

### Other Editors

Any editor supporting LSP can use treesitter-ls. Configure it as a language server with the `treesitter-ls` command.

## Supported Languages

treesitter-ls supports any language with a Tree-sitter grammar available in nvim-treesitter. Common languages include:

- Lua, Python, Rust, Go, C, C++
- JavaScript, TypeScript, TSX, JSX
- HTML, CSS, JSON, YAML, TOML
- Markdown, LaTeX
- Bash, Fish, Zsh
- SQL, GraphQL
- And many more...

Run `treesitter-ls list-languages` for the complete list.

## Query Inheritance

Some languages inherit queries from base languages:

| Language | Inherits From |
|----------|---------------|
| TypeScript | ecma |
| JavaScript | ecma, jsx |
| TSX | typescript, jsx |

When you install a language with inheritance, the base queries are automatically downloaded.

## Troubleshooting

### Parser fails to load

1. Check if the parser exists: `ls ~/.local/share/treesitter-ls/parser/`
2. Reinstall: `treesitter-ls language install <language> --force`
3. Check for ABI compatibility with your Tree-sitter version

### No syntax highlighting

1. Verify queries exist: `ls ~/.local/share/treesitter-ls/queries/<language>/`
2. Check LSP logs for errors
3. Ensure your editor has semantic tokens enabled

### Queries not working for TypeScript/JavaScript

These languages use query inheritance. Ensure base queries are installed:

```bash
treesitter-ls language install typescript --force
# This automatically installs 'ecma' queries
```
