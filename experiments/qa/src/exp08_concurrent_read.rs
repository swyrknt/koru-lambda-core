// Experiment 8: Concurrent reader-during-writer.
//
// HYPOTHESIS
// ----------
// A hypothetical `children_of(d)` API could be built in either of two ways:
//   (A) Scan the relationships DashMap each call (O(R)).
//   (B) Maintain a reverse index `DashMap<parent_id, Vec<child_id>>` updated
//       inside synthesize.
//
// If the reverse index is maintained OUTSIDE synthesize (e.g., from user
// code observing a synth-log), there's a torn window between:
//   (1) synthesize() returning a new child
//   (2) the user pushing an entry into the external reverse index
// during which the index is stale.
//
// If the reverse index is updated INSIDE synthesize (two DashMap mutations
// per synth), it lands atomically from the caller's POV but may be observed
// separately by a concurrent reader.
//
// This experiment prototypes both (external updater running behind the
// writers) and (internal reverse-index co-updated via a shared DashMap) and
// measures how often a reader can see a child in the reverse index that is
// NOT yet in engine.relationships (or vice versa).
//
// METHOD
// ------
// 8 writer threads synthesize(anchor, d0) where anchor is a unique-per-op
// intermediate. Writers push to an external DashMap<String, Vec<String>>.
// 1 reader thread polls the external index and compares to engine state.

use dashmap::DashMap;
use koru_lambda_core::{Distinction, DistinctionEngine};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

type ReverseIndex = DashMap<String, Vec<String>>;

fn synth_with_index(
    engine: &DistinctionEngine,
    index: &ReverseIndex,
    a: &Distinction,
    b: &Distinction,
) -> Distinction {
    // Pre-check: irreflexive case wouldn't create a child
    if a.id() == b.id() {
        return engine.synthesize(a, b);
    }
    // First commit to engine
    let before = engine.distinction_count();
    let child = engine.synthesize(a, b);
    let after = engine.distinction_count();
    // Only record as a new child if it's truly new (avoid duplicates in idx)
    if after > before {
        index
            .entry(a.id().to_string())
            .or_default()
            .push(child.id().to_string());
        index
            .entry(b.id().to_string())
            .or_default()
            .push(child.id().to_string());
    }
    child
}

fn is_in_engine_relationships(engine: &DistinctionEngine, parent: &str, child: &str) -> bool {
    let (min, max) = if parent < child { (parent, child) } else { (child, parent) };
    engine
        .get_relationships_snapshot()
        .iter()
        .any(|(a, b)| a == min && b == max)
}

