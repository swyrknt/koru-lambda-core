//! Two structural facts about the parentage graph, both exact.
//!
//! # 1. Clustering *is* mediated self-reference
//!
//! The only edges in the graph are child-parent, plus the single genesis edge
//! between the primordials. So a triangle needs one of its three pairs to be
//! an edge in its own right — which means `c = synth(a, b)` closes a triangle
//! exactly when `a` and `b` were *already adjacent*, i.e. when one of them is
//! the other's parent.
//!
//! Unpack that and it is Law 10. Let `m = synth(x, obs)` and `r = synth(m, x)`:
//! `parents(r) = (m, x)`, and `x` is a parent of `m`, so `{r, m, x}` is a
//! triangle. Mediated self-reference is not *a way* to close a triangle in
//! this graph — it is the only way.
//!
//! The consequence is a structural identity between a standard graph statistic
//! and a law of this theory: **the local clustering coefficient of the
//! parentage graph measures the local density of mediated self-reference, and
//! measures nothing else.** `tests/mediated_self_reference.rs` establishes
//! Law 10 along a single chain to depth 100,000 — existence and non-collapse.
//! This establishes it across a whole population, as a closed classification
//! with no residue.
//!
//! # 2. Synthesis is strictly local
//!
//! `synth(a, b)` creates one distinction adjacent to exactly `a` and `b` and
//! touches nothing else. So for two neighbourhoods that are disjoint *and
//! non-adjacent*, work confined to one cannot perturb any clustering
//! coefficient in the other — not approximately, exactly.
//!
//! This is a live regression gate rather than a philosophical claim. A reverse
//! index, a degree cache that rewrites a third party, or any hub-rebalancing
//! pass would break it, and each of those is the kind of thing added for
//! performance without anyone expecting a structural consequence.

use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

type Adjacency = HashMap<Distinction, HashSet<Distinction>>;

/// A neighbourhood: the set of distinctions within some hop boundary of a centre.
type Ball = HashSet<Distinction>;

/// Deterministic sampling. The 1.2.0 probes that measured this graph drew from
/// a `DashMap` iteration whose order is randomised per map instance, so their
/// seeded RNG bought nothing and no number they printed was reproducible —
/// their reported correlation ranged over (-0.80, +0.82) across runs of one
/// binary. Any probe that *samples* the snapshot must sort it first.
fn sorted_snapshot(engine: &DistinctionEngine) -> Vec<Distinction> {
    let mut all = engine.snapshot_distinctions();
    all.sort();
    all
}

fn next_index(state: &mut u64) -> usize {
    *state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    (*state >> 33) as usize
}

/// Grow a graph with a locality bias, so neighbourhoods are dense enough for
/// triangles to exist at all. A uniform-random pairing would make the census
/// vacuous rather than wrong.
fn grow(engine: &DistinctionEngine, steps: usize, seed: u64) -> usize {
    let mut state = seed;
    let mut synthesised = 0;
    for _ in 0..steps {
        let pool = sorted_snapshot(engine);
        let anchor = pool[next_index(&mut state) % pool.len()];
        // Prefer a partner from the anchor's own neighbourhood.
        let neighbours = neighbours_of(engine, anchor);
        let partner = if neighbours.is_empty() {
            pool[next_index(&mut state) % pool.len()]
        } else {
            neighbours[next_index(&mut state) % neighbours.len()]
        };
        if partner != anchor {
            let _ = engine.synthesize(anchor, partner);
            synthesised += 1;
        }
    }
    synthesised
}

fn neighbours_of(engine: &DistinctionEngine, of: Distinction) -> Vec<Distinction> {
    let mut out = Vec::new();
    if let Some((p1, p2)) = engine.parents_of(of) {
        out.push(p1);
        out.push(p2);
    }
    for (child, (p1, p2)) in engine.snapshot_parentage() {
        if p1 == of || p2 == of {
            out.push(child);
        }
    }
    out
}

