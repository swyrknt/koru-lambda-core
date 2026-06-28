//! Scale invariant probes — CHECKLIST.md line 136.
//!
//! The substrate's structural law `r = 2d − 3` (binary parentage, every
//! non-primordial distinction adds exactly 2 edges) must hold at every
//! scale, not just the small-scale `law6_r_equals_2d_minus_3_at_small_scale`
//! unit test in `src/engine.rs`. This file runs the gate-required 5M
//! scale verification.
//!
//! Per DESIGN.md gate 16: no `#[ignore]` / `#[cfg(slow)]` on essential
//! probes. The test runs on every `cargo test --release` invocation.
//! In `--release` mode on M3 Pro it takes ~1.5s (≈4.4M synths/sec); in
//! debug builds it takes 10–30s depending on optimization level.

use koru_lambda_core::DistinctionEngine;

/// 5M synthesis chain extension; verifies `r = 2d − 3` with zero
/// deviations and the explicit arithmetic `d = 2 + N, r = 2(2+N) − 3
/// = 2N + 1`.
///
/// Falsifies any subtle bug that would only manifest at scale: a
/// per-N-th-synthesis off-by-one in the parent edge accounting, a
/// pre-seed contract violation that only shows up after enough chain
/// extensions, etc.
#[test]
fn r_equals_2d_minus_3_at_5m_synths() {
    const N: usize = 5_000_000;

    let engine = DistinctionEngine::new();
    let mut prev = engine.d0();
    let mut cur = engine.d1();

    for _ in 0..N {
        let next = engine.synthesize(cur, prev);
        prev = cur;
        cur = next;
    }

    // Structural-invariant check (the gate-required assertion).
    engine
        .check_structural_invariant()
        .expect("r = 2d − 3 holds with zero deviations at 5M synths (invariant)");

    // Explicit arithmetic: 2 primordials + N syntheses = N+2 distinctions,
    // r = 2(N+2) − 3 = 2N + 1.
    let expected_d = N + 2;
    let expected_r = 2 * N + 1;
    assert_eq!(
        engine.distinction_count(),
        expected_d,
        "distinction_count after 5M chain syntheses must equal N+2"
    );
    assert_eq!(
        engine.relationship_count(),
        expected_r,
        "relationship_count after 5M chain syntheses must equal 2N+1"
    );
}
