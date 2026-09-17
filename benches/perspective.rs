//! v2.0.0 API criterion bench harness — measures the four new
//! API surfaces added by E02 (S02-S05). No shipping gate on these numbers
//! yet; they exist as baseline references for post-2.0.0 regression watch.
//!
//! The four surfaces:
//!
//! - **`verify(RawDistinctionId) -> Result<Distinction, VerifyError>`**
//!   (`src/engine.rs`) — the trust-boundary entry point. Compared against
//!   `has(Distinction) -> bool` (the pre-verify baseline) to confirm the
//!   `Result`-wrapping cost is noise-level.
//! - **`synthesize_novel(a, b) -> SynthesisOutcome`** (`src/engine.rs`) —
//!   the write-coupled novelty probe. Compared against `synthesize(a, b)`
//!   to confirm the `synthesize_inner` refactor shares the hot path.
//! - **`Projection<'e, S>::materialize`** (`src/projection.rs`) — cone
//!   traversal + per-node compute. Probed at 3 signals × 2 cone sizes.
//! - **`Projection<'e, S>::canonical_bytes`** (`src/projection.rs`) —
//!   wire-format serialize of a pre-materialized projection.
//!
//! Also exercises the `CoreSignal::__materialize_special` fast-path via
//! the `HopDistance` bench (the one substrate signal that overrides the
//! default `None` dispatch and reuses the hop counts carried in the
//! BFS-produced `(Distinction, hop)` pairs).
//!
//! Run with `cargo bench --bench perspective --release`. Same thermal-
//! idle discipline as `substrate.rs` for cross-run comparability.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use koru_lambda_core::projection::Direction;
use koru_lambda_core::{
    Adjacency, Degree, Distinction, DistinctionEngine, HopDistance, Projection, RawDistinctionId,
};
use std::time::Instant;

// ---------------------------------------------------------------------------
// Helpers — chain builders shared by the four bench groups
// ---------------------------------------------------------------------------

/// Build a chain of `n` synthesized distinctions on a fresh engine and
/// return `(engine, chain)`. The chain is `[d0, d1, s2, s3, ..., s_{n+1}]`
/// where each `s_i` is the synthesis of the two preceding distinctions.
fn build_chain(n: usize) -> (DistinctionEngine, Vec<Distinction>) {
    let engine = DistinctionEngine::new();
    let mut chain = Vec::with_capacity(n + 2);
    chain.push(engine.d0());
    chain.push(engine.d1());
    for _ in 0..n {
        let last = chain[chain.len() - 1];
        let prev = chain[chain.len() - 2];
        chain.push(engine.synthesize(last, prev));
    }
    (engine, chain)
}

// ---------------------------------------------------------------------------
// 1. verify() throughput vs has() baseline
// ---------------------------------------------------------------------------