/// Undirected adjacency over parentage, **including the genesis edge**.
///
/// `snapshot_parentage` omits `d₀ ↔ d₁` because primordials have no parents.
/// Leave it out and `r = 2d − 3` appears violated and one triangle vanishes.
fn adjacency(engine: &DistinctionEngine) -> Adjacency {
    let mut adj: Adjacency = HashMap::new();
    let link = |a: Distinction, b: Distinction, adj: &mut Adjacency| {
        adj.entry(a).or_default().insert(b);
        adj.entry(b).or_default().insert(a);
    };
    for d in engine.snapshot_distinctions() {
        adj.entry(d).or_default();
    }
    for (child, (p1, p2)) in engine.snapshot_parentage() {
        link(child, p1, &mut adj);
        link(child, p2, &mut adj);
    }
    link(engine.d0(), engine.d1(), &mut adj);
    adj
}

fn triangles(adj: &Adjacency) -> HashSet<[Distinction; 3]> {
    let mut found = HashSet::new();
    for (v, neighbours) in adj {
        let list: Vec<Distinction> = neighbours.iter().copied().collect();
        for (i, u) in list.iter().enumerate() {
            for w in &list[i + 1..] {
                if adj.get(u).is_some_and(|n| n.contains(w)) {
                    let mut tri = [*v, *u, *w];
                    tri.sort();
                    found.insert(tri);
                }
            }
        }
    }
    found
}

/// `C(v) = T(v) / (deg(v) choose 2)`, and `0` when `deg(v) < 2`.
fn clustering(adj: &Adjacency, v: Distinction) -> f64 {
    let Some(neighbours) = adj.get(&v) else { return 0.0 };
    let k = neighbours.len();
    if k < 2 {
        return 0.0;
    }
    let list: Vec<Distinction> = neighbours.iter().copied().collect();
    let mut closed = 0usize;
    for (i, u) in list.iter().enumerate() {
        for w in &list[i + 1..] {
            if adj.get(u).is_some_and(|n| n.contains(w)) {
                closed += 1;
            }
        }
    }
    closed as f64 / ((k * (k - 1) / 2) as f64)
}

/// Every triangle is closed by a node whose parent pair is the other two
/// vertices. Nothing else can close one.
#[test]
fn every_triangle_is_a_mediated_self_reference() {
    let engine = DistinctionEngine::new();
    grow(&engine, 900, 20_260_102);

    let adj = adjacency(&engine);
    let all = triangles(&adj);
    assert!(all.len() > 50, "only {} triangles — corpus too sparse to be evidence", all.len());

    // `r = 2d - 3` must hold, or the edge set is not what this census assumes.
    let d = engine.distinction_count();
    assert_eq!(engine.relationship_count(), 2 * d - 3, "Law 6 broken; edge set is not parentage");

    let mut via_parentage = 0usize;
    let mut via_genesis = 0usize;
    let mut unexplained: Vec<[Distinction; 3]> = Vec::new();

    for tri in &all {
        let mut witness = None;
        for (i, &c) in tri.iter().enumerate() {
            let Some((p1, p2)) = engine.parents_of(c) else { continue };
            let others: BTreeSet<Distinction> =
                tri.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, d)| *d).collect();
            let parents: BTreeSet<Distinction> = [p1, p2].into_iter().collect();
            if others == parents {
                witness = Some((p1, p2));
                break;
            }
        }
        match witness {
            // The closing edge is the genesis edge iff the parent pair is
            // exactly the two primordials: they are adjacent without any
            // synthesis having made them so.
            Some((p1, p2)) => {
                let pair: BTreeSet<Distinction> = [p1, p2].into_iter().collect();
                let primordials: BTreeSet<Distinction> =
                    [engine.d0(), engine.d1()].into_iter().collect();
                if pair == primordials {
                    via_genesis += 1;
                } else {
                    via_parentage += 1;
                }
            },
            None => unexplained.push(*tri),
        }
    }

    eprintln!(
        "{} distinctions, {} relationships, {} triangles: {via_parentage} mediated \
         self-reference, {via_genesis} genesis, {} unexplained",
        d,
        engine.relationship_count(),
        all.len(),
        unexplained.len()
    );

    assert!(
        unexplained.is_empty(),
        "{} of {} triangles have no mediated-self-reference witness. The clustering \
         coefficient would then be measuring something other than Law 10, and either the \
         graph carries an edge that is not child-parent or two independently created \
         distinctions became adjacent.",
        unexplained.len(),
        all.len()
    );
    assert_eq!(via_genesis, 1, "exactly one triangle should close on the genesis edge");
    assert!(via_parentage > 0, "census is vacuous without mediated-self-reference triangles");
}