fn main() {
    println!("=== Experiment 8: Concurrent Reader-During-Writer Reverse Index ===\n");

    let engine = Arc::new(DistinctionEngine::new());
    let index: Arc<ReverseIndex> = Arc::new(DashMap::new());
    let stop = Arc::new(AtomicBool::new(false));
    const NUM_WRITERS: usize = 8;
    const NUM_READER_SWEEPS: usize = 2_000;
    const PHASE1_OP_CAP: u64 = 30_000;

    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();

    // Writers synthesize pairs where one parent is d0 (so we test the
    // high fan-out hub case).
    let total_writes = Arc::new(AtomicU64::new(0));

    let mut handles = Vec::new();
    for w in 0..NUM_WRITERS {
        let engine_c = Arc::clone(&engine);
        let index_c = Arc::clone(&index);
        let stop_c = Arc::clone(&stop);
        let total_c = Arc::clone(&total_writes);
        let d0_c = d0.clone();
        let d1_c = d1.clone();
        handles.push(thread::spawn(move || {
            let mut counter = (w as u64) * 10_000_000;
            while !stop_c.load(Ordering::Relaxed) {
                if total_c.load(Ordering::Relaxed) >= PHASE1_OP_CAP {
                    break;
                }
                // Build a unique anchor per op using a short chain
                let mut anchor = if counter & 1 == 0 { d0_c.clone() } else { d1_c.clone() };
                for _ in 0..3 {
                    let bit = if counter & 1 == 1 { d1_c.clone() } else { d0_c.clone() };
                    anchor = synth_with_index(&engine_c, &index_c, &anchor, &bit);
                    counter = counter.wrapping_add(counter.wrapping_mul(2654435761));
                }
                // Finally: synth against d0 (the hub).
                let _child = synth_with_index(&engine_c, &index_c, &d0_c, &anchor);
                total_c.fetch_add(1, Ordering::Relaxed);
                counter += 1;
            }
        }));
    }

    // Let writers warm up.
    thread::sleep(std::time::Duration::from_millis(50));

    // Reader: snapshot children_of(d0) from the external index and verify
    // every entry is present in engine.relationships.
    let engine_r = Arc::clone(&engine);
    let index_r = Arc::clone(&index);
    let start = Instant::now();
    let mut orphans_total: u64 = 0; // children in index but not engine
    let mut orphans_max_sweep: u64 = 0;
    let mut missing_total: u64 = 0; // engine rel has child but index doesn't
    let mut sweeps_with_orphans: u64 = 0;
    let mut biggest_index_size = 0usize;

    for _ in 0..NUM_READER_SWEEPS {
        // Snap the external reverse index for d0
        let children_snap: Vec<String> = index_r
            .get(d0.id())
            .map(|e| e.value().clone())
            .unwrap_or_default();

        biggest_index_size = biggest_index_size.max(children_snap.len());

        // Snap engine relationships in the same tight moment
        let rels = engine_r.get_relationships_snapshot();
        let mut engine_children_of_d0 = std::collections::HashSet::new();
        for (a, b) in &rels {
            if a == d0.id() {
                engine_children_of_d0.insert(b.clone());
            } else if b == d0.id() {
                engine_children_of_d0.insert(a.clone());
            }
        }

        let mut orphan_this_sweep = 0u64;
        for c in &children_snap {
            if !engine_children_of_d0.contains(c) {
                orphan_this_sweep += 1;
            }
        }
        if orphan_this_sweep > 0 {
            sweeps_with_orphans += 1;
            orphans_total += orphan_this_sweep;
            orphans_max_sweep = orphans_max_sweep.max(orphan_this_sweep);
        }

        let children_set: std::collections::HashSet<&String> = children_snap.iter().collect();
        let mut missing_this_sweep = 0u64;
        for c in &engine_children_of_d0 {
            if !children_set.contains(c) {
                missing_this_sweep += 1;
            }
        }
        missing_total += missing_this_sweep;
    }
    let elapsed = start.elapsed();
    stop.store(true, Ordering::Release);
    for h in handles {
        h.join().unwrap();
    }

    println!("  Writers: {}   Reader sweeps: {}   Duration: {:?}", NUM_WRITERS, NUM_READER_SWEEPS, elapsed);
    println!("  Total writer ops: {}", total_writes.load(Ordering::Relaxed));
    println!("  Largest index[d0] observed: {}", biggest_index_size);
    println!();
    println!("  Orphans (index says child, engine relationships do NOT): total={} max_per_sweep={} sweeps_with_orphans={} / {}",
        orphans_total, orphans_max_sweep, sweeps_with_orphans, NUM_READER_SWEEPS);
    println!("  Missing (engine has child, index does NOT yet): total={}", missing_total);

    // Analysis:
    // Because synth_with_index() commits to engine FIRST then index SECOND,
    // the "index ahead of engine" window is the irreflexive short-circuit
    // path (no writes) or the race where the reader reads before inserts land.
    //
    // With this ordering, we should see:
    //   * orphans near zero (index never gets ahead of engine under this design)
    //   * missing > 0 (engine commits happen before the index entry)
    //
    // This confirms that an EXTERNALLY maintained reverse index can be
    // stale vs engine but never ahead. For a consumer, this means the safer
    // API semantic is: "children_of returns a best-effort view that may LAG
    // engine.relationships; it never leads."
    println!();
    if orphans_total == 0 && missing_total > 0 {
        println!("  VERDICT: CONFIRMED (engine-first pattern) --");
        println!("    - External reverse index NEVER leads engine.relationships.");
        println!("    - It can LAG: during a live writer, engine commits before index pushes.");
        println!("    - Implication for #1 API: an externally maintained children_of() is");
        println!("      'eventually consistent behind engine' under this design.");
    } else if orphans_total > 0 {
        println!("  VERDICT: ORPHANS OBSERVED -- index sometimes reports children whose");
        println!("    engine-side relationship isn't visible yet. This would only happen");
        println!("    if reordering (index.push before engine.synth) occurred, which it");
        println!("    doesn't in this prototype. If observed, investigate memory ordering.");
    } else {
        println!("  VERDICT: NO TEARS OBSERVED -- reader was too slow or writers too fast to overlap.");
    }

    // --- Phase 2: swap ordering to show the failure mode that MUST be avoided ---
    println!("\n-- Phase 2: index-first ordering (anti-pattern) --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let index: Arc<ReverseIndex> = Arc::new(DashMap::new());
        let stop = Arc::new(AtomicBool::new(false));
        let op_cap = Arc::new(std::sync::atomic::AtomicU64::new(0));
        const OP_CAP: u64 = 10_000;
        const SWEEPS: usize = 20_000;
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();

        fn bad_synth(
            engine: &DistinctionEngine,
            index: &ReverseIndex,
            a: &Distinction,
            b: &Distinction,
        ) -> Distinction {
            if a.id() == b.id() {
                return engine.synthesize(a, b);
            }
            // Compute predicted child id (replicating engine semantics)
            use sha2::{Digest, Sha256};
            let (first, second) = if a.id() < b.id() {
                (a.id(), b.id())
            } else {
                (b.id(), a.id())
            };
            let new_id_str = format!("{}:{}", first, second);
            let new_id = format!("{:x}", Sha256::digest(new_id_str.as_bytes()));
            // Push into index BEFORE engine commit
            index
                .entry(a.id().to_string())
                .or_default()
                .push(new_id.clone());
            index
                .entry(b.id().to_string())
                .or_default()
                .push(new_id.clone());
            // Artificial delay simulating any inter-thread stall between
            // index write and engine commit (lock acquisition, context switch).
            // In real systems this gap may be nanoseconds but nonzero. A loop
            // provides a reproducible stall large enough for the reader to
            // catch the racing window.
            for _ in 0..5000 {
                std::hint::spin_loop();
            }
            // Then call engine
            engine.synthesize(a, b)
        }

        let mut writers = Vec::new();
        for w in 0..NUM_WRITERS {
            let engine_c = Arc::clone(&engine);
            let index_c = Arc::clone(&index);
            let stop_c = Arc::clone(&stop);
            let cap_c = Arc::clone(&op_cap);
            let d0_c = d0.clone();
            let d1_c = d1.clone();
            writers.push(thread::spawn(move || {
                // Each writer advances a private forward chain pairing with d0
                // every step -- this MAXIMIZES the d0-hub fan-out visible in the index.
                let mut current = if w & 1 == 0 { d1_c.clone() } else { d0_c.clone() };
                while !stop_c.load(Ordering::Relaxed) {
                    if cap_c.load(Ordering::Relaxed) >= OP_CAP {
                        break;
                    }
                    current = bad_synth(&engine_c, &index_c, &d0_c, &current);
                    cap_c.fetch_add(1, Ordering::Relaxed);
                    // Introduce novelty to avoid saturating
                    current = bad_synth(&engine_c, &index_c, &current, &d1_c);
                    cap_c.fetch_add(1, Ordering::Relaxed);
                }
            }));
        }

        thread::sleep(std::time::Duration::from_millis(5));

        let mut orphans_total = 0u64;
        for _ in 0..SWEEPS {
            let children_snap: Vec<String> = index
                .get(d0.id())
                .map(|e| e.value().clone())
                .unwrap_or_default();
            let rels = engine.get_relationships_snapshot();
            let mut eng_children = std::collections::HashSet::new();
            for (a, b) in &rels {
                if a == d0.id() {
                    eng_children.insert(b.clone());
                } else if b == d0.id() {
                    eng_children.insert(a.clone());
                }
            }
            for c in &children_snap {
                if !eng_children.contains(c) {
                    orphans_total += 1;
                }
            }
        }
        stop.store(true, Ordering::Release);
        for h in writers {
            h.join().unwrap();
        }

        println!("  With INDEX-FIRST ordering, orphans observed: {}", orphans_total);
        println!("  (writer op cap: {}, sweeps: {})", OP_CAP, SWEEPS);
        if orphans_total > 0 {
            println!("  VERDICT: REPRODUCES ghost-children bug --");
            println!("    - The index can report children whose relationships are not yet");
            println!("      visible via engine.get_relationships_snapshot.");
            println!("    - Implication: API design must either (a) update the index WITHIN");
            println!("      synthesize() atomically, or (b) update AFTER engine commit, as");
            println!("      Phase 1 did.");
        } else {
            println!("  No orphans observed -- timing window missed; try more sweeps");
        }
    }

    println!("\n=== Done ===");
}
