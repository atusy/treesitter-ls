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
            git
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            # macOS-specific dependencies
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            pkgs.libiconv
          ];

          shellHook = ''
            echo "🌲 treesitter-ls development environment"
            echo "Rust: $(rustc --version)"
            echo "Cargo: $(cargo --version)"
            echo ""
            echo "Available commands:"
            echo "  cargo build    - Build the project"
            echo "  cargo test     - Run tests"
            echo "  cargo clippy   - Run linter"
            echo "  make           - Build release"
            echo "  make test_nvim - Run Neovim tests"
          '';

          # Environment variables
          RUST_BACKTRACE = "1";
          RUST_LOG = "info";
        };

        packages.default = pkgs.rustPlatform.buildRustPackage {
          pname = "treesitter-ls";
          version = "0.1.0";
          src = ./.;

          cargoLock = {
            lockFile = ./Cargo.lock;
          };

          nativeBuildInputs = with pkgs; [
            pkg-config
          ];

          buildInputs = with pkgs; [
            openssl
          ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin [
            pkgs.darwin.apple_sdk.frameworks.Security
            pkgs.darwin.apple_sdk.frameworks.SystemConfiguration
            pkgs.libiconv
          ];

          meta = with pkgs.lib; {
            description = "A Tree-sitter Language Server";
            homepage = "https://github.com/atusy/treesitter-ls";
            license = licenses.mit;
            maintainers = [ ];
          };
        };
      }
    );
}
