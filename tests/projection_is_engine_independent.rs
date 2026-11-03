//! Projection is engine-independent: two engines with identical synthesis
//! histories produce byte-identical projection output for the same
//! specification, at quiescence.
//!
//! That is the cross-engine condition of `docs/SPEC.md` §9, and the thing it
//! rules out is projection materialization being an engine-side artifact —
//! something that depends on which engine asked rather than on what the
//! graph holds. If it were, two conforming implementations could agree on
//! every identifier and still disagree on every projection.
//!
//! Scope, so the name does not overclaim, and this is the part the old name
//! obscured. Both engines are driven from **one closure over one corpus in
//! one order**. So what this establishes is determinism (Axiom 1) extended
//! across engine instances — same operations, different engine, same bytes.
//! It does **not** establish the order-independence of Law 8 or Law 9, which
//! its former name and position implied: nothing here shuffles the history.
//! `tests/order_independence.rs` is the test that does, and the gap between
//! the two went unnoticed for three months because this file's name sounded
//! like it covered both.
//!
//! The other axis is one compilation target, two engines; `tests/cond_d_prime.rs`
//! covers one history, two targets. Neither is a substitute for the other.
//!
//! *Renamed from `cond_d_falsifier.rs` per PRB-5. The old name required a
//! reader to know what "Cond D" meant, and the docstring cited the condition
//! as both §9 and §7 in consecutive paragraphs: §7 is the wire format, §9 is
//! Conditions. One of the two was simply wrong, and a reader checking the
//! claim would have landed in the wrong section half the time.*

// Native-only: `rand = "=0.8.5"` (workspace-pinned) is not available on
// wasm32 (moved to `[target.'cfg(not(target_arch = "wasm32"))'.dev-dependencies]`
// so `wasm-pack test` doesn't drag `wait-timeout` and other wasm-hostile
// transitives). The wasm-side Axiom-4 falsifier lives in
// `tests/wasm_smoke.rs`.
#![cfg(not(target_arch = "wasm32"))]

use koru_lambda_core::projection::{Adjacency, Direction};
use koru_lambda_core::{Distinction, DistinctionEngine};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

/// Deterministic seeded corpus of parent-index pairs. `StdRng` at the
/// workspace-pinned `rand = "=0.8.5"` yields a byte-stable sequence
/// across CI/local/wasm targets; drawing `u32` (not `usize`) keeps the
/// sample sequence portable across pointer widths.
#[must_use]
fn seeded_corpus_pairs(seed: u64, count: usize) -> Vec<(u32, u32)> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut pairs = Vec::with_capacity(count);
    for _ in 0..count {
        let a: u32 = rng.random_range(0..1024);
        let b: u32 = rng.random_range(0..1024);
        pairs.push((a, b));
    }
    pairs
}

/// Cross-engine projection independence probe. Two engines with
/// identical synthesis histories, queried with the same projection
/// spec at quiescence, must produce byte-identical canonical output
/// (Cond D). Corresponds to `THEORY.md § The synthesis/projection
/// dual` falsifier.
///
/// Corpus size 500 is the original probe's parameter;
/// falsifier power comes from depth-of-random-walk, not corpus size
/// specifically.
///
/// NOTE: scalar count parity is a weaker probe than Law 8's
/// set-equality claim. Under identical synthesis histories with
/// Axiom-4 content addressing, divergent sets are nearly impossible
/// in practice, but a future strengthening story could add a
/// set-equality check.
#[test]
fn cond_d_cross_engine_projection_independence() {
    // Named constants document intent and fail-compile if primordial
    // count ever changes. `chain1[PRIMORDIAL_COUNT + ROOT_OFFSET]` is
    // the shallowest non-primordial root whose downstream cone under
    // seed=2024 is guaranteed non-degenerate. The falsifier's power
    // is invariant under root choice.
    const PRIMORDIAL_COUNT: usize = 2; // d0, d1
    const ROOT_OFFSET: usize = 8; // 8th non-primordial synthesis

    // Seeded corpus — deterministic across runs.
    let corpus_pairs: Vec<(u32, u32)> = seeded_corpus_pairs(2024, 500);

    let build = |e: &DistinctionEngine| -> Vec<Distinction> {
        let mut chain = vec![e.d0(), e.d1()];
        for &(a, b) in &corpus_pairs {
            let child =
                e.synthesize(chain[a as usize % chain.len()], chain[b as usize % chain.len()]);
            chain.push(child);
        }
        chain
    };

    let e1 = DistinctionEngine::new();
    let e2 = DistinctionEngine::new();
    let chain1 = build(&e1);
    let chain2 = build(&e2);

    // Parity pre-assertion: engines byte-equivalent at the substrate
    // level BEFORE projection. If this fails, the failure is a Law 8
    // regression, not a Cond D regression — the probe distinguishes.
    assert_eq!(
        e1.distinction_count(),
        e2.distinction_count(),
        "parity pre-assert: distinction_count differs — Law 8 regression, engines diverged from same input; see THEORY.md §Law 8"
    );
    assert_eq!(
        e1.relationship_count(),
        e2.relationship_count(),
        "parity pre-assert: relationship_count differs — Law 6 primitivity violated (r = 2d − 3 broken); see THEORY.md §Law 6"
    );

    // Chain-length parity is a third Law-8-family scalar check: identical
    // synthesis histories must produce identical-length chains. Doesn't
    // add new probe class — reinforces the count-parity family (weaker
    // than set-equality, per the R3 probe-boundary note above).
    assert_eq!(
        chain1.len(),
        chain2.len(),
        "parity pre-assert: chain lengths differ — Law 8 regression, engines diverged from same input; see THEORY.md §Law 8"
    );

    // Materialize the same projection on each engine.
    //
    // Diagnostic-locality improvement: bind the root from EACH chain
    // separately and assert equality before projecting. If Axiom-4
    // content-addressing regresses (same input
    // history producing different distinction bytes at the same chain
    // index), the failure fires HERE with a clear message pointing at
    // Axiom 4 — not later as an opaque byte-diff in `canonical_bytes()`
    // that a reader has to trace back to a root-mismatch root cause.
    let root1 = chain1[PRIMORDIAL_COUNT + ROOT_OFFSET];
    let root2 = chain2[PRIMORDIAL_COUNT + ROOT_OFFSET];
    assert_eq!(
        root1, root2,
        "parity pre-assert: root distinctions differ at same chain index — Axiom-4 content-addressing broken; see THEORY.md §Axiom 4"
    );
    let proj1 =
        e1.project(root1).direction(Direction::Downstream).hops(3).signal(Adjacency).materialize();
    let proj2 =
        e2.project(root2).direction(Direction::Downstream).hops(3).signal(Adjacency).materialize();

    // The load-bearing assertions — byte equivalence.
    assert_eq!(
        proj1.projection_id(),
        proj2.projection_id(),
        "Cond D: same spec must produce same ProjectionId — cross-engine content-address determinism broken"
    );
    assert_eq!(
        proj1.canonical_bytes(),
        proj2.canonical_bytes(),
        "Cond D violation: cross-engine projection determinism broken — replay/wire/snapshot consumers will fail; see docs/SPEC.md §9 and THEORY.md"
    );
}
