# Pop API Surface Rationalization Tasks

**Spec:** [pop-api-rationalization-spec.md](../design-spec/pop-api-rationalization-spec.md)  
**Started:** 2026-03-02  
**Status:** COMPLETED  
**Work Type:** ENHANCEMENT

## Phase 1: Demote `pop_r()` Visibility

### Tasks
(All completed)

### Completed
✅ Task 1: Single-threaded caches — change `pub fn pop_r()` to `pub(crate) fn pop_r()`
  - ✅ src/lru.rs
  - ✅ src/lfu.rs
  - ✅ src/lfuda.rs
  - ✅ src/slru.rs
  - ✅ src/gdsf.rs
✅ Task 2: Concurrent caches — demote `pub fn pop_r()` to `pub(crate)`
  - ✅ src/concurrent/lru.rs
  - ✅ src/concurrent/lfu.rs
  - ✅ src/concurrent/lfuda.rs
  - ✅ src/concurrent/slru.rs
  - ✅ src/concurrent/gdsf.rs
✅ Task 3: Run validation pipeline

### Progress Log
| Time | Action | Result |
|------|--------|--------|
| 2026-03-02 | Verified clean build | ✅ Pass |
| 2026-03-02 | Checked test/example usage | No external `pop_r()` usage found |
| 2026-03-02 | Updated single-threaded caches | ✅ 5 files updated |
| 2026-03-02 | Updated concurrent caches | ✅ 5 files updated |
| 2026-03-02 | cargo fmt --check | ✅ Pass |
| 2026-03-02 | cargo clippy | ✅ Pass |
| 2026-03-02 | cargo test | ✅ 67 passed |
| 2026-03-02 | cargo doc | ✅ Pass |
