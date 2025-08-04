use cache_rs::{GdsfCache, LfuCache, LfudaCache, LruCache, SlruCache};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::num::NonZeroUsize;

pub fn criterion_benchmark(c: &mut Criterion) {
    const CACHE_SIZE: usize = 1000;
    let mut group = c.benchmark_group("Cache Operations");

    // LRU benchmarks
    {
        let mut cache = LruCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap());
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
        let mut cache = LfuCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap());
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
        let mut cache = LfudaCache::new(NonZeroUsize::new(CACHE_SIZE).unwrap());
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
        let mut cache = SlruCache::new(
            NonZeroUsize::new(CACHE_SIZE).unwrap(),
            NonZeroUsize::new(CACHE_SIZE / 2).unwrap(),
        );
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
        let mut cache = GdsfCache::new(NonZeroUsize::new(CACHE_SIZE * 10).unwrap()); // Larger size for GDSF
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
