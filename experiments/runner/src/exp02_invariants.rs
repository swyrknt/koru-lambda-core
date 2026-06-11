//! Experiment 2: Invariant sweep.
//!
//! Builds engines at multiple scales and verifies
//!     relationship_count == 2 * distinction_count - 3
//! which is the exact r = 2d - 3 law for d >= 2 claimed by the theory.
//!
//! Usage:
//!   exp02_invariants [N1 N2 N3 ...]
//! Default: 100 10000 1000000 5000000

mod common;

use std::env;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let sizes: Vec<usize> = if args.is_empty() {
        vec![100usize, 10_000, 1_000_000, 5_000_000]
    } else {
        args.iter().map(|s| s.parse().expect("usize")).collect()
    };

    println!(
        "{:>10} {:>12} {:>14} {:>14} {:>10} {:>10}",
        "target", "d_count", "r_count", "expected_r", "delta", "secs"
    );

    for target in sizes {
        let t0 = std::time::Instant::now();
        let (engine, _tail) = common::build_chain_engine(target);
        let elapsed = t0.elapsed();

        let d = engine.distinction_count();
        let r = engine.relationship_count();
        // Theory: r = 2d - 3 (assuming d >= 2).
        let expected_r = if d >= 2 { 2 * d - 3 } else { 0 };
        let delta = r as i64 - expected_r as i64;

        println!(
            "{:>10} {:>12} {:>14} {:>14} {:>10} {:>10.3}",
            target,
            d,
            r,
            expected_r,
            delta,
            elapsed.as_secs_f64()
        );

        if delta != 0 {
            eprintln!(
                "AXIOM VIOLATION at target={} : r_count={} expected={}",
                target, r, expected_r
            );
        }
    }
}
