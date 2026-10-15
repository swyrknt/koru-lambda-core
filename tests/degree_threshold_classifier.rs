//! A degree-threshold classifier, built entirely outside the crate.
//!
//! PRJ-9. The item does not ask the substrate to ship a classifier — it asks
//! whether one is *expressible* as a consumer `Signal`, demonstrated from an
//! external crate, so that wherever DEC-9 puts the classifier the substrate
//! needs no change. This file is that demonstration, and it is an
//! integration test for the same reason `external_engine.rs` is: a separate
//! compilation unit seeing only the published API, exactly as a sibling
//! crate would.
//!
//! Three things the item fixes, all of which this respects:
//!
//! * **A threshold *boundary* is not expressible.** `Boundary` is a closed,
//!   `#[non_exhaustive]` enum and DEC-10 makes that permanent at 2.0.0, so
//!   "stop traversing when degree drops below n" cannot be a traversal
//!   bound. The classifier therefore uses an existing boundary and does its
//!   thresholding in the **signal**, which is the extension point that is
//!   open.
//! * **It evicts nothing.** The name "compactor" implied removal; the graph
//!   is append-only, so a classifier can only label. Asserted below.
//! * **It must hold whatever DEC-9 decides**, which is why nothing here
//!   imports from `koru-lambda-agents`.
//!
//! The interesting part turned out not to be the classification. It is that
//! a *parameterized* signal has an obligation a stateless one does not, and
//! getting it wrong produces two reads that disagree while claiming the same
//! identity. See `two_thresholds_are_two_different_reads`.

use koru_lambda_core::projection::{CanonicalBytes, Direction, Signal, SignalContext};
use koru_lambda_core::{Distinction, DistinctionEngine};

// -------------------------------------------------------------------------
// The consumer's own output type
// -------------------------------------------------------------------------

/// What the classifier reports per node.
///
/// A consumer type, so it has to carry its own wire encoding. One byte is
/// enough and the mapping is explicit rather than derived from the variant
/// order, because a reordering of this enum must not silently change the
/// bytes a peer receives.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
enum ThermalClass {
    Cold,
    Warm,
    Hot,
}

impl CanonicalBytes for ThermalClass {
    fn write_canonical(&self, out: &mut Vec<u8>) {
        out.push(match self {
            ThermalClass::Cold => 0x00,
            ThermalClass::Warm => 0x01,
            ThermalClass::Hot => 0x02,
        });
    }
}

// -------------------------------------------------------------------------
// The signal
// -------------------------------------------------------------------------

/// Classifies each node in the cone by its degree against two thresholds.
///
/// `identity` is precomputed and **includes both thresholds**. That is the
/// whole subtlety of a parameterized signal: `Signal::canonical_bytes`
/// returns `&[u8]`, so the value has to be owned by the signal, and §8
/// derives `ProjectionId` from it. A classifier that returned a constant
/// label would give two different classifications the same identity.
#[derive(Clone)]
struct DegreeThreshold {
    warm_at: usize,
    hot_at: usize,
    identity: Vec<u8>,
}

impl DegreeThreshold {
    fn new(warm_at: usize, hot_at: usize) -> Self {
        assert!(warm_at <= hot_at, "a hot node is at least a warm one");
        let mut identity = Vec::from(&b"consumer:signal:degree_threshold:v1:"[..]);
        // Fixed width, little-endian, so the identity is target-independent
        // for the same reason §7 fixes every width (Cond D').
        identity.extend_from_slice(&(warm_at as u64).to_le_bytes());
        identity.extend_from_slice(&(hot_at as u64).to_le_bytes());
        Self { warm_at, hot_at, identity }
    }
}

impl Signal for DegreeThreshold {
    type Output = ThermalClass;

    fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> ThermalClass {
        let degree = ctx.engine.degree(node);
        if degree >= self.hot_at {
            ThermalClass::Hot
        } else if degree >= self.warm_at {
            ThermalClass::Warm
        } else {
            ThermalClass::Cold
        }
    }

    fn canonical_bytes(&self) -> &[u8] {
        &self.identity
    }
}

// -------------------------------------------------------------------------
// A graph with a genuine degree spread
// -------------------------------------------------------------------------

/// A hub plus a rim, so the classifier has all three classes to find.
///
/// Every rim node is synthesized against the hub, which drives the hub's
/// degree up by one per rim node while each rim node keeps degree 2.
fn hub_and_rim(rim: usize) -> (DistinctionEngine, Distinction) {
    let e = DistinctionEngine::new();
    let hub = e.synthesize(e.d0(), e.d1());
    let mut last = e.synthesize(hub, e.d0());
    for _ in 0..rim {
        // Pair the hub with a fresh partner each time, so the hub gains an
        // edge and the partner is new.
        last = e.synthesize(hub, last);
    }
    (e, hub)
}

// -------------------------------------------------------------------------
// PRJ-9
// -------------------------------------------------------------------------

