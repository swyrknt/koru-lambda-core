//! An engine implemented outside the crate, projected over.
//!
//! SEAL-2. `ReadOnlyEngine` was sealed, so this file could not have
//! compiled: `private::Sealed` is unreachable from here. DEC-10 unsealed it
//! so alternative engines stay possible — a fixture, a lazy loader, a view
//! onto storage — and an unsealing nothing exercises is a claim rather than
//! a capability. This is the exercise.
//!
//! It is an integration test for that reason. It is a separate compilation
//! unit and sees only the published API, exactly as a sibling crate would,
//! so if the trait were re-sealed or a method added without a default body
//! this file stops compiling.

use koru_lambda_core::projection::{
    Adjacency, Degree, Direction, HopDistance, ReadOnlyEngine, Signal,
};
use koru_lambda_core::{Distinction, DistinctionEngine, ParentPair};
use std::collections::HashMap;

/// A parentage table and nothing else.
///
/// No `DashMap`, no atomics, no synthesis: it answers from a `HashMap`
/// built once. That is the point — it shares no implementation with
/// `DistinctionEngine`, so a projection materialized over it is reading
/// through the trait rather than through the substrate's engine.
struct TableEngine {
    d0: Distinction,
    d1: Distinction,
    parents: HashMap<Distinction, ParentPair>,
    degrees: HashMap<Distinction, usize>,
}

impl TableEngine {
    /// Copy a real engine's parentage into a plain table.
    ///
    /// Built from a witnessed engine so the distinctions are genuine. An
    /// external engine inventing identifiers is a different scenario — and
    /// one the substrate cannot save it from, which is what the trait's
    /// "a consumer engine owns its own correctness" paragraph says.
    fn mirroring(engine: &DistinctionEngine) -> Self {
        let parents: HashMap<Distinction, ParentPair> =
            engine.snapshot_parentage().into_iter().collect();
        let mut degrees: HashMap<Distinction, usize> = HashMap::new();
        for d in engine.snapshot_distinctions() {
            degrees.insert(d, engine.degree(d));
        }
        Self { d0: engine.d0(), d1: engine.d1(), parents, degrees }
    }
}

impl ReadOnlyEngine for TableEngine {
    fn d0(&self) -> Distinction {
        self.d0
    }
    fn d1(&self) -> Distinction {
        self.d1
    }
    fn distinction_count(&self) -> usize {
        self.degrees.len()
    }
    fn relationship_count(&self) -> usize {
        // Each entry contributes its two parent edges.
        self.parents.len() * 2
    }
    fn parents_of(&self, d: Distinction) -> Option<ParentPair> {
        self.parents.get(&d).copied()
    }
    fn degree(&self, d: Distinction) -> usize {
        self.degrees.get(&d).copied().unwrap_or(0)
    }
    fn has(&self, d: Distinction) -> bool {
        self.degrees.contains_key(&d)
    }
    fn snapshot_parentage(&self) -> Vec<(Distinction, ParentPair)> {
        self.parents.iter().map(|(c, p)| (*c, *p)).collect()
    }
}

/// Build a small graph and mirror it into a foreign engine.
fn pair() -> (DistinctionEngine, TableEngine, Distinction) {
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let f = e.synthesize(d, e.d1());
    let _ = e.synthesize(f, c);
    let mirror = TableEngine::mirroring(&e);
    (e, mirror, d)
}

/// The unsealing is real: a foreign engine materializes a projection.
#[test]
fn a_foreign_engine_materializes_a_projection() {
    let (_, mirror, root) = pair();

    let proj =
        mirror.project(root).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();

    assert!(proj.contains(&root), "the root is in its own upstream cone");
    assert!(!proj.canonical_bytes().is_empty());
}

/// A projection over the foreign engine is byte-identical to one over the
/// engine it mirrors.
///
/// This is Cond D across implementations rather than across instances: the
/// wire bytes depend on the parentage and the signal, not on who stored
/// them. It is also the strongest statement available about an external
/// engine — the substrate cannot promise this for an engine it has never
/// seen, only demonstrate it for one that answers faithfully.
#[test]
fn a_faithful_foreign_engine_produces_identical_bytes() {
    let (real, mirror, root) = pair();

    for direction in [Direction::Upstream, Direction::Downstream, Direction::Undirected] {
        let native =
            real.project(root).direction(direction).hops(2).signal(Adjacency).materialize();
        let foreign =
            mirror.project(root).direction(direction).hops(2).signal(Adjacency).materialize();

        assert_eq!(
            native.canonical_bytes(),
            foreign.canonical_bytes(),
            "{direction:?}: wire bytes must not depend on which engine answered"
        );
        assert_eq!(
            native.projection_id(),
            foreign.projection_id(),
            "{direction:?}: projection identity is derived from the spec, not the engine"
        );
    }
}

