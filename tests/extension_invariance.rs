//! Which reads are stable under extension, and which are not.
//!
//! The graph is append-only, so a projection taken over a partially-built
//! graph sometimes equals the same projection taken later and sometimes does
//! not — and *which* depends on the axes, not on the workload.
//!
//! Ancestry is fixed at creation: a distinction's parents are written once,
//! inside the insert, and nothing is ever removed. So an upstream read of a
//! witnessed root cannot change, no matter how much is appended afterwards —
//! not even an unbounded one, because the complete ancestry closure of a
//! fixed node is itself fixed. Degree is different: it grows as children
//! appear. Downstream and undirected reads are different again, because the
//! children index is rebuilt from the whole engine on every materialize.
//!
//! # The prediction, stated before the measurement
//!
//! Of the eighteen axis combinations the crate ships — three directions,
//! three signals, two boundary kinds — **exactly four are extension-invariant:**
//! `Upstream` crossed with `{Adjacency, HopDistance}` crossed with either
//! boundary. The other fourteen must change once the graph grows around the
//! root.
//!
//! # Why this is worth a test rather than a comment
//!
//! A projection identifier covers the *specification*, not the output, by
//! design — which means two specs that differ in whether they are
//! extension-invariant have indistinguishable identities. The property is real,
//! it is load-bearing for anyone building a read-feeds-write loop on this
//! substrate, and nothing in the published surface can currently express it.
//! A test is the only place it is written down.

use koru_lambda_core::projection::{Adjacency, Degree, Direction, HopDistance, Signal};
use koru_lambda_core::{Distinction, DistinctionEngine};

/// Materialized bytes plus the spec-derived identifier.
struct Read {
    id: Vec<u8>,
    bytes: Vec<u8>,
}

fn read<S: Signal>(
    engine: &DistinctionEngine,
    root: Distinction,
    direction: Direction,
    saturated: bool,
    signal: S,
) -> Read {
    let builder = engine.project(root).direction(direction);
    let bounded = if saturated { builder.saturated() } else { builder.hops(2) };
    let p = bounded.signal(signal).materialize();
    Read { id: p.projection_id().as_bytes().to_vec(), bytes: p.canonical_bytes() }
}

fn next(state: &mut u64) -> usize {
    *state = state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
    (*state >> 33) as usize
}

#[test]
fn exactly_four_of_eighteen_reads_survive_extension() {
    let engine = DistinctionEngine::new();

    // A root created early, so the graph grows around it rather than from it.
    let root = engine.synthesize(engine.d0(), engine.d1());
    let mut pool = vec![engine.d0(), engine.d1(), root];
    let mut state = 20_261_005u64;

    // Phase one: build a neighbourhood around the root.
    for _ in 0..120 {
        let a = pool[next(&mut state) % pool.len()];
        let b = pool[next(&mut state) % pool.len()];
        if a != b {
            pool.push(engine.synthesize(a, b));
        }
    }

    // Capture all eighteen reads. The macro is needed because `Signal` is a
    // trait whose three implementors are distinct types, so the signal axis
    // cannot be a runtime loop.
    macro_rules! capture {
        ($($signal:expr),+ $(,)?) => {{
            let mut out: Vec<(String, Read)> = Vec::new();
            for direction in [Direction::Upstream, Direction::Downstream, Direction::Undirected] {
                for saturated in [false, true] {
                    $(
                        out.push((
                            format!(
                                "{direction:?}/{}/{}",
                                stringify!($signal),
                                if saturated { "saturated" } else { "hops(2)" }
                            ),
                            read(&engine, root, direction, saturated, $signal),
                        ));
                    )+
                }
            }
            out
        }};
    }

    let before = capture!(Adjacency, Degree, HopDistance);
    assert_eq!(before.len(), 18, "three directions x three signals x two boundaries");

    // Phase two: extend. Deliberately synthesize *from* the root as well as
    // around it, so its downstream cone genuinely grows — otherwise a
    // downstream read could appear invariant for want of new children.
    for _ in 0..200 {
        let a = pool[next(&mut state) % pool.len()];
        let b = pool[next(&mut state) % pool.len()];
        if a != b {
            pool.push(engine.synthesize(a, b));
        }
        let c = pool[next(&mut state) % pool.len()];
        if c != root {
            pool.push(engine.synthesize(root, c));
        }
    }

    let after = capture!(Adjacency, Degree, HopDistance);

    let mut invariant = Vec::new();
    let mut changed = Vec::new();
    for ((label, b), (label_after, a)) in before.iter().zip(after.iter()) {
        assert_eq!(label, label_after, "capture order drifted between phases");
        // The identifier is derived from the specification alone, so it must
        // not move even where the bytes do. This is also exactly why it cannot
        // express the property this test measures.
        assert_eq!(
            b.id, a.id,
            "{label}: the projection identifier changed under extension, but it is \
             specified as a function of the spec alone"
        );
        if b.bytes == a.bytes {
            invariant.push(label.clone());
        } else {
            changed.push(label.clone());
        }
    }

    eprintln!("extension-invariant ({}):", invariant.len());
    for l in &invariant {
        eprintln!("  {l}");
    }
    eprintln!("changed ({}):", changed.len());
    for l in &changed {
        eprintln!("  {l}");
    }

    let expected: Vec<&str> = vec![
        "Upstream/Adjacency/hops(2)",
        "Upstream/HopDistance/hops(2)",
        "Upstream/Adjacency/saturated",
        "Upstream/HopDistance/saturated",
    ];
    let mut got: Vec<&str> = invariant.iter().map(String::as_str).collect();
    got.sort_unstable();
    let mut want = expected.clone();
    want.sort_unstable();

    assert_eq!(
        got, want,
        "the set of extension-invariant reads is not the predicted one. Ancestry is fixed at \
         creation and the graph is append-only, so upstream reads of a witnessed root should \
         be stable under any amount of appending, while degree grows and the children index \
         is rebuilt from the whole engine. A different set means one of those three facts is \
         wrong."
    );
}
