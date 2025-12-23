# Miri Integration Specification for cache-rs

## Executive Summary

This document outlines a comprehensive plan for integrating [Miri](https://github.com/rust-lang/miri) into the cache-rs project to detect undefined behavior in unsafe code. Miri is Rust's interpreter for detecting memory safety violations, data races, and other undefined behavior at runtime.

## Why Miri for cache-rs?

### Current State
- **111 unsafe blocks** across the codebase
- Heavy use of raw pointers for doubly-linked list operations
- Manual memory management with `Box::into_raw()` and `Box::from_raw()`
- Complex pointer arithmetic and node manipulation
- Critical safety invariants that must be maintained

### Benefits of Miri Integration
1. **Detect Memory Safety Violations**: Identify use-after-free, double-free, and invalid pointer dereferences
2. **Validate Safety Comments**: Verify that SAFETY comments accurately describe the invariants
3. **Catch Data Races**: Detect potential race conditions in unsafe code
4. **Ensure no_std Compatibility**: Verify that unsafe code works correctly in no_std environments
5. **Continuous Validation**: Prevent regressions in memory safety through CI integration

### What Miri Can Detect
- Use-after-free
- Double-free
- Memory leaks (with `-Zmiri-leak-check`)
- Invalid pointer arithmetic
- Unaligned pointer access
- Reads of uninitialized memory
- Violations of pointer aliasing rules (Stacked Borrows)
- Data races (with `-Zmiri-track-raw-pointers`)

## Local Development Workflow

### Prerequisites

```bash
# Install Rust nightly toolchain (required for Miri)
rustup toolchain install nightly

# Install Miri component
rustup +nightly component add miri

# Setup Miri (downloads pre-built standard library)
cargo +nightly miri setup
```

### Basic Usage

#### Running All Tests with Miri
```bash
# Run all tests under Miri
cargo +nightly miri test

# Run tests with leak checking enabled
cargo +nightly miri test --features hashbrown -- -Zmiri-leak-check

# Run tests with detailed output
cargo +nightly miri test --verbose
```

#### Running Specific Tests
```bash
# Test a specific cache implementation
cargo +nightly miri test --lib lru

# Test a specific test function
cargo +nightly miri test --test no_std_tests test_lru_in_no_std

# Run doc tests under Miri
cargo +nightly miri test --doc
```

#### Advanced Miri Flags

```bash
# Enable strict provenance checking (recommended)
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test

# Enable symbolic alignment checking
MIRIFLAGS="-Zmiri-symbolic-alignment-check" cargo +nightly miri test

# Track raw pointer tags for better diagnostics
MIRIFLAGS="-Zmiri-track-raw-pointers" cargo +nightly miri test

# Disable isolation to allow file system access (if needed)
MIRIFLAGS="-Zmiri-disable-isolation" cargo +nightly miri test

# Combination of recommended flags
MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check" cargo +nightly miri test
```

### Creating a Miri Test Configuration

Create `miri-test.sh` in project root:

```bash
#!/bin/bash
set -e

echo "Running Miri tests for cache-rs"
echo "================================"
echo ""

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Check if nightly is installed
if ! rustup toolchain list | grep -q "nightly"; then
    echo -e "${YELLOW}Installing nightly toolchain...${NC}"
    rustup toolchain install nightly
fi

# Check if miri is installed
if ! rustup +nightly component list --installed | grep -q "miri"; then
    echo -e "${YELLOW}Installing Miri component...${NC}"
    rustup +nightly component add miri
    cargo +nightly miri setup
fi

echo -e "${GREEN}Running basic Miri tests...${NC}"
cargo +nightly miri test --lib

echo ""
echo -e "${GREEN}Running Miri tests with leak checking...${NC}"
MIRIFLAGS="-Zmiri-leak-check" cargo +nightly miri test --lib

echo ""
echo -e "${GREEN}Running Miri tests with strict provenance...${NC}"
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test --lib

echo ""
echo -e "${GREEN}Running no_std tests under Miri...${NC}"
cargo +nightly miri test --test no_std_tests

echo ""
echo -e "${GREEN}All Miri tests passed!${NC}"
```

### Miri Configuration File

Create `.miri-config.toml` in project root (optional):

```toml
# Miri configuration for cache-rs
# Note: This is a proposed format; actual implementation may vary

[miri]
# Enable strict provenance checking
strict-provenance = true

# Enable symbolic alignment checking
symbolic-alignment-check = true

# Enable leak checking
leak-check = true

# Track raw pointers for better diagnostics
track-raw-pointers = true

# Disable features that Miri doesn't support
# (Miri runs in a limited environment)
disable-isolation = false

[env]
# Set environment variables for tests
RUST_BACKTRACE = "1"
```

### Development Workflow Integration

1. **Before Committing**: Run basic Miri tests
   ```bash
   cargo +nightly miri test --lib
   ```

2. **Before PR**: Run comprehensive Miri tests
   ```bash
   ./miri-test.sh
   ```

3. **After Modifying Unsafe Code**: Run focused Miri tests
   ```bash
   cargo +nightly miri test --lib <module_name>
   ```

## CI/CD Integration

### GitHub Actions Workflow

Create `.github/workflows/miri.yml`:

```yaml
name: Miri

on:
  push:
    branches:
      - main
      - develop
      - release/*
      - feature/*
  pull_request:
    branches:
      - main
      - develop
      - release/*
      - feature/*
  schedule:
    # Run Miri weekly on Sundays at 00:00 UTC
    # Helpful for catching issues with nightly changes
    - cron: '0 0 * * 0'

jobs:
  miri:
    name: Miri
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: nightly
          components: miri

      - name: Setup Miri
        run: cargo miri setup

      - name: Run Miri tests
        run: |
          # Run tests with basic checks
          cargo miri test --lib
          
          # Run tests with leak checking
          MIRIFLAGS="-Zmiri-leak-check" cargo miri test --lib
          
          # Run tests with strict provenance
          MIRIFLAGS="-Zmiri-strict-provenance" cargo miri test --lib

      - name: Run Miri on no_std tests
        run: cargo miri test --test no_std_tests

      # Optional: Run with additional flags for thorough checking
      - name: Run Miri with strict checking
        run: |
          MIRIFLAGS="-Zmiri-strict-provenance -Zmiri-symbolic-alignment-check -Zmiri-track-raw-pointers" \
            cargo miri test --lib
        continue-on-error: true  # These strict checks might be too strict initially

  # Separate job for Miri on different features
  miri-features:
    name: Miri (Feature Combinations)
    runs-on: ubuntu-latest
    strategy:
      matrix:
        features:
          - default
          - no-default-features
          - std
          - nightly
    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: nightly
          components: miri

      - name: Setup Miri
        run: cargo miri setup

      - name: Run Miri with features
        run: |
          if [ "${{ matrix.features }}" = "default" ]; then
            cargo miri test --lib
          elif [ "${{ matrix.features }}" = "no-default-features" ]; then
            cargo miri test --lib --no-default-features
          else
            cargo miri test --lib --features ${{ matrix.features }}
          fi
```

### Alternative: Integrate into Existing rust.yml

Add to `.github/workflows/rust.yml` as a new job:

```yaml
  # Add this job to the existing rust.yml workflow
  miri:
    name: Miri
    runs-on: ubuntu-latest
    steps:
      - name: Checkout code
        uses: actions/checkout@v4

      - name: Install Rust nightly
        uses: actions-rust-lang/setup-rust-toolchain@v1
        with:
          toolchain: nightly
          components: miri

      - name: Cache cargo registry
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry
            ~/.cargo/git
            target
          key: ${{ runner.os }}-cargo-miri-nightly-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: |
            ${{ runner.os }}-cargo-miri-nightly-
            ${{ runner.os }}-cargo-nightly-
            ${{ runner.os }}-cargo-

      - name: Setup Miri
        run: cargo miri setup

      - name: Run Miri tests
        run: cargo miri test --lib

      - name: Run Miri with leak checking
        run: MIRIFLAGS="-Zmiri-leak-check" cargo miri test --lib
```

### CI Best Practices

1. **Start Conservative**: Begin with basic Miri tests, gradually add stricter flags
2. **Cache Wisely**: Miri setup can be slow; use GitHub Actions cache
3. **Separate Job**: Keep Miri as a separate job so it doesn't block fast checks
4. **Schedule Regular Runs**: Run Miri weekly to catch issues with nightly changes
5. **Allow Failures Initially**: Use `continue-on-error: true` for strict checks initially
6. **Feature Matrix**: Test different feature combinations
7. **Monitor Performance**: Miri tests are slow (~10-50x slower than normal tests)

## Expected Challenges and Solutions

### Challenge 1: Miri is Slow
**Problem**: Miri interprets every instruction, making tests 10-50x slower.

**Solutions**:
- Run Miri only on library tests, not benchmarks
- Use `continue-on-error` for non-critical strict checks
- Run comprehensive Miri tests on schedule, not every push
- Focus on unit tests rather than integration tests

### Challenge 2: Stacked Borrows Violations
**Problem**: Miri's Stacked Borrows model may flag valid unsafe code.

**Solutions**:
- Use `-Zmiri-track-raw-pointers` for better diagnostics
- Consider `-Zmiri-tree-borrows` (experimental alternative)
- Document any known false positives
- File issues with Miri team if needed

### Challenge 3: Limited Environment
**Problem**: Miri doesn't support all system calls and features.

**Solutions**:
- Use `-Zmiri-disable-isolation` if file/network access needed
- Skip tests that require unsupported features
- Focus on testing unsafe memory operations, not I/O

### Challenge 4: no_std Compatibility
**Problem**: Ensuring Miri tests work in no_std context.

**Solutions**:
- Test both std and no_std configurations
- Use `--no-default-features` flag for no_std tests
- Verify alloc crate usage under Miri

### Challenge 5: Nightly Dependency
**Problem**: Miri requires nightly Rust, which can be unstable.

**Solutions**:
- Pin nightly version if needed: `rust-toolchain.toml`
- Use `continue-on-error` in CI initially
- Have fallback to skip Miri if unavailable
- Schedule weekly runs to catch nightly regressions early

## Verification and Validation

### Success Criteria

1. **All Tests Pass**: All existing tests pass under Miri
2. **No Memory Leaks**: Tests pass with `-Zmiri-leak-check`
3. **No UB Detected**: No undefined behavior detected in any tests
4. **CI Integration**: Miri runs automatically in CI
5. **Documentation**: Clear documentation for developers

### Monitoring and Maintenance

1. **Regular Reviews**: Review Miri output monthly
2. **Update Documentation**: Keep Miri docs updated with findings
3. **Track Issues**: Document any Miri limitations or false positives
4. **Stay Updated**: Follow Miri development for new features

## Rollout Plan

### Phase 1: Local Setup (Week 1)
- [ ] Install Miri locally
- [ ] Run basic Miri tests
- [ ] Document any issues found
- [ ] Create local testing scripts

### Phase 2: Initial Testing (Week 2)
- [ ] Run Miri on all cache implementations
- [ ] Fix any UB issues found
- [ ] Add Miri testing documentation to README
- [ ] Create troubleshooting guide

### Phase 3: CI Integration (Week 3)
- [ ] Create Miri workflow file
- [ ] Test CI integration in feature branch
- [ ] Optimize CI performance
- [ ] Merge to main branch

### Phase 4: Hardening (Week 4+)
- [ ] Add strict provenance checks
- [ ] Enable leak checking
- [ ] Test all feature combinations
- [ ] Document best practices

## Documentation Updates

### README.md Updates

Add to the "Development" section:

```markdown
### Miri Testing

Test unsafe code for undefined behavior using Miri:

```bash
# Install Miri
rustup +nightly component add miri
cargo +nightly miri setup

# Run Miri tests
cargo +nightly miri test

# Run with leak checking
MIRIFLAGS="-Zmiri-leak-check" cargo +nightly miri test
```

See [MIRI_INTEGRATION_SPEC.md](MIRI_INTEGRATION_SPEC.md) for detailed information.
```

### Create MIRI.md (Developer Guide)

Create a dedicated guide for developers working with unsafe code and Miri.

## Estimated Timeline

- **Setup and Local Testing**: 1-2 days
- **Initial Issue Resolution**: 3-5 days (depending on issues found)
- **CI Integration**: 1-2 days
- **Documentation**: 1 day
- **Testing and Validation**: 2-3 days

**Total**: 1-2 weeks for complete integration

## Recommended Next Steps

1. **Approval**: Review this specification and provide feedback
2. **Proof of Concept**: Run Miri locally on a single module (e.g., LRU)
3. **Document Findings**: Create report of any issues discovered
4. **Iterate**: Refine approach based on findings
5. **Full Integration**: Implement complete plan

## Resources

- [Miri Documentation](https://github.com/rust-lang/miri)
- [Miri Flags Reference](https://github.com/rust-lang/miri/blob/master/README.md#miri-flags)
- [Rust Unsafe Code Guidelines](https://rust-lang.github.io/unsafe-code-guidelines/)
- [Stacked Borrows Explained](https://github.com/rust-lang/unsafe-code-guidelines/blob/master/wip/stacked-borrows.md)

## Conclusion

Integrating Miri into cache-rs will significantly improve code quality and safety by:
1. Detecting memory safety issues early
2. Validating safety invariants continuously
3. Preventing regressions in unsafe code
4. Increasing confidence in the correctness of optimizations

The investment in Miri integration will pay dividends in reliability and maintainability, especially given the extensive use of unsafe code in this project.
