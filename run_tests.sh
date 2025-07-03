#!/bin/bash

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== TreeSitter-LS Test Suite ===${NC}"

# Check if tree-sitter library exists
if [ ! -f "./tree-sitter-rust/tree-sitter-rust/libtree_sitter_rust.dylib" ]; then
    echo -e "${RED}Error: Tree-sitter Rust library not found${NC}"
    echo "Please build the tree-sitter library first:"
    echo "  cd tree-sitter-rust && cargo build --release"
    exit 1
fi

# Check if highlights.scm exists
if [ ! -f "./highlights.scm" ]; then
    echo -e "${RED}Error: highlights.scm not found${NC}"
    echo "Please ensure highlights.scm is present in the project root"
    exit 1
fi

echo -e "${YELLOW}Building project...${NC}"
if ! cargo build; then
    echo -e "${RED}Build failed${NC}"
    exit 1
fi

echo -e "${GREEN}Build successful${NC}"

echo -e "${YELLOW}Running unit tests...${NC}"
if cargo test --lib -- --nocapture; then
    echo -e "${GREEN}Unit tests passed${NC}"
else
    echo -e "${RED}Unit tests failed${NC}"
    exit 1
fi

echo -e "${YELLOW}Running logic tests...${NC}"
if cargo test --test logic_tests -- --nocapture; then
    echo -e "${GREEN}Logic tests passed${NC}"
else
    echo -e "${RED}Logic tests failed${NC}"
    exit 1
fi

echo -e "${GREEN}=== All tests passed! ===${NC}"

# Optional: Run with release build for better performance
echo -e "${YELLOW}Running tests with release build...${NC}"
if cargo test --release -- --nocapture; then
    echo -e "${GREEN}Release tests passed${NC}"
else
    echo -e "${YELLOW}Release tests had issues (non-critical)${NC}"
fi

echo -e "${GREEN}=== Test suite completed successfully! ===${NC}"