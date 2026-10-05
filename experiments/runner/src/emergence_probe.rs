//! What appears when the draw policy encodes nothing?
//!
//! The theory's framework claim is that complexity follows from letting
//! synthesis run. The honest version of that question is narrower and
//! answerable: **which structural invariants survive a policy that encodes no
//! structure?** Anything that survives is the substrate's. Anything that needs
//! a policy is the policy's.
//!
//! This distinction is not academic here. The project has twice measured its
//! own harness and published the result as a property of the substrate: the
//! previous release's "scale-free topology" existed only because the test
//! driver installed preferential attachment and then discovered it, and Law 11
//! lost its independent status when the hub turned out to track the alphabet —
//! a policy choice — rather than the anchor. So this probe is two arms, and
//! the policy-free arm is the one that counts.
//!
//! # The prediction, stated before the measurement
//!
//! A global mean clustering coefficient of ~0.156 was observed in passing,
//! stable to three decimals across a 3x scale range, under a locality-biased
//! growth policy. The open question is whether it is a constant of the
//! substrate or of that policy.
//!
//! Prediction: **it is the policy's.** Under uniform-random pairing, two
//! distinctions drawn from a pool of size N are adjacent with probability on
//! the order of 4/N — mean degree is exactly 4 at every scale by Law 6 — so
//! the pair is almost never already adjacent, and a triangle closes only when
//! it is. Clustering should therefore fall toward zero as N grows, and fall
//! roughly as 1/N. Under locality bias the partner is *chosen* from the
//! anchor's neighbours, so a triangle closes almost every step.
//!
//! If clustering instead persists at some non-zero value under uniform
//! pairing, that is a constant nobody installed, and it would be the first
//! item in this theory's ledger that is not bookkeeping.
//!
//! Two controls make the arms comparable: `r = 2d - 3` and mean degree are
//! asserted in both, because they are forced by Law 6 and so must be identical
//! regardless of policy. If they differ, the harness is broken rather than the
//! substrate interesting.

use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::{HashMap, HashSet};

/// How a pair is chosen. The whole experiment is the contrast.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Policy {
    /// Both parents drawn uniformly from everything that exists. Encodes
    /// nothing: no locality, no degree preference, no recency.
    Uniform,
    /// Anchor drawn uniformly, partner drawn from the anchor's neighbours.
    /// Encodes locality, and is the policy the ~0.156 figure came from.
    LocalityBiased,
}

/// One measurement of a grown graph.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    /// Draw policy used to grow it.
    pub policy: Policy,
    /// Distinctions in the engine.
    pub distinctions: usize,
    /// Relationships, which Law 6 forces to `2d - 3`.
    pub relationships: usize,
    /// Mean degree, which Law 6 forces toward 4.
    pub mean_degree: f64,
    /// Highest degree observed — a hub, if the policy produced one.
    pub max_degree: usize,
    /// Global mean of the local clustering coefficient.
    pub mean_clustering: f64,
    /// Triangles in the parentage graph.
    pub triangles: usize,
    /// Triangles with no mediated-self-reference witness. Must be zero: the
    /// only edges are child-parent, so there is no other way to close one.
    pub unexplained_triangles: usize,
}

fn next(state: &mut u64) -> usize {
    *state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    (*state >> 33) as usize
}

/// Sorted, because `snapshot_distinctions` inherits map iteration order. The
/// previous release's emergence probes drew from unsorted snapshots, so their
/// seeded RNG bought nothing and no number they printed was reproducible.
fn sorted(engine: &DistinctionEngine) -> Vec<Distinction> {
    let mut all = engine.snapshot_distinctions();
    all.sort();
    all
}

fn adjacency(engine: &DistinctionEngine) -> HashMap<Distinction, HashSet<Distinction>> {
    let mut adj: HashMap<Distinction, HashSet<Distinction>> = HashMap::new();
    for d in engine.snapshot_distinctions() {
        adj.entry(d).or_default();
    }
    for (child, (a, b)) in engine.snapshot_parentage() {
        adj.entry(child).or_default().insert(a);
        adj.entry(a).or_default().insert(child);
        adj.entry(child).or_default().insert(b);
        adj.entry(b).or_default().insert(child);
    }
    // The genesis edge is the one relationship not derivable from parentage.
    adj.entry(engine.d0()).or_default().insert(engine.d1());
    adj.entry(engine.d1()).or_default().insert(engine.d0());
    adj
}

/// Grow a graph to `steps` syntheses under `policy`, then measure it.
#[must_use]
pub fn measure(policy: Policy, steps: usize, seed: u64) -> Shape {
    let engine = DistinctionEngine::new();
    let mut state = seed;
    let mut pool = sorted(&engine);

    for _ in 0..steps {
        let (left, right) = match policy {
            Policy::Uniform => {
                let a = pool[next(&mut state) % pool.len()];
                let b = pool[next(&mut state) % pool.len()];
                (a, b)
            },
            Policy::LocalityBiased => {
                let a = pool[next(&mut state) % pool.len()];
                let mut neighbours: Vec<Distinction> = Vec::new();
                if let Some((p1, p2)) = engine.parents_of(a) {
                    neighbours.push(p1);
                    neighbours.push(p2);
                }
                for (child, (p1, p2)) in engine.snapshot_parentage() {
                    if p1 == a || p2 == a {
                        neighbours.push(child);
                    }
                }
                let b = if neighbours.is_empty() {
                    pool[next(&mut state) % pool.len()]
                } else {
                    neighbours.sort();
                    neighbours[next(&mut state) % neighbours.len()]
                };
                (a, b)
            },
        };
        if left != right {
            let child = engine.synthesize(left, right);
            // Keep the pool in sync without a full re-sort each step.
            if let Err(at) = pool.binary_search(&child) {
                pool.insert(at, child);
            }
        }
    }

    let adj = adjacency(&engine);
    let d = engine.distinction_count();
    let r = engine.relationship_count();

    let mut clustering_sum = 0.0;
    let mut max_degree = 0usize;
    let mut triangles: HashSet<[Distinction; 3]> = HashSet::new();
    for (v, neighbours) in &adj {
        let k = neighbours.len();
        max_degree = max_degree.max(engine.degree(*v));
        if k < 2 {
            continue;
        }
        let list: Vec<Distinction> = neighbours.iter().copied().collect();
        let mut closed = 0usize;
        for (i, u) in list.iter().enumerate() {
            for w in &list[i + 1..] {
                if adj.get(u).is_some_and(|n| n.contains(w)) {
                    closed += 1;
                    let mut tri = [*v, *u, *w];
                    tri.sort();
                    triangles.insert(tri);
                }
            }
        }
        clustering_sum += closed as f64 / ((k * (k - 1) / 2) as f64);
    }

    // Classify: every triangle must have a node whose parent pair is the
    // other two. There is no other way to close one when the only edges are
    // child-parent.
    let mut unexplained = 0usize;
    for tri in &triangles {
        let witnessed = tri.iter().enumerate().any(|(i, &c)| {
            engine.parents_of(c).is_some_and(|(p1, p2)| {
                let others: HashSet<Distinction> =
                    tri.iter().enumerate().filter(|(j, _)| *j != i).map(|(_, x)| *x).collect();
                others == HashSet::from([p1, p2])
            })
        });
        if !witnessed {
            unexplained += 1;
        }
    }

    Shape {
        policy,
        distinctions: d,
        relationships: r,
        mean_degree: 2.0 * r as f64 / d as f64,
        max_degree,
        mean_clustering: clustering_sum / adj.len() as f64,
        triangles: triangles.len(),
        unexplained_triangles: unexplained,
    }
}
