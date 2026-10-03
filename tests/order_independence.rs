//! Laws 8 and 9: one structure, many witnesses.
//!
//! THEORY names a single experiment as the one that would refute its central
//! claim: two engines fed the **same parent pairs in different orders**,
//! queried at quiescence with the same `{root, direction, boundary, signal}`,
//! must produce byte-identical `canonical_bytes`.
//!
//! That experiment did not exist. `cond_d_falsifier.rs` is the test that
//! looks like it — but it drives both engines from one closure over one
//! corpus in one order, and its own comments say "identical synthesis
//! histories". Identical input producing identical output is Axiom 1
//! (determinism). Order-independence is a strictly stronger claim, and it is
//! the one the architecture rests on: it is what licenses "every engine is a
//! partial witness of one grow-only structure".
//!
//! The permutation here is not a shuffle. `synthesize` rejects a parent the
//! engine has not witnessed, so the only legal reorderings are different
//! *topological* orders of the same DAG. This module builds one, verifies it
//! genuinely differs from the original, and only then compares bytes. The
//! verification matters as much as the comparison: a reordering test that
//! silently degenerates to the original order proves nothing, which is
//! exactly how the existing falsifier came to assert less than it claimed.

use koru_lambda_core::projection::{Adjacency, Degree, Direction, HopDistance, Signal};
use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::HashSet;

/// One synthesis request: the two parents, in the order they were supplied.
type Pair = (Distinction, Distinction);

/// What a materialized projection commits to: its spec-derived identity and
/// its canonical wire bytes. Named so the two are never compared crosswise.
struct Materialized {
    id: Vec<u8>,
    bytes: Vec<u8>,
}

