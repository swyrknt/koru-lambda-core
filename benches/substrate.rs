//! Substrate criterion bench harness — Step 1 measurement gate.
//!
//! Measures the substrate's headline throughput numbers per DESIGN.md
//! Part 10.5 budget gates 11 and 12:
//!
//! - **Gate 11:** single-thread `synthesize` throughput ≥ 450K ops/sec
//!   (floor 300K).
//! - **Gate 12:** 8-thread `synthesize` throughput ≥ 12M ops/sec AND
//!   ≥ 4× single-thread (floor 8M absolute + 4× ratio non-negotiable).
//!
//! Plus secondary measurements for the API surface:
//!
//! - `synthesize` saturation fast-path (consumer-side hot path).
//! - `parents_of` and `degree` queries.
//! - Byte fold per call (the ByteMapping cost).
//!
//! Run with `cargo bench --bench substrate --release`. For deterministic
//! comparison across runs, ensure thermal idle (5min cooldown between
//! runs, no charging, no other heavy processes — same protocol as the
//! Step 1 hard-gate capture).

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use koru_lambda_core::{ByteMapping, Distinction, DistinctionEngine};
use std::sync::Arc;
use std::time::Instant;

// ---------------------------------------------------------------------------
// Gate 11 — single-thread synthesize throughput
// ---------------------------------------------------------------------------

fn bench_synth_novel_single_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth_novel_single_thread");
    group.throughput(Throughput::Elements(1));
    group.bench_function("chain_extension", |b| {
        b.iter_custom(|iters| {
            // Each measurement creates a fresh engine and warms up a
            // chain, then times `iters` novel extensions. The warmup
            // ensures we measure the steady-state hot path, not the
            // cold-engine setup cost.
            let engine = DistinctionEngine::new();
            let mut prev = engine.d0();
            let mut cur = engine.d1();
            for _ in 0..100 {
                let next = engine.synthesize(cur, prev);
                prev = cur;
                cur = next;
            }
            let start = Instant::now();
            for _ in 0..iters {
                let next = engine.synthesize(cur, prev);
                prev = cur;
                cur = next;
                black_box(next);
            }
            start.elapsed()
        });
    });
    group.finish();
}

fn bench_synth_saturated_single_thread(c: &mut Criterion) {
    // Consumer-side hot path: most synthesize calls in real workloads
    // hit the saturation fast-path (returning an existing distinction).
    // Should be faster than the novel path since it skips the closure
    // and only does a SHA-256 + DashMap lookup.
    let mut group = c.benchmark_group("synth_saturated_single_thread");
    group.throughput(Throughput::Elements(1));
    group.bench_function("repeated_d0_d1", |b| {
        let engine = DistinctionEngine::new();
        let a = engine.d0();
        let b_param = engine.d1();
        let _ = engine.synthesize(a, b_param); // ensure pre-populated
        b.iter(|| {
            let result = engine.synthesize(a, b_param);
            black_box(result);
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// Gate 12 — 8-thread synthesize throughput
// ---------------------------------------------------------------------------

fn bench_synth_8_thread(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth_8_thread_independent_chains");
    group.throughput(Throughput::Elements(1));
    group.bench_function("8x_chain_extension", |b| {
        b.iter_custom(|iters| {
            let engine = Arc::new(DistinctionEngine::new());

            // 8 independent seed distinctions so each thread builds a
            // distinct chain (no cross-thread saturation hiding work).
            let seeds: Vec<Distinction> = (0..8u8)
                .map(|i| {
                    let mut acc = engine.synthesize(engine.d0(), engine.d1());
                    for _ in 0..=i {
                        acc = engine.synthesize(acc, engine.d0());
                    }
                    acc
                })
                .collect();

            // Each thread does `per_thread` extensions. Total work
            // ≈ iters synth calls (criterion divides total/duration
            // to report ops/sec).
            let per_thread = (iters / 8).max(1);

            let start = Instant::now();
            let handles: Vec<_> = (0..8)
                .map(|i| {
                    let engine = Arc::clone(&engine);
                    let seed = seeds[i];
                    std::thread::spawn(move || {
                        let mut prev = engine.d0();
                        let mut cur = seed;
                        for _ in 0..per_thread {
                            let next = engine.synthesize(cur, prev);
                            prev = cur;
                            cur = next;
                            black_box(next);
                        }
                    })
                })
                .collect();
            for h in handles {
                h.join().expect("bench thread join (invariant)");
            }
            start.elapsed()
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// API surface — traversal and query costs
// ---------------------------------------------------------------------------

fn bench_parents_of_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("parents_of_lookup");
    group.throughput(Throughput::Elements(1));

    let engine = DistinctionEngine::new();
    let mut chain = vec![engine.d0(), engine.d1()];
    for _ in 0..1000 {
        let prev = chain[chain.len() - 1];
        let prev2 = chain[chain.len() - 2];
        let next = engine.synthesize(prev, prev2);
        chain.push(next);
    }
    let probe = chain[500]; // mid-chain, definitely non-primordial

    group.bench_function("mid_chain", |b| {
        b.iter(|| {
            let parents = engine.parents_of(probe);
            black_box(parents);
        });
    });
    group.finish();
}

fn bench_degree_lookup(c: &mut Criterion) {
    let mut group = c.benchmark_group("degree_lookup");
    group.throughput(Throughput::Elements(1));

    let engine = DistinctionEngine::new();
    // Fold a few bytes so d0/d1 have non-trivial degree.
    for byte in 0u8..=15 {
        let _ = ByteMapping::map_byte_to_distinction(byte, &engine);
    }
    let d0 = engine.d0();

    group.bench_function("d0_high_degree", |b| {
        b.iter(|| {
            let degree = engine.degree(d0);
            black_box(degree);
        });
    });
    group.finish();
}

// ---------------------------------------------------------------------------
// ByteMapping — fold cost
// ---------------------------------------------------------------------------

fn bench_byte_fold(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_fold");
    // Per-byte throughput. Each fold is 17 synthesize calls
    // (1 seed + 16 inner), so ops/sec at the synthesize layer
    // is roughly 17× this number.
    group.throughput(Throughput::Elements(1));

    group.bench_function("map_byte_warm_engine", |b| {
        // Pre-populate so the fold hits mostly saturated paths
        // (the steady-state cost in long-running consumer use).
        let engine = DistinctionEngine::new();
        for warmup_byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(warmup_byte, &engine);
        }
        let mut byte_cycle = 0u8;
        b.iter(|| {
            let d = ByteMapping::map_byte_to_distinction(byte_cycle, &engine);
            byte_cycle = byte_cycle.wrapping_add(1);
            black_box(d);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_synth_novel_single_thread,
    bench_synth_saturated_single_thread,
    bench_synth_8_thread,
    bench_parents_of_lookup,
    bench_degree_lookup,
    bench_byte_fold,
);
criterion_main!(benches);