/// Every shipped signal works over a foreign engine, including the one
/// dispatched through the sealed `CoreSignal` path.
///
/// `HopDistance` is the case worth naming: it overrides
/// `__materialize_special` on a trait a consumer cannot implement, so it
/// exercises substrate-internal dispatch reading through a foreign engine.
#[test]
fn every_shipped_signal_reads_through_the_trait() {
    let (real, mirror, root) = pair();

    fn bytes_over<S: Signal>(e: &DistinctionEngine, root: Distinction, s: S) -> Vec<u8> {
        e.project(root)
            .direction(Direction::Upstream)
            .hops(3)
            .signal(s)
            .materialize()
            .canonical_bytes()
    }
    fn bytes_foreign<S: Signal>(m: &TableEngine, root: Distinction, s: S) -> Vec<u8> {
        m.project(root)
            .direction(Direction::Upstream)
            .hops(3)
            .signal(s)
            .materialize()
            .canonical_bytes()
    }

    assert_eq!(bytes_over(&real, root, Adjacency), bytes_foreign(&mirror, root, Adjacency));
    assert_eq!(bytes_over(&real, root, Degree), bytes_foreign(&mirror, root, Degree));
    assert_eq!(bytes_over(&real, root, HopDistance), bytes_foreign(&mirror, root, HopDistance));
}

/// `restore_projection` is verification, and a foreign engine cannot be
/// verified against: restoring needs a witness, which a table is not.
///
/// This records where the boundary actually falls. Projecting *over* a
/// foreign engine is supported; trusting bytes *into* one is not, because
/// verification is `DistinctionEngine::verify` and that is the engine's own
/// witness. The unsealing widens the read side only.
#[test]
fn restoring_still_requires_a_witnessing_engine() {
    let (real, mirror, root) = pair();
    let wire = mirror
        .project(root)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .canonical_bytes();

    // The real engine witnessed these distinctions, so it restores them —
    // and restoring is re-materialisation, so the bytes come back identical.
    let restored =
        real.restore_projection(&wire, Adjacency).expect("the witnessing engine restores its own");
    assert_eq!(restored.canonical_bytes(), wire, "restoring re-derives rather than decodes");

    // A fresh engine has not, so the same bytes are foreign to it.
    let stranger = DistinctionEngine::new();
    assert!(
        stranger.restore_projection(&wire, Adjacency).is_err(),
        "an engine that never witnessed these distinctions must refuse them"
    );
}

/// `HopDistance::compute` answers instead of panicking.
///
/// PRJ-2. It was `unreachable!()`, documented as unreachable by
/// construction — and a consumer signal is *handed* a `SignalContext`, so
/// delegating to a shipped signal from inside its own `compute` reached it.
/// This test is that delegation, which is why it lives outside the crate.
#[test]
fn a_consumer_signal_can_delegate_to_hop_distance() {
    use koru_lambda_core::projection::SignalContext;

    /// Returns `HopDistance`'s answer, doubled — so the test fails if the
    /// delegation silently returns a sentinel instead of a real hop count.
    #[derive(Clone)]
    struct DoubledHops;

    impl Signal for DoubledHops {
        type Output = usize;
        fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> usize {
            HopDistance.compute(ctx, node) * 2
        }
        fn canonical_bytes(&self) -> &[u8] {
            b"test:doubled_hops"
        }
    }

    let (real, _, root) = pair();

    let hops =
        real.project(root).direction(Direction::Upstream).hops(3).signal(HopDistance).materialize();
    let doubled =
        real.project(root).direction(Direction::Upstream).hops(3).signal(DoubledHops).materialize();

    // Same cone, and every delegated value is twice the dispatched one.
    let mut checked = 0usize;
    for (node, h) in hops.output().iter() {
        let d = doubled.output().iter().find_map(|(n, v)| (n == node).then_some(*v));
        assert_eq!(d, Some(h * 2), "delegating to HopDistance disagreed at {node:?}");
        checked += 1;
    }
    assert!(checked >= 2, "the cone should have several nodes to compare");

    // A node outside the cone gets no hop count rather than a wrong one —
    // asked through a consumer signal, because a `SignalContext` cannot be
    // built out here. That inability is the whole point: the only way to
    // hold one is to be inside a `compute`, which is the path that reached
    // the old panic.
    #[derive(Clone)]
    struct HopsOfA(Distinction);

    impl Signal for HopsOfA {
        type Output = usize;
        fn compute(&self, ctx: &SignalContext<'_>, _node: Distinction) -> usize {
            HopDistance.compute(ctx, self.0)
        }
        fn canonical_bytes(&self) -> &[u8] {
            b"test:hops_of_a"
        }
    }

    // Downstream of the root, so genuinely not in its upstream cone.
    //
    // The first attempt used `synthesize(d0, d1)` on a *fresh* engine,
    // expecting a stranger. Content addressing had other ideas: that is
    // the same identifier the real engine already held, so it was in the
    // cone at one hop. Law 8 is not avoidable by using a different engine,
    // which is the point of Law 8.
    let outside = real.synthesize(root, real.d1());
    let asked = real
        .project(root)
        .direction(Direction::Upstream)
        .hops(3)
        .signal(HopsOfA(outside))
        .materialize();

    for (_, v) in asked.output().iter() {
        assert_eq!(*v, usize::MAX, "a node outside the cone has no hop count");
    }
}

