# Testing Guide for TreeSitter-LS

This document describes the comprehensive test suite for treesitter-ls and how to run it.

## Prerequisites

Before running tests, ensure you have:

1. **Tree-sitter Rust library built:**
   ```bash
   cd tree-sitter-rust
   cargo build --release
   ```

2. **Highlights configuration file:**
   - `highlights.scm` should be present in the project root

## Test Structure

The test suite is organized into several categories:

### 1. Unit Tests (`src/tests.rs`)

Tests individual components of the language server:

- **Symbol Indexing Tests:**
  - `test_symbol_indexing_functions` - Tests function definition indexing
  - `test_symbol_indexing_structs_and_enums` - Tests type definition indexing  
  - `test_symbol_indexing_local_variables` - Tests local variable indexing
  - `test_symbol_indexing_constants_and_statics` - Tests constant indexing

- **Pattern Extraction Tests:**
  - `test_pattern_extraction` - Tests extraction of identifiers from various patterns
  - `test_get_symbol_at_position` - Tests symbol resolution at cursor positions

- **Configuration Tests:**
  - `test_configuration_parsing` - Tests parsing of TreeSitterSettings
  - `test_highlight_query_loading` - Tests loading highlight queries from files/strings

- **Semantic Token Tests:**
  - `test_semantic_tokens_generation` - Tests basic semantic token generation
  - `test_semantic_tokens_with_multiple_types` - Tests complex semantic tokens
  - `test_semantic_tokens_empty_file` - Tests edge case handling
  - `test_semantic_tokens_unsupported_language` - Tests error handling

- **Utility Tests:**
  - `test_node_to_range_conversion` - Tests LSP range conversion
  - `test_get_language_for_document` - Tests language detection
  - `test_multiple_files_symbol_isolation` - Tests cross-file symbol handling

### 2. Integration Tests (`tests/integration_tests.rs`)

Tests full LSP protocol interactions:

- **Definition Jumping Tests:**
  - `test_lsp_definition_jumping_local_variables` - Tests jumping to local variable definitions
  - `test_lsp_definition_jumping_functions` - Tests jumping to function definitions
  - `test_lsp_definition_jumping_structs` - Tests jumping to struct definitions
  - `test_lsp_definition_jumping_no_symbol` - Tests handling when no symbol found
  - `test_lsp_definition_jumping_undefined_symbol` - Tests undefined symbol handling

- **Document Lifecycle Tests:**
  - `test_lsp_document_changes` - Tests document updates and re-indexing

### 3. Performance Tests (`tests/performance_tests.rs`)

Tests performance characteristics and scalability:

- **Parsing Performance:**
  - `test_large_file_parsing_performance` - Tests parsing of large files (100+ functions)
  - `test_semantic_tokens_performance` - Tests semantic token generation speed

- **Request Performance:**
  - `test_multiple_definition_requests_performance` - Tests definition request latency
  - `test_document_update_performance` - Tests document update speed

- **Memory Usage:**
  - `test_memory_usage_with_many_symbols` - Tests memory efficiency with many symbols

## Running Tests

### Quick Test Run
```bash
# Run all tests
cargo test

# Run only unit tests
cargo test --lib

# Run only integration tests  
cargo test --test integration_tests

# Run only performance tests
cargo test --test performance_tests
```

### Using the Test Runner Script
```bash
# Make executable (first time only)
chmod +x run_tests.sh

# Run full test suite with colored output
./run_tests.sh
```

The test runner script will:
1. Verify prerequisites (tree-sitter library and highlights.scm)
2. Build the project
3. Run unit tests
4. Run integration tests
5. Run performance tests
6. Run performance tests with release build

### Running Specific Tests
```bash
# Run a specific test
cargo test test_symbol_indexing_functions

# Run tests with output
cargo test -- --nocapture

# Run tests in release mode (for performance)
cargo test --release
```

## Test Configuration

Tests use a mock TreeSitterSettings configuration:

```json
{
  "treesitter": {
    "rust": {
      "library": "./tree-sitter-rust/tree-sitter-rust/libtree_sitter_rust.dylib",
      "highlight": [
        {"path": "./highlights.scm"}
      ]
    }
  },
  "filetypes": {
    "rust": ["rs"]
  }
}
```

## Expected Test Results

### Unit Tests
- Should pass completely with the tree-sitter library installed
- Tests cover core functionality like symbol indexing and semantic tokens

### Integration Tests  
- Test full LSP request/response cycles
- Verify definition jumping works for various symbol types
- Test document lifecycle management

### Performance Tests
- Parsing: < 1000ms for files with 100+ functions
- Semantic tokens: < 500ms for medium-sized files  
- Definition requests: < 10ms average
- Document updates: < 200ms average

## Troubleshooting

### Common Issues

1. **Tree-sitter library not found:**
   ```
   Error: Failed to load library ./tree-sitter-rust/...
   ```
   **Solution:** Build the tree-sitter library:
   ```bash
   cd tree-sitter-rust && cargo build --release
   ```

2. **Highlights file not found:**
   ```
   Error: Failed to read query file ./highlights.scm
   ```
   **Solution:** Ensure `highlights.scm` exists in project root

3. **Tests timeout:**
   **Solution:** Run with release build for better performance:
   ```bash
   cargo test --release
   ```

### Debug Mode

For detailed test output:
```bash
RUST_LOG=debug cargo test -- --nocapture
```

## Adding New Tests

When adding new functionality, add corresponding tests:

1. **Unit tests** in `src/tests.rs` for individual components
2. **Integration tests** in `tests/integration_tests.rs` for LSP interactions  
3. **Performance tests** in `tests/performance_tests.rs` for scalability

Follow the existing patterns:
- Use `setup_rust_language()` helper for test setup
- Create descriptive test names  
- Test both success and error cases
- Include performance assertions for critical paths