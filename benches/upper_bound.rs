//! Upper-bound mock — does the merged-map design actually hit 4× ratio?
//!
//! Compares two engine designs side-by-side under identical workload:
//!
//! - **CurrentEngine** — mirrors the production three-map layout (
//!   `all_distinctions`, `parents_of`, `degree_counts`). Hot path:
//!   one `or_insert_with` closure that does 3 inserts + 2 fetch_adds.
//! - **MergedEngine** — proposed single-map layout (`nodes`
//!   containing both parents and degree). Hot path: one `or_insert_with`
//!   for the new child + 2 `get().fetch_add()` for parents (with the
//!   write-lock released between entry and gets, fixing B1 deadlock).
//!
//! Both use identical SHA-256, identical canonical (min, max) ordering,
//! identical IdentityHasher, identical chain workload (8 independent
//! chains under 8 threads). The ONLY difference is the storage layout.
//!
//! Run: `cargo bench --bench upper_bound`
//!
//! This is a verification mock for the proposed Step 1 substrate
//! refactor; it does not affect `src/engine.rs`.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Shared infrastructure
// ---------------------------------------------------------------------------

/// Mock copy of the production IdentityHasher (so we don't reach into
/// the crate's internals from a bench).
#[derive(Default)]
struct IdHasher {
    state: u64,
}

impl Hasher for IdHasher {
    fn finish(&self) -> u64 {
        self.state
    }
    fn write(&mut self, bytes: &[u8]) {
        let prefix: [u8; 8] =
            bytes[..8].try_into().expect("identity hash key is 16 bytes (invariant)");
        self.state = u64::from_le_bytes(prefix);
    }
    fn write_usize(&mut self, _: usize) {} // slice/array length prefix — no-op
}

type IdBuildHasher = BuildHasherDefault<IdHasher>;

// Type aliases — mirror src/engine.rs conventions to silence
// clippy::type_complexity on the bench's storage layout.
type Bytes = [u8; 16];
type ParentPair = (Bytes, Bytes);
type IdMap = DashMap<Bytes, Bytes, IdBuildHasher>;
type ParentsMap = DashMap<Bytes, ParentPair, IdBuildHasher>;
type DegreeMap = DashMap<Bytes, AtomicUsize, IdBuildHasher>;
type NodeMap = DashMap<Bytes, MergedNode, IdBuildHasher>;

const D0: [u8; 16] = [0; 16];
const D1: [u8; 16] = {
    let mut a = [0u8; 16];
    a[0] = 1;
    a
};

/// SHA-256 over canonical (min, max) parents; leading 16 bytes.
#[inline]
fn synth_bytes(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update(first);
    h.update(second);
    let mut out = [0u8; 16];
    out.copy_from_slice(&h.finalize()[..16]);
    out
}

// ---------------------------------------------------------------------------
// CurrentEngine — three-map mock (mirrors production)
// ---------------------------------------------------------------------------

struct CurrentEngine {
    all: IdMap,
    parents: ParentsMap,
    degrees: DegreeMap,
}

impl CurrentEngine {
    fn new() -> Self {
        let all = DashMap::with_hasher(IdBuildHasher::default());
        all.insert(D0, D0);
        all.insert(D1, D1);
        let parents = DashMap::with_hasher(IdBuildHasher::default());
        let degrees = DashMap::with_hasher(IdBuildHasher::default());
        degrees.insert(D0, AtomicUsize::new(0));
        degrees.insert(D1, AtomicUsize::new(0));
        Self { all, parents, degrees }
    }

    fn synthesize(&self, a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
        if a == b {
            return a;
        }
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        let new_bytes = synth_bytes(a, b);
        if self.all.get(&new_bytes).is_some() {
            return new_bytes;
        }
        self.all.entry(new_bytes).or_insert_with(|| {
            self.parents.insert(new_bytes, (first, second));
            self.degrees.insert(new_bytes, AtomicUsize::new(0));
            self.degrees
                .get(&first)
                .expect("degree pre-seeded at parent insertion (invariant)")
                .fetch_add(1, Ordering::Release);
            self.degrees
                .get(&second)
                .expect("degree pre-seeded at parent insertion (invariant)")
                .fetch_add(1, Ordering::Release);
            new_bytes
        });
        new_bytes
    }
}

// ---------------------------------------------------------------------------
// MergedEngine — one-map mock (proposed redesign)
// ---------------------------------------------------------------------------

// `parents` is written but not read in this bench — its purpose is to
// reproduce the production memory layout / allocation cost so the
// comparison against CurrentEngine is fair. Real code reads it via
// `parents_of()`. dead_code suppression is bench-only.
#[allow(dead_code)]
struct MergedNode {
    parents: Option<ParentPair>,
    degree: AtomicUsize,
}

struct MergedEngine {
    nodes: NodeMap,
}

impl MergedEngine {
    fn new() -> Self {
        let nodes = DashMap::with_hasher(IdBuildHasher::default());
        nodes.insert(D0, MergedNode { parents: None, degree: AtomicUsize::new(0) });
        nodes.insert(D1, MergedNode { parents: None, degree: AtomicUsize::new(0) });
        Self { nodes }
    }

