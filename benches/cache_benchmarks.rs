// Simple benchmarks using criterion instead of unstable test feature
use cache_rs::{GdsfCache, LfuCache, LfudaCache, LruCache, SlruCache};
use criterion::{black_box, criterion_group, criterion_main, Criterion};
use std::num::NonZeroUsize;

// Benchmark configuration
const CACHE_SIZE: usize = 1_000;
const NUM_OPERATIONS: usize = 10_000;

// Simple linear congruential generator for reproducible benchmarks
struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_mul(1103515245).wrapping_add(12345) & 0x7fffffff;
        self.state
    }

    fn next_f64(&mut self) -> f64 {
        (self.next_u64() as f64) / (0x7fffffff as f64)
    }
}

// Helper function to generate Zipf-like distribution
fn zipf_sample(n: usize, skew: f64) -> Vec<usize> {
    let mut rng = SimpleRng::new(42);

    // Calculate Zipf normalization constant
    let mut norm: f64 = 0.0;
    for i in 1..=n {
        norm += 1.0 / (i as f64).powf(skew);
    }

    // Generate samples using inverse transform sampling
    let mut samples = Vec::with_capacity(NUM_OPERATIONS);
    for _ in 0..NUM_OPERATIONS {
        let u: f64 = rng.next_f64();
        let mut sum: f64 = 0.0;
        let mut sample: usize = 1;

        while sample <= n {
            sum += 1.0 / (sample as f64).powf(skew) / norm;
            if sum >= u {
                break;
            }
            sample += 1;
        }

        samples.push(sample.saturating_sub(1) % n);
    }

    samples
}

fn benchmark_caches(c: &mut Criterion) {
    let cache_size = NonZeroUsize::new(CACHE_SIZE).unwrap();
    let samples = zipf_sample(CACHE_SIZE * 2, 0.8);

    let mut group = c.benchmark_group("Cache Mixed Access");

    // LRU Cache benchmarks
    group.bench_function("LRU", |b| {
        b.iter(|| {
            let mut cache = LruCache::new(cache_size);
            for &idx in &samples {
                if idx % 4 == 0 {
                    // 25% puts
                    black_box(cache.put(idx, idx));
                } else {
                    // 75% gets
                    black_box(cache.get(&idx));
                }
            }
        });
    });

    // LFU Cache benchmarks
    group.bench_function("LFU", |b| {
        b.iter(|| {
            let mut cache = LfuCache::new(cache_size);
            for &idx in &samples {
                if idx % 4 == 0 {
                    // 25% puts
                    black_box(cache.put(idx, idx));
                } else {
                    // 75% gets
                    black_box(cache.get(&idx));
                }
            }
        });
    });

    // LFUDA Cache benchmarks
    group.bench_function("LFUDA", |b| {
        b.iter(|| {
            let mut cache = LfudaCache::new(cache_size);
            for &idx in &samples {
                if idx % 4 == 0 {
                    // 25% puts
                    black_box(cache.put(idx, idx));
                } else {
                    // 75% gets
                    black_box(cache.get(&idx));
                }
            }
        });
    });

    // SLRU Cache benchmarks
    group.bench_function("SLRU", |b| {
        let protected_size = NonZeroUsize::new(CACHE_SIZE / 2).unwrap(); // 50% protected
        b.iter(|| {
            let mut cache = SlruCache::new(cache_size, protected_size);
            for &idx in &samples {
                if idx % 4 == 0 {
                    // 25% puts
                    black_box(cache.put(idx, idx));
                } else {
                    // 75% gets
                    black_box(cache.get(&idx));
                }
            }
        });
    });

    // GDSF Cache benchmarks
    group.bench_function("GDSF", |b| {
        b.iter(|| {
            let mut cache = GdsfCache::new(cache_size);
            for &idx in &samples {
                if idx % 4 == 0 {
                    // 25% puts - Use idx as size too, smaller items more likely due to Zipf
                    let size = ((idx % 100) + 1) as u64; // Size between 1-100
                    black_box(cache.put(idx, idx, size));
                } else {
                    // 75% gets
                    black_box(cache.get(&idx));
                }
            }
        });
    });

    group.finish();
}

criterion_group!(benches, benchmark_caches);
criterion_main!(benches);
