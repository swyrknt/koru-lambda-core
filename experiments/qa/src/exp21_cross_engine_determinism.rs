//! Experiment 21: Cross-engine determinism.
//!
//! CLAIM UNDER TEST (CLAUDE.md):
//!   "Content addressing is engine-state-independent. Same chain on different
//!    engines with different histories -> identical IDs."
//!
//! Exp 7 showed log replay yields byte-identical state on a SINGLE engine.
//! Exp 21 strengthens the claim: two engines, COLD, in SEPARATE threads,
//! driven by identical synthesis sequences, must produce byte-identical
//! `all_distinctions` and `relationships` sets at every snapshot. We also
//! run a closed-form cross-check (predict_id) so we are not merely comparing
//! the engine to itself.
//!
//! FALSIFICATION DESIGN
//! --------------------
//!   Phase 1 (baseline): Two cold engines A and B run an identical 100K-step
//!                       program (seed S). Snapshot every 1000 steps.
//!                       Compare sorted-distinction and sorted-relationship
//!                       sets, plus a SHA256 fingerprint of both.
//!
//!   Phase 2 (history adversary): A has 50K UNRELATED synths injected first.
//!                                Both then run the same test chain. Per-step
//!                                test-chain output IDs must still match.
//!
//!   Phase 3 (concurrency): Same workload on both engines, but 8 worker
//!                          threads each, different shuffle seed. Post-
//!                          quiescence sets must match.
//!
//!   Phase 4 (closed form): Run the chain through `predict_id` (pure SHA256)
//!                          and compare engine output IDs to that prediction.
//!                          Refutes any chance the engine and our test share
//!                          a bug.
//!
//!   Phase 5 (three-way): Three independent engines, one fingerprint each.
//!                        Any drift surfaces by majority disagreement.
//!
//! A single differing ID at any snapshot in any phase = claim refuted.

use koru_lambda_core::{Distinction, DistinctionEngine};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use std::collections::BTreeSet;
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

type IdxPair = (usize, usize);

// ---------- Pure helpers --------------------------------------------------

/// Closed-form prediction of the synthesized ID. Mirrors engine.rs:101-116.
fn predict_id(a: &str, b: &str) -> String {
    use sha2::{Digest, Sha256};
    if a == b {
        return a.to_string();
    }
    let (first, second) = if a < b { (a, b) } else { (b, a) };
    let new_id_str = format!("{}:{}", first, second);
    format!("{:x}", Sha256::digest(new_id_str.as_bytes()))
}

fn sorted_distinctions(engine: &DistinctionEngine) -> Vec<String> {
    let mut v: Vec<String> = engine
        .get_distinctions_snapshot()
        .into_iter()
        .map(|d| d.id().to_string())
        .collect();
    v.sort();
    v
}

fn sorted_relationships(engine: &DistinctionEngine) -> Vec<(String, String)> {
    let mut v = engine.get_relationships_snapshot();
    v.sort();
    v
}

fn engine_fingerprint(engine: &DistinctionEngine) -> String {
    use sha2::{Digest, Sha256};
    let dists = sorted_distinctions(engine);
    let rels = sorted_relationships(engine);
    let mut h = Sha256::new();
    h.update(b"D|");
    for d in &dists {
        h.update(d.as_bytes());
        h.update(b"\n");
    }
    h.update(b"R|");
    for (a, b) in &rels {
        h.update(a.as_bytes());
        h.update(b":");
        h.update(b.as_bytes());
        h.update(b"\n");
    }
    format!("{:x}", h.finalize())
}

/// Build a deterministic "program": (parent_a_idx, parent_b_idx) pairs over
/// an index space where 0 = d0, 1 = d1, and index 2+k = output of step k.
fn build_program(seed: u64, num_steps: usize) -> Vec<(usize, usize)> {
    let mut rng = rand::rngs::StdRng::seed_from_u64(seed);
    let mut prog = Vec::with_capacity(num_steps);
    for k in 0..num_steps {
        let pool = 2 + k;
        let i = rng.gen_range(0..pool);
        let mut j = rng.gen_range(0..pool);
        if j == i {
            j = (j + 1) % pool;
        }
        prog.push((i, j));
    }
    prog
}

