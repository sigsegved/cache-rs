# Cache Eviction and TTL Mechanisms: A Comparative Study of Production Systems

**A Technical Analysis of Eviction Strategies in High-Performance Caching Libraries**

---

## Abstract

This document presents a comprehensive analysis of eviction mechanisms and time-to-live (TTL) implementations across six widely-deployed caching systems: Caffeine (Java), Moka (Rust), Redis, CacheLib (Meta), Memcached, and Squid. We examine the architectural decisions, data structures, and algorithms these systems employ to balance hit rates, memory efficiency, and operational complexity. Our analysis reveals common patterns—particularly the tension between eager and lazy expiration strategies—and highlights innovations such as hierarchical timer wheels, lock amortization through write buffers, and probabilistic frequency estimation. These findings inform the design of TTL support for cache-rs, identifying techniques that can be adapted for a Rust-based caching library with both single-threaded and concurrent implementations.

---

## 1. Introduction

Cache eviction is the process of removing entries from a cache when capacity limits are exceeded or when entries become stale. The choice of eviction algorithm directly impacts cache hit rates, memory utilization, and system performance. Time-to-live (TTL) expiration adds temporal constraints, requiring caches to track when entries become invalid and ensure stale data is never served.

Production caching systems must balance several competing concerns: maximizing hit rates to reduce load on backing stores, minimizing memory overhead per entry, maintaining O(1) or amortized O(1) operation latency, and supporting concurrent access without excessive lock contention. Different systems make different trade-offs based on their deployment contexts.

This survey examines how six influential caching systems address these challenges. We focus particularly on mechanisms for TTL expiration and preferential eviction of expired entries, as these are the primary concerns for extending cache-rs with TTL support.

### 1.1 Systems Surveyed

| System | Language | Primary Use Case | Notable Features |
|--------|----------|------------------|------------------|
| **Caffeine** | Java | Application caching | Window-TinyLFU, timer wheels |
| **Moka** | Rust | Application caching | TinyLFU (Caffeine port), per-entry TTL |
| **Redis** | C | Distributed cache/datastore | Approximated LRU/LFU, probabilistic sampling |
| **CacheLib** | C++ | Large-scale caching (Meta) | Background TTL reaper, DRAM+Flash |
| **Memcached** | C | Distributed memory cache | Slab allocation, LRU per slab class |
| **Squid** | C++ | HTTP proxy cache | Heap-based LRU, refresh patterns |

---

## 2. Eviction Policy Algorithms

### 2.1 The Limitations of Pure LRU

Least Recently Used (LRU) eviction is popular due to its simplicity and O(1) implementation via a doubly-linked list combined with a hash map. However, LRU performs poorly under certain workloads. In scan-based access patterns, where a large set of keys is accessed sequentially once, LRU pollutes the cache with items unlikely to be accessed again. LRU also fails to capture frequency information: an item accessed once recently will be retained over an item accessed thousands of times slightly earlier.

These limitations motivated the development of more sophisticated policies that consider both recency and frequency.

### 2.2 Window-TinyLFU (Caffeine, Moka)

Caffeine introduced Window-TinyLFU (W-TinyLFU), which achieves near-optimal hit rates across diverse workloads while maintaining O(1) operations and low memory overhead. The architecture divides the cache into two regions:

**Admission Window (1% of capacity):** A small LRU queue that captures recent arrivals. Items enter here first, allowing the cache to identify recency bursts that pure frequency-based policies would reject.

**Main Space (99% of capacity):** A Segmented LRU (SLRU) divided into probationary and protected segments. Items promoted from the window compete against the main space's victim for admission.

**TinyLFU Admission Filter:** When an item is evicted from the window, it competes against the main space's LRU victim. A frequency sketch (4-bit CountMin Sketch) estimates each item's historical access frequency. The item with higher estimated frequency is retained.

The frequency sketch requires only 8 bytes per cache entry to achieve accuracy. Unlike policies such as ARC and LIRS that retain metadata for evicted keys (requiring 2-3x memory), TinyLFU discards evicted keys entirely.

**Adaptivity:** The window and main space sizes are dynamically adjusted using hill climbing optimization. The algorithm samples hit rates and adjusts the partition to find the optimal balance for the current workload. This allows W-TinyLFU to handle both recency-biased and frequency-biased workloads effectively.

