//! Projecting changes nothing — measured for the shipped signals, and shown
//! to be a *contract* rather than a guarantee for a consumer's.
//!
//! PRJ-6 and PRJ-8. Cond E was stated as closed by `ReadOnlyEngine`'s method
//! inventory, which is true of the engine handle a signal is given: there is
//! no `synthesize` on it. It is not true of the signal.
//!
//! A consumer `Signal` is an arbitrary type. Nothing stops it holding an
//! `Arc<DistinctionEngine>` of its own and synthesizing through that inside
//! `compute`. `Mutating` below does exactly that, and the projection
//! succeeds — so read-only is enforced for the three shipped signals and is
//! a documented obligation for anyone else's. That distinction was in no
//! document, and the type system cannot make it anything else.
//!
//! The recipe at the bottom is what a consumer runs against their own
//! signal. It is five lines of public API rather than an exported helper,
//! because a helper would have to be generic over the builder typestate to
//! say anything a consumer could not say more plainly themselves.

use koru_lambda_core::projection::{
    Adjacency, Degree, Direction, HopDistance, Signal, SignalContext,
};
use koru_lambda_core::{Distinction, DistinctionEngine, ParentPair};
use std::sync::Arc;

/// Everything about an engine a caller can observe, for before/after
/// comparison. Parentage sorted, because `snapshot_parentage` does not
/// promise an order.
#[derive(PartialEq, Eq, Debug)]
struct Observed {
    distinctions: usize,
    relationships: usize,
    parentage: Vec<(Distinction, ParentPair)>,
    degrees: Vec<(Distinction, usize)>,
}

fn observe(e: &DistinctionEngine) -> Observed {
    let mut parentage = e.snapshot_parentage();
    parentage.sort_by_key(|(c, _)| *c.as_bytes());
    let mut nodes = e.snapshot_distinctions();
    nodes.sort_by_key(|d| *d.as_bytes());
    let degrees = nodes.into_iter().map(|d| (d, e.degree(d))).collect();
    Observed {
        distinctions: e.distinction_count(),
        relationships: e.relationship_count(),
        parentage,
        degrees,
    }
}

fn graph() -> (DistinctionEngine, Distinction) {
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let f = e.synthesize(c, e.d1());
    let _ = e.synthesize(d, f);
    (e, c)
}

/// Every shipped signal, every direction, every boundary: nothing moves.
///
/// Not "the counts are equal" — the whole parentage map and every node's
/// degree, which is what "byte-identical state" has to mean. Counts alone
/// pass for two engines holding different graphs of the same size, which is
/// a mistake this suite has already made once.
#[test]
fn no_core_projection_changes_the_engine() {
    let (e, root) = graph();
    let before = observe(&e);

    for direction in [Direction::Upstream, Direction::Downstream, Direction::Undirected] {
        for saturated in [false, true] {
            macro_rules! project {
                ($sig:expr) => {{
                    let b = e.project(root).direction(direction);
                    let built = if saturated { b.saturated() } else { b.hops(2) };
                    let p = built.signal($sig).materialize();
                    // Touch everything a projection can answer, in case a
                    // read is lazy and defers work into the accessor.
                    let _ = p.canonical_bytes();
                    let _ = p.projection_id();
                    let _ = p.output().iter().count();
                    let _ = p.contains(&root);
                    let _ = p.spec().root;
                }};
            }
            project!(Adjacency);
            project!(Degree);
            project!(HopDistance);

            assert_eq!(
                observe(&e),
                before,
                "{direction:?}/saturated={saturated} changed the engine"
            );
        }
    }
}

