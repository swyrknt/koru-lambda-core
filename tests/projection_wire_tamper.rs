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

use koru_lambda_core::projection::{
    Adjacency, Boundary, CanonicalBytes, Direction, Signal, SignalContext,
};
use koru_lambda_core::{Distinction, DistinctionEngine};

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

    // The header is **55** bytes, not 47: §7 puts the 8-byte entry count
    // after the signal identity. So the first entry's identifier spans
    // 55..71 and its u32 output_len 71..75.
    //
    // This test and the fixture case both used 63, with a comment reading
    // "47 + 16" — which is inside the identifier, not the length field. Both
    // rejected, so both looked right, and the entry array's own length field
    // had nothing probing it in either implementation. The fixture case is
    // renamed `tampered_entry_identifier` and a real length probe added
    // beside it; both offsets are asserted here.
    let mut bad_id = wire.clone();
    bad_id[63] = bad_id[63].wrapping_add(1);
    assert!(
        e.restore_projection(&bad_id, Adjacency).is_err(),
        "an entry naming a distinction this engine would not produce must be refused"
    );

    let mut bad_len = wire.clone();
    bad_len[71] = bad_len[71].wrapping_add(1);
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

// -------------------------------------------------------------------------
// PRJ-1 — a consumer's encoder cannot reach bytes it does not own
// -------------------------------------------------------------------------

/// An output type whose encoder is hostile to the buffer it is given.
///
/// `CanonicalBytes::write_canonical` takes `&mut Vec<u8>`, so the obvious
/// question about the signature is what happens when an implementation
/// treats that buffer as its own. This one clears it first and writes
/// trailing padding after, which is the shape that would corrupt a frame:
/// given the wire buffer, the `clear` would discard the header and every
/// entry already written, and the entry length — computed as a delta — would
/// underflow.
struct Greedy(usize);

impl CanonicalBytes for Greedy {
    fn write_canonical(&self, out: &mut Vec<u8>) {
        out.clear(); // would destroy the header and all prior entries
        out.extend_from_slice(&(self.0 as u64).to_le_bytes());
        out.extend_from_slice(&[0xAA; 4]); // and then overrun
    }
}

#[derive(Clone)]
struct GreedySignal;

impl Signal for GreedySignal {
    type Output = Greedy;

    fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> Greedy {
        Greedy(ctx.engine.degree(node))
    }

    fn canonical_bytes(&self) -> &[u8] {
        b"test:greedy"
    }
}

/// The frame survives it, because what the encoder receives is scratch.
///
/// Every entry is present and correctly framed, the header is intact, and
/// the projection restores. The hostile `clear` reached only the twelve
/// bytes this entry had written itself — which it is entitled to, since it
/// decides its own encoding anyway.
///
/// This is the invariant the signature's safety rests on. If
/// `try_canonical_bytes` is ever changed to pass `out` directly — the
/// obvious "optimization", worth one `memcpy` per entry — this test fails
/// instead of the corruption being discovered on a wire.
#[test]
fn a_hostile_encoder_cannot_reach_bytes_it_did_not_write() {
    let (e, c) = history();
    let proj = e
        .project(c)
        .direction(Direction::Undirected)
        .saturated()
        .signal(GreedySignal)
        .materialize();

    let wire = proj.canonical_bytes();
    let entries = proj.output().iter().count();

    assert_eq!(&wire[..4], b"KPRJ", "the header survived an encoder that cleared its buffer");
    assert_eq!(wire[4], 0x02, "and so did the version byte after it");
    assert_eq!(&wire[5..21], c.as_bytes(), "and the root");

    // 55-byte header (§7), then per entry: 16 id + 4 len + 12 payload.
    assert_eq!(
        wire.len(),
        55 + entries * (16 + 4 + 12),
        "every entry is framed at its own declared length"
    );

    // And the declared length is this entry's twelve bytes, not a delta
    // against a buffer the encoder had emptied.
    let first_len = u32::from_le_bytes(wire[71..75].try_into().unwrap());
    assert_eq!(first_len, 12, "the length is what the encoder wrote, measured in isolation");

    let restored = e
        .restore_projection(&wire, GreedySignal)
        .expect("a frame written through scratch buffers round-trips");
    assert_eq!(restored.canonical_bytes(), wire);
}

/// `canonical_bytes` is the provided method over the same encoding.
///
/// It exists for callers who want the bytes owned; nothing on the wire
/// consults it. Asserted so the two cannot drift apart silently.
#[test]
fn the_provided_method_agrees_with_the_required_one() {
    for v in [0usize, 1, 255, 256, usize::MAX] {
        let mut written = Vec::new();
        v.write_canonical(&mut written);
        assert_eq!(written, v.canonical_bytes(), "usize");
    }
    for v in [true, false] {
        let mut written = Vec::new();
        v.write_canonical(&mut written);
        assert_eq!(written, v.canonical_bytes(), "bool");
    }

    let (e, c) = history();
    for v in [None, e.parents_of(c)] {
        let mut written = Vec::new();
        v.write_canonical(&mut written);
        assert_eq!(written, v.canonical_bytes(), "Option<ParentPair>");
    }
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
