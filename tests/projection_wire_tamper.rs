//! What a successful restore does not establish, and the one error variant
//! that is unreachable by arithmetic.
//!
//! PRJ-3 and TST-4. The rejection suite covered the five header offsets
//! `docs/SPEC.md` §11 fixes, truncation, an inflated entry count and a
//! well-formed forgery. Three regions of the wire had never been tampered
//! with: the boundary payload, an entry's declared output length, and an
//! entry's output bytes.
//!
//! Two of the three are refused and are now `reject.cases` in
//! `conformance/vectors.json`, so the independent Python implementation
//! checks them too. The third is accepted, and that is the finding.

use koru_lambda_core::projection::{Adjacency, Boundary, Direction};
use koru_lambda_core::DistinctionEngine;

/// `docs/SPEC.md` §10.1's history, which the fixture's cases are taken over.
fn history() -> (DistinctionEngine, koru_lambda_core::Distinction) {
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let _ = e.synthesize(d, e.d1());
    (e, c)
}

/// A boundary change that does not change the cone is undetectable.
///
/// Rooted at `c` this graph has diameter 2, so `Hops(2)` and `Hops(3)`
/// enumerate the same entries. Flipping the boundary payload from one to the
/// other therefore produces bytes this engine *would* have produced, and the
/// restore succeeds — correctly, by the guarantee as stated: "bytes that do
/// not match what this engine would have produced are rejected."
///
/// What it costs is agreement about *which read this is*. §8 hashes the
/// specification rather than the output, so the sender and the receiver end
/// up holding projections with different identities over identical bytes.
/// The mitigation is to compare the identity out of band, which is what a
/// specification's fingerprint is for.
///
/// Pinned rather than fixed. A stricter restore would have to take the
/// expected specification as an argument, which would make the header's
/// copy of it redundant — a design change, not a repair. This test exists so
/// that change would be visible rather than silent.
#[test]
fn a_boundary_that_does_not_change_the_cone_is_undetectable() {
    let (e, c) = history();
    let sent = e.project(c).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
    let wire = sent.canonical_bytes();

    // Byte 21 is the boundary tag, 22..30 the payload: Hops(2) -> Hops(3).
    let mut tampered = wire.clone();
    tampered[22] = 0x03;
    assert_ne!(tampered, wire, "the tamper changed the bytes");

    let received = e
        .restore_projection(&tampered, Adjacency)
        .expect("a spec that yields the same cone re-materialises identically");

    assert_eq!(sent.spec().boundary, Boundary::Hops(2), "the sender asked for two hops");
    assert_eq!(received.spec().boundary, Boundary::Hops(3), "the receiver believes three");
    assert_eq!(received.canonical_bytes(), tampered, "and the bytes verify");
    assert_ne!(
        sent.projection_id(),
        received.projection_id(),
        "so the two sides hold different identities for the same bytes — which is \
         why ProjectionId is the thing to compare out of band"
    );
}

/// The same tamper *is* detected once the two boundaries differ in the cone.
///
/// The counterpart to the test above, and the reason it is a narrow property
/// rather than a general hole: at `Hops(1)` the cone is smaller, so claiming
/// `Hops(1)` over entries that only `Hops(2)` produces fails.
#[test]
fn a_boundary_that_does_change_the_cone_is_detected() {
    let (e, c) = history();
    let wire = e
        .project(c)
        .direction(Direction::Undirected)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .canonical_bytes();

    let mut tampered = wire.clone();
    tampered[22] = 0x01; // Hops(2) -> Hops(1), which drops an entry
    assert!(
        e.restore_projection(&tampered, Adjacency).is_err(),
        "a boundary claiming fewer entries than the array carries must be refused"
    );
}

/// An entry's declared length, and an entry's output bytes.
///
/// Both are `reject.cases` in the fixture now; this asserts them against the
/// live engine as well, because the fixture's own test resolves them through
/// the same code path and a second angle is cheap.
#[test]
fn tampering_an_entry_is_refused() {
    let (e, c) = history();
    let wire = e
        .project(c)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .canonical_bytes();

    // The first entry's u32 output_len sits at 47 + 16.
    let mut bad_len = wire.clone();
    bad_len[63] = bad_len[63].wrapping_add(1);
    assert!(
        e.restore_projection(&bad_len, Adjacency).is_err(),
        "a declared length that does not match the output must be refused"
    );

    // An output payload byte: well-formed framing, wrong value.
    let mut bad_payload = wire.clone();
    let last = bad_payload.len() - 1;
    bad_payload[last] ^= 0xFF;
    assert!(
        e.restore_projection(&bad_payload, Adjacency).is_err(),
        "a per-node value this engine would not produce must be refused"
    );
}

/// `RestoreError::IntegerOverflow` cannot fire on this target, by arithmetic.
///
/// TST-4 asks that every public error variant be constructed by a test. This
/// one cannot be, and the reason is not difficulty: both construction sites
/// are `usize::try_from(u64)`, which is infallible wherever `usize` is 64
/// bits. The variant exists for wasm32, where `usize` is narrower than the
/// wire's `u64` length fields — so the test that would reach it is a wasm
/// one, and the wasm suite is deferred with the bindings (CONF-7, CONF-12).
///
/// Asserted here as a statement rather than left as a gap: the conversion is
/// infallible on this target, so if this assertion ever fails, `usize` got
/// narrower and the variant became reachable.
#[test]
fn integer_overflow_is_unreachable_where_usize_is_64_bits() {
    assert_eq!(usize::BITS, 64, "this test's premise is the target's pointer width");
    assert!(
        usize::try_from(u64::MAX).is_ok(),
        "the conversion both IntegerOverflow sites guard is infallible here"
    );
}
