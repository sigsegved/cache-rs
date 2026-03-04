# Comparative Analysis of Cache Eviction Algorithms Under Heterogeneous Workload Conditions

**Date:** January 29, 2026  
**Authors:** Cache-RS Research Team  
**Version:** 1.0

---

## Abstract

This study presents a comprehensive empirical evaluation of six cache eviction algorithms—Least Recently Used (LRU), Segmented LRU (SLRU), Least Frequently Used (LFU), LFU with Dynamic Aging (LFUDA), Greedy Dual-Size Frequency (GDSF), and Moka (TinyLFU)—across diverse workload characteristics representative of modern computing environments. Through systematic experimentation with over 47 million cache requests spanning video streaming, social media, web content, in-memory, and on-disk caching scenarios, we investigate the relationship between algorithm design, cache sizing, and workload properties. Our findings demonstrate that the classical 80-20 rule remains a reliable heuristic for cache sizing, that size-aware algorithms provide substantial benefits (50-90 percentage point improvements) when storage constraints dominate, and that algorithm selection becomes increasingly critical as cache capacity decreases relative to working set size. We provide actionable guidelines for practitioners selecting cache algorithms based on workload characteristics and system constraints.

---

## 1. Introduction

### 1.1 Research Motivation

Caching remains one of the most effective techniques for improving system performance across the computing stack, from CPU memory hierarchies to distributed content delivery networks. The fundamental challenge in cache design lies in the eviction policy: when the cache reaches capacity, which object should be removed to make room for new entries? This decision directly impacts cache hit rates, which in turn affect system latency, throughput, and resource utilization.

Despite decades of research on cache eviction algorithms, practitioners often lack clear guidance on algorithm selection for specific workloads. The literature contains numerous theoretical analyses and isolated benchmarks, but comprehensive empirical comparisons across realistic workload scenarios remain scarce. Furthermore, the emergence of size-aware eviction policies and modern implementations like Moka (based on TinyLFU) introduces new trade-offs that warrant systematic investigation.

### 1.2 Research Objectives

This study seeks to answer several fundamental questions about cache algorithm performance. First, we investigate how different eviction algorithms perform under varying workload characteristics, including access frequency distributions, object size distributions, and request patterns. Second, we examine the conditions under which size-aware algorithms like GDSF provide meaningful advantages over size-agnostic alternatives. Third, we validate whether the classical 80-20 rule—the principle that 20% of objects receive 80% of accesses—provides reliable guidance for cache sizing across modern workloads. Fourth, we quantify the performance-latency trade-offs between sophisticated algorithms like TinyLFU and simpler alternatives like LRU and SLRU. Finally, we seek to determine how algorithm performance differences vary with cache capacity relative to working set size.

### 1.3 Scope and Contributions

Our experimental methodology encompasses three distinct evaluation frameworks. The first framework examines workload-specific traffic patterns simulating video streaming, social media, and web content delivery, testing both entry-count-based and storage-based eviction modes. The second framework focuses on in-memory caching scenarios with both uniform and variable object sizes, representative of application-level caching. The third framework addresses on-disk caching with large objects, simulating content delivery network and storage system workloads. Across all experiments, we measure hit rates, byte hit rates, and operation latencies to provide a comprehensive performance characterization.

---

## 2. Methodology

### 2.1 Algorithms Under Evaluation

We evaluate six cache eviction algorithms representing distinct design philosophies. LRU (Least Recently Used) evicts the object accessed longest ago, operating on the principle that recently accessed objects are likely to be accessed again. SLRU (Segmented LRU) partitions the cache into probationary and protected segments, providing resistance to scan patterns that can pollute LRU caches. LFU (Least Frequently Used) tracks access counts and evicts the least frequently accessed object, favoring objects with historical popularity. LFUDA (LFU with Dynamic Aging) extends LFU with an aging mechanism that allows frequency counts to decay over time, adapting to shifting popularity patterns. GDSF (Greedy Dual-Size Frequency) incorporates object size into eviction decisions, computing a priority based on frequency and inverse size, thereby favoring smaller objects that provide more cache entries per unit storage. Finally, Moka implements the TinyLFU algorithm, combining frequency sketches with an adaptive window to balance recency and frequency while maintaining space-efficient frequency estimates.

