# Miri Developer Guide

This guide provides practical information for developers working with Miri in the cache-rs project.

## Table of Contents

- [What is Miri?](#what-is-miri)
- [Quick Start](#quick-start)
- [Common Use Cases](#common-use-cases)
- [Understanding Miri Output](#understanding-miri-output)
- [Troubleshooting](#troubleshooting)
- [Best Practices](#best-practices)
- [FAQ](#faq)

## What is Miri?

Miri is an interpreter for Rust's mid-level intermediate representation (MIR). It can detect:

- **Memory safety violations**: Use-after-free, double-free, buffer overflows
- **Undefined behavior**: Invalid pointer arithmetic, unaligned access, data races
- **Memory leaks**: Unreleased memory allocations
- **Pointer provenance issues**: Using pointers beyond their valid lifetime

Miri is especially valuable for cache-rs because we use extensive unsafe code for performance.

## Quick Start

### Installation

```bash
# Install nightly Rust
rustup toolchain install nightly

# Install Miri
rustup +nightly component add miri

# Setup Miri (downloads standard library)
cargo +nightly miri setup
```

### Running Tests

```bash
# Run all tests
cargo +nightly miri test

# Run library tests only (recommended)
cargo +nightly miri test --lib

# Run a specific test
cargo +nightly miri test --lib test_lru_cache_basic

# Run with verbose output
cargo +nightly miri test --lib --verbose
```

### Using the Helper Script

```bash
# Run comprehensive test suite
./scripts/miri-test.sh

# Make executable if needed
chmod +x scripts/miri-test.sh
```

## Common Use Cases

### Testing a Specific Cache Implementation

```bash
# Test LRU cache
cargo +nightly miri test --lib lru

# Test SLRU cache
cargo +nightly miri test --lib slru

# Test LFU cache
cargo +nightly miri test --lib lfu
```

### Checking for Memory Leaks

```bash
# Enable leak checking
MIRIFLAGS="-Zmiri-leak-check" cargo +nightly miri test --lib
```

If you see a leak, the output will show:
```
error: memory leaked: ...
```

### Validating Pointer Operations

```bash
# Enable strict provenance checking
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test --lib
```

This ensures pointers are used within their valid provenance.

### Testing no_std Code

```bash
# Run no_std integration tests
cargo +nightly miri test --test no_std_tests
```

### Debugging Specific Unsafe Code

When working on unsafe code, run Miri frequently:

```bash
# Test after each change
cargo +nightly miri test --lib <module_name>

# Enable backtraces for better error reporting
RUST_BACKTRACE=1 cargo +nightly miri test --lib <module_name>
```

## Understanding Miri Output

### Successful Test

```
running 25 tests
test list::test_append ... ok
test list::test_move_to_front ... ok
...
test result: ok. 25 passed; 0 failed; 0 ignored; 0 measured
```

### Memory Leak

```
error: memory leaked: alloc1234 (24 bytes, alignment 8)
  --> src/lru.rs:123:9
   |
   | let node = Box::new(Entry::new(key, value));
   |            ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Action**: Check for missing cleanup code, especially in error paths.

### Use-After-Free

```
error: Undefined Behavior: pointer to alloc1234 was dereferenced after this allocation got freed
  --> src/list.rs:89:13
   |
   | let value = unsafe { (*node).value };
   |             ^^^^^^^^^^^^^^^^^^^^^^^^^
```

**Action**: Review pointer lifetime and ensure the allocation is still valid.

### Invalid Pointer Arithmetic

```
error: Undefined Behavior: out-of-bounds pointer arithmetic
  --> src/list.rs:45:9
   |
   | unsafe { node.offset(1) }
   |          ^^^^^^^^^^^^^^^^^^
```

**Action**: Check pointer calculations and bounds.

### Provenance Issues

```
error: Undefined Behavior: attempting a read access using <untagged> at alloc1234
  --> src/lru.rs:156:9
   |
   | unsafe { *ptr }
   |          ^^^^^
```

**Action**: Pointer may have been used after its provenance expired. Review pointer creation and usage.

## Troubleshooting

### Issue: "Miri component not found"

```bash
# Solution
rustup +nightly component add miri
cargo +nightly miri setup
```

### Issue: "Tests are extremely slow"

This is expected. Miri interprets every instruction, making tests 10-50x slower.

**Solutions**:
- Focus on library tests: `cargo +nightly miri test --lib`
- Test specific modules: `cargo +nightly miri test --lib lru`
- Skip benchmarks (they're too slow under Miri)

### Issue: "False positive about pointer provenance"

Miri's Stacked Borrows model can be conservative. Options:

1. **Verify it's actually a false positive**: Review the unsafe code carefully
2. **Track pointers for better diagnostics**:
   ```bash
   MIRIFLAGS="-Zmiri-track-raw-pointers" cargo +nightly miri test --lib
   ```
3. **Try Tree Borrows** (experimental alternative):
   ```bash
   MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri test --lib
   ```

### Issue: "Test fails under Miri but passes normally"

This likely indicates **real undefined behavior** that only manifests under Miri's stricter checking.

**Action**:
1. Read the Miri error carefully
2. Review the safety invariants in the unsafe code
3. Check SAFETY comments match actual behavior
4. Fix the underlying issue

### Issue: "Can't build for no_std"

Miri needs nightly, and no_std tests should work:

```bash
cargo +nightly miri test --test no_std_tests
```

If this fails, check feature flags in tests.

## Best Practices

### 1. Run Miri Regularly

```bash
# Before committing
cargo +nightly miri test --lib

# Before opening PR
./scripts/miri-test.sh
```

### 2. Test Incrementally

Don't wait until all code is written:

```bash
# After writing a new function with unsafe code
cargo +nightly miri test --lib <module>
```

### 3. Use Safety Comments

Every `unsafe` block should have a SAFETY comment that Miri helps verify:

```rust
// SAFETY: node comes from our map, so it's valid and properly aligned
unsafe {
    (*node).value = new_value;
}
```

### 4. Enable Strict Checks During Development

```bash
# Catch issues early
MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test --lib
```

### 5. Document Known Limitations

If Miri reports a false positive (rare), document it:

```rust
// SAFETY: This is safe because...
// Note: Miri may report a false positive here due to...
unsafe { ... }
```

### 6. Test Error Paths

Ensure cleanup code runs correctly:

```rust
#[test]
fn test_cleanup_on_panic() {
    // Test that unsafe code cleans up properly
}
```

## FAQ

### Q: Do I need to run Miri for every change?

**A**: For changes to unsafe code, yes. For safe code changes, regular tests are sufficient.

### Q: Why are Miri tests so slow?

**A**: Miri interprets every single instruction to check for UB. This is 10-50x slower than native execution.

### Q: Can Miri catch all bugs?

**A**: No. Miri can only detect UB that occurs during test execution. It can't prove absence of bugs.

### Q: Should Miri tests block CI?

**A**: Initially, no. Use `continue-on-error: true` in CI until all issues are resolved. Then make them mandatory.

### Q: What if Miri finds a real bug?

**A**: Great! Fix it:
1. Understand the error message
2. Review the safety invariants
3. Fix the unsafe code
4. Re-run Miri to verify
5. Add a test case if possible

### Q: Can I disable Miri for specific tests?

**A**: Yes, but avoid this if possible:

```rust
#[test]
#[cfg_attr(miri, ignore)]  // Skip under Miri
fn test_large_benchmark() {
    // This test is too slow under Miri
}
```

### Q: How do I debug complex Miri failures?

**A**:
1. Enable backtraces: `RUST_BACKTRACE=1`
2. Track pointers: `MIRIFLAGS="-Zmiri-track-raw-pointers"`
3. Run single test: `cargo +nightly miri test --lib specific_test`
4. Add debug prints (they work under Miri)
5. Simplify the test case

### Q: Does Miri work with hashbrown?

**A**: Yes! hashbrown is Miri-compatible and tests well.

### Q: What about the nightly feature?

**A**: Test both with and without:

```bash
cargo +nightly miri test --lib --features nightly
cargo +nightly miri test --lib
```

### Q: Can I run Miri in parallel?

**A**: Miri runs tests sequentially for correctness. Use `--test-threads=1` if needed:

```bash
cargo +nightly miri test --lib -- --test-threads=1
```

## Additional Resources

- [Miri GitHub Repository](https://github.com/rust-lang/miri)
- [Miri Flags Reference](https://github.com/rust-lang/miri/blob/master/README.md)
- [Rust Unsafe Code Guidelines](https://rust-lang.github.io/unsafe-code-guidelines/)
- [Stacked Borrows Explained](https://github.com/rust-lang/unsafe-code-guidelines/blob/master/wip/stacked-borrows.md)
- [Tree Borrows (Experimental)](https://perso.crans.org/vanille/treebor/)

## Getting Help

If you encounter issues with Miri:

1. Check this guide's troubleshooting section
2. Review [MIRI_INTEGRATION_SPEC.md](MIRI_INTEGRATION_SPEC.md)
3. Search [Miri issues](https://github.com/rust-lang/miri/issues)
4. Ask in project discussions or file an issue

Remember: Miri errors usually indicate real problems. Take them seriously and investigate thoroughly.
