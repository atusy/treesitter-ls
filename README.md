# treesitter-ls

A Language Server Protocol (LSP) server written in Rust that uses Tree‑sitter for fast parsing and language‑aware features.

## Features

- Dynamic parsers: Loads Tree‑sitter language parsers from shared libraries at runtime (via `libloading`).
- Semantic tokens: Full, range, and delta semantic highlighting powered by Tree‑sitter queries.
- Go to definition: Language‑agnostic resolver using Tree‑sitter “locals” queries (`local.definition.*`, `local.reference.*`).
- Selection ranges: AST‑based selection expansion using parent nodes.
- Code actions: Refactor to reorder parameters inside a `parameters` node (for grammars that expose it).
- Async runtime: Built with `tokio` and `tower-lsp`.

## Build

```bash
cargo build --release
```

Binary: `target/release/treesitter-ls`

## Configuration (Initialization Options)

The server reads configuration from LSP `initializationOptions`. Shape:

```json
{
  "runtimepath": ["/path/to/parsers"],
  "languages": {
    "rust": {
      "library": "/abs/path/to/rust.so",          // optional (overrides runtimepath)
      "filetypes": ["rs"],                         // required; file extensions without dot
      "highlight": [                               // required; list of query sources
        { "path": "/abs/path/to/highlights.scm" },
        { "query": "(identifier) @variable" }
      ],
      "locals": [                                  // optional; enables goto-definition
        { "path": "/abs/path/to/locals.scm" }
      ]
    }
  }
}
```

Notes:
- If `library` is omitted, the server searches each `runtimepath` directory for `<language>.so` (Linux) or `<language>.dylib` (macOS). The `<language>` key is the map key (e.g. `rust`).
- “Go to definition” relies on locals queries that emit captures like `local.definition.*` and `local.reference.*` (see `queries/*/locals.scm` in this repo for examples).
- Semantic tokens use capture names mapped to LSP token types (e.g. `@function`, `@type`, `@variable`, etc.).

## Neovim Example

Below is a minimal configuration using `vim.lsp.start` or `nvim-lspconfig`. Adjust paths to your environment.

```lua
vim.lsp.config.treesitter_ls = {
  cmd = { "/abs/path/to/treesitter-ls/target/release/treesitter-ls" },
  settings = {
    runtimepath = {
      -- Search here for <language>.so or <language>.dylib
      "/abs/path/to/tree-sitter-parsers",
    },
    languages = {
      rust = {
        -- Optional explicit parser path (overrides runtimepath search)
        -- library = "/abs/path/to/rust.so",
        filetypes = { "rs" },
        highlight = {
          { path = "/abs/path/to/treesitter-ls/queries/rust/highlights.scm" },
          { query = [[
            ;; Your custom additions
            (comment) @comment
          ]] },
        },
        -- Enable goto-definition by providing locals queries
        locals = {
          { path = "/abs/path/to/treesitter-ls/queries/rust/locals.scm" },
        },
      },
    },
  },
}

vim.api.nvim_create_autocmd("FileType", {
  pattern = { "rust" },
  group = vim.api.nvim_create_augroup("treesitter_ls", { clear = true }),
  callback = function(ctx)
    vim.lsp.start({
      name = "treesitter_ls",
      cmd = vim.lsp.config.treesitter_ls.cmd,
      root_dir = vim.fs.dirname(vim.fs.find({ "Cargo.toml" }, { upward = true })[1]),
      init_options = vim.lsp.config.treesitter_ls.settings,
    })
    -- Avoid double-highlighting: disable Neovim Tree‑sitter
    pcall(vim.treesitter.stop, ctx.buf)
  end,
})
```

## Parser Libraries

You need Tree‑sitter parser shared libraries. Typical artifact names:
- Linux: `<language>.so`
- macOS: `<language>.dylib`

For example, build the Rust grammar (see the grammar’s README for up‑to‑date instructions):

```bash
git clone https://github.com/tree-sitter/tree-sitter-rust.git
# Use your platform’s build instructions to produce rust.so / rust.dylib
```

## Queries

- This repo includes example queries under `queries/<language>/`:
  - Highlights: `queries/rust/highlights.scm`, `queries/lua/highlights.scm`
  - Locals (for definitions/references): `queries/rust/locals.scm`, `queries/lua/locals.scm`
- You can combine multiple sources using `{ path = ... }` and `{ query = ... }` entries; the server concatenates them.

## Supported LSP Capabilities

- `textDocument/semanticTokens/full`, `/range`, `/full/delta`
- `textDocument/definition` (requires locals queries)
- `textDocument/selectionRange`
- `textDocument/codeAction` (parameter reordering in `parameters` nodes)
- `textDocument/didOpen`/`didChange` (full sync)
