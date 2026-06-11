//! Exp 4: Reverse-index hub cost.
//!
//! The TODO claims `children_of(d)` is O(1) amortized. But d0 and d1 are mega-hubs
//! whose degree scales with graph size (ByteMapping routes through them 8x per byte).
//!
//! Method:
//!   - Populate a real engine up to N distinctions (10K, 100K, 1M).
//!   - Build a prototype reverse index externally by iterating
//!     get_relationships_snapshot() once.
//!   - Measure:
//!     * degree (vec.len())         -- d0, d1, random
//!     * full enumeration (clone)   -- d0, d1, random
//!   - Compare Vec<String> vs DashSet<String> under concurrent writes.

use dashmap::{DashMap, DashSet};
use koru_lambda_core::{ByteMapping, Distinction, DistinctionEngine};
use rand::{Rng, SeedableRng};
use std::sync::Arc;
use std::time::Instant;

fn build_engine(target: usize) -> DistinctionEngine {
    let engine = DistinctionEngine::new();
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();

    // Seed lots of diverse bytes via ByteMapping to get d0/d1 heavily connected.
    for b in 0u8..=255 {
        let _ = ByteMapping::map_byte_to_distinction(b, &engine);
    }

    // After byte seeding, synthesize pairs of distinctions until target is reached.
    let mut pool: Vec<Distinction> = Vec::new();
    pool.push(d0.clone());
    pool.push(d1.clone());
    for b in 0u8..=255 {
        pool.push(ByteMapping::map_byte_to_distinction(b, &engine));
    }

    let mut rng = rand::rngs::StdRng::seed_from_u64(0xC0FFEE);
    let mut iter = 0u64;
    while engine.distinction_count() < target {
        let i = rng.gen_range(0..pool.len());
        let j = rng.gen_range(0..pool.len());
        if i == j {
            continue;
        }
        let new_d = engine.synthesize(&pool[i], &pool[j]);
        // Periodically add recent distinctions to the pool so degree keeps growing.
        if iter.is_multiple_of(3) {
            pool.push(new_d);
        }
        iter += 1;
        // Mix in some d0/d1 syntheses to guarantee hub growth.
        if iter.is_multiple_of(5) {
            let k = rng.gen_range(0..pool.len());
            let _ = engine.synthesize(&d0, &pool[k]);
        }
        if iter.is_multiple_of(7) {
            let k = rng.gen_range(0..pool.len());
            let _ = engine.synthesize(&d1, &pool[k]);
        }
    }
    engine
}

fn build_reverse_index_vec(engine: &DistinctionEngine) -> DashMap<String, Vec<String>> {
    let idx: DashMap<String, Vec<String>> = DashMap::new();
    for rel in engine.get_relationships_snapshot() {
        let (a, b) = rel;
        idx.entry(a.clone()).or_default().push(b.clone());
        idx.entry(b).or_default().push(a);
    }
    idx
}

fn build_reverse_index_set(engine: &DistinctionEngine) -> DashMap<String, DashSet<String>> {
    let idx: DashMap<String, DashSet<String>> = DashMap::new();
    for rel in engine.get_relationships_snapshot() {
        let (a, b) = rel;
        idx.entry(a.clone()).or_default().insert(b.clone());
        idx.entry(b).or_default().insert(a);
    }
    idx
}

fn measure_vec(
    idx: &DashMap<String, Vec<String>>,
    id: &str,
    iters: u32,
) -> (u128, u128, usize) {
    // degree
    let t = Instant::now();
    let mut sum_len = 0usize;
    for _ in 0..iters {
        sum_len += idx.get(id).map(|e| e.value().len()).unwrap_or(0);
    }
    let degree_ns = t.elapsed().as_nanos() / iters as u128;

    // full enumeration (clone all)
    let t = Instant::now();
    let mut last_len = 0usize;
    for _ in 0..iters {
        let v: Vec<String> = idx.get(id).map(|e| e.value().clone()).unwrap_or_default();
        last_len = v.len();
    }
    let enum_ns = t.elapsed().as_nanos() / iters as u128;

    (degree_ns, enum_ns, sum_len / iters as usize)
}

fn measure_set(
    idx: &DashMap<String, DashSet<String>>,
    id: &str,
    iters: u32,
) -> (u128, u128, usize) {
    let t = Instant::now();
    let mut sum_len = 0usize;
    for _ in 0..iters {
        sum_len += idx.get(id).map(|e| e.value().len()).unwrap_or(0);
    }
    let degree_ns = t.elapsed().as_nanos() / iters as u128;

    let t = Instant::now();
    for _ in 0..iters {
        let _v: Vec<String> = idx
            .get(id)
            .map(|e| e.value().iter().map(|k| k.clone()).collect())
            .unwrap_or_default();
    }
    let enum_ns = t.elapsed().as_nanos() / iters as u128;

    (degree_ns, enum_ns, sum_len / iters as usize)
}

