//! Concurrent Cache Benchmarks
//!
//! Benchmarks for measuring concurrent cache performance across different
//! access patterns and segment configurations.

use cache_rs::config::{
    ConcurrentCacheConfig, ConcurrentGdsfCacheConfig, ConcurrentLfuCacheConfig,
    ConcurrentLfudaCacheConfig, ConcurrentLruCacheConfig, ConcurrentSlruCacheConfig,
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};
use cache_rs::{
    ConcurrentGdsfCache, ConcurrentLfuCache, ConcurrentLfudaCache, ConcurrentLruCache,
    ConcurrentSlruCache,
};
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use std::num::NonZeroUsize;
use std::sync::Arc;
use std::thread;

const CACHE_SIZE: usize = 10_000;
const OPS_PER_THREAD: usize = 1_000;

fn lru_config(capacity: usize) -> ConcurrentLruCacheConfig {
    ConcurrentCacheConfig {
        base: LruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size: u64::MAX,
        },
        segments: 16,
    }
}

fn lru_config_with_segments(capacity: usize, segments: usize) -> ConcurrentLruCacheConfig {
    ConcurrentCacheConfig {
        base: LruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size: u64::MAX,
        },
        segments,
    }
}

fn slru_config(capacity: usize, protected: usize) -> ConcurrentSlruCacheConfig {
    ConcurrentCacheConfig {
        base: SlruCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            protected_capacity: NonZeroUsize::new(protected).unwrap(),
            max_size: u64::MAX,
        },
        segments: 16,
    }
}

fn lfu_config(capacity: usize) -> ConcurrentLfuCacheConfig {
    ConcurrentCacheConfig {
        base: LfuCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            max_size: u64::MAX,
        },
        segments: 16,
    }
}

fn lfuda_config(capacity: usize) -> ConcurrentLfudaCacheConfig {
    ConcurrentCacheConfig {
        base: LfudaCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            initial_age: 0,
            max_size: u64::MAX,
        },
        segments: 16,
    }
}

fn gdsf_config(capacity: usize) -> ConcurrentGdsfCacheConfig {
    ConcurrentCacheConfig {
        base: GdsfCacheConfig {
            capacity: NonZeroUsize::new(capacity).unwrap(),
            initial_age: 0.0,
            max_size: u64::MAX,
        },
        segments: 16,
    }
}

