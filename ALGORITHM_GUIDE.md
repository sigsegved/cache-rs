# Cache Algorithm Guide

This guide provides detailed information about each eviction algorithm in cache-rs, including use cases, code examples, and guidance on when to use each one.

## Table of Contents

- [LRU (Least Recently Used)](#lru-least-recently-used)
- [SLRU (Segmented LRU)](#slru-segmented-lru)
- [LFU (Least Frequently Used)](#lfu-least-frequently-used)
- [LFUDA (LFU with Dynamic Aging)](#lfuda-lfu-with-dynamic-aging)
- [GDSF (Greedy Dual-Size Frequency)](#gdsf-greedy-dual-size-frequency)
- [Algorithm Comparison](#algorithm-comparison)
- [Real-World Use Cases](#real-world-use-cases)

---

## LRU (Least Recently Used)

### When to Use

LRU is ideal when your workload exhibits **temporal locality**—recently accessed items are likely to be accessed again soon.

**Best for:**
- Web session storage
- Database query result caching
- General-purpose caching where simplicity matters
- Workloads where recency is the best predictor of future access

**Avoid when:**
- Some items are consistently more popular than others (use LFU)
- Sequential scans could pollute your cache (use SLRU)
- Object sizes vary significantly (use GDSF)

### Example: Session Cache

```rust
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use std::num::NonZeroUsize;

#[derive(Clone)]
struct Session {
    user_id: u64,
    username: String,
    role: String,
}

let config = LruCacheConfig {
    capacity: NonZeroUsize::new(10_000).unwrap(),
    max_size: u64::MAX,
};
let mut sessions: LruCache<String, Session> = LruCache::init(config, None);

// Create session on login
fn create_session(cache: &mut LruCache<String, Session>, session_id: String, user: Session) {
    cache.put(session_id, user);
}

// Validate session on each request (also refreshes LRU position)
fn get_session<'a>(cache: &'a mut LruCache<String, Session>, session_id: &str) -> Option<&'a Session> {
    cache.get(&session_id.to_string())
}

// Logout
fn destroy_session(cache: &mut LruCache<String, Session>, session_id: &str) {
    cache.remove(&session_id.to_string());
}
```

---

## SLRU (Segmented LRU)

### When to Use

SLRU provides **scan resistance** by requiring items to be accessed twice before they're protected from eviction. This prevents a single sequential scan from evicting your entire hot working set.

**Best for:**
- Database buffer pools
- File system caches
- Any workload where sequential scans occur alongside random access
- When you need scan resistance but don't want frequency tracking overhead

**Avoid when:**
- Access patterns are purely random (LRU is simpler)
- Item popularity varies significantly (LFU/LFUDA better)

### Example: Buffer Pool

```rust
use cache_rs::SlruCache;
use cache_rs::config::SlruCacheConfig;
use std::num::NonZeroUsize;

let config = SlruCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    protected_capacity: NonZeroUsize::new(200).unwrap(),  // 20% protected
    max_size: u64::MAX,
};
let mut buffer_pool = SlruCache::init(config, None);

// First access: page enters probationary segment
buffer_pool.put("page:1", vec![0u8; 4096]);

// Sequential scan won't evict hot pages because scanned pages
// stay in probationary and get evicted first
for i in 2..10000 {
    buffer_pool.put(format!("page:{}", i), vec![0u8; 4096]);
}

// Second access: page promoted to protected segment
let _ = buffer_pool.get(&"page:1".to_string());
// Now page:1 is protected from sequential scan eviction
```

---

## LFU (Least Frequently Used)

### When to Use

LFU is ideal when **popularity** is the best predictor of future access. Items that have been accessed many times are likely to be accessed again.

**Best for:**
- CDN edge caches (popular content stays cached)
- API response caching (frequent endpoints stay hot)
- Content recommendation systems
- Any workload with stable popularity distribution

**Avoid when:**
- Popularity changes over time (use LFUDA)
- You need scan resistance (use SLRU)
- Objects are different sizes (use GDSF)

### Example: API Response Cache

```rust
use cache_rs::LfuCache;
use cache_rs::config::LfuCacheConfig;
use std::num::NonZeroUsize;

let config = LfuCacheConfig {
    capacity: NonZeroUsize::new(1000).unwrap(),
    max_size: 50 * 1024 * 1024,  // 50 MB
};
let mut api_cache: LfuCache<String, String> = LfuCache::init(config, None);

fn cached_api_call(
    cache: &mut LfuCache<String, String>,
    endpoint: &str,
    fetch: impl Fn(&str) -> String,
) -> String {
    if let Some(response) = cache.get(&endpoint.to_string()) {
        return response.clone();  // Cache hit - frequency incremented
    }
    
    let response = fetch(endpoint);
    cache.put(endpoint.to_string(), response.clone());
    response
}

// Popular endpoints accumulate high frequency and stay cached
// even when many rare endpoints are requested
```

---

## LFUDA (LFU with Dynamic Aging)

### When to Use

LFUDA solves LFU's "cache pollution" problem where historically popular items can never be evicted. The aging mechanism allows new items to eventually compete with old popular items.

**Best for:**
- Long-running services where popularity changes
- News/social media feeds (trending content changes)
- E-commerce (seasonal products, flash sales)
- Any workload where "what's hot" evolves over time

**Avoid when:**
- Popularity is stable (LFU is simpler and slightly faster)
- Recency matters more than frequency (use LRU)

### Example: Trending Content Cache

```rust
use cache_rs::LfudaCache;
use cache_rs::config::LfudaCacheConfig;
use std::num::NonZeroUsize;

let config = LfudaCacheConfig {
    capacity: NonZeroUsize::new(10_000).unwrap(),
    initial_age: 0,
    max_size: u64::MAX,
};
let mut trending_cache = LfudaCache::init(config, None);

// Yesterday's viral post (accessed 10,000 times yesterday)
trending_cache.put("post:yesterday_viral", "old_content");

// As cache operations happen, the global age increases
// New content can eventually compete with old popular content

// Today's trending post starts fresh
trending_cache.put("post:today_trending", "new_content");

// Over time, if yesterday's post stops being accessed,
// today's post can overtake it in priority
```

---

## GDSF (Greedy Dual-Size Frequency)

### When to Use

GDSF is designed for **variable-sized objects**. It considers frequency, size, and age to maximize cache hit rate rather than byte hit rate.

**Best for:**
- File/object caching with varying sizes
- CDN metadata caching (where you track what's on disk)
- Image/video thumbnail caches
- Any workload where object sizes vary significantly

**Avoid when:**
- All objects are the same size (LFU is simpler)
- You care about byte hit rate, not request hit rate

### Example: CDN Metadata Cache

```rust
use cache_rs::GdsfCache;
use cache_rs::config::GdsfCacheConfig;
use std::num::NonZeroUsize;
use std::path::PathBuf;

/// Cache metadata: maps cache keys to on-disk filenames
#[derive(Clone)]
struct CacheEntry {
    filename: PathBuf,
    content_type: String,
    etag: String,
}

let config = GdsfCacheConfig {
    capacity: NonZeroUsize::new(1_000_000).unwrap(),
    initial_age: 0.0,
    max_size: u64::MAX,
};
let mut cache_index: GdsfCache<String, CacheEntry> = GdsfCache::init(config, None);

/// Compute cache key from request
fn compute_cache_key(url: &str, vary_headers: &[(&str, &str)]) -> String {
    let mut key = url.to_string();
    for (name, value) in vary_headers {
        key.push_str(&format!("|{}={}", name, value));
    }
    key
}

fn serve_request(
    cache_index: &mut GdsfCache<String, CacheEntry>,
    url: &str,
    vary_headers: &[(&str, &str)],
    fetch_and_store: impl Fn(&str) -> (CacheEntry, u64),
) -> CacheEntry {
    let cache_key = compute_cache_key(url, vary_headers);
    
    if let Some(entry) = cache_index.get(&cache_key) {
        return entry;  // Cache hit
    }
    
    let (entry, content_size) = fetch_and_store(url);
    
    // GDSF uses content_size in priority calculation
    // Small popular files get higher priority than large rare files
    cache_index.put(cache_key, entry.clone(), content_size);
    
    entry
}
```

---

## Algorithm Comparison

| Algorithm | Eviction Strategy | Best For | Complexity |
|-----------|-------------------|----------|------------|
| **LRU** | Least recently accessed | General purpose, temporal locality | O(1) |
| **SLRU** | LRU with two segments | Scan resistance, buffer pools | O(1) |
| **LFU** | Least frequently accessed | Stable popularity patterns | O(log F)* |
| **LFUDA** | LFU + aging | Changing popularity | O(log P) |
| **GDSF** | (Frequency/Size) + age | Variable-sized objects | O(log P) |

*F = number of distinct frequencies (bounded, effectively O(1)). P = number of distinct priority values (can grow with cache size).

### Decision Guide

1. **Start with LRU** if you're unsure—it works well for most workloads
2. **Switch to SLRU** if sequential scans are hurting your hit rate
3. **Switch to LFU** if some items are consistently more popular
4. **Switch to LFUDA** if popularity changes over time
5. **Switch to GDSF** if object sizes vary significantly

---

## Real-World Use Cases

### Web Application Session Cache

Use **LRU**. Sessions have strong temporal locality—active users access their sessions repeatedly, and inactive sessions should be evicted.

```rust
use cache_rs::LruCache;
use cache_rs::config::LruCacheConfig;
use std::num::NonZeroUsize;

#[derive(Clone)]
struct Session {
    user_id: u64,
    username: String,
    permissions: Vec<String>,
}

let config = LruCacheConfig {
    capacity: NonZeroUsize::new(10_000).unwrap(),
    max_size: u64::MAX,
};
let mut sessions: LruCache<String, Session> = LruCache::init(config, None);

// Create session on login
fn login(cache: &mut LruCache<String, Session>, session_id: String, user: Session) {
    cache.put(session_id, user);
}

// Validate session (also refreshes LRU position)
fn validate(cache: &mut LruCache<String, Session>, session_id: &str) -> Option<&Session> {
    cache.get(&session_id.to_string())
}

// Logout
fn logout(cache: &mut LruCache<String, Session>, session_id: &str) {
    cache.remove(&session_id.to_string());
}
```

### Database Buffer Pool

Use **SLRU**. Database workloads often mix random access (index lookups) with sequential scans (table scans). SLRU prevents scans from evicting your hot working set.

```rust
use cache_rs::SlruCache;
use cache_rs::config::SlruCacheConfig;
use std::num::NonZeroUsize;

const PAGE_SIZE: usize = 8192;

let config = SlruCacheConfig {
    capacity: NonZeroUsize::new(10_000).unwrap(),      // 10K pages
    protected_capacity: NonZeroUsize::new(2_000).unwrap(), // 20% protected
    max_size: u64::MAX,
};
let mut buffer_pool: SlruCache<u64, Vec<u8>> = SlruCache::init(config, None);

fn read_page(pool: &mut SlruCache<u64, Vec<u8>>, page_id: u64, read_from_disk: impl Fn(u64) -> Vec<u8>) -> Vec<u8> {
    if let Some(page) = pool.get(&page_id) {
        return page.clone();  // Cache hit, promoted if in probationary
    }
    
    let page = read_from_disk(page_id);
    pool.put(page_id, page.clone());  // Enters probationary
    page
}

// Sequential scan: pages enter probationary and get evicted before
// touching the protected segment where your hot index pages live
fn table_scan(pool: &mut SlruCache<u64, Vec<u8>>, start: u64, end: u64, read_from_disk: impl Fn(u64) -> Vec<u8>) {
    for page_id in start..end {
        let _ = read_page(pool, page_id, &read_from_disk);
    }
}
```

### CDN Edge Cache

Use **GDSF** for metadata tracking. CDNs serve objects of varying sizes, and GDSF optimizes for hit rate by preferring small popular objects over large rare ones.

```rust
use cache_rs::GdsfCache;
use cache_rs::config::GdsfCacheConfig;
use std::num::NonZeroUsize;
use std::path::PathBuf;

#[derive(Clone)]
struct CacheEntry {
    disk_path: PathBuf,
    content_type: String,
    etag: String,
    size: u64,
}

let config = GdsfCacheConfig {
    capacity: NonZeroUsize::new(1_000_000).unwrap(),
    initial_age: 0.0,
    max_size: u64::MAX,
};
let mut index: GdsfCache<String, CacheEntry> = GdsfCache::init(config, None);

fn cache_key(url: &str, vary: &[(&str, &str)]) -> String {
    let mut key = url.to_string();
    for (h, v) in vary {
        key.push_str(&format!("|{}={}", h, v));
    }
    key
}

fn serve(
    index: &mut GdsfCache<String, CacheEntry>,
    url: &str,
    vary: &[(&str, &str)],
    fetch_origin: impl Fn(&str) -> (CacheEntry, Vec<u8>),
    write_disk: impl Fn(&PathBuf, &[u8]),
) -> CacheEntry {
    let key = cache_key(url, vary);
    
    if let Some(entry) = index.get(&key) {
        return entry;  // Metadata hit, read content from entry.disk_path
    }
    
    let (entry, content) = fetch_origin(url);
    write_disk(&entry.disk_path, &content);
    
    // GDSF prioritizes: small popular > large rare
    index.put(key, entry.clone(), entry.size);
    entry
}
```

### API Rate Limiting

Use **Concurrent LRU**. Rate limit entries are small and uniform, and you need thread-safe access. The LRU eviction ensures you're tracking active clients.

```rust
use cache_rs::concurrent::ConcurrentLruCache;
use cache_rs::config::{ConcurrentCacheConfig, LruCacheConfig};
use std::num::NonZeroUsize;
use std::sync::Arc;

#[derive(Clone)]
struct RateLimit {
    count: u32,
    window_start: u64,
}

let config = ConcurrentCacheConfig {
    base: LruCacheConfig {
        capacity: NonZeroUsize::new(100_000).unwrap(),
        max_size: u64::MAX,
    },
    segments: 32,
};
let limiter = Arc::new(ConcurrentLruCache::init(config, None));

fn check_rate_limit(
    limiter: &ConcurrentLruCache<String, RateLimit>,
    client_ip: &str,
    max_requests: u32,
    window_secs: u64,
    now: u64,
) -> bool {
    let key = client_ip.to_string();
    
    match limiter.get(&key) {
        Some(entry) if now - entry.window_start < window_secs => {
            if entry.count >= max_requests {
                return false;  // Rate limited
            }
            limiter.put(key, RateLimit { count: entry.count + 1, window_start: entry.window_start });
            true
        }
        _ => {
            limiter.put(key, RateLimit { count: 1, window_start: now });
            true
        }
    }
}
```

### News Feed / Social Media

Use **LFUDA**. Content popularity changes rapidly—yesterday's viral post shouldn't block today's trending content from being cached.

```rust
use cache_rs::LfudaCache;
use cache_rs::config::LfudaCacheConfig;
use std::num::NonZeroUsize;

#[derive(Clone)]
struct Post {
    id: u64,
    content: String,
    author: String,
}

let config = LfudaCacheConfig {
    capacity: NonZeroUsize::new(50_000).unwrap(),
    initial_age: 0,
    max_size: u64::MAX,
};
let mut feed_cache: LfudaCache<u64, Post> = LfudaCache::init(config, None);

fn get_post(
    cache: &mut LfudaCache<u64, Post>,
    post_id: u64,
    fetch_db: impl Fn(u64) -> Post,
) -> Post {
    if let Some(post) = cache.get(&post_id) {
        return post.clone();  // Frequency incremented, age updated
    }
    
    let post = fetch_db(post_id);
    cache.put(post_id, post.clone());
    post
}

// Yesterday's viral post had freq=10000, age=0, priority=10000
// Today's trending post has freq=100, age=9950, priority=10050
// New content can compete because aging levels the playing field
```

### Static Asset Server

Use **LFU** or **GDSF**. Static assets have stable popularity (your logo.png is always popular), and sizes vary (CSS vs images vs fonts).

```rust
use cache_rs::LfuCache;
use cache_rs::config::LfuCacheConfig;
use std::num::NonZeroUsize;

let config = LfuCacheConfig {
    capacity: NonZeroUsize::new(10_000).unwrap(),
    max_size: 500 * 1024 * 1024,  // 500 MB
};
let mut asset_cache: LfuCache<String, Vec<u8>> = LfuCache::init(config, None);

fn serve_asset(
    cache: &mut LfuCache<String, Vec<u8>>,
    path: &str,
    read_file: impl Fn(&str) -> Vec<u8>,
) -> Vec<u8> {
    if let Some(data) = cache.get(&path.to_string()) {
        return data.clone();  // logo.png freq=50000 stays cached
    }
    
    let data = read_file(path);
    cache.put(path.to_string(), data.clone());
    data
}
```

---

## Further Reading

- [README.md](README.md) - Main documentation and quick start
- [API Documentation](https://docs.rs/cache-rs) - Complete API reference
- [Benchmarks](benches/) - Performance measurements
