//! `degree()` is the edge count `docs/SPEC.md` §6.1 defines, on every shape.
//!
//! §6 used to specify the `Degree` signal as "the node's degree" and define
//! the word nowhere. The engine computes a participation count plus an
//! addend — `+1` for a primordial, `+2` for anything else — and neither
//! number appeared in the specification.
//!
//! An agent implementing the substrate from §1–§11 alone chose incident
//! edges, produced 2 where the fixture says 3 for the primordials, and
//! failed four of the eighteen published projections. Its sharper finding:
//! more than one rule reproduces the published history, because that history
//! is a chain, so the definition was not recoverable even by working
//! backwards from the vectors.
//!
//! §6.1 now states it as a principle rather than an addend: degree is the
//! number of edges incident to a node, over an edge set of two edges per
//! non-primordial (one to each parent) plus **one genesis edge** between
//! `d₀` and `d₁`. The addends are that definition seen from the inside —
//! `+2` is a non-primordial's own upward pair, `+1` is a primordial's
//! endpoint on the genesis edge.
//!
//! This test is why that claim is checkable rather than asserted. It builds
//! the edge set independently from `snapshot_parentage`, counts endpoints,
//! and compares against `degree()` — including on a branching graph, which
//! is where the candidate definitions diverge and which the fixture does not
//! contain.

use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::HashMap;

/// The edge set of §6.1, built from recorded parentage and nothing else.
fn incident_edges(e: &DistinctionEngine) -> HashMap<Distinction, usize> {
    let mut deg: HashMap<Distinction, usize> =
        e.snapshot_distinctions().into_iter().map(|d| (d, 0)).collect();

    // Two edges per non-primordial: one to each parent.
    for (child, (a, b)) in e.snapshot_parentage() {
        *deg.get_mut(&child).expect("the child is registered") += 2;
        *deg.get_mut(&a).expect("a parent is registered") += 1;
        *deg.get_mut(&b).expect("a parent is registered") += 1;
    }

    // The genesis edge, one endpoint on each primordial. This is the term
    // that was missing from the specification, and the `+1` in `r = 2d − 3`.
    *deg.get_mut(&e.d0()).expect("d0") += 1;
    *deg.get_mut(&e.d1()).expect("d1") += 1;
    deg
}

/// Build a shape and check the definition holds over it.
fn check(name: &str, build: impl Fn(&DistinctionEngine)) {
    let e = DistinctionEngine::new();
    build(&e);

    let expected = incident_edges(&e);
    for (d, want) in &expected {
        assert_eq!(e.degree(*d), *want, "{name}: degree({d:?}) disagrees with the §6.1 edge count");
    }

    // Every edge has two endpoints, and the edge set has 2d − 3 members —
    // which is Law 6, and the reason the genesis edge has to be counted.
    let endpoints: usize = expected.values().sum();
    assert_eq!(endpoints % 2, 0, "{name}: an odd endpoint total is not an edge set");
    let edges = endpoints / 2;
    assert_eq!(
        edges,
        2 * e.distinction_count() - 3,
        "{name}: the edge set must have 2d − 3 members"
    );
    assert_eq!(edges, e.relationship_count(), "{name}: and that is the relationship count");
}

/// The published history: a chain. This is the shape the fixture contains,
/// and the one on which rival definitions agree.
#[test]
fn degree_is_the_edge_count_on_the_published_chain() {
    check("published chain", |e| {
        let c = e.synthesize(e.d0(), e.d1());
        let d = e.synthesize(c, e.d0());
        let _ = e.synthesize(d, e.d1());
    });
}

/// A diamond: one node with two children that share a further child.
///
/// The discriminating shape. A node here is reachable by two routes, so
/// definitions that count unique neighbours, parent edges only, or
/// first-visit participation diverge from the edge count — and none of the
/// eighteen published projections would notice, because they are all taken
/// over the chain above.
#[test]
fn degree_is_the_edge_count_on_a_branching_graph() {
    check("diamond", |e| {
        let c = e.synthesize(e.d0(), e.d1());
        let f = e.synthesize(c, e.d0());
        let g = e.synthesize(c, e.d1());
        let _ = e.synthesize(f, g);
    });
}

/// A node used as a parent many times, so the participation count dominates
/// the addend and a wrong addend cannot hide inside a small number.
#[test]
fn degree_is_the_edge_count_when_one_node_is_reused() {
    check("fan from d1", |e| {
        let mut cur = e.synthesize(e.d0(), e.d1());
        for _ in 0..40 {
            cur = e.synthesize(cur, e.d1());
        }
    });
}

/// The primordials' degree is not a special case, it is the genesis edge.
///
/// On a fresh engine with one synthesis, each primordial has one edge to the
/// child plus the genesis edge. An implementation omitting the genesis term
/// reports 1 and passes nothing in `lift.canonical_bytes` or the `Degree`
/// projections.
#[test]
fn a_primordial_carries_its_genesis_endpoint() {
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());

    assert_eq!(e.degree(e.d0()), 2, "one edge to c, plus the genesis edge");
    assert_eq!(e.degree(e.d1()), 2, "one edge to c, plus the genesis edge");
    assert_eq!(e.degree(c), 2, "its own two upward edges, and no children yet");
    assert_eq!(e.relationship_count(), 3, "two parent edges and the genesis edge");
}

/// A distinction the engine never witnessed is not in the edge set.
#[test]
fn an_unwitnessed_distinction_has_no_edges() {
    let e = DistinctionEngine::new();
    let _ = e.synthesize(e.d0(), e.d1());

    // Witnessed by a different engine, so genuine bytes this one has not seen.
    let stranger = DistinctionEngine::new();
    let a = stranger.synthesize(stranger.d0(), stranger.d1());
    let foreign = stranger.synthesize(a, stranger.d0());

    assert_eq!(e.degree(foreign), 0, "not witnessed here, so not in this edge set");
}