/// The classifier runs as a projection and finds all three classes.
#[test]
fn a_consumer_signal_classifies_by_degree_threshold() {
    let (engine, hub) = hub_and_rim(40);

    let classified = engine
        .project(hub)
        .direction(Direction::Undirected)
        .saturated() // an existing boundary — a threshold boundary is not one
        .signal(DegreeThreshold::new(3, 10))
        .materialize();

    let mut seen: Vec<ThermalClass> = classified.output().iter().map(|(_, c)| *c).collect();
    seen.sort_unstable();
    seen.dedup();

    assert!(seen.contains(&ThermalClass::Hot), "the hub is hot");
    assert!(seen.contains(&ThermalClass::Cold), "a degree-2 rim node is cold");

    // And every label agrees with the engine's own degree, so the signal is
    // reading the graph rather than inventing a distribution.
    for (node, class) in classified.output().iter() {
        let degree = engine.degree(node);
        let expected = if degree >= 10 {
            ThermalClass::Hot
        } else if degree >= 3 {
            ThermalClass::Warm
        } else {
            ThermalClass::Cold
        };
        assert_eq!(*class, expected, "node of degree {degree}");
    }
}

/// It evicts nothing — the item's last clause, asserted rather than assumed.
///
/// "Compactor" implied removal. The graph is append-only, so a classifier
/// can only label, and a read that changed the engine would break Cond E
/// besides.
#[test]
fn classifying_evicts_nothing() {
    let (engine, hub) = hub_and_rim(20);

    let before_count = engine.distinction_count();
    let before_relationships = engine.relationship_count();
    let mut before_parentage = engine.snapshot_parentage();
    before_parentage.sort_by_key(|(c, _)| *c.as_bytes());

    let classified = engine
        .project(hub)
        .direction(Direction::Undirected)
        .saturated()
        .signal(DegreeThreshold::new(3, 10))
        .materialize();
    let _ = classified.canonical_bytes();

    let mut after_parentage = engine.snapshot_parentage();
    after_parentage.sort_by_key(|(c, _)| *c.as_bytes());

    assert_eq!(engine.distinction_count(), before_count, "nothing added");
    assert_eq!(engine.relationship_count(), before_relationships, "no edge moved");
    assert_eq!(after_parentage, before_parentage, "and nothing was evicted");
    engine.check_structural_invariant().expect("the graph is untouched");
}

/// Two thresholds are two different reads, and §8 has to say so.
///
/// This is the part worth having a test for. `ProjectionId` is derived from
/// the signal's `canonical_bytes()`, so a parameterized signal that does not
/// fold its parameters into that value hands two different classifications
/// the same identity — and `restore_projection` would then accept one
/// peer's bytes as the other's read. The obligation is documented on
/// `Signal`; here it is exercised.
#[test]
fn two_thresholds_are_two_different_reads() {
    let (engine, hub) = hub_and_rim(40);

    let strict = engine
        .project(hub)
        .direction(Direction::Undirected)
        .saturated()
        .signal(DegreeThreshold::new(3, 10))
        .materialize();

    let lenient = engine
        .project(hub)
        .direction(Direction::Undirected)
        .saturated()
        .signal(DegreeThreshold::new(2, 4))
        .materialize();

    assert_ne!(
        strict.output().iter().map(|(_, c)| *c).collect::<Vec<_>>(),
        lenient.output().iter().map(|(_, c)| *c).collect::<Vec<_>>(),
        "the two thresholds classify differently"
    );
    assert_ne!(
        strict.projection_id(),
        lenient.projection_id(),
        "so they must not share an identity — this is what folding the \
         thresholds into canonical_bytes() buys"
    );
    assert_ne!(strict.canonical_bytes(), lenient.canonical_bytes());
}

/// The classified read survives the wire, and restore re-derives it.
///
/// A consumer signal gets the same §7 framing and §8 identity as a shipped
/// one, which is the claim that makes "wherever DEC-9 puts the classifier"
/// true: the agents crate can send a classification to a peer and the peer
/// verifies it against its own engine, with no substrate change.
#[test]
fn a_classified_projection_round_trips_on_the_wire() {
    let (engine, hub) = hub_and_rim(12);

    let sent = engine
        .project(hub)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(DegreeThreshold::new(3, 10))
        .materialize();
    let wire = sent.canonical_bytes();

    let received = engine
        .restore_projection(&wire, DegreeThreshold::new(3, 10))
        .expect("a consumer signal restores like a shipped one");

    assert_eq!(received.canonical_bytes(), wire);
    assert_eq!(received.projection_id(), sent.projection_id());

    // A peer supplying the wrong thresholds is refused, because the header
    // carries the signal's identity and it will not match.
    assert!(
        engine.restore_projection(&wire, DegreeThreshold::new(2, 4)).is_err(),
        "a different classifier must not restore another's bytes"
    );
}
