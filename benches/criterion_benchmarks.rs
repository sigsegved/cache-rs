use cache_rs::config::{
    GdsfCacheConfig, LfuCacheConfig, LfudaCacheConfig, LruCacheConfig, SlruCacheConfig,
};
use cache_rs::{GdsfCache, LfuCache, LfudaCache, LruCache, SlruCache};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::num::NonZeroUsize;

// Helper functions to create caches with the init pattern
fn make_lru<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LruCache<K, V> {
    let config = LruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size: u64::MAX,
    };
    LruCache::init(config, None)
}

fn make_lfu<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LfuCache<K, V> {
    let config = LfuCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        max_size: u64::MAX,
    };
    LfuCache::init(config, None)
}

fn make_lfuda<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> LfudaCache<K, V> {
    let config = LfudaCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        initial_age: 0,
        max_size: u64::MAX,
    };
    LfudaCache::init(config, None)
}

fn make_slru<K: std::hash::Hash + Eq + Clone, V: Clone>(
    cap: usize,
    protected_cap: usize,
) -> SlruCache<K, V> {
    let config = SlruCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        protected_capacity: NonZeroUsize::new(protected_cap).unwrap(),
        max_size: u64::MAX,
    };
    SlruCache::init(config, None)
}

fn make_gdsf<K: std::hash::Hash + Eq + Clone, V: Clone>(cap: usize) -> GdsfCache<K, V> {
    let config = GdsfCacheConfig {
        capacity: NonZeroUsize::new(cap).unwrap(),
        initial_age: 0.0,
        max_size: u64::MAX,
    };
    GdsfCache::init(config, None)
}

pub fn criterion_benchmark(c: &mut Criterion) {
    const CACHE_SIZE: usize = 1000;
    let mut group = c.benchmark_group("Cache Operations");

    // LRU benchmarks
    {
        let mut cache = make_lru(CACHE_SIZE);
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }

        group.bench_function("LRU get hit", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.get(&(i % CACHE_SIZE)));
                }
            });
        });

        group.bench_function("LRU get miss", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.get(&(i + CACHE_SIZE)));
                }
            });
        });

        group.bench_function("LRU put existing", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.put(i % CACHE_SIZE, i));
                }
            });
        });
    }

    // LFU benchmarks
    {
        let mut cache = make_lfu(CACHE_SIZE);
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }

        group.bench_function("LFU get hit", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.get(&(i % CACHE_SIZE)));
                }
            });
        });
    }

    // LFUDA benchmarks
    {
        let mut cache = make_lfuda(CACHE_SIZE);
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }

        group.bench_function("LFUDA get hit", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.get(&(i % CACHE_SIZE)));
                }
            });
        });
    }

    // SLRU benchmarks
    {
        let mut cache = make_slru(CACHE_SIZE, CACHE_SIZE / 2);
        for i in 0..CACHE_SIZE {
            cache.put(i, i);
        }

        group.bench_function("SLRU get hit", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.get(&(i % CACHE_SIZE)));
                }
            });
        });
    }

    // GDSF benchmarks
    {
        let mut cache = make_gdsf(CACHE_SIZE * 10);
        for i in 0..CACHE_SIZE {
            cache.put(i, i, ((i % 10) + 1) as u64); // Size between 1-10, cast to u64
        }

        group.bench_function("GDSF get hit", |b| {
            b.iter(|| {
                for i in 0..100 {
                    black_box(cache.get(&(i % CACHE_SIZE)));
                }
            });
        });
    }

    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
