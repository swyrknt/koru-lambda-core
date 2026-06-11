//! Experiment 10: Native single/multi-threaded synthesize throughput.
//!
//! Measures the engine's raw synthesize ops/sec at thread counts 1, 2, 4, 8, 16.
//! Each thread owns a per-thread unique chain head (seeded via a byte
//! distinction) so that threads mostly produce novel distinctions without
//! colliding on the same SHA256 result.

use std::sync::Arc;
use std::thread;
use std::time::Instant;

use koru_lambda_core::{Canonicalizable, Distinction, DistinctionEngine};

fn run(threads: usize, total: usize) -> f64 {
    let engine = Arc::new(DistinctionEngine::new());

    // Seed each thread with a unique starting distinction.
    let mut seeds: Vec<Distinction> = Vec::with_capacity(threads);
    for i in 0..threads {
        let byte = (i as u8).to_canonical_structure(&engine);
        // Make each seed different by combining with d1 (one idempotent
        // synth) so threads have different chains.
        let seed = engine.synthesize(&byte, engine.d1());
        seeds.push(seed);
    }

    let per = (total + threads - 1) / threads;

    let start = Instant::now();
    let handles: Vec<_> = seeds
        .into_iter()
        .map(|seed| {
            let engine = Arc::clone(&engine);
            thread::spawn(move || {
                let d1 = engine.d1().clone();
                let mut current = seed;
                for _ in 0..per {
                    current = engine.synthesize(&current, &d1);
                }
                current
            })
        })
        .collect();
    for h in handles {
        let _ = h.join().unwrap();
    }
    let elapsed = start.elapsed().as_secs_f64();

    let ops = (per * threads) as f64;
    let tp = ops / elapsed;
    println!(
        "threads={:<3} ops={:<9} elapsed={:.3}s throughput={:.0} ops/sec",
        threads, ops as u64, elapsed, tp,
    );
    tp
}

fn main() {
    let total: usize = std::env::var("SYNTHS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(2_000_000);

    println!("# exp10_throughput total = {}", total);

    let thread_counts = [1usize, 2, 4, 8, 16];

    // Warm up at 1 thread.
    let _ = run(1, 100_000);

    let mut results = Vec::new();
    for &t in &thread_counts {
        let tp = run(t, total);
        results.push((t, tp));
    }
    let base = results[0].1;
    println!("\n## Scaling relative to 1 thread");
    for (t, tp) in &results {
        println!("threads={:<3} throughput={:>10.0} ops/sec  scale={:.2}x", t, tp, tp / base);
    }
}