struct StepSnapshot {
    step: usize,
    distinction_count: usize,
    relationship_count: usize,
    fingerprint: String,
}

fn run_program(
    engine: &DistinctionEngine,
    program: &[(usize, usize)],
    snapshot_every: usize,
) -> Vec<StepSnapshot> {
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut universe: Vec<Distinction> = Vec::with_capacity(program.len() + 2);
    universe.push(d0);
    universe.push(d1);
    let mut snaps = Vec::new();
    for (k, (i, j)) in program.iter().enumerate() {
        let a = universe[*i].clone();
        let b = universe[*j].clone();
        let c = engine.synthesize(&a, &b);
        universe.push(c);
        let step = k + 1;
        if snapshot_every > 0 && step % snapshot_every == 0 {
            snaps.push(StepSnapshot {
                step,
                distinction_count: engine.distinction_count(),
                relationship_count: engine.relationship_count(),
                fingerprint: engine_fingerprint(engine),
            });
        }
    }
    snaps
}

fn run_program_capture_ids(engine: &DistinctionEngine, program: &[(usize, usize)]) -> Vec<String> {
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut universe: Vec<Distinction> = Vec::with_capacity(program.len() + 2);
    universe.push(d0);
    universe.push(d1);
    let mut ids = Vec::with_capacity(program.len());
    for (i, j) in program.iter() {
        let a = universe[*i].clone();
        let b = universe[*j].clone();
        let c = engine.synthesize(&a, &b);
        ids.push(c.id().to_string());
        universe.push(c);
    }
    ids
}

// ---------- Phases --------------------------------------------------------

fn phase1_baseline(num_steps: usize, snap_every: usize) -> bool {
    println!("\n-- Phase 1: cold A vs cold B, identical chain --");
    let prog = build_program(0x0A17_151A_AC1E_u64, num_steps);
    let barrier = Arc::new(Barrier::new(2));
    let prog_a = prog.clone();
    let bar_a = Arc::clone(&barrier);
    let h_a = thread::spawn(move || {
        let eng = DistinctionEngine::new();
        bar_a.wait();
        run_program(&eng, &prog_a, snap_every)
    });
    let prog_b = prog.clone();
    let bar_b = Arc::clone(&barrier);
    let h_b = thread::spawn(move || {
        let eng = DistinctionEngine::new();
        bar_b.wait();
        run_program(&eng, &prog_b, snap_every)
    });
    let snaps_a = h_a.join().unwrap();
    let snaps_b = h_b.join().unwrap();
    assert_eq!(snaps_a.len(), snaps_b.len());
    let mut count_eq = 0usize;
    let mut count_ne = 0usize;
    let mut first_div: Option<usize> = None;
    for (sa, sb) in snaps_a.iter().zip(snaps_b.iter()) {
        let same = sa.distinction_count == sb.distinction_count
            && sa.relationship_count == sb.relationship_count
            && sa.fingerprint == sb.fingerprint;
        if same {
            count_eq += 1;
        } else {
            count_ne += 1;
            if first_div.is_none() {
                first_div = Some(sa.step);
            }
        }
    }
    println!(
        "  snapshots: {}  eq: {}  ne: {}",
        snaps_a.len(),
        count_eq,
        count_ne
    );
    let last_a = snaps_a.last().unwrap();
    let last_b = snaps_b.last().unwrap();
    println!(
        "  final A d={} r={} fp={}",
        last_a.distinction_count,
        last_a.relationship_count,
        &last_a.fingerprint[..16]
    );
    println!(
        "  final B d={} r={} fp={}",
        last_b.distinction_count,
        last_b.relationship_count,
        &last_b.fingerprint[..16]
    );
    let inv_a =
        last_a.relationship_count as i64 == 2 * last_a.distinction_count as i64 - 3;
    let inv_b =
        last_b.relationship_count as i64 == 2 * last_b.distinction_count as i64 - 3;
    println!("  r = 2d - 3  A: {}  B: {}", inv_a, inv_b);
    match first_div {
        Some(s) => {
            println!("  FIRST DIVERGENCE step {}", s);
            false
        }
        None => {
            println!("  VERDICT phase 1: CONFIRMED");
            true
        }
    }
}

