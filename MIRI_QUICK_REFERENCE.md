# Miri Quick Reference Card

Quick commands and patterns for using Miri in cache-rs development.

## Installation (One-time Setup)

```bash
rustup toolchain install nightly
rustup +nightly component add miri
cargo +nightly miri setup
```

## Basic Commands

| Command | Description |
|---------|-------------|
| `cargo +nightly miri test --lib` | Run all library tests |
| `cargo +nightly miri test --lib <module>` | Test specific module |
| `cargo +nightly miri test --test no_std_tests` | Test no_std compatibility |
| `./scripts/miri-test.sh` | Run comprehensive test suite |

## Miri Flags

| Flag | Purpose | Command |
|------|---------|---------|
| `-Zmiri-leak-check` | Detect memory leaks | `MIRIFLAGS="-Zmiri-leak-check" cargo +nightly miri test --lib` |
| `-Zmiri-strict-provenance` | Check pointer provenance | `MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri test --lib` |
| `-Zmiri-symbolic-alignment-check` | Check alignment | `MIRIFLAGS="-Zmiri-symbolic-alignment-check" cargo +nightly miri test --lib` |
| `-Zmiri-track-raw-pointers` | Better diagnostics | `MIRIFLAGS="-Zmiri-track-raw-pointers" cargo +nightly miri test --lib` |

## Development Workflow

### 1. Before Committing
```bash
cargo +nightly miri test --lib
```

### 2. After Changing Unsafe Code
```bash
cargo +nightly miri test --lib <module_name>
```

### 3. Before Opening PR
```bash
./scripts/miri-test.sh
```

### 4. Debugging Failures
```bash
RUST_BACKTRACE=1 MIRIFLAGS="-Zmiri-track-raw-pointers" cargo +nightly miri test --lib <test_name>
```

## Common Error Patterns

### Memory Leak
**Error**: `error: memory leaked: alloc1234`
**Fix**: Check cleanup code, especially in error paths

### Use-After-Free
**Error**: `dereferenced after this allocation got freed`
**Fix**: Review pointer lifetimes and allocation ownership

### Invalid Pointer Arithmetic
**Error**: `out-of-bounds pointer arithmetic`
**Fix**: Check pointer calculations and bounds

### Provenance Violation
**Error**: `attempting a read access using <untagged>`
**Fix**: Pointer used after provenance expired

## Feature Testing

```bash
# Test with default features
cargo +nightly miri test --lib

# Test without default features
cargo +nightly miri test --lib --no-default-features

# Test with std feature
cargo +nightly miri test --lib --features std

# Test with nightly feature
cargo +nightly miri test --lib --features nightly
```

## Tips

- ✅ Run Miri on unsafe code changes
- ✅ Enable leak checking for thorough validation
- ✅ Use strict provenance during development
- ⚠️ Miri is 10-50x slower than regular tests
- ⚠️ Focus on library tests, skip benchmarks
- 📖 Read MIRI.md for detailed information

## CI Integration

Miri runs automatically in CI:
- On every push and PR
- Weekly scheduled runs
- Multiple feature combinations tested
- Strict checks run with `continue-on-error`

## Documentation

- **MIRI.md**: Comprehensive developer guide
- **MIRI_INTEGRATION_SPEC.md**: Complete integration specification
- **scripts/miri-test.sh**: Automated test script