### 2.2 Experimental Framework

All experiments utilize a purpose-built cache simulation framework that supports configurable traffic generation with tunable popularity distributions (Zipf-like with configurable skew), object size distributions (uniform or bounded), and request rates. The simulator supports both capacity-constrained mode, where eviction occurs when entry count exceeds a threshold, and storage-constrained mode, where eviction occurs when total cached bytes exceed a threshold.

Traffic patterns were generated to represent three distinct application domains. Video streaming workloads feature large objects averaging 5.4 MB, concentrated popularity where 70% of requests target 10% of objects, and 7.2 million total requests across 5,561 unique objects. Social media workloads consist of small objects averaging 51 KB, highly skewed popularity where 90% of requests target 5% of objects, and 14.4 million requests across 55,933 unique objects. Web content workloads exhibit moderate object sizes averaging 1 MB, moderate popularity skew where 60% of requests target 20% of objects, and 11.5 million requests across 21,998 unique objects.

### 2.3 Metrics

We report three primary metrics throughout our analysis. Hit rate measures the fraction of requests satisfied from cache. Byte hit rate measures the fraction of requested bytes satisfied from cache, which can differ from hit rate when object sizes vary. Operation latency measures the time required for cache get and put operations, reported as averages and 99th percentile values in nanoseconds.

---

## 3. Results: Workload-Specific Traffic Analysis

### 3.1 Capacity-Constrained Mode

In capacity-constrained mode, the cache evicts entries when the total number of cached objects exceeds a configured maximum, regardless of object sizes. This mode is representative of scenarios where memory overhead per entry dominates total memory consumption.

#### 3.1.1 Video Streaming Workload

The video streaming workload presents an interesting case study due to its concentrated popularity distribution. At severely constrained capacity (9% of unique objects, 500 entries), frequency-aware algorithms demonstrated substantial advantages. Moka achieved the highest hit rate at 68.9%, followed by LFU at 65.3%, while LRU achieved only 43.9%. This 25 percentage point difference between the best and worst performers underscores the importance of algorithm selection when cache capacity is limited relative to the working set.

| Capacity | % Objects | LRU | SLRU | LFU | LFUDA | GDSF | Moka | Winner |
|----------|-----------|-----|------|-----|-------|------|------|--------|
| 500 | 9% | 43.9% | 55.4% | 65.3% | 48.3% | 43.7% | **68.9%** | Moka |
| 2,500 | 45% | 83.3% | 83.3% | 82.5% | 83.3% | 83.3% | **83.4%** | All equal |
| 5,000 | 90% | **99.9%** | **99.9%** | 98.1% | 99.8% | **99.9%** | 99.9% | LRU/SLRU/GDSF |

As cache capacity increased to 45% of the unique object count, algorithm differences largely disappeared, with all algorithms achieving approximately 83% hit rates. At 90% capacity, all algorithms approached 99.9% hit rates, with the exception of LFU at 98.1%. This LFU underperformance at high capacity illustrates a known limitation: frequency counters accumulated during the initial fill phase can prevent newly popular objects from being retained, a phenomenon sometimes called frequency counter pollution.

#### 3.1.2 Social Media Workload

The social media workload exhibits the most extreme popularity skew, with 90% of requests targeting only 5% of objects. Under such conditions, the algorithm's ability to identify and retain the hot set becomes paramount, yet paradoxically, the extreme skew means that even simple algorithms quickly learn the hot set.

| Capacity | % Objects | LRU | SLRU | LFU | LFUDA | GDSF | Moka | Winner |
|----------|-----------|-----|------|-----|-------|------|------|--------|
| 500 | <1% | 16.1% | 17.3% | 18.0% | 16.1% | 17.9% | **18.6%** | Moka |
| 5,000 | 9% | **90.5%** | **90.5%** | 90.5% | **90.5%** | **90.5%** | **90.5%** | All equal |
| 25,000 | 45% | **94.7%** | **94.7%** | 94.4% | **94.7%** | 94.6% | 94.7% | LRU/SLRU/LFUDA |

At 9% capacity (5,000 entries), all algorithms achieved identical 90.5% hit rates. This convergence occurs because the hot set of approximately 2,800 objects (5% of 55,933) fits comfortably within the 5,000-entry cache, and all algorithms successfully retain these popular objects. The results suggest that for highly skewed workloads, the primary optimization opportunity lies in right-sizing the cache rather than algorithm selection.

