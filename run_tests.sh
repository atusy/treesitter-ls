#!/bin/bash

# TDD-focused test runner for TreeSitter-LS
# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

# Test configuration
VERBOSE=${VERBOSE:-false}
FAIL_FAST=${FAIL_FAST:-false}
WATCH_MODE=${WATCH_MODE:-false}
TEST_FILTER=${TEST_FILTER:-""}

# Print banner
echo -e "${CYAN}===============================================${NC}"
echo -e "${CYAN}    TreeSitter-LS TDD Test Suite${NC}"
echo -e "${CYAN}===============================================${NC}"

# Function to run a test suite with proper error handling
run_test_suite() {
    local test_name="$1"
    local test_command="$2"
    local description="$3"
    
    echo -e "${BLUE}→ Running ${test_name}...${NC}"
    if [ "$VERBOSE" = "true" ]; then
        echo -e "${YELLOW}  Command: ${test_command}${NC}"
    fi
    
    if eval "$test_command"; then
        echo -e "${GREEN}✓ ${description} passed${NC}"
        return 0
    else
        echo -e "${RED}✗ ${description} failed${NC}"
        if [ "$FAIL_FAST" = "true" ]; then
            echo -e "${RED}Stopping due to FAIL_FAST=true${NC}"
            exit 1
        fi
        return 1
    fi
}

# Function to check prerequisites
check_prerequisites() {
    echo -e "${YELLOW}Checking prerequisites...${NC}"
    
    local all_good=true
    
    # Check tree-sitter library
    if [ ! -f "./tree-sitter-rust/tree-sitter-rust/libtree_sitter_rust.dylib" ]; then
        echo -e "${RED}✗ Tree-sitter Rust library not found${NC}"
        echo -e "${YELLOW}  Run: cd tree-sitter-rust && cargo build --release${NC}"
        all_good=false
    else
        echo -e "${GREEN}✓ Tree-sitter Rust library found${NC}"
    fi
    
    # Check highlights file
    if [ ! -f "./highlights.scm" ]; then
        echo -e "${RED}✗ highlights.scm not found${NC}"
        echo -e "${YELLOW}  Please ensure highlights.scm is present in project root${NC}"
        all_good=false
    else
        echo -e "${GREEN}✓ highlights.scm found${NC}"
    fi
    
    # Check Rust toolchain
    if ! command -v cargo &> /dev/null; then
        echo -e "${RED}✗ Cargo not found${NC}"
        all_good=false
    else
        echo -e "${GREEN}✓ Cargo found${NC}"
    fi
    
    if [ "$all_good" = "false" ]; then
        echo -e "${RED}Prerequisites not met. Please fix the issues above.${NC}"
        exit 1
    fi
    
    echo -e "${GREEN}All prerequisites met!${NC}"
}

# Function to run code quality checks
run_quality_checks() {
    echo -e "${BLUE}Running code quality checks...${NC}"
    
    local quality_passed=true
    
    # Check formatting
    if ! run_test_suite "fmt_check" "cargo fmt --check" "Code formatting"; then
        quality_passed=false
    fi
    
    # Run clippy
    if ! run_test_suite "clippy" "cargo clippy -- -D warnings" "Linting (clippy)"; then
        quality_passed=false
    fi
    
    # Quick compilation check
    if ! run_test_suite "check" "cargo check" "Compilation check"; then
        quality_passed=false
    fi
    
    if [ "$quality_passed" = "true" ]; then
        echo -e "${GREEN}✓ All quality checks passed${NC}"
        return 0
    else
        echo -e "${YELLOW}Some quality checks failed${NC}"
        return 1
    fi
}

