//! Experiment 11: Distinction::clone() microbenchmark.
//!
//! How expensive is cloning a Distinction (String-backed id)? What fraction
//! of synthesize() time does it represent? Informs the v2.0 proposal to
//! replace String ids with [u8; 16].

use std::hint::black_box;
use std::time::Instant;

use koru_lambda_core::DistinctionEngine;

fn bench<F: FnMut() -> R, R>(label: &str, iters: u64, mut f: F) -> f64 {
    // Warmup.
    for _ in 0..(iters / 10).max(1) {
        black_box(f());
    }
    let t0 = Instant::now();
    for _ in 0..iters {
        black_box(f());
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let per_ns = (elapsed * 1e9) / iters as f64;
    println!("{:40}  {:>10.3} ns/op  ({:.2e} ops total in {:.3}s)", label, per_ns, iters as f64, elapsed);
    per_ns
}

fn main() {
    let iters: u64 = std::env::var("ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000_000);

    println!("# exp11_clone_cost iters = {}", iters);

    let engine = DistinctionEngine::new();
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    // A synthesized 64-hex-char id distinction (the realistic case).
    let deep = engine.synthesize(&d0, &d1);
    let deep2 = engine.synthesize(&deep, &d1);

    println!(
        "id lengths -> d0={} d1={} deep={} deep2={}",
        d0.id().len(),
        d1.id().len(),
        deep.id().len(),
        deep2.id().len()
    );

    // 1. Raw clone on short id (primordial, length 1).
    let clone_short = bench("clone short id (len=1)", iters, || d1.clone());

    // 2. Raw clone on long id (SHA256 hex, length 64).
    let clone_long = bench("clone long id (len=64)", iters, || deep.clone());

    // 3. String allocation alone (produce a fresh String of length 64).
    let dummy = deep.id().to_string();
    let string_clone = bench("String clone len=64", iters, || dummy.clone());

    // 4. End-to-end saturated synthesize (repeated call, same pair).
    //    This exercises the existence check -> fast path.
    let saturated = bench("synthesize (saturated hit)", iters, || {
        engine.synthesize(&deep, &deep2)
    });

    // 5. Repeated novel chain synthesize. We drive a fresh engine per batch
    //    so we measure *novel* synthesize cost. We amortize by using a
    //    smaller iteration count because each op allocates real state.
    let novel_iters: u64 = 100_000;
    let t0 = Instant::now();
    let engine2 = DistinctionEngine::new();
    let mut current = engine2.synthesize(engine2.d0(), engine2.d1());
    let d1b = engine2.d1().clone();
    for _ in 0..novel_iters {
        current = engine2.synthesize(&current, &d1b);
    }
    let novel_elapsed = t0.elapsed().as_secs_f64();
    let novel_per_ns = (novel_elapsed * 1e9) / novel_iters as f64;
    println!(
        "{:40}  {:>10.3} ns/op  ({:.0} ops/sec)",
        "synthesize (novel chain)",
        novel_per_ns,
        novel_iters as f64 / novel_elapsed
    );

    println!("\n## Ratios");
    println!(
        "clone_long / synth(novel)     = {:.2}%   (share of novel synth cost that a single clone costs)",
        100.0 * clone_long / novel_per_ns
    );
    println!(
        "clone_long / synth(saturated) = {:.2}%",
        100.0 * clone_long / saturated
    );
    println!(
        "Each synthesize() has ~3+ String clones on the hot path \
(canonical-ordering comparisons, inserts, return-clone).\nEstimated clone budget per synth = 3 * {:.2} ns = {:.2} ns \
 ({:.1}% of novel synth).",
        clone_long,
        3.0 * clone_long,
        300.0 * clone_long / novel_per_ns
    );
    println!(
        "\nShort-id clone vs long-id clone ratio = {:.2} (shows per-byte copy dominates)",
        clone_long / clone_short
    );
    println!(
        "String::clone (len 64) vs Distinction::clone (len 64) = {:.2} ns vs {:.2} ns",
        string_clone, clone_long
    );

    // Dummy read so the optimizer cannot elide:
    black_box(&current);
}