fn bench_verify(c: &mut Criterion) {
    let mut group = c.benchmark_group("verify");
    group.throughput(Throughput::Elements(1));

    // Registered-bytes path — the hot case downstream deserializers hit.
    group.bench_function("registered_distinction", |b| {
        let (engine, chain) = build_chain(1000);
        let probe_bytes: [u8; 16] = *chain[500].as_bytes();
        b.iter(|| {
            let raw = RawDistinctionId::from_bytes(black_box(probe_bytes));
            let result = engine.verify(raw);
            black_box(result)
        });
    });

    // Foreign-bytes error path — deserializer rejecting garbage input.
    group.bench_function("foreign_bytes", |b| {
        let (engine, _chain) = build_chain(1000);
        let foreign: [u8; 16] = [0xCC; 16];
        b.iter(|| {
            let raw = RawDistinctionId::from_bytes(black_box(foreign));
            let result = engine.verify(raw);
            black_box(result)
        });
    });

    // has() baseline — the pre-S04 API a caller would have used if they
    // already held a `Distinction`. Compare against `registered_distinction`
    // to confirm `verify` is a `has` wrapped in `Result` construction with
    // near-zero measurable overhead.
    group.bench_function("has_baseline", |b| {
        let (engine, chain) = build_chain(1000);
        let probe = chain[500];
        b.iter(|| {
            let hit = engine.has(black_box(probe));
            black_box(hit)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 2. synthesize_novel vs synthesize — chain extension parity
// ---------------------------------------------------------------------------

fn bench_synthesize_novel(c: &mut Criterion) {
    let mut group = c.benchmark_group("synth_novel_via_synthesize_novel");
    group.throughput(Throughput::Elements(1));
    group.bench_function("chain_extension", |b| {
        b.iter_custom(|iters| {
            // Fresh engine per measurement so we always exercise the
            // novel-insert hot path (not the saturation fast-path).
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
                let outcome = engine.synthesize_novel(cur, prev);
                let next = outcome.distinction();
                prev = cur;
                cur = next;
                let _ = black_box(outcome);
            }
            start.elapsed()
        });
    });
    group.finish();

    let mut baseline = c.benchmark_group("synth_novel_via_synthesize");
    baseline.throughput(Throughput::Elements(1));
    baseline.bench_function("chain_extension", |b| {
        b.iter_custom(|iters| {
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
    baseline.finish();
}

// ---------------------------------------------------------------------------
// 3. Projection::materialize — signal × cone-size grid
// ---------------------------------------------------------------------------

fn bench_projection_materialize(c: &mut Criterion) {
    // Build once; every bench iteration re-materializes against the same
    // engine state. 10K-chain engine is large enough that the mid-chain
    // root has real upstream parentage at 3 hops.
    let (engine, chain) = build_chain(10_000);
    let root = chain[5_000];

    let mut group = c.benchmark_group("materialize");
    group.throughput(Throughput::Elements(1));

    group.bench_function("adjacency_hops_1", |b| {
        b.iter(|| {
            let proj = engine
                .project(black_box(root))
                .direction(Direction::Upstream)
                .hops(1)
                .signal(Adjacency)
                .materialize();
            black_box(proj)
        });
    });

    group.bench_function("adjacency_hops_3", |b| {
        b.iter(|| {
            let proj = engine
                .project(black_box(root))
                .direction(Direction::Upstream)
                .hops(3)
                .signal(Adjacency)
                .materialize();
            black_box(proj)
        });
    });

    group.bench_function("degree_hops_3", |b| {
        b.iter(|| {
            let proj = engine
                .project(black_box(root))
                .direction(Direction::Upstream)
                .hops(3)
                .signal(Degree)
                .materialize();
            black_box(proj)
        });
    });

    // HopDistance exercises the `CoreSignal::__materialize_special`
    // fast-path — the one signal that reuses the hop counts already
    // carried in the BFS-produced pairs rather than a per-node compute().
    group.bench_function("hop_distance_hops_3", |b| {
        b.iter(|| {
            let proj = engine
                .project(black_box(root))
                .direction(Direction::Upstream)
                .hops(3)
                .signal(HopDistance)
                .materialize();
            black_box(proj)
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// 4. Projection::canonical_bytes — wire-format serialize cost
// ---------------------------------------------------------------------------

fn bench_canonical_bytes(c: &mut Criterion) {
    let (engine, chain) = build_chain(10_000);
    let root = chain[5_000];

    // Pre-materialize outside the timer. Every iteration times only
    // `canonical_bytes()` — the wire-format serialize path.
    let proj: Projection<'_, Adjacency> =
        engine.project(root).direction(Direction::Upstream).hops(3).signal(Adjacency).materialize();

    let mut group = c.benchmark_group("canonical_bytes");
    group.throughput(Throughput::Elements(1));
    group.bench_function("adjacency_hops_3", |b| {
        b.iter(|| {
            let bytes = proj.canonical_bytes();
            black_box(bytes)
        });
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_verify,
    bench_synthesize_novel,
    bench_projection_materialize,
    bench_canonical_bytes,
);
criterion_main!(benches);
