// Experiment 12: Commutative log equivalence under concurrent writes.
//
// HYPOTHESIS
// ----------
// The theory guardian's concern: is the synthesis log truly commutative?
// Given two engines processing the SAME set of (parent_a, parent_b) pairs
// in DIFFERENT arrival orders, do they produce:
//   (a) identical distinction_count and relationship_count?
//   (b) identical engine state (same all_distinctions set, same relationships set)?
//   (c) logs that are equal as multisets (after canonicalization)?
//
// Commutativity + content addressing should guarantee (a) and (b). Log
// contents (c) depend on whether the log records (a, b) or canonical
// (min(a.id, b.id), max(a.id, b.id)).
//
// METHOD
// ------
// 1. Build a "seed" population of parents deterministically.
// 2. Generate N = 2000 pairs drawn from the seeds.
// 3. Engine A: apply in forward order. Log each applied pair.
// 4. Engine B: apply in shuffled order. Log each applied pair.
// 5. Compare engine states. Compare logs as ordered lists and as
//    canonicalized multisets.
// 6. Repeat under concurrent writes (multi-threaded) for both engines
//    and compare final states.

use koru_lambda_core::{Distinction, DistinctionEngine};
use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand::{Rng, RngCore};
use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};
use std::thread;

/// A synthesis log entry.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
struct LogEntry {
    parent_a: String,
    parent_b: String,
}

impl LogEntry {
    fn canonical(&self) -> (String, String) {
        if self.parent_a < self.parent_b {
            (self.parent_a.clone(), self.parent_b.clone())
        } else {
            (self.parent_b.clone(), self.parent_a.clone())
        }
    }
}

/// Synthesize and log. Returns the child.
fn synth_and_log(
    engine: &DistinctionEngine,
    log: &Mutex<Vec<LogEntry>>,
    a: &Distinction,
    b: &Distinction,
) -> Distinction {
    let child = engine.synthesize(a, b);
    log.lock().unwrap().push(LogEntry {
        parent_a: a.to_hex(),
        parent_b: b.to_hex(),
    });
    child
}

fn build_seeds(engine: &DistinctionEngine) -> Vec<Distinction> {
    let mut seeds = vec![engine.d0().clone(), engine.d1().clone()];
    // Build ~20 seeds by chaining primordials deterministically.
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    seeds.push(engine.synthesize(&d0, &d1));
    let mut cur = seeds.last().unwrap().clone();
    for i in 0..18 {
        let partner = if i % 2 == 0 { d0.clone() } else { d1.clone() };
        cur = engine.synthesize(&cur, &partner);
        seeds.push(cur.clone());
    }
    seeds
}

fn engine_state_hash(engine: &DistinctionEngine) -> (BTreeSet<String>, BTreeSet<(String, String)>) {
    let distinctions: BTreeSet<String> = engine
        .get_distinctions_snapshot()
        .iter()
        .map(|d| d.to_hex())
        .collect();
    let relationships: BTreeSet<(String, String)> = engine
        .get_relationships_snapshot()
        .into_iter()
        .collect();
    (distinctions, relationships)
}

