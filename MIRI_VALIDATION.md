# Miri Stacked Borrows Validation

This document demonstrates that the Stacked Borrows violations detected by Miri are real issues that have been fixed.

## Test Results

### With the Fix (Current Code) ✅

Running the tests with our fixed code:

```bash
$ MIRIFLAGS="-Zmiri-ignore-leaks" cargo +nightly miri test --test test_miri_stacked_borrows
```

**Result**: ✅ All 5 tests PASS

```
running 5 tests
test test_gdsf_stacked_borrows_violation ... ok
test test_get_mut_stacked_borrows ... ok
test test_intensive_cache_operations_under_miri ... ok
test test_lfu_stacked_borrows_violation ... ok
test test_lfuda_stacked_borrows_violation ... ok

test result: ok. 5 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Without the Fix (Original Buggy Code) ❌

The original code had this pattern in `src/gdsf.rs`, `src/lfuda.rs`, and `src/lfu.rs`:

```rust
// BUGGY CODE (before fix)
unsafe fn update_priority(&mut self, key: &K) -> *mut Entry<(K, V)> {
    let metadata = self.map.get_mut(key).unwrap();  // ❌ Aliasing violation!
    // ...
}

// Called like:
let (key_ref, _) = (*node).get_value();
self.update_priority(key_ref);  // ❌ key_ref creates protected reference
```

When run under Miri, this caused:

```
error: Undefined Behavior: not granting access to tag <267242> because that would 
remove [SharedReadOnly for <270413>] which is strongly protected
    --> /home/runner/.rustup/toolchains/nightly-.../library/alloc/src/boxed.rs:1493:9
     |
1493 |         Box(unsafe { Unique::new_unchecked(raw) }, alloc)
     |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ 
     |
help: <267242> was created by a SharedReadWrite retag at offsets [0x0..0x28]
    --> src/list.rs:378:52
     |
 378 |     let node = unsafe { NonNull::new_unchecked(Box::into_raw(Box::new(...))) };
     |                                                ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
help: <270413> is this argument
    --> src/gdsf.rs:208:42
     |
 208 |     unsafe fn update_priority(&mut self, key: &K) -> *mut Entry<(K, V)>
     |                                          ^^^
```

## The Fix

Our fix refactored the code to pass node pointers directly:

```rust
// FIXED CODE (current)
unsafe fn update_priority_by_node(&mut self, node: *mut Entry<(K, V)>) -> *mut Entry<(K, V)> {
    let (key_ref, _) = (*node).get_value();
    let key_cloned = key_ref.clone();  // ✅ Clone breaks aliasing chain
    let metadata = self.map.get_mut(&key_cloned).unwrap();  // ✅ No aliasing!
    // ...
}

// Called like:
self.update_priority_by_node(node);  // ✅ Pass pointer directly
```

## Why This Fixes the Issue

1. **Before**: We extracted `key_ref: &K` from the node and passed it to `update_priority`. This created a "protected" reference in Miri's Stacked Borrows model. When we then tried to access `self.map.get_mut(key)`, Miri detected an aliasing conflict because we were accessing the same memory through two different paths.

2. **After**: We pass the node pointer directly. Inside the method, we extract and clone the key. The cloned key is a new owned value, so accessing the HashMap through it doesn't conflict with any references. The aliasing chain is broken.

## Reproducing the Original Bug

To see the original bug, you can temporarily revert the fixes:

```bash
# Revert to the commit before the fix
git show c056380:src/gdsf.rs > /tmp/gdsf_buggy.rs
git show c056380:src/lfuda.rs > /tmp/lfuda_buggy.rs
git show c056380:src/lfu.rs > /tmp/lfu_buggy.rs

# Copy the buggy versions (DON'T commit this!)
cp /tmp/gdsf_buggy.rs src/gdsf.rs
cp /tmp/lfuda_buggy.rs src/lfuda.rs
cp /tmp/lfu_buggy.rs src/lfu.rs

# Run Miri tests - they will FAIL with UB errors
cargo +nightly miri test --lib test_gdsf_basic_operations

# Restore the fixes
git checkout HEAD -- src/gdsf.rs src/lfuda.rs src/lfu.rs
```

The Miri error you'll see:
```
error: Undefined Behavior: not granting access to tag <X> because that would 
remove [SharedReadOnly for <Y>] which is strongly protected
```

This confirms the bug was real and the fix resolves it.

## Test Coverage

The new test files validate the fixes:

### `tests/test_miri_stacked_borrows.rs`
Comprehensive test suite with 5 tests:

1. **`test_gdsf_stacked_borrows_violation`** - Exercises GDSF cache get operations that trigger priority updates
2. **`test_lfuda_stacked_borrows_violation`** - Exercises LFUDA cache get operations that trigger priority updates
3. **`test_lfu_stacked_borrows_violation`** - Exercises LFU cache get operations that trigger frequency updates
4. **`test_intensive_cache_operations_under_miri`** - Stress test with evictions and multiple accesses
5. **`test_get_mut_stacked_borrows`** - Tests both get and get_mut paths

### `tests/test_stacked_borrows_minimal.rs`
Minimal reproduction tests with 3 focused cases:

1. **`test_minimal_stacked_borrows_case`** - Simplest case that triggers the bug (single insert + get)
2. **`test_repeated_accesses_trigger_multiple_violations`** - Shows multiple violations from repeated access
3. **`test_issue_is_in_hashmap_access_with_borrowed_key`** - Demonstrates the exact point of failure

All tests:
- Would have failed with the original buggy code under Miri
- Pass with the fixed code under Miri
- Also pass under normal test runs (not just Miri)

## Validation

To validate the fix yourself:

```bash
# Run the Miri tests
cargo +nightly miri test --test test_miri_stacked_borrows

# Run all library tests under Miri
MIRIFLAGS="-Zmiri-ignore-leaks" cargo +nightly miri test --lib

# Run regular tests to ensure no regressions
cargo test --lib
```

All tests should pass, confirming the undefined behavior has been eliminated.