/// Projecting twice gives identical bytes and leaves the count alone.
#[test]
fn projecting_twice_is_identical_and_adds_nothing() {
    let (e, root) = graph();

    let once = e
        .project(root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(Degree)
        .materialize()
        .canonical_bytes();
    let count = e.distinction_count();

    let twice = e
        .project(root)
        .direction(Direction::Undirected)
        .saturated()
        .signal(Degree)
        .materialize()
        .canonical_bytes();

    assert_eq!(once, twice, "a projection is a function of the graph, not of when it ran");
    assert_eq!(e.distinction_count(), count, "and reading witnesses nothing");
}

/// Restoring is a read too, even though it re-materialises.
#[test]
fn restoring_changes_nothing_either() {
    let (e, root) = graph();
    let wire = e
        .project(root)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .canonical_bytes();

    let before = observe(&e);
    let restored = e.restore_projection(&wire, Adjacency).expect("its own bytes restore");
    assert_eq!(restored.canonical_bytes(), wire);
    assert_eq!(observe(&e), before, "verification re-derives without recording");
}

// -------------------------------------------------------------------------
// PRJ-6 — and why it is a contract for a consumer's signal
// -------------------------------------------------------------------------

/// A consumer signal that mutates, holding an engine handle of its own.
///
/// The engine a signal is *given* is `&dyn ReadOnlyEngine`, which has no
/// mutating method — that is what closes Cond E for the shipped signals. It
/// says nothing about what a consumer's type carries in its own fields.
#[derive(Clone)]
struct Mutating {
    engine: Arc<DistinctionEngine>,
}

impl Signal for Mutating {
    type Output = usize;

    fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> usize {
        // Reads through the context, as any signal would.
        let d = ctx.engine.degree(node);
        // And then writes through its own handle, which nothing prevents.
        let _ = self.engine.synthesize(node, self.engine.d1());
        d
    }

    fn canonical_bytes(&self) -> &[u8] {
        b"test:mutating"
    }
}

/// The hole, demonstrated: a consumer signal can grow the engine mid-read.
///
/// This is not a defect to fix — it is the reason PRJ-6 asks for the
/// guarantee to be *stated* in two halves rather than claimed as one. A
/// signal is an arbitrary consumer type, and no signature can stop it
/// holding a handle. What the substrate can do is close the handle it hands
/// over, document the obligation, and give consumers a way to check
/// themselves. The recipe below is that way.
#[test]
fn a_consumer_signal_can_break_the_read_only_contract() {
    let e = Arc::new(DistinctionEngine::new());
    let c = e.synthesize(e.d0(), e.d1());
    let _ = e.synthesize(c, e.d0());

    let before = observe(&e);
    let signal = Mutating { engine: Arc::clone(&e) };

    let proj = DistinctionEngine::project_arc(&e, c)
        .direction(Direction::Upstream)
        .saturated()
        .signal(signal)
        .materialize();
    let _ = proj.canonical_bytes();

    let after = observe(&e);
    assert_ne!(
        after, before,
        "if this ever passes, a signature started enforcing what only a contract can"
    );
    assert!(
        after.distinctions > before.distinctions,
        "the signal synthesized through its own handle during the read"
    );

    // And the engine is still well-formed — it was a legal write, just one
    // the contract forbids. That is what makes it a contract and not a bug.
    e.check_structural_invariant().expect("a contract breach is not corruption (invariant)");
}

/// The self-test PRJ-8 asks for, written as a consumer would run it.
///
/// Five lines of public API: observe, project, observe, compare. Shipped as
/// a recipe rather than an exported helper, because a helper would have to
/// be generic over the builder's typestate to say anything this does not.
#[test]
fn the_self_test_recipe_catches_a_mutating_signal() {
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

    let e = Arc::new(DistinctionEngine::new());
    let c = e.synthesize(e.d0(), e.d1());
    let _ = e.synthesize(c, e.d0());

    assert!(is_read_only(&e, c, Adjacency), "a shipped signal passes");
    assert!(is_read_only(&e, c, Degree), "a shipped signal passes");
    assert!(
        !is_read_only(&e, c, Mutating { engine: Arc::clone(&e) }),
        "and the recipe catches one that does not"
    );
}
