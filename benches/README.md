# Cache Benchmarks

This directory contains benchmarks for comparing the performance of different cache implementations.

## Running Benchmarks

The benchmarks use criterion.rs and work on stable Rust:

```bash
# Run the benchmarks
cargo bench
```

## Benchmark Types

1. **Mixed Access Pattern**: Simulates a realistic workload with a mix of puts and gets following a Zipf distribution
2. **Individual Operations**: Measures specific operations like get hits, get misses, and puts for detailed analysis

## Interpreting Results

The benchmark results show the time per operation in nanoseconds or microseconds, where lower values are better. 

### Sample Results

Based on our testing, here are typical performance characteristics:

- **LRU**: Fastest for simple operations (~887ns get hit)
- **SLRU**: Good balance (~983ns get hit) 
- **GDSF**: More complex but size-aware (~7.5µs get hit)
- **LFU/LFUDA**: Higher overhead due to frequency tracking (~20-22µs get hit)

Results may vary based on your specific hardware and workload characteristics.
