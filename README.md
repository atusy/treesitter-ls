# treesitter-ls

A Language Server Protocol (LSP) implementation written in Rust, leveraging the `tree-sitter` parsing library to provide language-specific features, such as semantic highlighting.

## Features

*   **Dynamic Parser Loading**: Load `tree-sitter` language parsers from shared libraries at runtime.
*   **Semantic Highlighting**: Provide rich semantic tokens for syntax highlighting based on `tree-sitter` queries.
*   **Asynchronous Processing**: Built with `tokio` and `tower-lsp` for high-performance, asynchronous handling of LSP requests.

## Building the Server

To build the `treesitter-ls` executable, navigate to the project root and run:

```bash
cargo build --release
```

The compiled executable will be located at `target/release/treesitter-ls`.

## Usage with Neovim (Example Configuration)

This language server requires `initializationOptions` to specify the `tree-sitter` parser library, the language function name, and an optional semantic tokens query. Below is an example configuration for Neovim using `nvim-lspconfig`.

**1. Obtain a Tree-sitter Parser Shared Library:**

First, you need a compiled `tree-sitter` parser for your desired language. For example, to get the Rust parser:

```bash
git clone https://github.com/tree-sitter/tree-sitter-rust.git
clang -shared -o tree-sitter-rust/libtree_sitter_rust.dylib tree-sitter-rust/src/parser.c tree-sitter-rust/src/scanner.c -I tree-sitter-rust/src
```

This will create `libtree_sitter_rust.dylib` in the `tree-sitter-rust` directory.

**2. Obtain the Semantic Tokens Query:**

For semantic highlighting, you'll need a `tree-sitter` query file (e.g., `highlights.scm`). You can usually find these in the `queries/` directory of the respective `tree-sitter` grammar repository. For Rust, you can get it from:

`https://raw.githubusercontent.com/tree-sitter/tree-sitter-rust/master/queries/highlights.scm`

Copy the content of this file.

**3. Neovim Configuration:**


```lua
vim.lsp.config.treesitter_ls = {
  cmd = {
    "path/to/treesitter-ls/target/release/treesitter-ls",
  },
  settings = {
    runtimepath = {
      -- look for ${language}.so or ${language}.dylib in these directories
      "path/to/your/parsers/directory",
    },
    languages = {
      rust = {
        -- Specify the path to the tree-sitter parser shared library instead of finding one from the runtimepath
        -- library = "path/to/treesitter/parser/rust.so ",
        rust = { "rs" },
        highlight = {
            { path = "path/to/highlight.scm" },
            { query = [[
            ;; extends
        (comment) @comment
        (function_item
          name: (identifier) @function
        )]] },
        },
      },
    },
  },
}
```

**Important:**

*   Replace `'/path/to/your/treesitter-ls/target/release/treesitter-ls'` with the actual absolute path to your compiled `treesitter-ls` executable.
*   Replace `'/path/to/your/tree-sitter-rust/libtree_sitter_rust.dylib'` with the actual absolute path to your compiled `tree-sitter-rust` shared library.
*   Paste the *entire content* of the `highlights.scm` file into the `semanticTokensQuery` field. Use `[[]]` for multiline strings in Lua.

```lua
vim.api.nvim_create_autocmd("FileType", {
	pattern = "rust",
	group = vim.api.nvim_create_augroup("treesitter_ls", { clear = true }),
	callback = function(ctx)
		vim.lsp.start({
			name = "treesitter_ls",
			cmd = vim.lsp.config.treesitter_ls.cmd,
			root_dir = vim.fs.dirname(vim.fs.find({ "Cargo.toml" }, { upward = true })[1]),
			init_options = vim.lsp.config.treesitter_ls.settings,
		})
		vim.treesitter.stop(ctx.buf)
		vim.cmd("syntax off")
	end,
})
```

After configuring, open a Rust file in Neovim, and the `treesitter-ls` should provide semantic highlighting based on your `tree-sitter` query.
