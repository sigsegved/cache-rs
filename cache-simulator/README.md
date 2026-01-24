# Cache Simulator

A tool for simulating and comparing different cache eviction algorithms with realistic traffic patterns.

## Features

- Simulate multiple cache eviction algorithms:
  - LRU (Least Recently Used)
  - LFU (Least Frequently Used)
  - LFUDA (LFU with Dynamic Aging)
  - SLRU (Segmented LRU)
  - GDSF (Greedy Dual Size Frequency)
  - [Moka](https://crates.io/crates/moka) (external high-performance cache for comparison)
- Compare sequential vs concurrent cache implementations
- Generate realistic traffic logs with configurable parameters
- Parallel processing for large-scale simulations
- Standalone traffic generator for creating large-scale test data

## Usage

### Running Simulations

```bash
# Run simulation with default test data
cargo run

# Run simulation with specific algorithms
cargo run -- --algorithms lru,lfu,gdsf

# Run simulation with custom memory and disk size
cargo run -- --memory-size 4 --disk-size 100

# Run simulation with custom input directory
cargo run -- --input-dir my_logs
```

### Using the Traffic Log Generator

The traffic generator creates realistic cache request logs with configurable parameters. You can use it either as a subcommand of cache-simulator or as a standalone binary:

#### As a subcommand:

```bash
# Generate logs with default settings
cargo run -- generate

# Generate high-volume video streaming traffic scenario
cargo run -- generate \
  --rps 1000 \
  --duration 24 \
  --objects 100000 \
  --popular-traffic 80 \
  --popular-objects 20 \
  --min-size 5 \
  --max-size 50 \
  --min-ttl 1 \
  --max-ttl 24 \
  --output video_logs
```

#### As a standalone binary:

```bash
# Generate logs with default settings
cargo run --bin traffic-generator

# Generate social media CDN traffic scenario
cargo run --bin traffic-generator -- \
  --rps 5000 \
  --duration 12 \
  --objects 500000 \
  --popular-traffic 90 \
  --popular-objects 10 \
  --min-size 0.01 \
  --max-size 5 \
  --min-ttl 24 \
  --max-ttl 168 \
  --output social_logs
```

### Full Command-Line Options

#### Traffic Generator (both standalone and subcommand)
```
USAGE:
    cache-simulator generate [OPTIONS]
    # OR
    traffic-generator [OPTIONS]

OPTIONS:
    --rps <RPS>                     Requests per second [default: 100]
    --duration <HOURS>              Duration in hours [default: 24]
    --objects <COUNT>               Number of unique objects [default: 10000]
    --popular-traffic <PERCENT>     Percentage of traffic from popular objects [default: 80]
    --popular-objects <PERCENT>     Percentage of objects that are popular [default: 20]
    --min-size <MB>                 Minimum object size in MB [default: 1]
    --max-size <MB>                 Maximum object size in MB [default: 10]
    --min-ttl <HOURS>               Minimum TTL in hours [default: 1]
    --max-ttl <HOURS>               Maximum TTL in hours [default: 24]
    -o, --output <DIR>              Output directory [default: traffic_logs]
```

#### Simulator
```
USAGE:
    cache-simulator simulate [OPTIONS]

OPTIONS:
    -i, --input-dir <DIR>          Directory containing log files
    -m, --memory-size <MB>         Memory size in megabytes [default: 1]
    -d, --disk-size <MB>           Disk size in megabytes [default: 50]
    -a, --algorithms <ALGOS>       Algorithms to simulate (lru, lfu, lfuda, slru, gdsf, moka)
        --mode <MODE>              Cache mode: sequential, concurrent, or both [default: both]
        --segments <COUNT>         Number of segments for concurrent caches [default: 16]
    -c, --capacity <COUNT>         Override cache capacity (number of objects)
        --output-csv <PATH>        Export results to CSV file
```

## Generated Traffic Pattern

The traffic generator creates realistic access patterns with the following characteristics:

1. **Shifting Popularity**: Object popularity shifts over time
2. **Zipf Distribution**: Within each popularity group, objects follow a Zipf-like distribution
3. **Size Distribution**: Object sizes are correlated with popularity (configurable)
4. **TTL Variation**: Objects have varied time-to-live settings
5. **Parallelized Generation**: One file per hour, generated in parallel

## Examples

### Video Streaming Service

```bash
cargo run -- generate \
  --rps 1000 \
  --duration 24 \
  --objects 100000 \
  --popular-traffic 80 \
  --popular-objects 20 \
  --min-size 5 \
  --max-size 50 \
  --min-ttl 1 \
  --max-ttl 24 \
  --output video_logs
```

### Social Media CDN

```bash
cargo run -- generate \
  --rps 5000 \
  --duration 12 \
  --objects 500000 \
  --popular-traffic 90 \
  --popular-objects 10 \
  --min-size 0.01 \
  --max-size 5 \
  --min-ttl 24 \
  --max-ttl 168 \
  --output social_logs
```

### API Gateway Cache

```bash
cargo run -- generate \
  --rps 10000 \
  --duration 6 \
  --objects 50000 \
  --popular-traffic 95 \
  --popular-objects 5 \
  --min-size 0.001 \
  --max-size 0.1 \
  --min-ttl 0.01 \
  --max-ttl 1 \
  --output api_logs
```