/// Benchmark concurrent read operations across all cache types
fn concurrent_reads(c: &mut Criterion) {
    let mut group = c.benchmark_group("Concurrent Reads");
    group.throughput(Throughput::Elements((8 * OPS_PER_THREAD) as u64));

    // Pre-populate caches
    let lru_cache: Arc<ConcurrentLruCache<usize, usize>> =
        Arc::new(ConcurrentLruCache::init(lru_config(CACHE_SIZE), None));
    let slru_cache: Arc<ConcurrentSlruCache<usize, usize>> = Arc::new(ConcurrentSlruCache::init(
        slru_config(CACHE_SIZE, CACHE_SIZE / 2),
        None,
    ));
    let lfu_cache: Arc<ConcurrentLfuCache<usize, usize>> =
        Arc::new(ConcurrentLfuCache::init(lfu_config(CACHE_SIZE), None));
    let lfuda_cache: Arc<ConcurrentLfudaCache<usize, usize>> =
        Arc::new(ConcurrentLfudaCache::init(lfuda_config(CACHE_SIZE), None));
    let gdsf_cache: Arc<ConcurrentGdsfCache<usize, usize>> = Arc::new(ConcurrentGdsfCache::init(
        gdsf_config(CACHE_SIZE * 10),
        None,
    ));

    // Fill caches
    for i in 0..CACHE_SIZE {
        lru_cache.put(i, i);
        slru_cache.put(i, i);
        lfu_cache.put(i, i);
        lfuda_cache.put(i, i);
        gdsf_cache.put(i, i, ((i % 10) + 1) as u64);
    }

    group.bench_function("LRU", |b| {
        b.iter(|| {
            let cache = Arc::clone(&lru_cache);
            run_concurrent_reads(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("SLRU", |b| {
        b.iter(|| {
            let cache = Arc::clone(&slru_cache);
            run_concurrent_reads(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("LFU", |b| {
        b.iter(|| {
            let cache = Arc::clone(&lfu_cache);
            run_concurrent_reads(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("LFUDA", |b| {
        b.iter(|| {
            let cache = Arc::clone(&lfuda_cache);
            run_concurrent_reads(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("GDSF", |b| {
        b.iter(|| {
            let cache = Arc::clone(&gdsf_cache);
            run_concurrent_reads_gdsf(cache, 8, OPS_PER_THREAD);
        });
    });

    group.finish();
}

/// Benchmark concurrent write operations across all cache types
fn concurrent_writes(c: &mut Criterion) {
    let mut group = c.benchmark_group("Concurrent Writes");
    group.throughput(Throughput::Elements((8 * OPS_PER_THREAD) as u64));

    group.bench_function("LRU", |b| {
        let cache: Arc<ConcurrentLruCache<usize, usize>> =
            Arc::new(ConcurrentLruCache::init(lru_config(CACHE_SIZE), None));
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_writes(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("SLRU", |b| {
        let cache: Arc<ConcurrentSlruCache<usize, usize>> = Arc::new(ConcurrentSlruCache::init(
            slru_config(CACHE_SIZE, CACHE_SIZE / 2),
            None,
        ));
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_writes(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("LFU", |b| {
        let cache: Arc<ConcurrentLfuCache<usize, usize>> =
            Arc::new(ConcurrentLfuCache::init(lfu_config(CACHE_SIZE), None));
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_writes(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("LFUDA", |b| {
        let cache: Arc<ConcurrentLfudaCache<usize, usize>> =
            Arc::new(ConcurrentLfudaCache::init(lfuda_config(CACHE_SIZE), None));
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_writes(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("GDSF", |b| {
        let cache: Arc<ConcurrentGdsfCache<usize, usize>> = Arc::new(ConcurrentGdsfCache::init(
            gdsf_config(CACHE_SIZE * 10),
            None,
        ));
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_writes_gdsf(cache, 8, OPS_PER_THREAD);
        });
    });

    group.finish();
}

/// Benchmark mixed read/write operations (80% reads, 20% writes)
fn concurrent_mixed(c: &mut Criterion) {
    let mut group = c.benchmark_group("Concurrent Mixed (80/20)");
    group.throughput(Throughput::Elements((8 * OPS_PER_THREAD) as u64));

    group.bench_function("LRU", |b| {
        let cache: Arc<ConcurrentLruCache<usize, usize>> =
            Arc::new(ConcurrentLruCache::init(lru_config(CACHE_SIZE), None));
        // Pre-populate
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_mixed(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("SLRU", |b| {
        let cache: Arc<ConcurrentSlruCache<usize, usize>> = Arc::new(ConcurrentSlruCache::init(
            slru_config(CACHE_SIZE, CACHE_SIZE / 2),
            None,
        ));
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_mixed(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("LFU", |b| {
        let cache: Arc<ConcurrentLfuCache<usize, usize>> =
            Arc::new(ConcurrentLfuCache::init(lfu_config(CACHE_SIZE), None));
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_mixed(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("LFUDA", |b| {
        let cache: Arc<ConcurrentLfudaCache<usize, usize>> =
            Arc::new(ConcurrentLfudaCache::init(lfuda_config(CACHE_SIZE), None));
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_mixed(cache, 8, OPS_PER_THREAD);
        });
    });

    group.bench_function("GDSF", |b| {
        let cache: Arc<ConcurrentGdsfCache<usize, usize>> = Arc::new(ConcurrentGdsfCache::init(
            gdsf_config(CACHE_SIZE * 10),
            None,
        ));
        for i in 0..CACHE_SIZE {
            cache.put(i, i, ((i % 10) + 1) as u64);
        }
        b.iter(|| {
            let cache = Arc::clone(&cache);
            run_concurrent_mixed_gdsf(cache, 8, OPS_PER_THREAD);
        });
    });

    group.finish();
}

/// Benchmark different segment counts for LRU cache
fn segment_count_comparison(c: &mut Criterion) {
    let mut group = c.benchmark_group("Segment Count Comparison (LRU)");
    group.throughput(Throughput::Elements((8 * OPS_PER_THREAD) as u64));

    for segments in [1, 4, 8, 16, 32, 64] {
        group.bench_with_input(
            BenchmarkId::new("segments", segments),
            &segments,
            |b, &seg_count| {
                let cache: Arc<ConcurrentLruCache<usize, usize>> = Arc::new(
                    ConcurrentLruCache::init(lru_config_with_segments(CACHE_SIZE, seg_count), None),
                );
                // Pre-populate
                for i in 0..CACHE_SIZE {
                    cache.put(i, i);
                }
                b.iter(|| {
                    let cache = Arc::clone(&cache);
                    run_concurrent_mixed(cache, 8, OPS_PER_THREAD);
                });
            },
        );
    }

    group.finish();
}

// Helper trait for generic cache operations
trait ConcurrentCache<K, V>: Send + Sync {
    fn cache_get(&self, key: &K) -> Option<V>;
    fn cache_put(&self, key: K, value: V);
}

impl<K, V> ConcurrentCache<K, V> for ConcurrentLruCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    fn cache_get(&self, key: &K) -> Option<V> {
        self.get(key)
    }
    fn cache_put(&self, key: K, value: V) {
        self.put(key, value);
    }
}

impl<K, V> ConcurrentCache<K, V> for ConcurrentSlruCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    fn cache_get(&self, key: &K) -> Option<V> {
        self.get(key)
    }
    fn cache_put(&self, key: K, value: V) {
        self.put(key, value);
    }
}

impl<K, V> ConcurrentCache<K, V> for ConcurrentLfuCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    fn cache_get(&self, key: &K) -> Option<V> {
        self.get(key)
    }
    fn cache_put(&self, key: K, value: V) {
        self.put(key, value);
    }
}

impl<K, V> ConcurrentCache<K, V> for ConcurrentLfudaCache<K, V>
where
    K: std::hash::Hash + Eq + Clone + Send,
    V: Clone + Send,
{
    fn cache_get(&self, key: &K) -> Option<V> {
        self.get(key)
    }
    fn cache_put(&self, key: K, value: V) {
        self.put(key, value);
    }
}

// Generic concurrent read runner
fn run_concurrent_reads<C>(cache: Arc<C>, num_threads: usize, ops_per_thread: usize)
where
    C: ConcurrentCache<usize, usize> + 'static,
{
    let mut handles = Vec::with_capacity(num_threads);
    for t in 0..num_threads {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = (t * ops_per_thread + i) % CACHE_SIZE;
                black_box(cache.cache_get(&key));
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

// Generic concurrent write runner
fn run_concurrent_writes<C>(cache: Arc<C>, num_threads: usize, ops_per_thread: usize)
where
    C: ConcurrentCache<usize, usize> + 'static,
{
    let mut handles = Vec::with_capacity(num_threads);
    for t in 0..num_threads {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = t * ops_per_thread + i;
                cache.cache_put(key, key);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

// Generic concurrent mixed runner (80% reads, 20% writes)
fn run_concurrent_mixed<C>(cache: Arc<C>, num_threads: usize, ops_per_thread: usize)
where
    C: ConcurrentCache<usize, usize> + 'static,
{
    let mut handles = Vec::with_capacity(num_threads);
    for t in 0..num_threads {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = (t * ops_per_thread + i) % CACHE_SIZE;
                if i % 5 == 0 {
                    // 20% writes
                    cache.cache_put(key, key);
                } else {
                    // 80% reads
                    black_box(cache.cache_get(&key));
                }
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

// GDSF-specific runners (different API with size parameter)
fn run_concurrent_reads_gdsf(
    cache: Arc<ConcurrentGdsfCache<usize, usize>>,
    num_threads: usize,
    ops_per_thread: usize,
) {
    let mut handles = Vec::with_capacity(num_threads);
    for t in 0..num_threads {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = (t * ops_per_thread + i) % CACHE_SIZE;
                black_box(cache.get(&key));
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

fn run_concurrent_writes_gdsf(
    cache: Arc<ConcurrentGdsfCache<usize, usize>>,
    num_threads: usize,
    ops_per_thread: usize,
) {
    let mut handles = Vec::with_capacity(num_threads);
    for t in 0..num_threads {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = t * ops_per_thread + i;
                let size = ((key % 10) + 1) as u64;
                cache.put(key, key, size);
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

fn run_concurrent_mixed_gdsf(
    cache: Arc<ConcurrentGdsfCache<usize, usize>>,
    num_threads: usize,
    ops_per_thread: usize,
) {
    let mut handles = Vec::with_capacity(num_threads);
    for t in 0..num_threads {
        let cache = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..ops_per_thread {
                let key = (t * ops_per_thread + i) % CACHE_SIZE;
                if i % 5 == 0 {
                    let size = ((key % 10) + 1) as u64;
                    cache.put(key, key, size);
                } else {
                    black_box(cache.get(&key));
                }
            }
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
}

criterion_group!(
    benches,
    concurrent_reads,
    concurrent_writes,
    concurrent_mixed,
    segment_count_comparison
);
criterion_main!(benches);