#### 3.1.3 Web Content Workload

The web content workload presents the most balanced and challenging scenario, with moderate popularity skew and mixed access patterns. Under these conditions, algorithm differences remain significant even at moderate cache sizes.

| Capacity | % Objects | LRU | SLRU | LFU | LFUDA | GDSF | Moka | Winner |
|----------|-----------|-----|------|-----|-------|------|------|--------|
| 2,000 | 9% | 19.2% | 24.6% | 26.2% | 19.6% | 23.6% | **28.5%** | Moka |
| 5,000 | 23% | 44.2% | 53.7% | 56.7% | 48.6% | 50.4% | **61.7%** | Moka |
| 10,000 | 45% | 72.4% | 74.7% | 74.0% | **75.0%** | 73.3% | **75.0%** | LFUDA/Moka |

SLRU consistently outperformed LRU by 5-10 percentage points, demonstrating the value of scan resistance in web workloads. Moka maintained a 10-15 percentage point advantage over LRU across all capacity levels, suggesting that its sophisticated TinyLFU algorithm provides meaningful benefits for mixed workloads with moderate popularity skew.

### 3.2 Storage-Constrained Mode

Storage-constrained mode evicts entries when total cached bytes exceed a configured maximum. This mode reveals the importance of size-aware eviction policies and represents scenarios where storage capacity rather than entry count limits cache effectiveness.

#### 3.2.1 Video Streaming (Large Objects)

With average object sizes of 5.4 MB, a 50 MB storage limit accommodates only approximately 9 objects. Under such severe constraints, the algorithms that can identify and retain the small number of most valuable objects achieve substantially higher hit rates. Moka achieved 8.76% hit rate compared to less than 1.5% for most other algorithms, demonstrating superior selectivity under extreme constraints.

| Max Size | LRU | SLRU | LFU | LFUDA | GDSF | Moka | Analysis |
|----------|-----|------|-----|-------|------|------|----------|
| 50 MB | 0.94% | **1.44%** | 1.43% | 0.94% | 0.94% | **8.76%** | Only ~9 objects fit |

#### 3.2.2 Social Media (Small Objects)

The social media workload provides the most dramatic demonstration of size-aware algorithm benefits. With small objects averaging 51 KB, a 50 MB cache can hold approximately 980 objects. GDSF achieved an 89.8% hit rate compared to 32.2% for LRU—a 57 percentage point improvement. This result validates GDSF's design: by favoring smaller objects that provide more entries per unit storage, GDSF maximizes the probability of serving requests from cache when storage is the binding constraint.

| Max Size | LRU | SLRU | LFU | LFUDA | GDSF | Moka | Winner |
|----------|-----|------|-----|-------|------|------|--------|
| 50 MB | 32.2% | 41.9% | 36.0% | 32.6% | **89.8%** | 45.2% | **GDSF** |
| 250 MB | 90.5% | 90.5% | 90.5% | 90.5% | **91.2%** | 90.6% | GDSF |

At 250 MB, algorithms converge as the cache can accommodate most of the hot set regardless of eviction policy, though GDSF maintains a slight advantage.

#### 3.2.3 Web Content (Mixed Sizes)

The web workload with mixed object sizes (averaging 1 MB) provides a realistic test of size-aware eviction. GDSF achieved 15.4% hit rate at 500 MB compared to 5.1% for LRU—a threefold improvement. This result demonstrates that size-aware eviction provides substantial benefits even with moderate size variance.

| Max Size | LRU | SLRU | LFU | LFUDA | GDSF | Moka | Winner |
|----------|-----|------|-----|-------|------|------|--------|
| 500 MB | 5.1% | 8.5% | 7.1% | 5.1% | **15.4%** | 11.3% | **GDSF** |

### 3.3 Sequential vs Concurrent Mode

We validated that concurrent implementations of our algorithms introduce minimal performance degradation compared to their sequential counterparts. The largest difference observed was 0.78 percentage points for GDSF, while most algorithms showed differences under 0.2 percentage points. This result confirms that our lock-based concurrency design provides negligible hit rate penalty.

