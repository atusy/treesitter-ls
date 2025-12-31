# Makefile for treesitter-ls

# Variables
CARGO = cargo
TARGET_DIR = target
RELEASE_DIR = $(TARGET_DIR)/release
BINARY_NAME = treesitter-ls
RELEASE_BINARY = $(RELEASE_DIR)/$(BINARY_NAME)

# Default target
.PHONY: all
all: build

# Build the project in release mode
.PHONY: build
build:
	$(CARGO) build --release

build-debug:
	$(CARGO) build

# Build the project in debug mode
.PHONY: debug
debug:
	$(CARGO) build

# Clean build artifacts
.PHONY: clean
clean:
	$(CARGO) clean

# Run tests
.PHONY: test
test:
	$(CARGO) test

# Check code formatting and linting
.PHONY: check
check:
	$(CARGO) check
	$(CARGO) clippy -- -D warnings
	$(CARGO) fmt --check

# Format code
.PHONY: fmt
format:
	$(CARGO) fmt

# Run linter (clippy)
.PHONY: lint
lint:
	$(CARGO) clippy -- -D warnings

# Install the binary to ~/.cargo/bin
.PHONY: install
install: build
	$(CARGO) install --path .

# Run all test files
test_nvim: deps build-debug
	nvim -es --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()"

# Run test from file at `$FILE` environment variable
test_nvim_file: deps build-debug
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')"

# Download 'mini.nvim' to use its 'mini.test' testing module
deps: deps/nvim deps/treesitter

deps/nvim: build-debug deps/nvim/mini.nvim deps/nvim/nvim-treesitter deps/nvim/catppuccin

deps/nvim/mini.nvim:
	git clone --filter=blob:none https://github.com/nvim-mini/mini.nvim $@

deps/nvim/nvim-treesitter:
	git clone --filter=blob:none https://github.com/nvim-treesitter/nvim-treesitter --branch main $@

deps/nvim/catppuccin:
	git clone --filter=blob:none https://github.com/catppuccin/nvim $@

target/debug/treesitter-ls: build-debug

deps/treesitter/.installed: target/debug/treesitter-ls
	@mkdir -p deps/treesitter
	./target/debug/treesitter-ls language install lua --data-dir deps/treesitter
	./target/debug/treesitter-ls language install luadoc --data-dir deps/treesitter
	./target/debug/treesitter-ls language install rust --data-dir deps/treesitter
	./target/debug/treesitter-ls language install markdown --data-dir deps/treesitter
	./target/debug/treesitter-ls language install markdown_inline --data-dir deps/treesitter
	./target/debug/treesitter-ls language install yaml --data-dir deps/treesitter
	./target/debug/treesitter-ls language install r --data-dir deps/treesitter
	@touch $@

deps/treesitter: deps/treesitter/.installed

deps/vim/prabirshrestha/vim-lsp:
	git clone --filter=blob:none https://github.com/prabirshrestha/vim-lsp $@

deps/vim: build-debug deps/vim/prabirshrestha/vim-lsp

# Show help
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build         - Build the project in release mode (default)"
	@echo "  debug         - Build the project in debug mode"
	@echo "  clean         - Clean build artifacts"
	@echo "  test          - Run tests"
	@echo "  check         - Run code checks (clippy, fmt)"
	@echo "  format        - Format code with rustfmt"
	@echo "  lint          - Run clippy linter"
	@echo "  install       - Install binary to ~/.cargo/bin"
	@echo "  test_nvim     - Run all Neovim test files"
	@echo "  test_nvim_file - Run test from file at \$$FILE environment variable"
	@echo "  deps          - Download dependencies for Neovim testing"
	@echo "  verify        - Check if the binary exists"
	@echo "  help          - Show this help message"

# Check if the binary exists
.PHONY: verify
verify:
	@if [ -f "$(RELEASE_BINARY)" ]; then \
		echo "✅ Binary exists at $(RELEASE_BINARY)"; \
		ls -la "$(RELEASE_BINARY)"; \
	else \
		echo "❌ Binary not found at $(RELEASE_BINARY)"; \
		echo "Run 'make build' to create it."; \
	fi