fn contention_test(n: usize) {
    println!("\n--- Contention: Vec vs DashSet under concurrent writes (n={n}) ---");
    let engine = Arc::new(build_engine(n));
    let d0_id = engine.d0().id().to_string();

    // Vec version: mutex-protected insertion, we time how long to build.
    let rels = engine.get_relationships_snapshot();
    let t = Instant::now();
    let idx_vec: DashMap<String, Vec<String>> = DashMap::new();
    let chunk_size = rels.len() / 8 + 1;
    rayon::scope(|s| {
        for chunk in rels.chunks(chunk_size) {
            let idx_vec = &idx_vec;
            s.spawn(move |_| {
                for (a, b) in chunk {
                    idx_vec.entry(a.clone()).or_default().push(b.clone());
                    idx_vec.entry(b.clone()).or_default().push(a.clone());
                }
            });
        }
    });
    println!("  Vec<String> parallel build: {:.2}ms", t.elapsed().as_secs_f64() * 1000.0);
    let vec_d0 = idx_vec.get(&d0_id).map(|e| e.value().len()).unwrap_or(0);
    println!("  Vec d0 degree: {vec_d0}");

    let t = Instant::now();
    let idx_set: DashMap<String, DashSet<String>> = DashMap::new();
    rayon::scope(|s| {
        for chunk in rels.chunks(chunk_size) {
            let idx_set = &idx_set;
            s.spawn(move |_| {
                for (a, b) in chunk {
                    idx_set.entry(a.clone()).or_default().insert(b.clone());
                    idx_set.entry(b.clone()).or_default().insert(a.clone());
                }
            });
        }
    });
    println!("  DashSet<String> parallel build: {:.2}ms", t.elapsed().as_secs_f64() * 1000.0);
    let set_d0 = idx_set.get(&d0_id).map(|e| e.value().len()).unwrap_or(0);
    println!("  DashSet d0 degree: {set_d0}");
}

fn main() {
    println!("=== Exp 4: Reverse-index hub cost ===");

    let args: Vec<String> = std::env::args().collect();
    let sizes: Vec<usize> = if args.len() > 1 && args[1] == "small" {
        vec![10_000, 100_000]
    } else {
        vec![10_000, 100_000, 1_000_000]
    };

    for &n in &sizes {
        println!("\n--- Engine size: {n} ---");
        let t = Instant::now();
        let engine = build_engine(n);
        println!(
            "  built in {:.2}s, distinctions={}, relationships={}",
            t.elapsed().as_secs_f64(),
            engine.distinction_count(),
            engine.relationship_count()
        );

        let t = Instant::now();
        let idx_vec = build_reverse_index_vec(&engine);
        println!("  reverse index (Vec) built in {:.2}s", t.elapsed().as_secs_f64());

        let d0_id = engine.d0().id().to_string();
        let d1_id = engine.d1().id().to_string();
        // Pick a random non-primordial.
        let all: Vec<String> = engine
            .get_distinctions_snapshot()
            .into_iter()
            .map(|d| d.id().to_string())
            .take(100)
            .filter(|id| id != &d0_id && id != &d1_id)
            .collect();
        let random_id = all.last().cloned().unwrap_or(d1_id.clone());

        println!(
            "  d0 degree={} d1 degree={} random degree={}",
            idx_vec.get(&d0_id).map(|e| e.value().len()).unwrap_or(0),
            idx_vec.get(&d1_id).map(|e| e.value().len()).unwrap_or(0),
            idx_vec.get(&random_id).map(|e| e.value().len()).unwrap_or(0),
        );

        let iters = if n >= 1_000_000 { 1_000 } else { 10_000 };
        for (label, id) in [("d0", &d0_id), ("d1", &d1_id), ("random", &random_id)] {
            let (deg_ns, enum_ns, deg) = measure_vec(&idx_vec, id, iters);
            println!(
                "  Vec   {label:<7} degree-lookup={deg_ns:>8}ns  enum-clone={enum_ns:>10}ns  deg={deg}"
            );
        }

        // Only build DashSet index for small sizes -- very expensive for 1M.
        if n <= 100_000 {
            let idx_set = build_reverse_index_set(&engine);
            for (label, id) in [("d0", &d0_id), ("d1", &d1_id), ("random", &random_id)] {
                let (deg_ns, enum_ns, deg) = measure_set(&idx_set, id, iters);
                println!(
                    "  Set   {label:<7} degree-lookup={deg_ns:>8}ns  enum-clone={enum_ns:>10}ns  deg={deg}"
                );
            }
        }
    }

    contention_test(100_000);
}