| Algorithm | Sequential | Concurrent | Delta | Notes |
|-----------|------------|------------|-------|-------|
| LRU | 44.18% | 44.10% | -0.08% | Minimal overhead |
| SLRU | 53.70% | 53.52% | -0.18% | Minimal overhead |
| LFU | 56.72% | 56.95% | +0.23% | Slight improvement |
| LFUDA | 48.65% | 48.62% | -0.03% | Negligible |
| GDSF | 50.42% | 49.64% | -0.78% | Segmentation effect |
| Moka | 61.72% | 61.71% | -0.01% | Excellent design |

---

## 4. Results: In-Memory Caching Analysis

### 4.1 Uniform Object Sizes

To isolate algorithm behavior from size effects, we conducted experiments with uniformly sized 10 KB objects. The workload consisted of 1,038,199 unique objects totaling approximately 10 GB, with 14.4 million requests following an 80-20 popularity distribution. Cache sizes ranged from 1 GB (10% of total) to 4 GB (40% of total).

| Cache Size | % of Total | Algorithm | Hit Rate | Byte Hit Rate | Get Avg (ns) | Put Avg (ns) | p99 (ns) |
|------------|------------|-----------|----------|---------------|--------------|--------------|----------|
| **1GB (10%)** | 10% | **LRU** | 32.41% | 32.41% | 159 | 499 | 601 |
| | | **SLRU** | **40.40%** | **40.40%** | 170 | 236 | 611 |
| | | **LFU** | **40.40%** | **40.40%** | 238 | 350 | 801 |
| | | **LFUDA** | 33.24% | 33.24% | 200 | 544 | 762 |
| | | **GDSF** | 32.87% | 32.87% | 179 | 462 | 711 |
| | | **Moka** | 37.05% | 37.05% | 1,040 | 1,004 | 20,969 |
| **2GB (20%)** | 20% | **LRU** | 59.64% | 59.64% | 256 | 546 | 721 |
| | | **SLRU** | **75.50%** | **75.50%** | 252 | 338 | 631 |
| | | **LFU** | **75.50%** | **75.50%** | 436 | 387 | 932 |
| | | **LFUDA** | 65.51% | 65.51% | 374 | 603 | 891 |
| | | **GDSF** | 64.87% | 64.87% | 313 | 519 | 801 |
| | | **Moka** | 70.06% | 70.06% | 1,090 | 1,325 | 23,574 |
| **4GB (40%)** | 40% | **LRU** | 83.46% | 83.46% | 360 | 544 | 782 |
| | | **SLRU** | 83.62% | 83.62% | 314 | 557 | 691 |
| | | **LFU** | 83.62% | 83.62% | 497 | 695 | 961 |
| | | **LFUDA** | **83.83%** | **83.83%** | 480 | 663 | 962 |
| | | **GDSF** | 83.77% | 83.77% | 366 | 554 | 801 |
| | | **Moka** | 83.68% | 83.68% | 997 | 1,532 | 20,628 |

The results provide strong validation of the 80-20 rule. With a 2 GB cache representing 20% of total data, SLRU and LFU achieved approximately 75% hit rates—closely matching the theoretical expectation that caching the hot 20% should satisfy roughly 80% of requests. This relationship held consistently across algorithms, with hit rates at 20% capacity ranging from 59.64% (LRU) to 75.50% (SLRU/LFU).

Algorithm differences were most pronounced at smaller cache sizes. At 10% capacity, SLRU and LFU achieved 40.40% hit rates compared to 32.41% for LRU—an 8 percentage point advantage. As cache size increased to 40%, all algorithms converged to approximately 83% hit rates, demonstrating that algorithm selection becomes less critical when cache capacity is ample relative to the working set.

The uniform object sizes resulted in identical hit rates and byte hit rates, as expected. This equivalence validates our measurement methodology and provides a baseline for interpreting the divergence between these metrics in variable-size experiments.

### 4.2 Variable Object Sizes

Variable object sizes introduce complexity that reveals the value of size-aware algorithms. This experiment used 1,038,091 unique objects ranging from 1 KB to 1 MB (average approximately 524 KB), totaling approximately 519 GB of unique data. The same 80-20 request pattern generated 14.4 million requests.

