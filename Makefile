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

# Install the binary to ~/.cargo/bin
.PHONY: install
install: build
	$(CARGO) install --path .

# Show help
.PHONY: help
help:
	@echo "Available targets:"
	@echo "  build    - Build the project in release mode (default)"
	@echo "  debug    - Build the project in debug mode"
	@echo "  clean    - Clean build artifacts"
	@echo "  test     - Run tests"
	@echo "  check    - Run code checks (clippy, fmt)"
	@echo "  format   - Format code with rustfmt"
	@echo "  install  - Install binary to ~/.cargo/bin"
	@echo "  help     - Show this help message"

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
