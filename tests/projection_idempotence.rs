//! Projecting twice gives the same bytes — for an arbitrary signal, and
//! shown to be a *different* property from read-only.
//!
//! PRB-3, continuing PRJ-8. PRJ-8 shipped the consumer self-test for
//! read-only: observe, project, observe, compare. That recipe is correct and
//! it does not check idempotence, which this module demonstrates rather than
//! argues — `Drifting` below touches the engine not at all, so the read-only
//! recipe passes it, and its output changes on every call, so two
//! materializations of one specification disagree byte for byte.
//!
//! Read-only and idempotent are independent obligations. A signal that keeps
//! its state in its own fields satisfies the first and breaks the second,
//! and nothing in `Signal`'s signature can prevent it: `compute` takes
//! `&self`, and interior mutability is a safe-Rust pattern.
//!
//! The second thing it shows is the §7 consequence. A drifting signal's
//! `projection_id` is **stable** across both materializations, because the
//! id hashes the signal's tag and never the output. Comparing ids would
//! report agreement here. `docs/SPEC.md` §7 says why that is not the
//! comparison to make.

use koru_lambda_core::projection::{
    Adjacency, Boundary, Degree, Direction, HopDistance, Signal, SignalContext,
};
use koru_lambda_core::{Distinction, DistinctionEngine, ParentPair};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Everything about an engine a caller can observe, for the read-only
/// recipe. Parentage sorted, because `snapshot_parentage` promises no order.
#[derive(PartialEq, Eq, Debug)]
struct Observed {
    parentage: Vec<(Distinction, ParentPair)>,
    distinctions: usize,
    relationships: usize,
}

fn observe(e: &DistinctionEngine) -> Observed {
    let mut parentage = e.snapshot_parentage();
    parentage.sort_by_key(|(child, _)| *child.as_bytes());
    Observed {
        parentage,
        distinctions: e.distinction_count(),
        relationships: e.relationship_count(),
    }
}

/// A graph with enough shape that every axis selects a different cone.
fn graph() -> (Arc<DistinctionEngine>, Distinction) {
    let e = Arc::new(DistinctionEngine::new());
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let f = e.synthesize(d, e.d1());
    let _ = e.synthesize(f, c);
    (e, d)
}

/// Read-only, and not idempotent.
///
/// The counter lives in the signal, not in the engine. `compute` never
/// writes through a handle of its own — it has none — so the engine is
/// untouched and the read-only recipe passes. The output is a different
/// number every call.
///
/// `Arc<AtomicUsize>` rather than `Cell`: `Signal` requires
/// `Clone + Send + Sync + 'static`.
#[derive(Clone)]
struct Drifting {
    calls: Arc<AtomicUsize>,
}

impl Signal for Drifting {
    type Output = usize;

    fn compute(&self, _ctx: &SignalContext<'_>, _node: Distinction) -> usize {
        self.calls.fetch_add(1, Ordering::Relaxed)
    }

    fn canonical_bytes(&self) -> &[u8] {
        b"test:drifting"
    }
}

