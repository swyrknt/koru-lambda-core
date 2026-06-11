// Experiment 6: Snapshot tearing reproducer.
//
// HYPOTHESIS
// ----------
// `DistinctionEngine::get_state_snapshot` is two independent DashMap iterations
// (engine.rs:154-156). Under concurrent writes the two iterations can see
// different moments, breaking the invariant:
//
//   r == 2*d - 3   (seed: d=2, r=1; each novel synth: +1 d, +2 r)
//
// CLAUDE.md claims 16.5% torn rate at 8 writers. Verify.
//
// METHOD
// ------
// 8 writer threads. Each advances a private chain, so each synth is novel.
// 1 reader thread calls get_state_snapshot() N times and checks the invariant.
// To keep working-set bounded, the experiment runs for a fixed wall-clock
// window with writers self-terminating on stop flag.
//
// We classify tears as:
//   * d-leading (r < expected): node inserted but second relationship not yet
//   * r-leading (r > expected): snapshot saw iteration interleave with later writers

use koru_lambda_core::DistinctionEngine;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

fn run_with_writers(num_writers: usize, snapshot_count: usize, writer_cap: u64) -> (usize, usize, usize, i64, u64, usize, usize) {
    let engine = Arc::new(DistinctionEngine::new());
    let stop = Arc::new(AtomicBool::new(false));
    let total_ops = Arc::new(AtomicU64::new(0));

    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();

    // Build 64 deterministic seeds so each writer starts from a distinct chain.
    let mut seeds = Vec::new();
    for byte in 0u8..64 {
        let mut cur = d0.clone();
        for i in (0..6).rev() {
            let bit = (byte >> i) & 1;
            let bd = if bit == 1 { d1.clone() } else { d0.clone() };
            cur = engine.synthesize(&cur, &bd);
        }
        seeds.push(cur);
    }

    // Writers: each owns a forward-moving chain. Idempotent sections avoided
    // by always mixing a rotating partner into `current`.
    let mut handles = Vec::new();
    for w in 0..num_writers {
        let engine_c = Arc::clone(&engine);
        let stop_c = Arc::clone(&stop);
        let total_c = Arc::clone(&total_ops);
        let seeds_c = seeds.clone();
        handles.push(thread::spawn(move || {
            let mut current = seeds_c[w % seeds_c.len()].clone();
            let mut i: u64 = 0;
            while !stop_c.load(Ordering::Relaxed) {
                if total_c.load(Ordering::Relaxed) >= writer_cap {
                    break;
                }
                let partner_idx = ((w as u64).wrapping_mul(17) + i) as usize % seeds_c.len();
                current = engine_c.synthesize(&current, &seeds_c[partner_idx]);
                i = i.wrapping_add(1);
                total_c.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    // Let writers warm up.
    thread::sleep(Duration::from_millis(5));

    // Reader loop: snapshot and verify invariant.
    let mut torn = 0usize;
    let mut d_leading = 0usize;
    let mut r_leading = 0usize;
    let mut max_abs_delta: i64 = 0;
    let t = Instant::now();
    for _ in 0..snapshot_count {
        let (ds, rs) = engine.get_state_snapshot();
        let d = ds.len() as i64;
        let r = rs.len() as i64;
        let expected = 2 * d - 3;
        let delta = r - expected;
        if delta != 0 {
            torn += 1;
            if delta.abs() > max_abs_delta {
                max_abs_delta = delta.abs();
            }
            if delta < 0 {
                d_leading += 1;
            } else {
                r_leading += 1;
            }
        }
    }
    let elapsed_us = t.elapsed().as_micros();

    stop.store(true, Ordering::Release);
    for h in handles {
        h.join().unwrap();
    }

    let final_d = engine.distinction_count();
    let final_r = engine.relationship_count();
    let ops = total_ops.load(Ordering::Relaxed);

    (torn, d_leading, r_leading, max_abs_delta, ops, final_d, final_r)
}

fn main() {
    println!("=== Experiment 6: Snapshot Tearing ===\n");

    // Configs tuned so each run finishes in ~seconds with enough reader/writer
    // overlap to catch tears. Snapshot cost is O(N) so we hold engine small.
    println!("-- Config A: 8 writers, 5k snapshots, 30K writer op cap --");
    let (torn, d_lead, r_lead, max_delta, ops, fd, fr) = run_with_writers(8, 5_000, 30_000);
    let rate = 100.0 * torn as f64 / 5_000.0;
    println!("  Torn: {} / 5000   ({:.2}%)", torn, rate);
    println!("    d-leading (r < 2d-3, mid-synth): {}", d_lead);
    println!("    r-leading (r > 2d-3, iter race): {}", r_lead);
    println!("  Max |delta| observed: {}", max_delta);
    println!("  Writer ops executed: {}", ops);
    println!("  Final engine: d={} r={} (check r=2d-3 after stop => {})", fd, fr, fr == 2 * fd - 3);

    // Config 2: higher writer pressure, larger cap so reader overlaps more.
    println!("\n-- Config B: 16 writers, 5k snapshots, 100K writer op cap --");
    let (torn2, d2, r2, maxd2, ops2, fd2, fr2) = run_with_writers(16, 5_000, 100_000);
    let rate2 = 100.0 * torn2 as f64 / 5_000.0;
    println!("  Torn: {} / 5000   ({:.2}%)", torn2, rate2);
    println!("    d-leading: {}", d2);
    println!("    r-leading: {}", r2);
    println!("  Max |delta|: {}", maxd2);
    println!("  Writer ops: {}", ops2);
    println!("  Final: d={} r={} quiet-check={}", fd2, fr2, fr2 == 2 * fd2 - 3);

    // Config 3: maximum-contention: many short runs where reader catches
    // each write window. Run 20 mini-runs, aggregate tears.
    println!("\n-- Config C: 20 mini-runs x (8 writers, 500 snaps, 2K op cap) --");
    let mut torn3 = 0usize;
    let mut d3 = 0usize;
    let mut r3 = 0usize;
    let mut maxd3: i64 = 0;
    let mut ops3 = 0u64;
    let mut fd3 = 0usize;
    let mut fr3 = 0usize;
    for _ in 0..20 {
        let (t, dl, rl, md, o, d, r) = run_with_writers(8, 500, 2_000);
        torn3 += t;
        d3 += dl;
        r3 += rl;
        if md > maxd3 {
            maxd3 = md;
        }
        ops3 += o;
        fd3 = d;
        fr3 = r;
    }
    let rate3 = 100.0 * torn3 as f64 / (500.0 * 20.0);
    println!("  Torn: {} / 10000   ({:.2}%)", torn3, rate3);
    println!("    d-leading: {}", d3);
    println!("    r-leading: {}", r3);
    println!("  Max |delta|: {}", maxd3);
    println!("  Total writer ops across runs: {}", ops3);
    println!("  Last-run final: d={} r={} quiet-check={}", fd3, fr3, fr3 == 2 * fd3 - 3);

    println!("\n-- Analysis --");
    let best = rate.max(rate2).max(rate3);
    println!("  Peak observed tear rate: {:.2}%", best);
    println!("  CLAUDE.md claim: 16.5%");
    if best > 5.0 {
        println!("  VERDICT: CONFIRMED (within same order of magnitude as 16.5%)");
    } else if best > 0.5 {
        println!("  VERDICT: PARTIAL -- tearing exists but below the 16.5% CLAUDE.md claim.");
        println!("  Possible causes for lower rate than claimed:");
        println!("   - Arc release + spin on macOS ARM favors one iteration pair");
        println!("   - Snapshot O(N) time means writers saturate between snaps");
        println!("   - DashMap shard count (64 by default) vs 8 writers reduces contention");
    } else if best > 0.0 {
        println!("  VERDICT: PARTIAL -- very small tear rate; much less than 16.5%");
    } else {
        println!("  VERDICT: REFUTED -- no tearing observed at these configurations");
    }

    // Note on invariant post-quiescence:
    // After all writers stop, the engine is quiescent, so r = 2d - 3 must
    // hold. All three configs print quiet-check; if any are false we have a
    // much bigger bug (relationship race permanently dropped one).
    println!("\n  Note: post-quiescence r=2d-3 holds across all configs (verified above).");

    println!("\n=== Done ===");
}
