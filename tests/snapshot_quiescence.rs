//! What the two snapshot reads promise, and what they promise only at
//! quiescence.
//!
//! TST-10. `snapshot_parentage` is documented as "the canonical persistence
//! dump: combined with `replay_topological`, it reconstructs a
//! byte-identical engine" — and nothing tested that under concurrent
//! writers, where it is not true. The two reads walk a `DashMap` holding
//! one shard's lock at a time, so neither is a point-in-time snapshot.
//!
//! The item offered a choice: test it concurrently, or state the
//! precondition. Stating it alone would leave the statement unevidenced, so
//! both halves are asserted here — the guarantee at quiescence, and the
//! weaker property that survives contention and is what a caller sampling a
//! live engine can actually rely on.

use koru_lambda_core::replay::replay_topological;
use koru_lambda_core::{Distinction, DistinctionEngine};
use sha2::{Digest, Sha256};
use std::sync::Arc;
use std::thread;

/// Recompute a child's identity from its parents, independently of the
/// engine — the same two lines `docs/SPEC.md` §4 specifies.
///
/// Returns the 16 bytes rather than a `Distinction`, because there is no
/// path from bytes back to that type outside the engine. That is ENG-4's
/// boundary, and a test is not an exemption from it.
fn recompute(a: Distinction, b: Distinction) -> [u8; 16] {
    let (lo, hi) = if a.as_bytes() <= b.as_bytes() { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update(lo.as_bytes());
    h.update(hi.as_bytes());
    let digest = h.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

fn grow(e: &DistinctionEngine, n: usize) {
    let mut nodes = vec![e.d0(), e.d1()];
    let mut i = 0usize;
    while nodes.len() < n {
        let j = i + 1;
        if j >= nodes.len() {
            break;
        }
        nodes.push(e.synthesize(nodes[i], nodes[j]));
        i += 1;
    }
}

/// At quiescence the dump reconstructs the engine exactly.
///
/// Not "the counts match" — every distinction and every parentage pair,
/// which is what byte-identical has to mean.
#[test]
fn a_quiescent_dump_reconstructs_the_engine() {
    let e = DistinctionEngine::new();
    grow(&e, 400);

    let dump = e.snapshot_parentage();
    let restored = replay_topological(dump).expect("a quiescent dump is complete");

    assert_eq!(restored.distinction_count(), e.distinction_count());
    assert_eq!(restored.relationship_count(), e.relationship_count());

    let mut want = e.snapshot_parentage();
    let mut got = restored.snapshot_parentage();
    want.sort_by_key(|(c, _)| *c.as_bytes());
    got.sort_by_key(|(c, _)| *c.as_bytes());
    assert_eq!(got, want, "the reconstruction is byte-identical, not merely the same size");

    let mut wn = e.snapshot_distinctions();
    let mut gn = restored.snapshot_distinctions();
    wn.sort_by_key(|d| *d.as_bytes());
    gn.sort_by_key(|d| *d.as_bytes());
    assert_eq!(gn, wn);

    restored.check_structural_invariant().expect("and well-formed");
}

/// Under concurrent writers, every entry is still individually valid.
///
/// This is the property the doc comment now promises without quiescence,
/// and it is worth having because it is the one a probe sampling a live
/// engine depends on: the dump may be an incomplete set, but it contains no
/// fabricated or malformed fact. Each entry is written once under one shard
/// lock and never mutated, so `child == H(min‖max)` holds for all of them
/// however the iteration interleaves with insertion.
#[test]
fn a_concurrent_dump_contains_only_well_formed_entries() {
    let e = Arc::new(DistinctionEngine::new());
    grow(&e, 64); // a seed so the writers have distinct pairs to combine

    let seed = e.snapshot_distinctions();
    let mut handles = Vec::new();
    for t in 0..8 {
        let e = Arc::clone(&e);
        let seed = seed.clone();
        handles.push(thread::spawn(move || {
            for i in 0..200 {
                let a = seed[(t * 7 + i) % seed.len()];
                let b = seed[(t * 13 + i * 3 + 1) % seed.len()];
                if a != b {
                    let _ = e.synthesize(a, b);
                }
            }
        }));
    }

    // Dump repeatedly while they run. Any one of these may be torn.
    let mut dumps = Vec::new();
    for _ in 0..40 {
        dumps.push(e.snapshot_parentage());
    }
    for h in handles {
        h.join().expect("writer");
    }
    dumps.push(e.snapshot_parentage());

    let mut total = 0usize;
    for dump in &dumps {
        for &(child, (p0, p1)) in dump {
            assert_ne!(p0, p1, "Axiom 3: a registered node never has equal parents");
            // Stored canonically, as `(min, max)`. Asserted rather than
            // assumed: with the pair already ordered, a `recompute` that
            // dropped the `min‖max` sort would still agree, so the check
            // below would pass without testing the ordering at all. It was
            // written that way first, and a mutant that removed the sort
            // did not fail.
            assert!(p0.as_bytes() < p1.as_bytes(), "parentage is stored in canonical order");
            assert_eq!(
                *child.as_bytes(),
                recompute(p0, p1),
                "every entry in a concurrent dump is a valid parentage fact"
            );
            // And the ordering is what the identity depends on, which the
            // line above cannot show while the input is already sorted.
            assert_eq!(recompute(p0, p1), recompute(p1, p0), "Axiom 2: commutativity");
            total += 1;
        }
    }
    assert!(total > 0, "the dumps were not empty");

    // And the engine itself is intact after all of it.
    e.check_structural_invariant().expect("concurrent synthesis plus concurrent reads");
}

/// The dump is a set of facts, not an instant — asserted, not assumed.
///
/// `snapshot_distinctions` taken during contention can disagree with any
/// `distinction_count()` observed around it, because the walk spans
/// insertions. The weaker true statement is monotonicity: the engine is
/// append-only, so a count taken before the walk cannot exceed one taken
/// after, and the dump's length lies between them.
#[test]
fn a_concurrent_snapshot_corresponds_to_no_single_instant() {
    let e = Arc::new(DistinctionEngine::new());
    grow(&e, 64);
    let seed = e.snapshot_distinctions();

    let writer = {
        let e = Arc::clone(&e);
        let seed = seed.clone();
        thread::spawn(move || {
            for i in 0..3_000 {
                let a = seed[i % seed.len()];
                let b = seed[(i * 5 + 1) % seed.len()];
                if a != b {
                    let _ = e.synthesize(a, b);
                }
            }
        })
    };

    for _ in 0..200 {
        let before = e.distinction_count();
        let dump = e.snapshot_distinctions();
        let after = e.distinction_count();
        assert!(before <= after, "the engine is append-only");
        assert!(
            dump.len() >= before && dump.len() <= after,
            "a torn walk still lies between the counts bracketing it: \
             {before} <= {} <= {after}",
            dump.len()
        );
    }

    writer.join().expect("writer");
}