fn phase2_history_adversary(num_steps: usize) -> bool {
    println!("\n-- Phase 2: A poisoned with unrelated history --");
    let prog = build_program(0xB055_DA7A_u64, num_steps);
    let noise = build_program(0x00F1_100D_C0DE_u64, 50_000);
    let prog_a = prog.clone();
    let prog_b = prog.clone();
    let h_a = thread::spawn(move || {
        let eng = DistinctionEngine::new();
        let _ = run_program(&eng, &noise, 0);
        let dn = eng.distinction_count();
        let rn = eng.relationship_count();
        let ids = run_program_capture_ids(&eng, &prog_a);
        (
            dn,
            rn,
            ids,
            engine_fingerprint(&eng),
            eng.distinction_count(),
            eng.relationship_count(),
        )
    });
    let h_b = thread::spawn(move || {
        let eng = DistinctionEngine::new();
        let ids = run_program_capture_ids(&eng, &prog_b);
        (
            0usize,
            0usize,
            ids,
            engine_fingerprint(&eng),
            eng.distinction_count(),
            eng.relationship_count(),
        )
    });
    let (dn_a, rn_a, ids_a, fp_a, df_a, rf_a) = h_a.join().unwrap();
    let (_, _, ids_b, fp_b, df_b, rf_b) = h_b.join().unwrap();
    println!("  A noise: d={} r={}", dn_a, rn_a);
    println!("  A final: d={} r={}", df_a, rf_a);
    println!("  B final: d={} r={}", df_b, rf_b);
    assert_eq!(ids_a.len(), ids_b.len());
    let mut mismatches = 0usize;
    let mut first: Option<usize> = None;
    for (k, (a, b)) in ids_a.iter().zip(ids_b.iter()).enumerate() {
        if a != b {
            mismatches += 1;
            if first.is_none() {
                first = Some(k);
            }
        }
    }
    println!(
        "  per-step ID matches: {} / {}",
        ids_a.len() - mismatches,
        ids_a.len()
    );
    println!("  engine fp differs (expected true): {}", fp_a != fp_b);
    if mismatches == 0 {
        println!("  VERDICT phase 2: CONFIRMED");
        true
    } else {
        println!("  FIRST MISMATCH at step {:?}", first);
        println!("  VERDICT phase 2: REFUTED");
        false
    }
}