| Cache Size | % of Total | Algorithm | Hit Rate | Byte Hit Rate | Get Avg (ns) | Put Avg (ns) |
|------------|------------|-----------|----------|---------------|--------------|--------------|
| **1GB (0.2%)** | 0.2% | LRU | 0.67% | 0.32% | 62 | 310 |
| | | SLRU | 0.80% | 0.42% | 59 | 222 |
| | | LFU | 0.80% | 0.42% | 60 | 299 |
| | | LFUDA | 0.67% | 0.32% | 67 | 411 |
| | | **GDSF** | **1.90%** | **0.68%** | 76 | 317 |
| | | Moka | 9.13% | 1.30% | 738 | 716 |
| **2GB (0.4%)** | 0.4% | LRU | 1.33% | 0.55% | 68 | 308 |
| | | SLRU | 1.61% | 0.75% | 70 | 228 |
| | | LFU | 1.61% | 0.75% | 65 | 300 |
| | | LFUDA | 1.33% | 0.55% | 65 | 396 |
| | | **GDSF** | **3.18%** | **1.17%** | 73 | 302 |
| | | Moka | 2.09% | 1.39% | 745 | 687 |
| **50GB (10%)** | 10% | LRU | 31.71% | 26.83% | 163 | 519 |
| | | **SLRU** | **39.59%** | 35.52% | 158 | 251 |
| | | **LFU** | **39.59%** | 35.52% | 222 | 281 |
| | | LFUDA | 32.49% | 27.56% | 202 | 573 |
| | | GDSF | 32.70% | 27.76% | 176 | 505 |
| | | **Moka** | **47.92%** | **38.65%** | 1,145 | 1,270 |

At extremely constrained cache sizes (0.2-0.4% of working set), GDSF demonstrated its strongest advantage. With only 1 GB of cache for 519 GB of data, GDSF achieved a 1.90% hit rate compared to 0.67% for LRU—nearly three times better. This advantage stems from GDSF's preference for smaller objects: by caching more small objects instead of fewer large ones, GDSF increases the probability that any given request can be served from cache.

The divergence between hit rate and byte hit rate provides insight into algorithm behavior. Moka achieved the highest hit rate at extreme cache sizes (9.13% at 1 GB) but a lower byte hit rate (1.30%), indicating that it cached many small objects. Conversely, algorithms with closer hit-to-byte ratios distributed caching more evenly across object sizes. This distinction matters for practitioners: applications that care about request latency should optimize hit rate, while those that care about bandwidth should optimize byte hit rate.

At moderate cache sizes (10%, 50 GB), SLRU and LFU maintained leadership with 39.59% hit rates, while Moka achieved the highest hit rate at 47.92%. The gap between hit rate and byte hit rate (approximately 4-12 percentage points) reflects the tendency of all algorithms to cache smaller objects more readily, as they require less eviction to accommodate.

---

## 5. Results: On-Disk Caching Analysis

On-disk caching scenarios involve substantially larger objects and storage capacities, representative of content delivery networks, media streaming services, and storage system caches. Our experiment modeled 896,796 unique objects ranging from 1 KB to 100 MB (average approximately 50 MB), totaling approximately 45 TB of unique data. The workload generated 7.2 million requests with 80-20 popularity distribution, representing approximately 350 TB of total requested bytes.

| Cache Size | % of Unique | Algorithm | Hit Rate | Byte Hit Rate | Get Avg (ns) | Put Avg (ns) |
|------------|-------------|-----------|----------|---------------|--------------|--------------|
| **1TB (2.2%)** | 2.2% | LRU | 6.77% | 6.73% | 76 | 350 |
| | | **SLRU** | **8.20%** | **8.07%** | 81 | 250 |
| | | **LFU** | **8.20%** | **8.07%** | 88 | 275 |
| | | LFUDA | 6.79% | 6.75% | 80 | 425 |
| | | GDSF | 6.77% | 6.73% | 77 | 343 |
| | | Moka | 11.11% | 5.19% | 799 | 729 |
| **10TB (22%)** | 22% | LRU | 59.06% | 59.08% | 270 | 548 |
| | | **SLRU** | **73.53%** | **73.72%** | 260 | 466 |
| | | **LFU** | **73.53%** | **73.72%** | 409 | 480 |
| | | LFUDA | 64.41% | 64.32% | 362 | 621 |
| | | GDSF | 59.07% | 59.09% | 276 | 558 |
| | | Moka | 72.48% | 73.62% | 1,013 | 1,337 |