```
┌─────────────────────────────────────────────────────────────────┐
│                    Window-TinyLFU Architecture                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│   ┌──────────┐                    ┌───────────────────────────┐ │
│   │  Window  │    TinyLFU         │        Main Space         │ │
│   │   LRU    │ ──Admission───────▶│  ┌─────────┐ ┌─────────┐  │ │
│   │   (1%)   │    Filter          │  │Probation│ │Protected│  │ │
│   └──────────┘                    │  │  SLRU   │ │  SLRU   │  │ │
│        ▲                          │  └─────────┘ └─────────┘  │ │
│        │                          │           (99%)           │ │
│    New Items                      └───────────────────────────┘ │
│                                                                  │
│   ┌──────────────────────────────────────────────────────────┐  │
│   │              4-bit CountMin Sketch                        │  │
│   │         (Frequency estimation, 8 bytes/entry)             │  │
│   └──────────────────────────────────────────────────────────┘  │
└─────────────────────────────────────────────────────────────────┘
```

### 2.3 Approximated LRU/LFU (Redis)

Redis takes a fundamentally different approach optimized for memory efficiency: probabilistic sampling rather than exact tracking. Instead of maintaining explicit ordering structures, Redis samples a small number of keys randomly and evicts the one with the oldest access time (LRU) or lowest frequency (LFU).

**Sampling Pool:** Redis 3.0+ maintains a pool of eviction candidates. When eviction is needed, Redis samples `maxmemory-samples` keys (default 5), adds them to the pool if they are better candidates than existing entries, and evicts the best candidate. This achieves results very close to true LRU with far less memory overhead.

**Morris Counters for LFU:** Redis's LFU implementation uses 8-bit Morris counters with probabilistic increments. When an item is accessed, the counter only increments with decreasing probability as the count increases. This allows a single byte to represent access frequencies spanning several orders of magnitude. Counter decay ensures that historical frequency eventually diminishes, allowing the algorithm to adapt to changing access patterns.

The key insight is that exact tracking is often unnecessary. For eviction decisions, we only need to identify *relatively* less valuable items, not compute exact rankings.

### 2.4 Slab-Based LRU (Memcached)

Memcached uses slab allocation to reduce memory fragmentation: items are grouped into size classes, and each slab class maintains its own LRU list. This design introduces an interesting eviction dynamic: when a slab fills, Memcached evicts from that slab's LRU even if other slabs have older items.

This per-slab-class eviction can cause suboptimal behavior when item size distribution changes over time. However, it enables efficient memory management and avoids fragmentation issues that plague general-purpose allocators under cache workloads.

---

## 3. TTL Expiration Mechanisms

TTL expiration mechanisms fall into two broad categories: eager expiration (proactive removal by background processes) and lazy expiration (removal upon access or eviction). Each approach has distinct trade-offs.

### 3.1 Lazy Expiration (Caffeine, Redis)

Caffeine does not proactively remove expired entries. Instead, expired entries are removed during regular cache maintenance, which is triggered by cache operations. This approach prioritizes throughput over timeliness of expiration.

```java
// Caffeine's approach: expiration checked during maintenance
LoadingCache<Key, Graph> graphs = Caffeine.newBuilder()
    .expireAfterWrite(10, TimeUnit.MINUTES)
    .build(key -> createExpensiveGraph(key));
```

**Scheduler-Assisted Cleanup:** Caffeine allows registering a `Scheduler` to trigger maintenance proactively. When provided, the scheduler estimates when the next entry will expire and schedules a cleanup task. This is not guaranteed—execution depends on thread availability—but it improves timeliness without dedicated background threads.

```java
// Scheduler enables prompt expiration removal
LoadingCache<Key, Graph> graphs = Caffeine.newBuilder()
    .scheduler(Scheduler.systemScheduler())
    .expireAfterWrite(10, TimeUnit.MINUTES)
    .build(key -> createExpensiveGraph(key));
```

**Redis's Lazy + Probabilistic Proactive Approach:** Redis combines lazy expiration (checking TTL on access) with periodic probabilistic cleanup. A background task samples random keys with TTL set and removes expired ones. The sampling rate adapts based on the proportion of expired keys found, intensifying cleanup when many keys are expiring.

### 3.2 Eager Expiration (CacheLib)

CacheLib, Meta's caching library designed for large-scale deployments, uses a dedicated background reaper thread. The reaper scans the cache at configurable intervals, removing expired items to reclaim memory proactively.

```cpp
// CacheLib TTL reaper configuration
config.enableItemReaperInBackground(
    std::chrono::milliseconds(10000),  // 10 second interval
    reaperConfig
);
```

The reaper is throttled to control CPU usage variance. Statistics track items visited versus items reaped, enabling operators to tune the interval based on expiration patterns.

