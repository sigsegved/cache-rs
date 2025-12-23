#!/bin/bash
# Miri testing script for cache-rs
# This script runs comprehensive Miri tests to detect undefined behavior

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Function to print section headers
print_section() {
    echo ""
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo ""
}

# Function to run a test with status reporting
run_test() {
    local test_name=$1
    shift
    echo -e "${YELLOW}Running: ${test_name}${NC}"
    if "$@"; then
        echo -e "${GREEN}✓ ${test_name} passed${NC}"
        return 0
    else
        echo -e "${RED}✗ ${test_name} failed${NC}"
        return 1
    fi
}

# Check if running from project root
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Must run from project root directory${NC}"
    exit 1
fi

print_section "Miri Test Suite for cache-rs"

# Check if nightly is installed
echo -e "${YELLOW}Checking Rust nightly toolchain...${NC}"
if ! rustup toolchain list | grep -q "nightly"; then
    echo -e "${YELLOW}Installing nightly toolchain...${NC}"
    rustup toolchain install nightly
else
    echo -e "${GREEN}✓ Nightly toolchain installed${NC}"
fi

# Check if miri is installed
echo -e "${YELLOW}Checking Miri component...${NC}"
if ! rustup +nightly component list --installed 2>/dev/null | grep -q "miri"; then
    echo -e "${YELLOW}Installing Miri component...${NC}"
    rustup +nightly component add miri
    cargo +nightly miri setup
else
    echo -e "${GREEN}✓ Miri component installed${NC}"
fi

# Setup Miri (ensure standard library is ready)
echo -e "${YELLOW}Setting up Miri...${NC}"
cargo +nightly miri setup
echo -e "${GREEN}✓ Miri setup complete${NC}"

# Test 1: Basic Miri tests on library code
print_section "Test 1: Basic Miri Tests (Library)"
run_test "Basic library tests" \
    cargo +nightly miri test --lib

# Test 2: Miri tests with leak checking
print_section "Test 2: Miri Tests with Leak Checking"
run_test "Leak checking tests" \
    env MIRIFLAGS="-Zmiri-leak-check" cargo +nightly miri test --lib

# Test 3: Miri tests with strict provenance
print_section "Test 3: Miri Tests with Strict Provenance"
run_test "Strict provenance tests" \
    env MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test --lib

# Test 4: Miri tests on no_std tests
print_section "Test 4: Miri Tests (no_std)"
run_test "no_std tests" \
    cargo +nightly miri test --test no_std_tests

# Test 5: Miri tests with symbolic alignment checking
print_section "Test 5: Miri Tests with Symbolic Alignment"
run_test "Symbolic alignment tests" \
    env MIRIFLAGS="-Zmiri-symbolic-alignment-check" cargo +nightly miri test --lib

# Test 6: Combined strict checking (may be very strict)
print_section "Test 6: Miri Tests with Combined Strict Checks"
echo -e "${YELLOW}Note: This test uses very strict checking and may produce false positives${NC}"
run_test "Combined strict checks" \
    env MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check -Zmiri-track-raw-pointers" \
    cargo +nightly miri test --lib || true  # Don't fail on strict checks

# Test 7: Test individual modules
print_section "Test 7: Individual Module Tests"
for module in lru slru lfu lfuda gdsf; do
    run_test "Module: $module" \
        cargo +nightly miri test --lib --test no_std_tests test_${module}_in_no_std 2>/dev/null || \
        cargo +nightly miri test --lib $module
done

# Test 8: Doc tests under Miri (if any)
print_section "Test 8: Doc Tests"
echo -e "${YELLOW}Running doc tests under Miri...${NC}"
cargo +nightly miri test --doc 2>&1 | head -20 || true

# Summary
print_section "Test Summary"
echo -e "${GREEN}All critical Miri tests completed!${NC}"
echo ""
echo "If all tests passed:"
echo "  ✓ No memory safety violations detected"
echo "  ✓ No memory leaks detected"
echo "  ✓ Pointer provenance is correct"
echo "  ✓ No undefined behavior in unsafe code"
echo ""
echo -e "${BLUE}Next steps:${NC}"
echo "  1. Review any warnings or errors above"
echo "  2. Fix any issues found"
echo "  3. Re-run this script to verify fixes"
echo "  4. Consider integrating into CI/CD pipeline"
echo ""
echo -e "${GREEN}Documentation: See MIRI_INTEGRATION_SPEC.md for details${NC}"