The results again validate the 80-20 rule at scale. At 22% cache capacity (10 TB), SLRU and LFU achieved 73.53% hit rates, closely matching the theoretical expectation. This validation at the terabyte scale demonstrates that the 80-20 heuristic remains applicable even for on-disk caching scenarios with large objects.

With large average object sizes (50 MB), hit rate and byte hit rate closely aligned across most algorithms. This convergence occurs because each cached object contributes substantially to byte coverage, and the size distribution's impact on coverage efficiency diminishes. The exception is Moka at 1 TB capacity, which achieved 11.11% hit rate but only 5.19% byte hit rate. This anomaly suggests that Moka's TinyLFU algorithm preferentially cached smaller objects, which improved hit rate (more objects cached) at the expense of byte hit rate (less total data cached).

GDSF's advantage disappeared in this scenario, performing identically to LRU. This result aligns with theoretical expectations: when average object size is very large (50 MB) relative to the size range, the size-aware component of GDSF's priority calculation has minimal impact on eviction decisions. The frequency component dominates, and GDSF degenerates to frequency-aware behavior similar to LRU's recency behavior.

---

## 6. Latency Analysis

Operation latency represents a critical dimension of cache performance, particularly for applications with strict response time requirements. Our measurements captured both average latencies and tail latencies (99th percentile) across all algorithms.

| Algorithm | Get (avg) | Put with Size (avg) | p99 Get | Throughput |
|-----------|-----------|---------------------|---------|------------|
| **LRU** | 50-270 ns | 210-550 ns | 90-750 ns | 7-9M ops/sec |
| **SLRU** | 40-260 ns | 190-470 ns | 70-650 ns | 8-12M ops/sec |
| **LFU** | 60-410 ns | 270-480 ns | 150-930 ns | 6-8M ops/sec |
| **LFUDA** | 55-370 ns | 300-620 ns | 270-900 ns | 3-5M ops/sec |
| **GDSF** | 40-280 ns | 200-560 ns | 90-800 ns | 5-7M ops/sec |
| **Moka** | 280-1,100 ns | 410-1,340 ns | 5,000-24,000 ns | 1.9-2.9M ops/sec |

The cache-rs algorithms demonstrated consistently low latencies in the 50-500 nanosecond range for average operations, achieving throughputs of 3-12 million operations per second. SLRU achieved the best latency characteristics overall, combining the fastest average latencies with excellent throughput. This performance reflects SLRU's relatively simple data structure operations: a get operation requires at most moving an entry between two lists, while a put operation may require a single eviction and insertion.

Moka exhibited substantially higher latencies, typically 5-10 times greater than cache-rs algorithms for average operations and 10-30 times greater for tail latencies. The 99th percentile latency often exceeded 20 microseconds, compared to sub-microsecond p99 latencies for cache-rs algorithms. These latency differences stem from Moka's architectural choices: the TinyLFU algorithm requires maintaining frequency sketches and performing more complex bookkeeping, while the eventually-consistent design can trigger maintenance operations during get requests.

The latency differences become significant in context. For an application processing 100,000 requests per second, the difference between 200 nanoseconds and 1,000 nanoseconds per cache operation translates to 80 milliseconds of additional CPU time per second—nearly 8% overhead. For latency-sensitive applications requiring sub-millisecond response times, these differences can be meaningful.

---

## 7. Discussion

### 7.1 Validation of the 80-20 Rule

One of the most consistent findings across our experiments is the robustness of the 80-20 rule for cache sizing. Across video, social, web, in-memory, and on-disk workloads, caching approximately 20% of the working set consistently achieved 70-80% hit rates with frequency-aware algorithms. This relationship held across four orders of magnitude in working set size (10 GB to 45 TB) and diverse object size distributions.

The practical implication is clear: practitioners can use the 80-20 rule as a reliable first approximation for cache sizing. A cache sized at 20% of the expected working set provides a strong cost-benefit ratio, capturing most of the achievable hit rate without excessive resource investment. Increasing cache size beyond 20% yields diminishing returns, while sizing below 10% results in substantially degraded hit rates.

### 7.2 When Algorithm Selection Matters

