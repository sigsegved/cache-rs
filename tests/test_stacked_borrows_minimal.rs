// Minimal reproduction test to demonstrate the Stacked Borrows violation
//
// This test shows exactly how the bug manifests and why the fix resolves it.
//
// Run with: cargo +nightly miri test --test test_stacked_borrows_minimal

#![cfg(test)]

use cache_rs::GdsfCache;
use std::num::NonZeroUsize;

/// Minimal test case that demonstrates the Stacked Borrows issue.
///
/// This test performs the minimal sequence of operations that would trigger
/// the undefined behavior in the original code:
///
/// 1. Create a cache and insert an item
/// 2. Call get(), which internally:
///    a. Gets a node pointer from the HashMap
///    b. Extracts a key reference from the node: `let (key_ref, _) = (*node).get_value()`
///    c. Passes key_ref to update_priority (ORIGINAL BUG) or the node to update_priority_by_node (FIX)
///    d. Inside update_priority, calls `self.map.get_mut(key_ref)` (ALIASING VIOLATION)
///
/// Under Miri with the original code:
/// - The key_ref is a borrowed reference into the node
/// - Calling self.map.get_mut(key_ref) creates a mutable borrow of the HashMap
/// - This violates Stacked Borrows because key_ref is "protecting" the node memory
/// - Miri detects: "not granting access to tag X because that would remove
///   [SharedReadOnly for Y] which is strongly protected"
///
/// With the fix:
/// - We pass the node pointer directly to update_priority_by_node
/// - Inside, we clone the key before using it with get_mut
/// - The cloned key breaks the aliasing chain
/// - No Stacked Borrows violation occurs
#[test]
fn test_minimal_stacked_borrows_case() {
    let mut cache = GdsfCache::new(NonZeroUsize::new(2).unwrap());
    
    // Insert an item
    cache.put("test_key", 42, 10);
    
    // This get() call would trigger undefined behavior in the original code
    // because it passes a borrowed key reference to update_priority
    let value = cache.get(&"test_key");
    
    assert_eq!(value, Some(42));
}

/// Demonstrates that repeated accesses (which change priority/frequency)
/// would trigger the issue multiple times in the original code.
#[test]
fn test_repeated_accesses_trigger_multiple_violations() {
    let mut cache = GdsfCache::new(NonZeroUsize::new(3).unwrap());
    
    cache.put("a", 1, 10);
    cache.put("b", 2, 20);
    
    // Each get() would trigger the violation in the original code
    // because each one calls update_priority with a borrowed key
    for i in 0..5 {
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
        println!("Iteration {} completed successfully", i);
    }
}

/// Shows that the issue occurs specifically when the borrowed reference
/// (from the node) is used to access the HashMap mutably.
#[test]
fn test_issue_is_in_hashmap_access_with_borrowed_key() {
    let mut cache = GdsfCache::new(NonZeroUsize::new(5).unwrap());
    
    // Set up multiple items to ensure HashMap has some structure
    for i in 0..3 {
        cache.put(i, i * 10, i as u64 + 1);
    }
    
    // Access them in a pattern that requires priority updates
    // In the original code, each access:
    // 1. Gets node from map (immutable borrow)
    // 2. Gets key from node (creates protected reference)
    // 3. Tries to get_mut on map with that key (VIOLATION!)
    for i in 0..3 {
        let _ = cache.get(&i);
    }
    
    // If we reach here, the fix is working
    assert!(true);
}
