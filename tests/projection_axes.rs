//! Cone membership against hand-computed sets, and the projection
//! identifier's sensitivity to each axis it commits to.
//!
//! PRJ-4 and PRJ-5. Both were covered in a way that read as coverage and
//! was not.
//!
//! The downstream cone had `downstream_cone_walks_children`, three
//! `contains` assertions and no cardinality check — it would pass an
//! implementation that returned every node in the engine. The undirected
//! cone had no set or size assertion anywhere. So "Downstream, Undirected
//! and `Saturated` traversals are tested" was true of the happy direction
//! and silent about over-inclusion, which is the failure a traversal bug
//! actually produces.
//!
//! The eighteen `projection_id`s pinned in `conformance/vectors.json` would
//! catch an axis being dropped from the hash, because the pinned value would
//! stop matching. They do not state the property: nothing asserted that
//! varying one axis *changes* the identifier, and all eighteen share a
//! single root, so the root axis was pinned and never varied. A fixture
//! records values; a property has to be written down.

use koru_lambda_core::projection::{
    Adjacency, Boundary, Degree, Direction, HopDistance, Projection, Signal,
};
use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::BTreeSet;

/// A graph whose shape is small enough to enumerate by hand.
///
/// ```text
///   c = synth(d0, d1)
///   f = synth(c,  d0)       // c's child, d0's child
///   g = synth(c,  d1)       // c's child, d1's child
///   h = synth(f,  g)        // f's and g's child
/// ```
///
/// Seven nodes: `d0`, `d1`, `c`, `f`, `g`, `h` — six, plus nothing else.
/// Every assertion below is read off this listing rather than from the
/// implementation.
struct Fixture {
    engine: DistinctionEngine,
    d0: Distinction,
    d1: Distinction,
    c: Distinction,
    f: Distinction,
    g: Distinction,
    h: Distinction,
}

fn fixture() -> Fixture {
    let engine = DistinctionEngine::new();
    let (d0, d1) = (engine.d0(), engine.d1());
    let c = engine.synthesize(d0, d1);
    let f = engine.synthesize(c, d0);
    let g = engine.synthesize(c, d1);
    let h = engine.synthesize(f, g);
    Fixture { engine, d0, d1, c, f, g, h }
}

fn cone<S: Signal>(p: &Projection<'_, S>) -> BTreeSet<Distinction> {
    p.output().iter().map(|(d, _)| d).collect()
}

fn set(items: &[Distinction]) -> BTreeSet<Distinction> {
    items.iter().copied().collect()
}

/// Downstream from `c`, saturated: `c` and everything descending from it.
///
/// `d0` and `d1` are `c`'s *parents*, so a correct downstream cone excludes
/// them. The previous test anchored at `d0` and asserted only membership, so
/// it could not tell a downstream walk from "return everything".
#[test]
fn downstream_saturated_is_exactly_the_descendants() {
    let x = fixture();
    let proj = x
        .engine
        .project(x.c)
        .direction(Direction::Downstream)
        .saturated()
        .signal(Adjacency)
        .materialize();

    assert_eq!(
        cone(&proj),
        set(&[x.c, x.f, x.g, x.h]),
        "downstream from c is c, f, g, h — and must not reach c's parents"
    );
}

/// Downstream one hop from `c`: `c` and its immediate children only.
#[test]
fn downstream_one_hop_stops_at_the_children() {
    let x = fixture();
    let proj = x
        .engine
        .project(x.c)
        .direction(Direction::Downstream)
        .hops(1)
        .signal(Adjacency)
        .materialize();

    assert_eq!(cone(&proj), set(&[x.c, x.f, x.g]), "one hop reaches f and g; h is two hops away");
}

/// Undirected from `c`, saturated: the whole connected graph.
#[test]
fn undirected_saturated_reaches_parents_and_children() {
    let x = fixture();
    let proj = x
        .engine
        .project(x.c)
        .direction(Direction::Undirected)
        .saturated()
        .signal(Adjacency)
        .materialize();

    assert_eq!(
        cone(&proj),
        set(&[x.d0, x.d1, x.c, x.f, x.g, x.h]),
        "undirected from c reaches every node in this graph"
    );
}

/// Undirected one hop from `h`: `h`, its parents, and nothing further.
///
/// The discriminating case. `h`'s parents are `f` and `g`; `f` and `g` share
/// the parent `c`, which is two hops away and must be excluded. An
/// implementation that forgot to bound the undirected walk would include it.
#[test]
fn undirected_one_hop_excludes_the_grandparent() {
    let x = fixture();
    let proj = x
        .engine
        .project(x.h)
        .direction(Direction::Undirected)
        .hops(1)
        .signal(Adjacency)
        .materialize();

    assert_eq!(
        cone(&proj),
        set(&[x.h, x.f, x.g]),
        "one undirected hop from h is f and g; c is two hops and must be excluded"
    );
}

