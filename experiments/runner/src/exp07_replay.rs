//! Experiment 7: Log replay prototype.
//!
//! 1. Build engine A with 1M unique syntheses, recording the synthesis log
//!    externally as Vec<(String, String)>.
//! 2. Serialize log to disk with bincode.
//! 3. On a fresh engine B, deserialize and replay the log. Measure
//!    throughput. Verify every distinction and relationship matches A.
//! 4. Shuffle the log into random order and replay on a third engine C.
//!    Verify C matches A (content addressing implies yes).

mod common;

use rand::seq::SliceRandom;
use rand::SeedableRng;
use rand::rngs::StdRng;
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Instant;

use koru_lambda_core::{Distinction, DistinctionEngine};

fn engines_equal(a: &DistinctionEngine, b: &DistinctionEngine) -> bool {
    if a.distinction_count() != b.distinction_count() {
        eprintln!(
            "distinction_count mismatch: a={} b={}",
            a.distinction_count(),
            b.distinction_count()
        );
        return false;
    }
    if a.relationship_count() != b.relationship_count() {
        eprintln!(
            "relationship_count mismatch: a={} b={}",
            a.relationship_count(),
            b.relationship_count()
        );
        return false;
    }

    // Compare as sorted id sets.
    let mut a_ids: Vec<String> = a
        .get_distinctions_snapshot()
        .into_iter()
        .map(|d| d.to_hex())
        .collect();
    let mut b_ids: Vec<String> = b
        .get_distinctions_snapshot()
        .into_iter()
        .map(|d| d.to_hex())
        .collect();
    a_ids.sort();
    b_ids.sort();
    if a_ids != b_ids {
        eprintln!("id sets differ");
        return false;
    }

    let mut a_rels = a.get_relationships_snapshot();
    let mut b_rels = b.get_relationships_snapshot();
    a_rels.sort();
    b_rels.sort();
    if a_rels != b_rels {
        eprintln!("relationship sets differ");
        return false;
    }

    true
}

fn replay(log: &[(String, String)]) -> (DistinctionEngine, f64) {
    let engine = DistinctionEngine::new();
    let t0 = Instant::now();
    for (a_id, b_id) in log {
        // Look up parents in this engine; for properly ordered logs the
        // parents always pre-exist. The fallback uses `from_hex` to
        // reconstruct the value from the persisted hex string — this only
        // hits if the log is replayed out-of-order, where the structural
        // invariant of synthesize() still holds (content addressing is
        // order-independent; see Exp 7 / Exp 12).
        let a = engine
            .get_distinction_by_id(a_id)
            .or_else(|| Distinction::from_hex(a_id).ok())
            .expect("malformed parent id in synthesis log");
        let b = engine
            .get_distinction_by_id(b_id)
            .or_else(|| Distinction::from_hex(b_id).ok())
            .expect("malformed parent id in synthesis log");
        let _ = engine.synthesize(&a, &b);
    }
    let elapsed = t0.elapsed();
    (engine, elapsed.as_secs_f64())
}

fn main() {
    let n: usize = std::env::var("SYNTHS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    println!("# exp07_replay synths = {}", n);

    // -------- Phase 1: build A and log --------
    println!("\n## Phase 1: build engine A + log");
    let t0 = Instant::now();
    let (engine_a, log) = common::build_chain_with_log(n);
    let build_secs = t0.elapsed().as_secs_f64();
    println!(
        "built: d_count={} r_count={} log_len={} build_secs={:.3} ({:.0} ops/sec)",
        engine_a.distinction_count(),
        engine_a.relationship_count(),
        log.len(),
        build_secs,
        n as f64 / build_secs,
    );

    // -------- Phase 2: serialize / deserialize --------
    println!("\n## Phase 2: bincode serialize / deserialize");
    let path: PathBuf = std::env::temp_dir().join("exp07_log.bin");
    let t0 = Instant::now();
    let bytes = bincode::serialize(&log).expect("bincode serialize");
    let serialize_secs = t0.elapsed().as_secs_f64();
    let size = bytes.len();
    {
        let mut f = fs::File::create(&path).expect("create");
        f.write_all(&bytes).expect("write");
        f.sync_all().ok();
    }
    println!(
        "serialized {} bytes in {:.3}s ({:.2} MB, {:.0} bytes/entry)",
        size,
        serialize_secs,
        size as f64 / (1024.0 * 1024.0),
        size as f64 / log.len() as f64
    );

    let t0 = Instant::now();
    let mut f = fs::File::open(&path).expect("open");
    let mut buf = Vec::with_capacity(size);
    f.read_to_end(&mut buf).expect("read");
    let deserialize_secs = t0.elapsed().as_secs_f64();
    let log_loaded: Vec<(String, String)> = bincode::deserialize(&buf).expect("bincode deserialize");
    println!(
        "deserialized {} entries in {:.3}s",
        log_loaded.len(),
        deserialize_secs
    );
    assert_eq!(log_loaded, log, "bincode round-trip mismatch");

    // -------- Phase 3: replay B in original order --------
    println!("\n## Phase 3: replay engine B (original order)");
    let (engine_b, replay_secs) = replay(&log_loaded);
    println!(
        "replay: d_count={} r_count={} secs={:.3} ({:.0} ops/sec)",
        engine_b.distinction_count(),
        engine_b.relationship_count(),
        replay_secs,
        log_loaded.len() as f64 / replay_secs,
    );
    let ok_b = engines_equal(&engine_a, &engine_b);
    println!("engine_a == engine_b : {}", ok_b);

    // -------- Phase 4: replay C in shuffled order --------
    println!("\n## Phase 4: replay engine C (shuffled)");
    let mut shuffled = log_loaded.clone();
    let mut rng = StdRng::seed_from_u64(0xC0FFEE);
    shuffled.shuffle(&mut rng);

    let (engine_c, replay_secs_c) = replay(&shuffled);
    println!(
        "replay(shuffled): d_count={} r_count={} secs={:.3} ({:.0} ops/sec)",
        engine_c.distinction_count(),
        engine_c.relationship_count(),
        replay_secs_c,
        shuffled.len() as f64 / replay_secs_c,
    );
    let ok_c = engines_equal(&engine_a, &engine_c);
    println!("engine_a == engine_c : {}", ok_c);

    // -------- Summary --------
    println!("\n## Summary");
    println!(
        "phase                throughput(ops/sec)    duration(s)    fidelity"
    );
    println!(
        "build                {:>16.0}    {:>10.3}    (reference)",
        n as f64 / build_secs,
        build_secs
    );
    println!(
        "replay (ordered)     {:>16.0}    {:>10.3}    {}",
        log_loaded.len() as f64 / replay_secs,
        replay_secs,
        if ok_b { "PERFECT" } else { "DIVERGENT" }
    );
    println!(
        "replay (shuffled)    {:>16.0}    {:>10.3}    {}",
        shuffled.len() as f64 / replay_secs_c,
        replay_secs_c,
        if ok_c { "PERFECT" } else { "DIVERGENT" }
    );
    println!(
        "log size on disk     {:>9} bytes        per entry   {:.1} bytes",
        size,
        size as f64 / log.len() as f64
    );

    if !ok_b || !ok_c {
        std::process::exit(1);
    }
}
