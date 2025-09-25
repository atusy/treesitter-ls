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
test_nvim: deps
	nvim -es --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run()"

# Run test from file at `$FILE` environment variable
test_nvim_file: deps
	nvim --headless --noplugin -u ./scripts/minimal_init.lua -c "lua MiniTest.run_file('$(FILE)')"

# Download 'mini.nvim' to use its 'mini.test' testing module
deps: deps/nvim deps/treesitter

deps/nvim: deps/nvim/mini.nvim deps/nvim/nvim-treesitter

deps/nvim/mini.nvim:
	git clone --filter=blob:none https://github.com/nvim-mini/mini.nvim $@

deps/nvim/nvim-treesitter:
	git clone --filter=blob:none https://github.com/nvim-treesitter/nvim-treesitter --branch main $@

deps/treesitter: deps/nvim/nvim-treesitter
	@mkdir -p $@
	nvim -n --clean --headless --cmd "lua (function() vim.opt.rtp:append(vim.uv.cwd() .. '/deps/nvim/nvim-treesitter'); require('nvim-treesitter').setup({ install_dir = vim.uv.cwd() .. '/deps/treesitter' }); require('nvim-treesitter').install({ 'lua', 'luadoc', 'rust', 'markdown', 'markdown_inline', 'yaml' }):wait(300000); vim.cmd.q() end)()"


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