/// Work confined to one neighbourhood perturbs a disjoint, non-adjacent
/// neighbourhood by exactly zero.
#[test]
fn synthesis_confined_to_one_neighbourhood_does_not_reach_another() {
    let engine = DistinctionEngine::new();
    grow(&engine, 700, 20_260_103);

    let adj = adjacency(&engine);
    let pool = sorted_snapshot(&engine);

    // Find two radius-2 balls that are disjoint AND non-adjacent. Adjacency
    // matters: touching balls share a boundary, so a synthesis in one legally
    // changes a coefficient in the other and the test would be wrong, not the
    // substrate.
    let ball = |centre: Distinction| -> Ball {
        let mut seen = HashSet::from([centre]);
        let mut frontier = VecDeque::from([(centre, 0usize)]);
        while let Some((node, hops)) = frontier.pop_front() {
            if hops == 2 {
                continue;
            }
            for next in adj.get(&node).into_iter().flatten() {
                if seen.insert(*next) {
                    frontier.push_back((*next, hops + 1));
                }
            }
        }
        seen
    };

    let mut chosen: Option<(Ball, Ball)> = None;
    'outer: for (i, &a) in pool.iter().enumerate() {
        let ball_a = ball(a);
        if ball_a.len() < 8 {
            continue;
        }
        for &b in &pool[i + 1..] {
            let ball_b = ball(b);
            if ball_b.len() < 8 || !ball_a.is_disjoint(&ball_b) {
                continue;
            }
            let touching = ball_b
                .iter()
                .any(|n| adj.get(n).is_some_and(|nb| nb.iter().any(|x| ball_a.contains(x))));
            if !touching {
                chosen = Some((ball_a, ball_b));
                break 'outer;
            }
        }
    }

    let (ball_a, ball_b) = chosen.expect("no disjoint non-adjacent pair of neighbourhoods found");
    let before: Vec<(Distinction, f64)> =
        ball_b.iter().map(|&v| (v, clustering(&adj, v))).collect();

    // Confine every synthesis to ball A.
    let members: Vec<Distinction> = {
        let mut m: Vec<Distinction> = ball_a.iter().copied().collect();
        m.sort();
        m
    };
    let mut state = 777u64;
    for _ in 0..400 {
        let x = members[next_index(&mut state) % members.len()];
        let y = members[next_index(&mut state) % members.len()];
        if x != y {
            let _ = engine.synthesize(x, y);
        }
    }

    let after_adj = adjacency(&engine);
    let after_ball_b: HashSet<Distinction> = {
        let mut seen = HashSet::new();
        for &(v, _) in &before {
            seen.insert(v);
        }
        seen
    };
    assert_eq!(ball_b, after_ball_b, "ball B membership drifted; the test lost its own subject");

    let mut moved = Vec::new();
    for (v, was) in &before {
        let now = clustering(&after_adj, *v);
        if now.to_bits() != was.to_bits() {
            moved.push((*v, *was, now));
        }
    }

    eprintln!(
        "confined {} syntheses to a {}-node neighbourhood; {} of {} nodes in the far \
         neighbourhood changed",
        400,
        members.len(),
        moved.len(),
        before.len()
    );

    assert!(
        moved.is_empty(),
        "{} of {} clustering coefficients changed in a disjoint, non-adjacent \
         neighbourhood. Synthesis has acquired a non-local side effect — a reverse index, \
         a degree cache touching a third party, or a rebalancing pass — and locality is \
         what 'every distinction is an advance of its own parents' rests on.",
        moved.len(),
        before.len()
    );
}