**Trade-off:** Eager expiration provides more predictable memory reclamation and ensures eviction handlers fire in a timely manner. However, it requires background threads (not suitable for `no_std` environments) and introduces synchronization overhead even for caches that could otherwise be lock-free.

### 3.3 Hierarchical Timer Wheels (Caffeine, Moka)

For variable per-entry expiration (where each entry can have a different TTL), Caffeine and Moka use hierarchical timer wheels. This data structure, originally designed for network timer management, provides O(1) insertion, deletion, and expiration with bounded memory overhead.

A timer wheel consists of multiple levels of circular buckets. Each level represents a different time granularity (e.g., milliseconds, seconds, minutes). Entries are hashed into buckets based on their expiration time. When time advances, the wheel "ticks," promoting entries from coarser to finer levels until they reach the innermost wheel and expire.

```
┌──────────────────────────────────────────────────────────────┐
│                 Hierarchical Timer Wheel                      │
├──────────────────────────────────────────────────────────────┤
│                                                               │
│  Level 0 (ms)    Level 1 (sec)    Level 2 (min)              │
│  ┌─┬─┬─┬─┐       ┌─┬─┬─┬─┐        ┌─┬─┬─┬─┐                  │
│  │ │ │ │ │       │ │ │ │ │        │ │ │●│ │   ← Entry with   │
│  └─┴─┴●┴─┘       └─┴●┴─┴─┘        └─┴─┴─┴─┘     5min TTL     │
│       ↑              ↑                                        │
│    current        next tick                                   │
│      tick         cascades                                    │
│                                                               │
│  Operations: Insert O(1), Cancel O(1), Expire O(1) amortized │
└──────────────────────────────────────────────────────────────┘
```

**Moka's Implementation:** Moka v0.12 removed background threads entirely. Timer wheel maintenance is performed during cache operations, similar to Caffeine. This eliminated the previous reliance on background threads while maintaining support for per-entry variable expiration.

---

## 4. Concurrency Strategies

High-performance concurrent caches must minimize lock contention without sacrificing correctness. The systems surveyed employ several innovative techniques.

### 4.1 Read/Write Buffers with Lock Amortization (Caffeine)

Caffeine avoids locking on every operation by recording operations in thread-local buffers and applying them in batches. This technique, called lock amortization, spreads the cost of synchronization across many operations.

**Read Buffer:** A striped ring buffer records access events. Each thread hashes to a stripe, reducing contention. When a stripe fills, the thread attempts to acquire the cache's exclusive lock and drain the buffer. If the lock is held, the thread returns immediately—read events can be lost without affecting correctness, only eviction policy accuracy.

**Write Buffer:** Writes cannot be lost, so the write buffer is a bounded queue that blocks when full. The priority of draining writes ensures the buffer typically stays small or empty.