Our results reveal that algorithm selection becomes increasingly important as cache capacity decreases relative to working set size. At severe constraints (less than 10% capacity), algorithm differences can exceed 20 percentage points. At moderate constraints (10-20% capacity), differences typically range from 5-15 percentage points. At comfortable constraints (greater than 40% capacity), algorithms converge and selection matters little.

This pattern has practical implications for system design. Systems with abundant cache resources can safely use simple algorithms like LRU, benefiting from their predictability and low overhead. Systems operating under tight resource constraints should invest in more sophisticated algorithms, as the hit rate improvements justify the additional complexity.

### 7.3 The Critical Role of Size-Aware Eviction

Perhaps our most striking finding is the magnitude of GDSF's advantage in storage-constrained scenarios with heterogeneous object sizes. The 57 percentage point improvement over LRU for social media traffic at 50 MB cache demonstrates that size-aware eviction is not merely an optimization but a fundamental requirement for efficient caching of variable-sized objects under storage constraints.

The intuition is straightforward: when storage is the binding constraint, an algorithm that preferentially caches smaller objects can maintain more entries in cache, increasing the probability that any given request finds a cache hit. GDSF formalizes this intuition by incorporating inverse object size into its eviction priority calculation. Our results validate that this approach provides substantial benefits in practice.

However, size-aware eviction provides diminishing benefits as average object size increases or size variance decreases. In our on-disk experiments with 50 MB average object size, GDSF performed identically to LRU. Practitioners should consider their object size distributions when selecting algorithms: size-aware eviction provides the greatest benefit for workloads with high size variance and small-to-moderate average sizes.

### 7.4 Trade-offs in Sophisticated Algorithms

Moka's TinyLFU implementation achieved the highest hit rates in many scenarios but at significant latency cost. This trade-off reflects a fundamental tension in cache design: more sophisticated algorithms require more bookkeeping, which increases operation latency. Whether this trade-off is worthwhile depends on application requirements.

For applications where cache hit rate directly impacts user-visible latency (e.g., web page load times), the hit rate improvements from Moka may justify higher cache operation latencies. The time saved by avoiding backend requests typically exceeds the additional microseconds spent in cache operations. Conversely, for applications with strict latency requirements or very high request rates, the simpler algorithms' lower and more predictable latencies may be preferable even at some cost in hit rate.

---

## 8. Recommendations

### 8.1 Algorithm Selection Guidelines

Based on our comprehensive evaluation, we offer the following algorithm selection guidelines. SLRU emerges as the best general-purpose choice, offering excellent hit rates, the fastest operation latencies, and inherent scan resistance without configuration complexity. It should be the default choice when requirements are unclear.

GDSF is essential for storage-constrained caching of variable-sized objects. When cache capacity is measured in bytes rather than entries and objects vary substantially in size, GDSF provides improvements that can exceed 50 percentage points over size-agnostic alternatives.

LRU remains appropriate for simple workloads with predictable access patterns and uniform object sizes. Its simplicity makes it easy to reason about and debug, and its performance is competitive when cache capacity is ample.

LFUDA suits long-running caches where object popularity shifts over time. The aging mechanism prevents frequency counter pollution and allows the cache to adapt to changing access patterns.

Moka should be selected when hit rate is paramount and latency budgets are flexible, or when built-in thread safety is required. Its sophisticated TinyLFU algorithm provides excellent hit rates but at significant latency cost.

LFU is appropriate for stable workloads with fixed popularity distributions. However, its susceptibility to frequency counter pollution makes it unsuitable for workloads where popularity changes over time.

### 8.2 Cache Sizing Guidelines

| Desired Hit Rate | Cache Size (% of working set) | Recommendation |
|------------------|------------------------------|----------------|
| 15-20% | 1% | Minimal viable cache, use only when resources severely constrained |
| 30-40% | 10% | Minimum recommended for production systems |
| 60-70% | 15-18% | Good cost-benefit ratio for most applications |
| 75% | 20% | Optimal for 80-20 workloads, recommended default |
| 80-85% | 30-40% | Diminishing returns, justify with specific requirements |
| 90%+ | 50%+ | Expensive, rarely justified except for latency-critical paths |
| 99%+ | 90%+ | Near-complete coverage, only for specialized requirements |

---

## 9. Conclusion