    fn synthesize(&self, a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
        if a == b {
            return a;
        }
        let (first, second) = if a <= b { (a, b) } else { (b, a) };
        let new_bytes = synth_bytes(a, b);
        if self.nodes.get(&new_bytes).is_some() {
            return new_bytes;
        }
        // Write-lock on new_bytes shard is held only for the duration
        // of this statement; RefMut dropped at the semicolon. This
        // releases the lock BEFORE the parent gets — avoids deadlock
        // if first or second hash to the same shard as new_bytes (B1
        // qa-sentinel concern).
        self.nodes.entry(new_bytes).or_insert_with(|| MergedNode {
            parents: Some((first, second)),
            degree: AtomicUsize::new(0),
        });
        self.nodes
            .get(&first)
            .expect("node pre-seeded at insertion (invariant)")
            .degree
            .fetch_add(1, Ordering::Release);
        self.nodes
            .get(&second)
            .expect("node pre-seeded at insertion (invariant)")
            .degree
            .fetch_add(1, Ordering::Release);
        new_bytes
    }
}

// ---------------------------------------------------------------------------
// Workload — chain extension, 8 independent chains for the multi-thread case
// ---------------------------------------------------------------------------

/// Build 8 per-thread seed distinctions by chaining synthesizes from
/// primordials, BEFORE the bench timer starts. Each seed is a real
/// registered distinction in the engine, so the per-thread chain
/// extensions in the bench can use it as a parent without foreign-byte
/// panic.
fn build_current_seeds(engine: &CurrentEngine) -> Vec<[u8; 16]> {
    (0..8u8)
        .map(|i| {
            let mut acc = engine.synthesize(D0, D1);
            for _ in 0..=i {
                acc = engine.synthesize(acc, D0);
            }
            acc
        })
        .collect()
}

fn build_merged_seeds(engine: &MergedEngine) -> Vec<[u8; 16]> {
    (0..8u8)
        .map(|i| {
            let mut acc = engine.synthesize(D0, D1);
            for _ in 0..=i {
                acc = engine.synthesize(acc, D0);
            }
            acc
        })
        .collect()
}

fn warmup_current(engine: &CurrentEngine, prev: &mut [u8; 16], cur: &mut [u8; 16], n: usize) {
    for _ in 0..n {
        let next = engine.synthesize(*cur, *prev);
        *prev = *cur;
        *cur = next;
    }
}

fn warmup_merged(engine: &MergedEngine, prev: &mut [u8; 16], cur: &mut [u8; 16], n: usize) {
    for _ in 0..n {
        let next = engine.synthesize(*cur, *prev);
        *prev = *cur;
        *cur = next;
    }
}

// ---------------------------------------------------------------------------
// Benches — current 3-map design
// ---------------------------------------------------------------------------

fn bench_current_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("mock_current_single_thread");
    group.throughput(Throughput::Elements(1));
    group.bench_function("chain", |b| {
        b.iter_custom(|iters| {
            let engine = CurrentEngine::new();
            let mut prev = D0;
            let mut cur = D1;
            warmup_current(&engine, &mut prev, &mut cur, 100);
            let start = Instant::now();
            for _ in 0..iters {
                let next = engine.synthesize(cur, prev);
                prev = cur;
                cur = next;
                black_box(next);
            }
            start.elapsed()
        });
    });
    group.finish();
}

fn bench_current_8t(c: &mut Criterion) {
    let mut group = c.benchmark_group("mock_current_8_thread");
    group.throughput(Throughput::Elements(1));
    group.bench_function("8x_chain", |b| {
        b.iter_custom(|iters| {
            let engine = Arc::new(CurrentEngine::new());
            // Build per-thread seeds in setup (BEFORE timer) so threads
            // start from properly-registered distinctions.
            let seeds = build_current_seeds(&engine);
            let per_thread = (iters / 8).max(1);
            let start = Instant::now();
            let handles: Vec<_> = (0..8u8)
                .map(|i| {
                    let engine = Arc::clone(&engine);
                    let seed = seeds[i as usize];
                    std::thread::spawn(move || {
                        let mut prev = D0;
                        let mut cur = seed;
                        for _ in 0..per_thread {
                            let next = engine.synthesize(cur, prev);
                            prev = cur;
                            cur = next;
                            black_box(next);
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().expect("thread join (invariant)");
            }
            start.elapsed()
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Benches — merged 1-map design
// ---------------------------------------------------------------------------

fn bench_merged_single(c: &mut Criterion) {
    let mut group = c.benchmark_group("mock_merged_single_thread");
    group.throughput(Throughput::Elements(1));
    group.bench_function("chain", |b| {
        b.iter_custom(|iters| {
            let engine = MergedEngine::new();
            let mut prev = D0;
            let mut cur = D1;
            warmup_merged(&engine, &mut prev, &mut cur, 100);
            let start = Instant::now();
            for _ in 0..iters {
                let next = engine.synthesize(cur, prev);
                prev = cur;
                cur = next;
                black_box(next);
            }
            start.elapsed()
        });
    });
    group.finish();
}

fn bench_merged_8t(c: &mut Criterion) {
    let mut group = c.benchmark_group("mock_merged_8_thread");
    group.throughput(Throughput::Elements(1));
    group.bench_function("8x_chain", |b| {
        b.iter_custom(|iters| {
            let engine = Arc::new(MergedEngine::new());
            // Build per-thread seeds in setup (BEFORE timer) so threads
            // start from properly-registered distinctions.
            let seeds = build_merged_seeds(&engine);
            let per_thread = (iters / 8).max(1);
            let start = Instant::now();
            let handles: Vec<_> = (0..8u8)
                .map(|i| {
                    let engine = Arc::clone(&engine);
                    let seed = seeds[i as usize];
                    std::thread::spawn(move || {
                        let mut prev = D0;
                        let mut cur = seed;
                        for _ in 0..per_thread {
                            let next = engine.synthesize(cur, prev);
                            prev = cur;
                            cur = next;
                            black_box(next);
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().expect("thread join (invariant)");
            }
            start.elapsed()
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_current_single,
    bench_current_8t,
    bench_merged_single,
    bench_merged_8t,
);
criterion_main!(benches);