```
┌─────────────────────────────────────────────────────────────────┐
│                  Caffeine Concurrency Model                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Thread 1        Thread 2        Thread 3                        │
│     │               │               │                            │
│     ▼               ▼               ▼                            │
│  ┌─────┐         ┌─────┐         ┌─────┐                        │
│  │Read │         │Read │         │Read │    Striped read        │
│  │Buf 0│         │Buf 1│         │Buf 2│    buffers (lossy)     │
│  └──┬──┘         └──┬──┘         └──┬──┘                        │
│     │               │               │                            │
│     └───────────────┼───────────────┘                            │
│                     ▼                                            │
│              ┌────────────┐                                      │
│              │Write Buffer│    Bounded queue (blocking)          │
│              └──────┬─────┘                                      │
│                     ▼                                            │
│              ┌────────────┐                                      │
│              │  Eviction  │    Single consumer drains            │
│              │   Policy   │    under exclusive lock              │
│              └────────────┘                                      │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Segment Sharding (Moka, ConcurrentHashMap)

An alternative to buffering is segment sharding: partitioning the cache into independent segments, each with its own lock. Operations on different segments proceed in parallel.

Moka uses this approach, with segments based on key hash. The trade-off is that eviction ordering becomes per-segment rather than global. An item in segment A might be evicted while segment B has older items. For most workloads with good key distribution, this approximation is acceptable.

### 4.3 Lock-Free Structures with Entry State Machines (Caffeine)

Caffeine uses atomic state transitions to handle races from concurrent operations. An entry can be in one of three states: alive (in both hash table and queues), retired (removed from hash table, pending queue removal), or dead (fully removed). State transitions are atomic, ensuring consistency even when operations are recorded and replayed out of order.

---

## 5. Memory Efficiency Techniques

### 5.1 Frequency Sketch (CountMin Sketch)

TinyLFU uses a 4-bit CountMin Sketch to estimate access frequencies. This probabilistic data structure uses multiple hash functions to map keys to counter arrays. The minimum count across hash locations estimates the key's frequency, with bounded overcount probability.

```
┌─────────────────────────────────────────────────────┐
│              4-bit CountMin Sketch                   │
├─────────────────────────────────────────────────────┤
│                                                      │
│   Hash 1:  [3][0][7][2][1][0][4][8]...              │
│   Hash 2:  [1][2][3][0][0][1][2][3]...              │
│   Hash 3:  [0][1][2][3][4][5][6][7]...              │
│   Hash 4:  [2][2][2][2][2][2][2][2]...              │
│                                                      │
│   estimate(key) = min(hash1[h1(key)], hash2[h2(key)]│
│                       hash3[h3(key)], hash4[h4(key)]│
│                                                      │
│   Memory: 8 bytes per cache entry (4 counters × 4b) │
│   Error: Bounded overcount, no undercount           │
└─────────────────────────────────────────────────────┘
```

The sketch periodically "ages" by halving all counters, allowing the frequency estimate to adapt to changing access patterns.

### 5.2 Morris Counters (Redis)

Redis's LFU uses 8-bit Morris counters for frequency tracking. Unlike regular counters, Morris counters increment probabilistically, with the probability decreasing as the count increases. This allows representing frequencies up to millions in a single byte.

```
Probability of increment = 1 / (current_count * lfu_log_factor + 1)
```

With `lfu_log_factor=10`, approximately 100 hits are needed to reach count 10, and millions of hits to saturate at 255. Counters decay over time (default: every minute), preventing stale frequency data from dominating.

### 5.3 Avoiding Key Duplication

Several systems store keys only once to reduce memory:
- **Caffeine:** Entry objects contain the key; the hash table stores entry references.
- **Redis:** Keys are stored in the main dictionary; TTL metadata references the same key.

Cache-rs currently duplicates keys in both the HashMap and List entries. This is a potential optimization target.

---

## 6. TTL Implementation Patterns

### 6.1 Pattern: Per-Entry Expiry Field

All surveyed systems store expiration as an absolute timestamp rather than relative TTL. This avoids recomputing expiration on every access and simplifies comparison.

```rust
// Absolute expiry (preferred)
struct Entry {
    expiry: Option<u64>,  // nanoseconds since epoch
}

fn is_expired(&self) -> bool {
    self.expiry.map(|e| now() >= e).unwrap_or(false)
}
```

### 6.2 Pattern: Lazy Check on Access

All systems check TTL during access operations. This guarantees stale data is never returned, regardless of background expiration status.

```rust
fn get(&mut self, key: &K) -> Option<&V> {
    let entry = self.map.get(key)?;
    if entry.is_expired() {
        self.remove_internal(key, EvictionReason::Expired);
        return None;
    }
    // ... proceed with access
}
```

### 6.3 Pattern: Preferential Expired Eviction

When eviction is needed, systems prefer evicting expired entries over valid but low-priority entries. Redis's `volatile-ttl` policy explicitly evicts keys with shortest remaining TTL. Caffeine's maintenance drains expired entries before considering LRU victims.

For cache-rs, this motivates tracking expired entries in a min-heap ordered by expiration time, enabling O(log n) identification of the next-to-expire entry.

### 6.4 Pattern: Stale-While-Revalidate

HTTP caching (Squid, Varnish) supports stale-while-revalidate: serving stale content to the current request while refreshing asynchronously. This reduces latency for cache "misses" that are only slightly stale.

```
              Request arrives
                    │
                    ▼
            ┌───────────────┐
            │  Entry found? │
            └───────┬───────┘
                    │ Yes
                    ▼
            ┌───────────────┐
            │   Expired?    │
            └───────┬───────┘
                    │ Yes
                    ▼
            ┌───────────────────┐
            │ Within stale-     │
            │ while-revalidate? │
            └───────┬───────────┘
                    │ Yes
                    ▼
        ┌───────────────────────────┐
        │ Return stale value        │
        │ + Trigger async refresh   │
        └───────────────────────────┘
```

---

## 7. Comparative Analysis

### 7.1 Expiration Strategy Comparison

| System | Primary Strategy | Background Thread | Latency Guarantee |
|--------|-----------------|-------------------|-------------------|
| Caffeine | Lazy + Scheduler | Optional | Best-effort |
| Moka | Lazy (on operation) | No (v0.12+) | Best-effort |
| Redis | Lazy + Probabilistic eager | Yes | Probabilistic |
| CacheLib | Eager (reaper) | Required | Configurable |
| Memcached | Lazy | No | On access only |
| Squid | Lazy + Refresh patterns | Yes | HTTP semantics |

### 7.2 Data Structure Comparison

| Feature | Caffeine | Moka | Redis | CacheLib |
|---------|----------|------|-------|----------|
| Frequency tracking | CountMin Sketch | CountMin Sketch | Morris Counter | None |
| TTL structure | Timer wheel | Timer wheel | Sampled | Background scan |
| Concurrency | Lock amortization | Segment sharding | Single-threaded* | Lock-based |
| Memory overhead | ~80 bytes/entry | ~100 bytes/entry | ~50 bytes/entry | Variable |

*Redis is single-threaded per shard; clustering provides scalability.

### 7.3 Suitability for cache-rs

Based on this analysis, the following techniques are most suitable for cache-rs:

1. **Lazy expiration with preferential eviction:** No background threads required, `no_std` compatible, maintains performance for single-threaded caches.

2. **Min-heap for expiry tracking:** O(log n) insertion, O(1) peek at next-to-expire, simple implementation. Lazy deletion handles removal without O(n) heap search.

3. **Optional scheduler for concurrent caches:** Following Caffeine's model, allow users to provide a scheduler for proactive cleanup when desired.

4. **Absolute timestamps:** Store expiration as absolute nanoseconds, avoiding repeated TTL arithmetic.

5. **Entry state machine:** Use atomic state transitions to handle concurrent access/eviction races in concurrent implementations.

---

## 8. Conclusions

Our survey reveals that production caching systems have converged on several key principles:

**Lazy expiration is sufficient for correctness.** All systems check TTL on access, ensuring stale data is never returned. Background expiration is an optimization, not a requirement.

**Preferential expired eviction improves behavior.** When capacity eviction is needed, removing expired entries first avoids evicting valid data unnecessarily.

**Lock amortization enables high concurrency.** Caffeine's read/write buffer approach achieves excellent throughput by batching synchronization costs.

**Probabilistic data structures reduce memory overhead.** CountMin Sketches and Morris counters provide useful frequency estimates with minimal memory.

**Timer wheels enable efficient variable TTL.** For per-entry expiration, hierarchical timer wheels provide O(1) operations without sorted data structures.

For cache-rs, we recommend adopting the hybrid lazy expiration model with a min-heap for tracking soon-to-expire entries. This approach maintains the library's `no_std` compatibility and lock-free single-threaded performance while providing robust TTL semantics. Optional background reaping can be offered for concurrent caches when the `std` feature is enabled.

---

## References

1. Manes, B. "Caffeine: A High Performance, Near Optimal Caching Library." GitHub, https://github.com/ben-manes/caffeine

2. Einziger, G., Friedman, R., and Manes, B. "TinyLFU: A Highly Efficient Cache Admission Policy." ACM Transactions on Storage, 2017.

3. Megiddo, N. and Modha, D.S. "ARC: A Self-Tuning, Low Overhead Replacement Cache." USENIX FAST, 2003.

4. Jiang, S. and Zhang, X. "LIRS: An Efficient Low Inter-reference Recency Set Replacement Policy to Improve Buffer Cache Performance." ACM SIGMETRICS, 2002.

5. Redis Documentation. "Key Eviction." https://redis.io/docs/latest/develop/reference/eviction/

6. CacheLib Documentation. "TTL Reaper." https://cachelib.org/docs/Cache_Library_User_Guides/ttl_reaper/

7. Moka Documentation. https://github.com/moka-rs/moka

8. Varghese, G. and Lauck, T. "Hashed and Hierarchical Timing Wheels: Data Structures for the Efficient Implementation of a Timer Facility." IEEE/ACM Transactions on Networking, 1997.

9. Squid Cache Documentation. http://www.squid-cache.org/Doc/

---

## Appendix A: Glossary

**CountMin Sketch:** A probabilistic data structure for frequency estimation using multiple hash functions and counter arrays.

**Morris Counter:** A probabilistic counter that increments with decreasing probability, enabling representation of large counts in few bits.

**Timer Wheel:** A circular buffer data structure for timer management, providing O(1) insertion and expiration.

**Lock Amortization:** A technique that batches operations to spread synchronization costs across multiple operations.

**Stale-While-Revalidate:** An HTTP caching pattern that serves stale content while refreshing asynchronously in the background.

**W-TinyLFU:** Window-TinyLFU, an admission policy combining a small recency window with frequency-based filtering.
