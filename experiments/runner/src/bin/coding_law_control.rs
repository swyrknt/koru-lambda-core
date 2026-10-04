//! Controls for the Coding Law ρ measurement — the null, and the mechanism.
//!
//! Run: `cargo run -p koru-experiments-runner --release --bin coding_law_control`

use koru_experiments_runner::coding_law_control::{
    mechanism_correlation, saturation_sweep, shuffled_null,
};
use koru_experiments_runner::coding_law_workload::{spearman_rho, CodingLawWorkload};
use koru_lambda_core::DistinctionEngine;

/// The pinned corpus parameters, so the headline number is reproduced here
/// before anything is said about it.
const ALPHA: f64 = 1.0;
const SEED: u64 = 0xC0DE;
const N: usize = 4096;

fn main() {
    let engine = DistinctionEngine::new();
    let workload = CodingLawWorkload::generate(&engine, ALPHA, SEED, N, 8 * N);
    let delta = workload.run_against(&engine);
    let rho = spearman_rho(&workload.freq, &delta);

    let novel = delta.iter().sum::<i64>() as f64 / 2.0;
    let saturation = 1.0 - novel / (8 * N) as f64;

    println!("=== pinned workload (alpha={ALPHA}, seed={SEED:#x}, N={N}, M=8N)");
    println!("  rho                 {rho:.4}");
    println!("  saturation rate     {saturation:.4}");
    println!("  gate                >= 0.985");

    println!("\n=== control 1: the null (degree deltas fixed, frequency permuted)");
    let null = shuffled_null(&workload.freq, &delta, 200, 0xA11CE);
    println!("  trials              {}", null.trials);
    println!("  mean rho            {:+.4}", null.mean);
    println!("  sd                  {:.4}", null.sd);
    println!("  max |rho|           {:.4}", null.max_abs);
    println!(
        "  verdict             {}",
        if null.max_abs < 0.985 {
            "the gate is not clearable by a non-relationship"
        } else {
            "THE GATE IS CLEARABLE BY NOISE"
        }
    );

    println!("\n=== control 2: the stated mechanism (rho should fall as saturation rises)");
    // Pool size, draw count and Zipf exponent varied to span the saturation
    // range the theory describes: a small pool drawn many times under a narrow
    // Zipf saturates heavily; a large pool drawn fewer times under a broad one
    // barely saturates at all.
    let shapes = [
        (8192usize, 8192usize, 0.5f64),
        (8192, 16384, 0.8),
        (4096, 32768, 1.0),
        (2048, 32768, 1.0),
        (1024, 32768, 1.2),
        (512, 32768, 1.4),
        (256, 32768, 1.6),
        (128, 32768, 1.8),
    ];
    let points = saturation_sweep(&shapes, SEED);
    println!("    pool    draws  alpha   saturation      rho");
    for p in &points {
        println!(
            "  {:6}  {:7}  {:5.1}   {:10.4}  {:+7.4}",
            p.pool, p.draws, p.alpha, p.saturation, p.rho
        );
    }
    let mech = mechanism_correlation(&points);
    println!("\n  rho(saturation, rho) {mech:+.4}");
    println!(
        "  verdict              {}",
        if mech < -0.5 {
            "saturation explains the shortfall, as THEORY claims"
        } else {
            "SATURATION DOES NOT EXPLAIN IT — THEORY's account needs replacing"
        }
    );
}
