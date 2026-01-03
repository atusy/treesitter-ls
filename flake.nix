{
  description = "treesitter-ls - A Tree-sitter Language Server";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs.nixpkgs.follows = "nixpkgs";
    };
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs {
          inherit system overlays;
        };
        # Rust 2024 edition requires nightly or recent stable
        rustToolchain = pkgs.rust-bin.stable.latest.default.override {
          extensions = [ "rust-src" "rust-analyzer" "clippy" ];
        };

        # Tree-sitter grammars for testing
        treesitterGrammars = with pkgs.tree-sitter-grammars; [
          tree-sitter-bash
          tree-sitter-c
          tree-sitter-go
          tree-sitter-javascript
          tree-sitter-json
          tree-sitter-lua
          tree-sitter-markdown
          tree-sitter-markdown-inline
          tree-sitter-python
          tree-sitter-rust
          tree-sitter-toml
          tree-sitter-typescript
          tree-sitter-yaml
        ];
      in
      {
        devShells.default = pkgs.mkShell {
          buildInputs = with pkgs; [
            # Rust toolchain
            rustToolchain

            # Build dependencies
            pkg-config
            openssl

            # Tree-sitter CLI (for parser compilation)
            tree-sitter

            # Development tools
            cargo-watch
            cargo-edit

            # For Neovim integration testing
            neovim
            vimPlugins.nvim-treesitter  # Provides bundled queries (highlights.scm, etc.)
            git
          ] ++ treesitterGrammars
            ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            # macOS-specific dependencies
            pkgs.apple-sdk_15
            pkgs.libiconv
          ];

          shellHook = ''
            echo "🌲 treesitter-ls development environment"
            echo "Rust: $(rustc --version)"
            echo "Cargo: $(cargo --version)"
            echo ""
            echo "Available commands: (run 'make help' for more)"
            echo "  cargo build    - Build the project (debug)"
            echo "  make           - Build the project (release)"
            echo "  cargo test     - Run tests"
            echo "  cargo clippy   - Run linter"
            echo "  make test_nvim - Run Neovim tests"
          '';

          # Environment variables
          RUST_BACKTRACE = "1";
          RUST_LOG = "info";
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "treesitter-ls";
          version = (pkgs.lib.importTOML ./Cargo.toml).package.version;
          src = self;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.apple-sdk_15
            pkgs.libiconv
          ];

          meta = with pkgs.lib; {
            description = "A Tree-sitter Language Server";
            homepage = "https://github.com/atusy/treesitter-ls";
            license = licenses.mit;
            maintainers = with maintainers; [ atusy ];
          };
        };
      }
    );
}