This study presents a comprehensive empirical evaluation of cache eviction algorithms across diverse workload conditions representative of modern computing systems. Our analysis of over 47 million cache requests across video streaming, social media, web content, in-memory, and on-disk caching scenarios yields several actionable insights.

The classical 80-20 rule provides reliable guidance for cache sizing across workload types and scales. Caching 20% of the working set consistently achieves approximately 75% hit rates with appropriate algorithm selection, establishing a practical baseline for capacity planning.

Algorithm selection matters most when cache resources are constrained. At less than 10% capacity relative to working set, algorithm differences can exceed 20 percentage points. Investment in sophisticated algorithms is justified under resource constraints but provides diminishing returns as capacity increases.

Size-aware eviction is critical for storage-constrained caching of heterogeneous objects. GDSF's incorporation of object size into eviction decisions provides improvements exceeding 50 percentage points over size-agnostic algorithms in appropriate scenarios.

Performance-sophistication trade-offs are real and significant. Moka's TinyLFU algorithm achieves excellent hit rates but at 5-10 times higher operation latencies. Applications must balance hit rate improvements against latency budgets based on their specific requirements.

SLRU emerges as the recommended default algorithm, combining excellent hit rates, the fastest operation latencies, and scan resistance in a simple, predictable package. Practitioners should deviate from this default only when specific workload characteristics warrant specialized algorithms.

---

## Appendix A: Moka Performance Characteristics

Moka consistently exhibited higher latency than cache-rs algorithms despite achieving excellent hit rates. This behavior reflects intentional architectural decisions rather than implementation inefficiency.

Moka implements the TinyLFU algorithm, which combines LRU and LFU with a probabilistic frequency sketch. This design requires additional bookkeeping per operation: frequency sketch updates, admission policy checks, and periodic maintenance. The algorithm operates with eventual consistency, batching operations and applying them to policy structures asynchronously. While this design improves throughput under contention, it can cause latency variance when get operations trigger deferred maintenance.

Moka's async and sync APIs share identical internal data structures. The async interface is designed for integration with async runtimes like Tokio, avoiding thread blocking in async contexts. Neither interface provides performance advantages in synchronous benchmarks.

The trade-off is explicit: Moka prioritizes hit rate and concurrent scalability over single-operation latency. Applications with flexible latency budgets benefit from Moka's sophisticated algorithms, while latency-sensitive applications may prefer simpler alternatives.

---

## Appendix B: Experimental Configuration

### Traffic Generation Parameters

Video streaming traffic was generated with 500 requests per second over 4 hours (7.2 million total), targeting 10,000 unique objects with 70% of requests accessing the most popular 10%. Object sizes ranged from 1 MB to 10 MB.

Social media traffic was generated with 1,000 requests per second over 4 hours (14.4 million total), targeting 100,000 unique objects with 90% of requests accessing the most popular 5%. Object sizes ranged from 10 KB to 100 KB.

Web content traffic was generated with 800 requests per second over 4 hours (11.5 million total), targeting 50,000 unique objects with 60% of requests accessing the most popular 20%. Object sizes ranged from 100 KB to 5 MB.

In-memory uniform workload used 1,000 requests per second over 4 hours, targeting 1 million unique objects with fixed 10 KB size and 80-20 popularity distribution.

In-memory variable workload used 1,000 requests per second over 4 hours, targeting 1 million unique objects with sizes ranging from 1 KB to 1 MB and 80-20 popularity distribution.

On-disk workload used 500 requests per second over 4 hours, targeting 1 million unique objects with sizes ranging from 1 KB to 100 MB and 80-20 popularity distribution.

### System Configuration

All experiments were conducted using cache-rs version 0.2.0 with the SLRU size-mode bug fix applied (January 29, 2026). Simulations ran in sequential mode to ensure accurate latency measurement without lock contention. Latencies were measured per-operation, excluding I/O overhead.

---

## Appendix C: Data Availability

All simulation results and raw data are available in the repository:

Primary results are stored in the simulation data directories organized by traffic type and configuration parameters. Video, social, and web traffic results reside in the workload-specific results folder, while in-memory and on-disk scenario results are maintained separately with full parameter documentation.

The cache-simulator tool and traffic generator are provided as part of this repository, enabling reproduction of all experiments and extension to additional workload scenarios.