# Function to run all tests by category
run_all_tests() {
    echo -e "${BLUE}Running comprehensive test suite...${NC}"
    
    local tests_passed=0
    local tests_total=0
    
    # Build project first
    echo -e "${YELLOW}Building project...${NC}"
    if ! cargo build; then
        echo -e "${RED}Build failed - cannot run tests${NC}"
        exit 1
    fi
    echo -e "${GREEN}✓ Build successful${NC}"
    
    # Unit tests (in src/simple_tests.rs)
    tests_total=$((tests_total + 1))
    if run_test_suite "unit_tests" "cargo test --lib ${TEST_FILTER} -- --nocapture" "Unit tests"; then
        tests_passed=$((tests_passed + 1))
    fi
    
    # Integration tests (tests/logic_tests.rs)
    tests_total=$((tests_total + 1))
    if run_test_suite "integration_tests" "cargo test --test logic_tests ${TEST_FILTER} -- --nocapture" "Integration tests"; then
        tests_passed=$((tests_passed + 1))
    fi
    
    # Behavioral tests (tests/behavior_tests.rs)
    tests_total=$((tests_total + 1))
    if run_test_suite "behavior_tests" "cargo test --test behavior_tests ${TEST_FILTER} -- --nocapture" "Behavioral tests"; then
        tests_passed=$((tests_passed + 1))
    fi
    
    # Test helpers (tests/test_helpers.rs)
    tests_total=$((tests_total + 1))
    if run_test_suite "helper_tests" "cargo test --test test_helpers ${TEST_FILTER} -- --nocapture" "Test helper validation"; then
        tests_passed=$((tests_passed + 1))
    fi
    
    # Performance tests with release build
    tests_total=$((tests_total + 1))
    if run_test_suite "performance_tests" "cargo test --release performance ${TEST_FILTER} -- --nocapture" "Performance tests"; then
        tests_passed=$((tests_passed + 1))
    fi
    
    # Print summary
    echo -e "${CYAN}===============================================${NC}"
    echo -e "${CYAN}Test Summary: ${tests_passed}/${tests_total} test suites passed${NC}"
    
    if [ "$tests_passed" -eq "$tests_total" ]; then
        echo -e "${GREEN}🎉 All test suites passed!${NC}"
        return 0
    else
        echo -e "${RED}❌ Some test suites failed${NC}"
        return 1
    fi
}

# Function to watch for changes and re-run tests
watch_tests() {
    echo -e "${YELLOW}Starting watch mode...${NC}"
    echo -e "${YELLOW}Watching for changes in src/ and tests/${NC}"
    echo -e "${YELLOW}Press Ctrl+C to stop${NC}"
    
    if ! command -v inotifywait &> /dev/null; then
        echo -e "${RED}inotifywait not found. Install inotify-tools for watch mode.${NC}"
        echo -e "${YELLOW}Falling back to simple loop...${NC}"
        
        while true; do
            run_all_tests
            echo -e "${YELLOW}Waiting 5 seconds...${NC}"
            sleep 5
        done
    else
        while true; do
            run_all_tests
            echo -e "${YELLOW}Waiting for file changes...${NC}"
            inotifywait -r -e modify,create,delete src/ tests/ 2>/dev/null
            echo -e "${YELLOW}Changes detected, re-running tests...${NC}"
        done
    fi
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --verbose|-v)
            VERBOSE=true
            shift
            ;;
        --fail-fast|-f)
            FAIL_FAST=true
            shift
            ;;
        --watch|-w)
            WATCH_MODE=true
            shift
            ;;
        --filter)
            TEST_FILTER="$2"
            shift 2
            ;;
        --quality-only|-q)
            check_prerequisites
            run_quality_checks
            exit $?
            ;;
        --help|-h)
            echo "TDD Test Runner for TreeSitter-LS"
            echo ""
            echo "Usage: $0 [options]"
            echo ""
            echo "Options:"
            echo "  --verbose, -v        Show detailed output"
            echo "  --fail-fast, -f      Stop on first failure"
            echo "  --watch, -w          Watch for changes and re-run tests"
            echo "  --filter PATTERN     Only run tests matching pattern"
            echo "  --quality-only, -q   Only run code quality checks"
            echo "  --help, -h           Show this help"
            echo ""
            echo "Environment variables:"
            echo "  VERBOSE=true         Enable verbose output"
            echo "  FAIL_FAST=true       Stop on first failure"
            echo "  TEST_FILTER=pattern  Filter tests by pattern"
            exit 0
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            echo "Use --help for usage information"
            exit 1
            ;;
    esac
done

# Main execution
check_prerequisites

# Run quality checks first
if ! run_quality_checks && [ "$FAIL_FAST" = "true" ]; then
    exit 1
fi

# Run tests
if [ "$WATCH_MODE" = "true" ]; then
    watch_tests
else
    run_all_tests
    exit $?
fi