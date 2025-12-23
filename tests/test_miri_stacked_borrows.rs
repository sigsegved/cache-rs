// Test to demonstrate Stacked Borrows violations that Miri detects
//
// This test file validates that:
// 1. The original problematic code causes undefined behavior detectable by Miri
// 2. The fixed code resolves the issue and passes Miri validation
//
// Run with: cargo +nightly miri test --test test_miri_stacked_borrows

#![cfg(test)]

use cache_rs::{GdsfCache, LfuCache, LfudaCache};
use std::num::NonZeroUsize;

/// This test demonstrates the Stacked Borrows violation in GDSF cache.
///
/// The issue occurs when:
/// 1. We get a key reference from a node in the cache
/// 2. We pass that reference to update_priority
/// 3. Inside update_priority, we try to get_mut on the HashMap using that key
/// 4. This creates an aliasing conflict: the borrowed key is "protected" but
///    we're trying to mutably access memory through a different path
///
/// Under Miri, this causes:
/// "error: Undefined Behavior: not granting access to tag <X> because that would
///  remove [SharedReadOnly for <Y>] which is strongly protected"
#[test]
fn test_gdsf_stacked_borrows_violation() {
    let mut cache = GdsfCache::new(NonZeroUsize::new(10).unwrap());
    
    // Insert some items
    cache.put("a", 1, 10);
    cache.put("b", 2, 20);
    cache.put("c", 3, 15);
    
    // Access items multiple times to trigger priority updates
    // The original buggy code would fail here under Miri
    for _ in 0..3 {
        assert_eq!(cache.get(&"a"), Some(1));
        assert_eq!(cache.get(&"b"), Some(2));
        assert_eq!(cache.get(&"c"), Some(3));
    }
    
    // If we get here, the fix is working correctly
    assert_eq!(cache.len(), 3);
}

/// This test demonstrates the Stacked Borrows violation in LFUDA cache.
///
/// Similar issue to GDSF - the update_priority method receives a borrowed key
/// that creates aliasing issues with HashMap access.
#[test]
fn test_lfuda_stacked_borrows_violation() {
    let mut cache = LfudaCache::new(NonZeroUsize::new(10).unwrap());
    
    // Insert some items
    cache.put("a", 1);
    cache.put("b", 2);
    cache.put("c", 3);
    
    // Access items multiple times to trigger priority updates
    // The original buggy code would fail here under Miri
    for _ in 0..3 {
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
    }
    
    // If we get here, the fix is working correctly
    assert_eq!(cache.len(), 3);
}

/// This test demonstrates the Stacked Borrows violation in LFU cache.
///
/// Similar issue - the update_frequency method receives a borrowed key
/// that creates aliasing issues with HashMap access.
#[test]
fn test_lfu_stacked_borrows_violation() {
    let mut cache = LfuCache::new(NonZeroUsize::new(10).unwrap());
    
    // Insert some items
    cache.put("a", 1);
    cache.put("b", 2);
    cache.put("c", 3);
    
    // Access items multiple times to trigger frequency updates
    // The original buggy code would fail here under Miri
    for _ in 0..3 {
        assert_eq!(cache.get(&"a"), Some(&1));
        assert_eq!(cache.get(&"b"), Some(&2));
        assert_eq!(cache.get(&"c"), Some(&3));
    }
    
    // If we get here, the fix is working correctly
    assert_eq!(cache.len(), 3);
}

/// More intensive test that exercises the cache under various conditions
/// to ensure Miri doesn't detect any issues.
#[test]
fn test_intensive_cache_operations_under_miri() {
    // Test GDSF with varying sizes
    let mut gdsf = GdsfCache::new(NonZeroUsize::new(5).unwrap());
    for i in 0..10 {
        gdsf.put(i, i * 10, i as u64 + 1);
        if i >= 5 {
            // Trigger evictions and priority updates
            for j in (i - 4)..=i {
                let _ = gdsf.get(&j);
            }
        }
    }
    
    // Test LFUDA with frequency changes
    let mut lfuda = LfudaCache::new(NonZeroUsize::new(5).unwrap());
    for i in 0..10 {
        lfuda.put(i, i * 10);
        if i >= 5 {
            // Access older items to change priorities
            for j in (i - 4)..=i {
                let _ = lfuda.get(&j);
            }
        }
    }
    
    // Test LFU with frequency tracking
    let mut lfu = LfuCache::new(NonZeroUsize::new(5).unwrap());
    for i in 0..10 {
        lfu.put(i, i * 10);
        if i >= 5 {
            // Access items different amounts to vary frequencies
            for j in (i - 4)..=i {
                for k in 0..=(j % 3) {
                    let _ = lfu.get(&j);
                    let _ = k; // use k
                }
            }
        }
    }
}

/// Test that demonstrates the fix works with get_mut as well
#[test]
fn test_get_mut_stacked_borrows() {
    let mut gdsf = GdsfCache::new(NonZeroUsize::new(10).unwrap());
    gdsf.put("a", 1, 10);
    gdsf.put("b", 2, 20);
    
    // get_mut also triggers priority updates
    if let Some(val) = gdsf.get_mut(&"a") {
        *val += 10;
    }
    assert_eq!(gdsf.get(&"a"), Some(11));
    
    let mut lfuda = LfudaCache::new(NonZeroUsize::new(10).unwrap());
    lfuda.put("a", 1);
    lfuda.put("b", 2);
    
    if let Some(val) = lfuda.get_mut(&"a") {
        *val += 10;
    }
    assert_eq!(lfuda.get(&"a"), Some(&11));
    
    let mut lfu = LfuCache::new(NonZeroUsize::new(10).unwrap());
    lfu.put("a", 1);
    lfu.put("b", 2);
    
    if let Some(val) = lfu.get_mut(&"a") {
        *val += 10;
    }
    assert_eq!(lfu.get(&"a"), Some(&11));
}