/// The read-only recipe PRJ-8 shipped, verbatim in shape.
fn is_read_only<S: Signal>(engine: &Arc<DistinctionEngine>, root: Distinction, s: S) -> bool {
    let before = observe(engine);
    let p = DistinctionEngine::project_arc(engine, root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(s)
        .materialize();
    let _ = p.canonical_bytes();
    observe(engine) == before
}

/// The recipe this module adds: materialize the same specification twice and
/// compare the bytes. Four lines of public API, shipped the same way — as a
/// recipe a consumer copies, not an exported helper, because a helper would
/// have to be generic over the builder's typestate to say anything a
/// consumer cannot say more plainly themselves.
fn is_idempotent<S: Signal>(engine: &Arc<DistinctionEngine>, root: Distinction, s: S) -> bool {
    let once = DistinctionEngine::project_arc(engine, root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(s.clone())
        .materialize()
        .canonical_bytes();
    let twice = DistinctionEngine::project_arc(engine, root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(s)
        .materialize()
        .canonical_bytes();
    once == twice
}

/// The gap, measured: read-only does not imply idempotent.
///
/// If this test ever fails on its first assertion, the read-only recipe has
/// become strong enough to catch a drifting signal and this module's premise
/// is wrong — which would be worth knowing, so it is asserted rather than
/// assumed.
#[test]
fn read_only_does_not_imply_idempotent() {
    let (e, root) = graph();
    let drifting = Drifting { calls: Arc::new(AtomicUsize::new(0)) };

    assert!(
        is_read_only(&e, root, drifting.clone()),
        "Drifting touches no engine state, so the read-only recipe must pass it"
    );
    assert!(
        !is_idempotent(&e, root, drifting),
        "and its output moves every call, so the idempotence recipe must catch it"
    );
}

/// The idempotence recipe passes the three shipped signals and catches the
/// drifting one. This is the consumer self-test the item asks for.
#[test]
fn the_idempotence_recipe_catches_a_drifting_signal() {
    let (e, root) = graph();

    assert!(is_idempotent(&e, root, Adjacency), "a shipped signal passes");
    assert!(is_idempotent(&e, root, Degree), "a shipped signal passes");
    assert!(is_idempotent(&e, root, HopDistance), "a shipped signal passes");
    assert!(
        !is_idempotent(&e, root, Drifting { calls: Arc::new(AtomicUsize::new(0)) }),
        "and a signal with state of its own does not"
    );
}

/// Idempotence over the whole axis set, not one combination.
///
/// PRJ-8 asserted this for `Degree` at Undirected/Saturated. A signal is
/// evaluated per node of the cone, and which nodes those are is what the
/// direction and boundary axes decide, so an idempotence claim made at one
/// combination is a claim about one cone shape.
#[test]
fn every_axis_combination_is_idempotent_for_the_shipped_signals() {
    let (e, root) = graph();
    let directions = [Direction::Upstream, Direction::Downstream, Direction::Undirected];
    let boundaries = [Boundary::Hops(1), Boundary::Hops(2), Boundary::Saturated];
    let mut checked = 0;

    for direction in directions {
        for boundary in boundaries {
            for (name, (a, b)) in [
                ("Adjacency", twice(&e, root, direction, boundary, Adjacency)),
                ("Degree", twice(&e, root, direction, boundary, Degree)),
                ("HopDistance", twice(&e, root, direction, boundary, HopDistance)),
            ] {
                assert_eq!(a, b, "{name} at {direction:?}/{boundary:?} is not idempotent");
                checked += 1;
            }
        }
    }

    assert_eq!(checked, 27, "3 signals x 3 directions x 3 boundaries");
}

/// A drifting signal's bytes move while its id stays put.
///
/// This is `docs/SPEC.md` §7's sentence as an executable assertion: §8
/// hashes the specification, so an id match says the two reads asked the
/// same question and says nothing about the answer. A consumer comparing
/// ids would call these two materializations equal.
#[test]
fn a_drifting_signal_keeps_a_stable_projection_id() {
    let (e, root) = graph();
    let s = Drifting { calls: Arc::new(AtomicUsize::new(0)) };

    let first = DistinctionEngine::project_arc(&e, root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(s.clone())
        .materialize();
    let first_bytes = first.canonical_bytes();
    let first_id = *first.projection_id().as_bytes();

    let second = DistinctionEngine::project_arc(&e, root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(s)
        .materialize();
    let second_bytes = second.canonical_bytes();
    let second_id = *second.projection_id().as_bytes();

    assert_ne!(first_bytes, second_bytes, "the output moved");
    assert_eq!(first_id, second_id, "and the id did not, because it hashes the axes");
}

/// The canonical bytes of two materializations of one specification, in the
/// order they were taken. Named because the pair is the unit of comparison
/// everywhere below, and because an unnamed one trips `clippy::type_complexity`.
type TwoReads = (Vec<u8>, Vec<u8>);

/// Materialize one specification twice and return both byte strings.
fn twice<S: Signal>(
    engine: &Arc<DistinctionEngine>,
    root: Distinction,
    direction: Direction,
    boundary: Boundary,
    signal: S,
) -> TwoReads {
    let once = DistinctionEngine::project_arc(engine, root)
        .direction(direction)
        .boundary(boundary)
        .signal(signal.clone())
        .materialize()
        .canonical_bytes();
    let twice = DistinctionEngine::project_arc(engine, root)
        .direction(direction)
        .boundary(boundary)
        .signal(signal)
        .materialize()
        .canonical_bytes();
    (once, twice)
}