fn main() {
    println!("=== Experiment 12: Log Commutativity ===\n");

    const NUM_PAIRS: usize = 2000;
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xC0FFEE);

    // Step 1: Build shared seed population in a throwaway engine to have ids.
    let mut proto = DistinctionEngine::new();
    let seeds = build_seeds(&proto);
    proto = DistinctionEngine::new();
    let _ = proto; // drop

    // Step 2: Generate NUM_PAIRS unique seed-index pairs.
    let mut raw_pairs: Vec<(usize, usize)> = Vec::with_capacity(NUM_PAIRS);
    while raw_pairs.len() < NUM_PAIRS {
        let i = rng.gen_range(0..seeds.len());
        let mut j = rng.gen_range(0..seeds.len());
        if i == j {
            j = (j + 1) % seeds.len();
        }
        raw_pairs.push((i, j));
    }

    // --- Phase 1: Engine A (forward) vs Engine B (shuffled), single-threaded ---
    println!("-- Phase 1: single-threaded, forward vs shuffled --");

    let engine_a = DistinctionEngine::new();
    let seeds_a = build_seeds(&engine_a);
    let log_a: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());
    for &(i, j) in &raw_pairs {
        synth_and_log(&engine_a, &log_a, &seeds_a[i], &seeds_a[j]);
    }

    let engine_b = DistinctionEngine::new();
    let seeds_b = build_seeds(&engine_b);
    let log_b: Mutex<Vec<LogEntry>> = Mutex::new(Vec::new());
    let mut shuffled = raw_pairs.clone();
    shuffled.shuffle(&mut rng);
    for &(i, j) in &shuffled {
        synth_and_log(&engine_b, &log_b, &seeds_b[i], &seeds_b[j]);
    }

    let (da, ra) = engine_state_hash(&engine_a);
    let (db, rb) = engine_state_hash(&engine_b);

    println!("  A: distinctions={}  relationships={}", da.len(), ra.len());
    println!("  B: distinctions={}  relationships={}", db.len(), rb.len());
    let d_eq = da == db;
    let r_eq = ra == rb;
    println!("  distinctions set equal: {}", d_eq);
    println!("  relationships set equal: {}", r_eq);
    println!("  Engine final state identical: {}", d_eq && r_eq);

    // Logs as raw ordered lists
    let la = log_a.lock().unwrap().clone();
    let lb = log_b.lock().unwrap().clone();
    let logs_raw_eq = la == lb;
    println!("  log_a == log_b (raw order): {}", logs_raw_eq);

    // Logs as multisets of raw (a, b)
    let mut ma: HashMap<(String, String), i64> = HashMap::new();
    for e in &la {
        *ma.entry((e.parent_a.clone(), e.parent_b.clone())).or_default() += 1;
    }
    let mut mb: HashMap<(String, String), i64> = HashMap::new();
    for e in &lb {
        *mb.entry((e.parent_a.clone(), e.parent_b.clone())).or_default() += 1;
    }
    println!("  log_a == log_b (raw multiset): {}", ma == mb);

    // Logs as multisets after canonical (min, max)
    let mut ca: HashMap<(String, String), i64> = HashMap::new();
    for e in &la {
        *ca.entry(e.canonical()).or_default() += 1;
    }
    let mut cb: HashMap<(String, String), i64> = HashMap::new();
    for e in &lb {
        *cb.entry(e.canonical()).or_default() += 1;
    }
    println!("  log_a == log_b (canonical multiset): {}", ca == cb);

    if d_eq && r_eq && ca == cb {
        println!("  VERDICT: CONFIRMED -- engine states are identical and logs are");
        println!("  equivalent under canonical (min,max) sort. Commutativity holds.");
    } else if d_eq && r_eq {
        println!("  VERDICT: PARTIAL -- engine state identical but log multisets differ.");
    } else {
        println!("  VERDICT: REFUTED -- engine state differs across arrival orders");
        println!("  This would break the axiom of commutativity. Escalate.");
    }

    // --- Phase 2: concurrent writes, two engines, same pair set ---
    println!("\n-- Phase 2: concurrent writes (8 writers) --");

    // Each writer picks pairs from the SAME raw set but in thread-interleaved order.
    fn run_concurrent(
        raw_pairs: &[(usize, usize)],
        seed_shuffle: u64,
    ) -> (DistinctionEngine, Vec<LogEntry>) {
        let engine = DistinctionEngine::new();
        let seeds = build_seeds(&engine);
        let engine = Arc::new(engine);
        let log: Arc<Mutex<Vec<LogEntry>>> = Arc::new(Mutex::new(Vec::new()));

        // Partition pairs across 8 writers with a shuffle.
        let mut shuf = raw_pairs.to_vec();
        let mut rng = rand::rngs::StdRng::seed_from_u64(seed_shuffle);
        shuf.shuffle(&mut rng);

        const NUM_THREADS: usize = 8;
        let chunk_size = (shuf.len() + NUM_THREADS - 1) / NUM_THREADS;
        let chunks: Vec<Vec<(usize, usize)>> = shuf
            .chunks(chunk_size)
            .map(|c| c.to_vec())
            .collect();

        let mut handles = Vec::new();
        for chunk in chunks {
            let engine_c = Arc::clone(&engine);
            let log_c = Arc::clone(&log);
            let seeds_c = seeds.clone();
            handles.push(thread::spawn(move || {
                for (i, j) in chunk {
                    let child = engine_c.synthesize(&seeds_c[i], &seeds_c[j]);
                    let _ = child;
                    log_c.lock().unwrap().push(LogEntry {
                        parent_a: seeds_c[i].to_hex(),
                        parent_b: seeds_c[j].to_hex(),
                    });
                }
            }));
        }
        for h in handles {
            h.join().unwrap();
        }

        let log_final = log.lock().unwrap().clone();
        let engine_final = Arc::try_unwrap(engine).unwrap_or_else(|_| panic!("still referenced"));
        (engine_final, log_final)
    }

    let (eng_c1, log_c1) = run_concurrent(&raw_pairs, 1);
    let (eng_c2, log_c2) = run_concurrent(&raw_pairs, 2);
    let (dc1, rc1) = engine_state_hash(&eng_c1);
    let (dc2, rc2) = engine_state_hash(&eng_c2);

    println!("  Concurrent run 1: distinctions={} relationships={}", dc1.len(), rc1.len());
    println!("  Concurrent run 2: distinctions={} relationships={}", dc2.len(), rc2.len());
    let c_d_eq = dc1 == dc2;
    let c_r_eq = rc1 == rc2;
    println!("  Concurrent runs produce identical state: {}", c_d_eq && c_r_eq);

    // Canonical-multiset log equivalence
    let mut m1: HashMap<(String, String), i64> = HashMap::new();
    for e in &log_c1 {
        *m1.entry(e.canonical()).or_default() += 1;
    }
    let mut m2: HashMap<(String, String), i64> = HashMap::new();
    for e in &log_c2 {
        *m2.entry(e.canonical()).or_default() += 1;
    }
    println!("  Logs equivalent under canonical multiset: {}", m1 == m2);
    let log_raw_eq = log_c1 == log_c2;
    println!("  Logs equal in raw ORDER: {} (expected false under concurrency)", log_raw_eq);

    // Compare against single-threaded engine A
    let cross_d_eq = dc1 == da;
    let cross_r_eq = rc1 == ra;
    println!("  Concurrent run 1 matches single-threaded A: d_eq={} r_eq={}", cross_d_eq, cross_r_eq);

    if c_d_eq && c_r_eq && cross_d_eq && cross_r_eq && m1 == m2 {
        println!("  VERDICT: CONFIRMED -- concurrent and serial runs converge to the");
        println!("  same engine state. Commutativity robust under thread interleaving.");
    } else {
        println!("  VERDICT: PARTIAL or REFUTED -- inspect above numbers.");
    }

    // --- Phase 3: does the raw log leak arrival order? ---
    println!("\n-- Phase 3: does the raw log leak arrival order? --");
    // The raw log DOES preserve insertion order (Vec<LogEntry> per writer).
    // Across writers, interleaving varies. If a replayer reads the log in raw
    // order on a fresh engine, will it land in the same final state?
    //
    // Because synthesize is commutative AND deterministic, ANY total order
    // over the log yields the same engine state. This is trivially true if
    // the log is append-only and each entry is independently synthesizable.
    //
    // Verification: replay log_c1 in raw order on a fresh engine.
    let replay_engine = DistinctionEngine::new();
    let replay_seeds = build_seeds(&replay_engine);
    let seed_map: HashMap<String, Distinction> = replay_seeds
        .iter()
        .map(|d| (d.to_hex(), d.clone()))
        .collect();
    // Build id -> distinction map incrementally from log
    let mut id_map: HashMap<String, Distinction> = seed_map;
    for e in &log_c1 {
        let a = id_map.get(&e.parent_a).cloned();
        let b = id_map.get(&e.parent_b).cloned();
        match (a, b) {
            (Some(a), Some(b)) => {
                let child = replay_engine.synthesize(&a, &b);
                id_map.insert(child.to_hex(), child);
            }
            _ => {
                // Parent not yet materialized: means the log entry references a parent
                // that is itself a synthesis output. In a real append-only log this
                // should have been logged earlier. Skip but count.
                // With seed-only parents in our design this never triggers.
                println!("    [warn] log entry with unresolved parent(s): {:?}", e);
            }
        }
    }
    let (dr, rr) = engine_state_hash(&replay_engine);
    println!("  Replay run: distinctions={} relationships={}", dr.len(), rr.len());
    println!("  Replay matches concurrent run 1: d_eq={} r_eq={}", dr == dc1, rr == rc1);

    if dr == dc1 && rr == rc1 {
        println!("  VERDICT: CONFIRMED -- raw-order replay round-trips perfectly.");
        println!("  Log format need NOT be canonicalized for replay correctness");
        println!("  (determinism + commutativity do the work).");
        println!("  Canonical (min,max) form is only needed for byte-identical log");
        println!("  EQUALITY comparisons across processes.");
    }

    println!("\n=== Done ===");
}