fn run_concurrent(
    pairs: &[(usize, usize)],
    shuffle_seed: u64,
) -> (DistinctionEngine, std::time::Duration) {
    let engine = Arc::new(DistinctionEngine::new());
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut seeds: Vec<Distinction> = vec![d0.clone(), d1.clone()];
    let mut cur = engine.synthesize(&d0, &d1);
    seeds.push(cur.clone());
    while seeds.len() < 256 {
        let partner = seeds[seeds.len() % 2].clone();
        cur = engine.synthesize(&cur, &partner);
        seeds.push(cur.clone());
    }
    let mut order = pairs.to_vec();
    let mut rng = rand::rngs::StdRng::seed_from_u64(shuffle_seed);
    order.shuffle(&mut rng);
    const NUM_THREADS: usize = 8;
    let chunk = order.len().div_ceil(NUM_THREADS);
    let chunks: Vec<Vec<IdxPair>> = order.chunks(chunk).map(|c| c.to_vec()).collect();
    let start = Instant::now();
    let bar = Arc::new(Barrier::new(chunks.len()));
    let mut handles = Vec::new();
    for c in chunks {
        let e = Arc::clone(&engine);
        let s = seeds.clone();
        let b = Arc::clone(&bar);
        handles.push(thread::spawn(move || {
            b.wait();
            for (i, j) in c {
                let _ = e.synthesize(&s[i], &s[j]);
            }
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let elapsed = start.elapsed();
    let eng = Arc::try_unwrap(engine).unwrap_or_else(|_| panic!("still referenced"));
    (eng, elapsed)
}

fn phase3_concurrent(num_pairs: usize) -> bool {
    println!("\n-- Phase 3: concurrent execution, two engines, different scheduling --");
    let mut rng = rand::rngs::StdRng::seed_from_u64(0xC0DE_C0DE_u64);
    let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(num_pairs);
    while pairs.len() < num_pairs {
        let i = rng.gen_range(0..256);
        let mut j = rng.gen_range(0..256);
        if j == i {
            j = (j + 1) % 256;
        }
        pairs.push((i, j));
    }

    let (a, ta) = run_concurrent(&pairs, 0xA1);
    let (b, tb) = run_concurrent(&pairs, 0xB2);
    println!(
        "  A {:.2}s d={} r={}",
        ta.as_secs_f64(),
        a.distinction_count(),
        a.relationship_count()
    );
    println!(
        "  B {:.2}s d={} r={}",
        tb.as_secs_f64(),
        b.distinction_count(),
        b.relationship_count()
    );
    let da: BTreeSet<String> = sorted_distinctions(&a).into_iter().collect();
    let db: BTreeSet<String> = sorted_distinctions(&b).into_iter().collect();
    let ra: BTreeSet<(String, String)> = sorted_relationships(&a).into_iter().collect();
    let rb: BTreeSet<(String, String)> = sorted_relationships(&b).into_iter().collect();
    let d_eq = da == db;
    let r_eq = ra == rb;
    let fp_a = engine_fingerprint(&a);
    let fp_b = engine_fingerprint(&b);
    println!(
        "  distinctions eq: {}  relationships eq: {}  fp eq: {}",
        d_eq,
        r_eq,
        fp_a == fp_b
    );
    if d_eq && r_eq && fp_a == fp_b {
        println!("  VERDICT phase 3: CONFIRMED");
        true
    } else {
        println!(
            "  divergence only-in-A={} only-in-B={}",
            da.difference(&db).count(),
            db.difference(&da).count()
        );
        println!("  VERDICT phase 3: REFUTED");
        false
    }
}

fn phase4_deep_chain_history(depth: usize) -> bool {
    println!("\n-- Phase 4: deep chain + closed-form predict_id --");
    let chain = build_program(0x00DE_C0DE_DEAF_u64, depth);
    let mut predicted: Vec<String> = Vec::with_capacity(depth);
    {
        let mut universe = vec!["0".to_string(), "1".to_string()];
        for (i, j) in &chain {
            let id = predict_id(&universe[*i], &universe[*j]);
            predicted.push(id.clone());
            universe.push(id);
        }
    }
    let noise = build_program(0xFEED_FACE_u64, 25_000);
    let prog_a = chain.clone();
    let prog_b = chain.clone();
    let h_a = thread::spawn(move || {
        let eng = DistinctionEngine::new();
        let _ = run_program(&eng, &noise, 0);
        run_program_capture_ids(&eng, &prog_a)
    });
    let h_b = thread::spawn(move || {
        let eng = DistinctionEngine::new();
        run_program_capture_ids(&eng, &prog_b)
    });
    let ids_a = h_a.join().unwrap();
    let ids_b = h_b.join().unwrap();
    let mut m_ab = 0usize;
    let mut m_pred = 0usize;
    let mut fa: Option<usize> = None;
    let mut fp: Option<usize> = None;
    for k in 0..ids_a.len() {
        if ids_a[k] != ids_b[k] {
            m_ab += 1;
            if fa.is_none() {
                fa = Some(k);
            }
        }
        if ids_a[k] != predicted[k] {
            m_pred += 1;
            if fp.is_none() {
                fp = Some(k);
            }
        }
    }
    println!("  depth {}", depth);
    println!("  A vs B mismatches:    {} (first {:?})", m_ab, fa);
    println!("  A vs predict mismatches: {} (first {:?})", m_pred, fp);
    if m_ab == 0 && m_pred == 0 {
        println!("  VERDICT phase 4: CONFIRMED");
        true
    } else {
        println!("  VERDICT phase 4: REFUTED");
        false
    }
}

fn phase5_three_engine_majority(num_steps: usize) -> bool {
    println!("\n-- Phase 5: three independent threads, majority vote --");
    let prog = build_program(0xCAFE_BABE_BAAD_u64, num_steps);
    let bar = Arc::new(Barrier::new(3));
    let pa = prog.clone();
    let pb = prog.clone();
    let pc = prog.clone();
    let ba = Arc::clone(&bar);
    let bb = Arc::clone(&bar);
    let bc = Arc::clone(&bar);
    let ha = thread::spawn(move || {
        let e = DistinctionEngine::new();
        ba.wait();
        let _ = run_program(&e, &pa, 0);
        engine_fingerprint(&e)
    });
    let hb = thread::spawn(move || {
        let e = DistinctionEngine::new();
        bb.wait();
        let _ = run_program(&e, &pb, 0);
        engine_fingerprint(&e)
    });
    let hc = thread::spawn(move || {
        let e = DistinctionEngine::new();
        bc.wait();
        let _ = run_program(&e, &pc, 0);
        engine_fingerprint(&e)
    });
    let fa = ha.join().unwrap();
    let fb = hb.join().unwrap();
    let fc = hc.join().unwrap();
    println!("  fp_A {}", &fa[..16]);
    println!("  fp_B {}", &fb[..16]);
    println!("  fp_C {}", &fc[..16]);
    let all_eq = fa == fb && fb == fc;
    println!(
        "  VERDICT phase 5: {}",
        if all_eq { "CONFIRMED" } else { "REFUTED" }
    );
    all_eq
}

fn main() {
    println!("=== Experiment 21: Cross-Engine Determinism ===");
    let total = Instant::now();
    let num_steps: usize = std::env::var("STEPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(100_000);
    let snap: usize = std::env::var("SNAP")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000);
    println!("  STEPS={} SNAP={}", num_steps, snap);
    let r1 = phase1_baseline(num_steps, snap);
    let r2 = phase2_history_adversary(num_steps);
    let r3 = phase3_concurrent(num_steps.min(40_000));
    let r4 = phase4_deep_chain_history(num_steps.min(20_000));
    let r5 = phase5_three_engine_majority(num_steps.min(50_000));
    println!("\n=== Summary ===");
    println!(
        "  Phase 1 baseline:                  {}",
        if r1 { "CONFIRMED" } else { "REFUTED" }
    );
    println!(
        "  Phase 2 history adversary:         {}",
        if r2 { "CONFIRMED" } else { "REFUTED" }
    );
    println!(
        "  Phase 3 concurrent:                {}",
        if r3 { "CONFIRMED" } else { "REFUTED" }
    );
    println!(
        "  Phase 4 deep + closed-form:        {}",
        if r4 { "CONFIRMED" } else { "REFUTED" }
    );
    println!(
        "  Phase 5 three-way majority:        {}",
        if r5 { "CONFIRMED" } else { "REFUTED" }
    );
    let overall = r1 && r2 && r3 && r4 && r5;
    println!(
        "  OVERALL: {}",
        if overall { "CONFIRMED" } else { "REFUTED" }
    );
    println!("  total: {:.2}s", total.elapsed().as_secs_f64());
}