/// Deterministic index pairs, so a failure is reproducible.
///
/// A small LCG rather than `rand`: this test must not acquire a dependency
/// whose stream could change under it, since the corpus *is* the experiment.
fn seeded_pairs(seed: u64, count: usize) -> Vec<(usize, usize)> {
    let mut state = seed;
    let mut next = || {
        state =
            state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    (0..count).map(|_| (next(), next())).collect()
}

/// Build a DAG and return its parentage as a list of concrete parent pairs,
/// in the order they were performed.
fn build_history(engine: &DistinctionEngine, pairs: &[(usize, usize)]) -> Vec<Pair> {
    let mut chain = vec![engine.d0(), engine.d1()];
    let mut history = Vec::with_capacity(pairs.len());
    for &(a, b) in pairs {
        let left = chain[a % chain.len()];
        let right = chain[b % chain.len()];
        history.push((left, right));
        let child = engine.synthesize(left, right);
        chain.push(child);
    }
    history
}

/// Produce a different topological order of the same parent pairs.
///
/// Greedy and deliberately adversarial about ordering: at each step it takes
/// the *last* eligible pair rather than the first, which maximises divergence
/// from the original sequence while respecting the only real constraint —
/// both parents must already be witnessed.
fn reorder(history: &[Pair], d0: Distinction, d1: Distinction) -> Vec<Pair> {
    let mut witnessed: HashSet<Distinction> = HashSet::new();
    witnessed.insert(d0);
    witnessed.insert(d1);

    let mut pending: Vec<Option<Pair>> = history.iter().copied().map(Some).collect();
    let mut out = Vec::with_capacity(history.len());

    while out.len() < history.len() {
        let mut progressed = false;
        // Scan back-to-front and take the first eligible pair found, which is
        // the last eligible pair in original order.
        for slot in pending.iter_mut().rev() {
            let Some((left, right)) = *slot else { continue };
            if witnessed.contains(&left) && witnessed.contains(&right) {
                // The child's identity is fixed by its parents, so we can
                // learn what becomes witnessed without an engine: the pair
                // determines the child. We record eligibility by replaying
                // through a scratch engine in the caller instead, so here we
                // only need to mark that this pair is consumed.
                *slot = None;
                out.push((left, right));
                progressed = true;
                break;
            }
        }
        if !progressed {
            // Remaining pairs depend on children not yet produced. Replay the
            // consumed prefix through a scratch engine to learn the new
            // witnessed set, then continue.
            let scratch = DistinctionEngine::new();
            for &(left, right) in &out {
                let child = scratch.synthesize(left, right);
                witnessed.insert(child);
            }
            witnessed.insert(scratch.d0());
            witnessed.insert(scratch.d1());
            // If a full replay taught us nothing new, the remaining pairs are
            // unreachable and the history was malformed.
            let still_blocked = pending
                .iter()
                .flatten()
                .all(|(l, r)| !witnessed.contains(l) || !witnessed.contains(r));
            assert!(
                !still_blocked,
                "reorder: {} pairs remain but none are eligible — history is not a valid DAG",
                pending.iter().flatten().count()
            );
        }
    }
    out
}

/// The falsifier THEORY names. If this fails, THEORY's central claim is wrong.
#[test]
fn same_pairs_in_different_orders_produce_identical_bytes() {
    let pairs = seeded_pairs(20_260_101, 600);

    let first = DistinctionEngine::new();
    let history = build_history(&first, &pairs);

    let permuted = reorder(&history, first.d0(), first.d1());

    // The experiment is vacuous unless the order genuinely differs. This is
    // the assertion the existing falsifier lacks.
    assert_eq!(permuted.len(), history.len(), "reorder dropped or duplicated pairs");
    let differing = history.iter().zip(&permuted).filter(|(a, b)| a != b).count();
    assert!(
        differing > history.len() / 10,
        "permutation is too close to the original order ({differing} of {} positions differ) — \
         the test would pass without exercising order-independence at all",
        history.len()
    );

    eprintln!("permutation differs from original at {differing} of {} positions", history.len());

    let second = DistinctionEngine::new();
    for &(left, right) in &permuted {
        // Every child the reordered run produces must already be witnessed by
        // the original. Checking incrementally localises a divergence to the
        // step that caused it, rather than reporting it as a set difference
        // hundreds of syntheses later.
        let child = second.synthesize(left, right);
        assert!(
            first.has(child),
            "reordered run produced a distinction the original never witnessed — \
             Axiom 4 or Law 9 broken at step {}",
            second.distinction_count()
        );
    }

    // Quiescence parity: the same structure, reached two ways.
    assert_eq!(
        first.distinction_count(),
        second.distinction_count(),
        "Law 8: distinction counts differ after reordering — the structure depends on arrival order"
    );
    assert_eq!(
        first.relationship_count(),
        second.relationship_count(),
        "Law 8: relationship counts differ after reordering"
    );

    let set_one: HashSet<Distinction> = first.snapshot_distinctions().into_iter().collect();
    let set_two: HashSet<Distinction> = second.snapshot_distinctions().into_iter().collect();
    assert_eq!(
        set_one, set_two,
        "Law 9: the two engines witnessed different distinction sets from the same pairs"
    );

    // Parentage must agree edge for edge, not merely in cardinality.
    let parents_one: HashSet<_> = first.snapshot_parentage().into_iter().collect();
    let parents_two: HashSet<_> = second.snapshot_parentage().into_iter().collect();
    assert_eq!(parents_one, parents_two, "Law 9: parentage differs after reordering");

    // The load-bearing assertion: identical bytes from the same read, across
    // every axis combination the crate ships.
    let root = {
        // A root with a non-degenerate cone in both directions: pick the
        // highest-degree non-primordial, which both engines agree on.
        let mut best = first.d0();
        let mut best_degree = 0;
        for d in first.snapshot_distinctions() {
            if d == first.d0() || d == first.d1() {
                continue;
            }
            let degree = first.degree(d);
            if degree > best_degree {
                best_degree = degree;
                best = d;
            }
        }
        assert!(best_degree > 0, "no non-primordial with positive degree — corpus is degenerate");
        best
    };

    // `Signal` is a trait and its three implementors are distinct types, so
    // the axis cannot be a runtime loop. A macro keeps the cross product
    // explicit rather than testing one signal and trusting the others.
    macro_rules! compare_axes {
        ($($signal:expr),+ $(,)?) => {
            for direction in [Direction::Upstream, Direction::Downstream, Direction::Undirected] {
                for saturated in [false, true] {
                    $(
                        let one = materialize(&first, root, direction, saturated, $signal);
                        let two = materialize(&second, root, direction, saturated, $signal);
                        let axes = format!(
                            "{direction:?}/{}/{}",
                            stringify!($signal),
                            if saturated { "saturated" } else { "hops(2)" }
                        );
                        assert_eq!(
                            one.id, two.id,
                            "projection identity differs for {axes} — \
                             the identifier is supposed to depend on the spec alone"
                        );
                        assert_eq!(
                            one.bytes, two.bytes,
                            "Laws 8 and 9 refuted for {axes}: the same read of the same \
                             structure gave different bytes depending on the order the \
                             pairs arrived in"
                        );
                    )+
                }
            }
        };
    }

    compare_axes!(Adjacency, Degree, HopDistance);
}

fn materialize<S: Signal>(
    engine: &DistinctionEngine,
    root: Distinction,
    direction: Direction,
    saturated: bool,
    signal: S,
) -> Materialized {
    let builder = engine.project(root).direction(direction);
    let bounded = if saturated { builder.saturated() } else { builder.hops(2) };
    let projection = bounded.signal(signal).materialize();
    Materialized {
        id: projection.projection_id().as_bytes().to_vec(),
        bytes: projection.canonical_bytes(),
    }
}