// -------------------------------------------------------------------------
// A foreign root reaches `project`, and used to produce bytes nobody accepts
// -------------------------------------------------------------------------

/// Found by the Python/Rust differential harness.
///
/// `synthesize` has always guarded a foreign parent with
/// `assert_registered`, and `restore_projection` rejects a foreign root
/// through `verify`. `project` guarded nothing. A `Distinction` is `Copy`
/// and carries no engine tag, and Law 8 means engines agree without
/// communicating — so holding a value another engine produced is the
/// designed case, not misuse, and passing one to `project` was an easy
/// mistake to make.
///
/// What it produced was worse than a wrong answer: an empty cone, which
/// serialized to a well-formed 55-byte §7 frame that **no engine would
/// accept, including the one that produced it**, because restore
/// re-materializes and the root fails `verify`. §7 defines canonical bytes
/// as what this engine would produce, so an engine that cannot reproduce
/// its own output has broken the definition rather than merely answered
/// oddly.
#[test]
#[should_panic(expected = "not registered in this engine")]
fn projecting_a_foreign_root_panics() {
    let holder = DistinctionEngine::new();
    let a = holder.synthesize(holder.d0(), holder.d1());
    let b = holder.synthesize(a, holder.d0());
    // `b` is two hops from the primordials, so no fresh engine holds it.
    let stranger = DistinctionEngine::new();
    assert_eq!(stranger.distinction_count(), 2, "the stranger holds only d₀ and d₁");

    let _ = stranger.project(b);
}

/// `project_arc` too — a guard on one entry point only moves the hole.
#[test]
#[should_panic(expected = "not registered in this engine")]
fn projecting_a_foreign_root_through_arc_panics() {
    let holder = DistinctionEngine::new();
    let a = holder.synthesize(holder.d0(), holder.d1());
    let b = holder.synthesize(a, holder.d0());

    let stranger = std::sync::Arc::new(DistinctionEngine::new());
    let _ = DistinctionEngine::project_arc(&stranger, b);
}

/// And the legitimate cross-engine case still works, which is the point.
///
/// This is the half that must not regress. Law 8 says two engines
/// performing the same operations agree without communicating, so a
/// distinction obtained from one engine and projected in another is correct
/// *when that engine has also witnessed it*. The guard tests witness state,
/// not provenance — which is the only thing it could test, since content
/// addressing makes the two engines' values byte-identical.
#[test]
fn a_root_witnessed_by_both_engines_still_projects() {
    let one = DistinctionEngine::new();
    let c_one = one.synthesize(one.d0(), one.d1());

    let two = DistinctionEngine::new();
    let c_two = two.synthesize(two.d0(), two.d1());
    assert_eq!(c_one, c_two, "Law 8: the same operation yields the same identity");

    // A value from `one`, projected in `two`.
    let p =
        two.project(c_one).direction(Direction::Upstream).hops(1).signal(Adjacency).materialize();

    assert!(p.output().iter().count() > 0, "the cone is not empty");
    assert!(
        two.restore_projection(&p.canonical_bytes(), Adjacency).is_ok(),
        "and the frame it produced is one its own engine accepts"
    );
}
