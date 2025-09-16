# Contributing to treesitter-ls

Thank you for your interest in contributing to treesitter-ls! This document provides guidelines and information for contributors.

## Table of Contents

- [Development Setup](#development-setup)
- [Architecture Overview](#architecture-overview)
- [Directory Structure](#directory-structure)
- [Development Workflow](#development-workflow)
- [Testing Guidelines](#testing-guidelines)
- [Code Style](#code-style)
- [Commit Guidelines](#commit-guidelines)
- [Adding New Features](#adding-new-features)

## Development Setup

### Prerequisites

- Rust (latest stable version)
- Cargo
- Tree-sitter CLI (optional, for grammar development)

### Building the Project

```bash
# Build in debug mode
cargo build

# Build in release mode
cargo build --release
# or
make build
```

### Running Tests

```bash
# Run all tests
cargo test
# or
make test

# Run with formatting and linting
make test format lint

# Run specific test
cargo test test_name

# Run tests with output
cargo test -- --nocapture
```

## Architecture Overview

treesitter-ls follows a **vertical slice architecture** where each module is responsible for a complete feature area. This design was chosen to avoid circular dependencies and maintain clear separation of concerns.

### Design Principles

1. **Single Responsibility**: Each module has one clear purpose
2. **Vertical Slices**: Features are self-contained within their modules
3. **Unidirectional Dependencies**: Higher-level modules depend on lower-level ones, never the reverse
4. **No Circular Dependencies**: Strict dependency hierarchy is maintained

### Key Architectural Decisions

#### Why Vertical Slices?

The initial codebase had several problems:
- Circular dependencies between `state/` and `treesitter/` modules
- Upward dependencies (e.g., `layers/` depending on `state/`)
- Scattered responsibilities across multiple modules
- "God objects" with too many responsibilities

The vertical slice architecture solves these by:
- Grouping related functionality together
- Making dependencies explicit and unidirectional
- Making it easy to find and modify features
- Simplifying testing by isolating concerns

## Directory Structure

```
src/
├── analysis/       # LSP feature implementations (vertical slice)
│   ├── definition.rs   # Go-to-definition functionality
│   ├── refactor.rs     # Code actions (parameter reordering)
│   ├── selection.rs    # Selection range expansion
│   ├── semantic.rs     # Semantic token highlighting
│   └── traits.rs       # AnalysisContext trait for dependency inversion
│
├── bin/            # Binary entry point
│   └── main.rs         # Application startup and initialization
│
├── config/         # Configuration management
│   └── settings.rs     # LSP initialization options parsing
│
├── document/       # Document management (vertical slice)
│   ├── injection_mapper.rs  # Coordinate mapping for language injections
│   ├── layer.rs        # LanguageLayer structure for parsed trees
│   ├── layer_manager.rs # Root and injection layer management
│   ├── model.rs        # Unified Document struct (implements AnalysisContext)
│   └── store.rs        # Thread-safe document storage with DashMap
│
├── language/       # Language service (vertical slice)
│   ├── config_store.rs       # Language configuration management
│   ├── filetype_resolver.rs  # File type to language mapping
│   ├── language_coordinator.rs # Stateless coordination between modules
│   ├── loader.rs             # Dynamic parser loading (.so/.dylib files)
│   ├── parser_pool.rs        # Parser pooling and factory
│   ├── query.rs              # Tree-sitter query execution
│   ├── query_loader.rs       # Query file loading from disk
│   ├── query_store.rs        # Query storage and retrieval
│   └── registry.rs           # Language registry
│
├── lsp/            # LSP server implementation
│   └── lsp_impl.rs     # LSP protocol handler and orchestration
│
└── text/           # Text manipulation utilities
    ├── edits.rs        # Text edit operations and range adjustments
    └── position.rs     # Coordinate conversions (byte ↔ Position)
```

### Module Responsibilities

#### `analysis/` - LSP Features
Each file implements a complete LSP feature:
- **definition.rs**: Handles `textDocument/definition` requests using Tree-sitter locals queries
- **semantic.rs**: Provides syntax highlighting via `textDocument/semanticTokens`
- **selection.rs**: Implements `textDocument/selectionRange` for smart selection
- **refactor.rs**: Code actions like parameter reordering

#### `document/` - Document Management
Manages document lifecycle and language layers:
- **model.rs**: Unified `Document` struct implementing `AnalysisContext`
- **layer.rs**: `LanguageLayer` structure for managing parsed trees
- **layer_manager.rs**: Manages root and injection layers
- **store.rs**: Thread-safe document storage with `DashMap`
- **injection_mapper.rs**: Handles coordinate mapping for language injections

#### `language/` - Language Services
Language configuration and parsing:
- **language_coordinator.rs**: Stateless coordination between language modules
- **config_store.rs**: Language configuration management
- **filetype_resolver.rs**: File type to language mapping
- **query_store.rs**: Query storage and retrieval
- **query_loader.rs**: Query file loading from disk
- **registry.rs**: Language registry
- **parser_pool.rs**: Parser pooling for efficient reuse
- **loader.rs**: Dynamic parser library loading
- **query.rs**: Tree-sitter query execution

#### `text/` - Text Operations
Text manipulation utilities:
- **edits.rs**: Text edit operations and range adjustments
- **position.rs**: Coordinate conversions between byte/line-column positions

### Design Rationale

#### Clean Architecture Implementation
The refactoring implemented clean architecture principles:
- **Dependency Inversion**: `AnalysisContext` trait allows analysis module to work without direct `Document` dependency
- **Module Boundaries**: Clear separation between document management, language services, and text operations
- **Coordinator Pattern**: `LanguageCoordinator` provides stateless coordination between modules

#### Injection System Design
Language injections (like code blocks in Markdown) are handled by:
- `document/layer_manager.rs`: Manages injection layers
- `document/injection_mapper.rs`: Coordinate mapping between layers

This separation ensures each module maintains its single responsibility.

#### Parser Pool Unification
We unified parser management into a single `DocumentParserPool` to:
- Avoid duplicate parser instances
- Reduce memory usage
- Simplify parser lifecycle management

## Development Workflow

### Adding a New LSP Feature

1. Create a new file in `src/analysis/`
2. Implement the feature handler
3. Add the handler to `src/lsp/lsp_impl.rs`
4. Write tests in the same file or in `tests/`

Example structure for a new feature:
```rust
// src/analysis/my_feature.rs
pub fn handle_my_feature(
    document: &StatefulDocument,
    params: MyFeatureParams,
) -> Option<MyFeatureResponse> {
    // Implementation
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_my_feature() {
        // Test implementation
    }
}
```

### Adding Language Support

1. Obtain or build the Tree-sitter parser library (.so/.dylib)
2. Create query files:
   - `queries/<language>/highlights.scm` for syntax highlighting
   - `queries/<language>/locals.scm` for go-to-definition
3. Configure the language in initialization options:
```json
{
  "languages": {
    "my_lang": {
      "filetypes": ["ml", "mli"],
      "library": "/path/to/my_lang.so"
    }
  }
}
```

## Testing Guidelines

### Test Organization

- **Unit tests**: Colocated with implementation in `#[cfg(test)]` modules
- **Integration tests**: In the `tests/` directory
- **Test utilities**: Shared test helpers in test modules

### Writing Tests

Follow the TDD approach:
1. Write a failing test that defines the desired behavior
2. Implement the minimum code to make it pass
3. Refactor while keeping tests green

Example test structure:
```rust
#[test]
fn test_semantic_tokens_with_injections() {
    // Arrange
    let text = "fn main() { /* comment */ }";
    let document = create_test_document(text);

    // Act
    let tokens = handle_semantic_tokens(&document);

    // Assert
    assert!(tokens.is_some());
    assert_eq!(tokens.unwrap().data.len(), expected_count);
}
```

### Running Specific Tests

```bash
# Run tests for a specific module
cargo test analysis::

# Run a single test
cargo test test_semantic_tokens_with_injections

# Run tests with debug output
cargo test -- --nocapture --test-threads=1
```

## Code Style

### Rust Guidelines

- Follow standard Rust naming conventions
- Use `rustfmt` for formatting (`cargo fmt`)
- Use `clippy` for linting (`cargo clippy -- -D warnings`)
- Prefer explicit types for public APIs
- Document public items with doc comments

### Error Handling Guidelines

This project follows strict error handling practices to ensure robustness:

#### Core Principles
1. **No unwrap() on lock operations**: All mutex/RwLock operations use proper error handling with poison recovery
2. **No panic! in production code**: Use Result types and proper error propagation instead
3. **Custom error types**: Use the `LspError` enum from `src/error.rs` for type-safe error handling
4. **Logging on recovery**: When recovering from poisoned locks, log warnings using the `log` crate

#### Lock Handling Patterns

```rust
use log::warn;

// ✅ Good - with poison recovery
match self.data.lock() {
    Ok(guard) => guard.get(key).cloned(),
    Err(poisoned) => {
        warn!(
            target: "treesitter_ls::lock_recovery",
            "Recovered from poisoned lock in module::function"
        );
        poisoned.into_inner().get(key).cloned()
    }
}

// ❌ Bad - will panic on poisoned lock
self.data.lock().unwrap().get(key).cloned()
```

#### Error Type Usage

```rust
use crate::error::{LspError, LspResult};

// Function returning a Result
pub fn process_document(uri: &str) -> LspResult<Document> {
    let doc = store.get(uri)
        .ok_or_else(|| LspError::document_not_found(uri))?;

    // Process document...
    Ok(doc)
}
```

#### Testing Poison Recovery

All lock-based modules include tests for poison recovery. See `tests/test_poison_recovery.rs` for examples.

### Code Organization

- Keep modules focused on a single responsibility
- Prefer composition over inheritance
- Use the type system to make illegal states unrepresentable
- Minimize mutable state

### Error Handling

- Use `Option` for operations that might not produce a value
- Use `Result` for operations that might fail with an error
- Provide context in error messages
- Avoid `unwrap()` except in tests or when impossible to fail

## Commit Guidelines

### Commit Message Format

```
<type>(<scope>): <brief description>

<detailed description>

<footer>
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `refactor`: Code restructuring without behavior change
- `test`: Test additions or modifications
- `docs`: Documentation changes
- `perf`: Performance improvements
- `chore`: Maintenance tasks

Example:
```
refactor(document): consolidate coordinate mapping

Move all coordinate conversion logic to document/coordinates.rs
to maintain single responsibility and avoid duplication.

All tests passing.
```

### Commit Best Practices

1. Make atomic commits (one logical change per commit)
2. Ensure all tests pass before committing
3. Run `make test format lint` before committing
4. Write clear, descriptive commit messages
5. Reference issues when applicable

## Adding New Features

### Checklist for New Features

- [ ] Design follows vertical slice architecture
- [ ] No circular dependencies introduced
- [ ] Unit tests written and passing
- [ ] Integration tests added if applicable
- [ ] Documentation updated
- [ ] Code formatted with `rustfmt`
- [ ] No `clippy` warnings
- [ ] Commit message follows guidelines

### Feature Development Process

1. **Design Phase**
   - Identify which vertical slice the feature belongs to
   - Define clear interfaces and dependencies
   - Consider injection and multi-language support

2. **Implementation Phase**
   - Write tests first (TDD)
   - Implement incrementally
   - Keep commits atomic and well-documented

3. **Review Phase**
   - Self-review for code quality
   - Ensure tests cover edge cases
   - Update documentation

### Performance Considerations

- Use parser pools to avoid recreating parsers
- Cache query compilation results
- Be mindful of large document performance
- Profile before optimizing

## Questions and Support

If you have questions about contributing:

1. Check existing issues and discussions
2. Look at similar features for examples
3. Open an issue for design discussions
4. Ask in pull request comments

Thank you for contributing to treesitter-ls!