/// Upstream saturated from `h`: every ancestor, which here is everything.
#[test]
fn upstream_saturated_is_exactly_the_ancestors() {
    let x = fixture();
    let proj = x
        .engine
        .project(x.h)
        .direction(Direction::Upstream)
        .saturated()
        .signal(Adjacency)
        .materialize();

    assert_eq!(
        cone(&proj),
        set(&[x.h, x.f, x.g, x.c, x.d0, x.d1]),
        "h's ancestry closes over the whole graph"
    );
}

/// Upstream from a primordial is just itself: it has no parents.
#[test]
fn a_primordial_has_no_ancestry() {
    let x = fixture();
    let proj = x
        .engine
        .project(x.d0)
        .direction(Direction::Upstream)
        .saturated()
        .signal(Adjacency)
        .materialize();

    assert_eq!(cone(&proj), set(&[x.d0]), "d0 is its own entire upstream cone");
}

// -------------------------------------------------------------------------
// PRJ-5 — each axis changes the identifier
// -------------------------------------------------------------------------

/// Every axis the specification says `projection_id` commits to actually
/// changes it, and no two of the shipped combinations collide.
///
/// `docs/SPEC.md` §8 derives the identifier from `{root, boundary,
/// direction, signal}`. The fixture pins eighteen values, which would catch
/// one of those being dropped from the hash — the pinned value would stop
/// matching. This asserts the property instead: vary one axis, hold the rest,
/// and the identifier must move. The root axis is the one the fixture could
/// never have covered, because all eighteen of its cases share a root.
#[test]
fn every_axis_changes_the_projection_id() {
    let x = fixture();
    let base = x
        .engine
        .project(x.c)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .projection_id();

    // Root.
    let other_root = x
        .engine
        .project(x.h)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .projection_id();
    assert_ne!(base, other_root, "a different root must give a different identifier");

    // Direction.
    for direction in [Direction::Downstream, Direction::Undirected] {
        let id = x
            .engine
            .project(x.c)
            .direction(direction)
            .hops(2)
            .signal(Adjacency)
            .materialize()
            .projection_id();
        assert_ne!(base, id, "{direction:?} must not share an identifier with Upstream");
    }

    // Boundary — a different hop count, and a different boundary kind.
    let other_hops = x
        .engine
        .project(x.c)
        .direction(Direction::Upstream)
        .hops(3)
        .signal(Adjacency)
        .materialize()
        .projection_id();
    assert_ne!(base, other_hops, "Hops(3) must differ from Hops(2)");

    let saturated = x
        .engine
        .project(x.c)
        .direction(Direction::Upstream)
        .saturated()
        .signal(Adjacency)
        .materialize()
        .projection_id();
    assert_ne!(base, saturated, "Saturated must differ from any Hops(n)");

    // Signal.
    let degree = x
        .engine
        .project(x.c)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Degree)
        .materialize()
        .projection_id();
    let hop_distance = x
        .engine
        .project(x.c)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(HopDistance)
        .materialize()
        .projection_id();
    assert_ne!(base, degree, "Degree must not share an identifier with Adjacency");
    assert_ne!(base, hop_distance, "HopDistance must not share an identifier with Adjacency");
    assert_ne!(degree, hop_distance, "Degree and HopDistance must differ from each other");
}

/// The whole cross product is collision-free, over two roots.
///
/// 3 signals × 3 directions × 3 boundaries × 2 roots = 54 specifications,
/// and 54 distinct identifiers. The fixture pins eighteen of these shapes at
/// one root; this says nothing in the space collides.
#[test]
fn the_specification_space_is_collision_free() {
    let x = fixture();
    let mut ids: BTreeSet<[u8; 32]> = BTreeSet::new();
    let mut n = 0usize;

    for root in [x.c, x.h] {
        for direction in [Direction::Upstream, Direction::Downstream, Direction::Undirected] {
            for boundary in [Boundary::Hops(1), Boundary::Hops(2), Boundary::Saturated] {
                // One arm per signal, because the signal is a type parameter.
                macro_rules! collect {
                    ($sig:expr) => {{
                        let b = x.engine.project(root).direction(direction);
                        let built = match boundary {
                            Boundary::Hops(k) => b.hops(k),
                            _ => b.saturated(),
                        };
                        let id = built.signal($sig).materialize().projection_id();
                        ids.insert(*id.as_bytes());
                        n += 1;
                    }};
                }
                collect!(Adjacency);
                collect!(Degree);
                collect!(HopDistance);
            }
        }
    }

    assert_eq!(n, 54, "two roots × three directions × three boundaries × three signals");
    assert_eq!(ids.len(), n, "every distinct specification must have a distinct identifier");
}
