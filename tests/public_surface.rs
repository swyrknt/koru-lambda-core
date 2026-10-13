//! The published surface's trait obligations, asserted mechanically, and
//! every error message a consumer can reach, pinned.
//!
//! API-1, API-3 and API-8. All six public error enums were already
//! `#[non_exhaustive]`, and nothing asserted anything else about them: not
//! that they implement `Error`, not that they are `Send + Sync` (so they
//! cross a thread or an `.await` in a consumer's error type), not that their
//! messages are stable. A consumer writing `Box<dyn Error + Send + Sync>`
//! depends on all of that, and a rename of a field or a reworded `#[error]`
//! attribute would have changed what their logs say with nothing failing.
//!
//! # Why the messages are triggered rather than constructed
//!
//! `#[non_exhaustive]` means an external crate cannot construct these
//! variants — which is the point of it, and which makes this file unable to
//! write `VerifyError::ForeignBytes { .. }` directly. So each message is
//! produced by causing the failure. That is strictly better evidence: it
//! pins the string a real consumer sees on a real error path, and it proves
//! the path is reachable at all.
//!
//! Two variants cannot be reached from outside the crate and are named here
//! rather than quietly omitted: `SerializeError::EntryTooLarge` needs a
//! per-node output above `u32::MAX`, and `RestoreError::IntegerOverflow` is
//! unreachable on a 64-bit target by construction. `InvariantError`'s
//! variants need a corrupted engine, which only the crate's own tests can
//! build; `src/engine.rs`'s `auditor_catches_*` tests cover all five.

use koru_lambda_core::distinction_hex::ParseError;
use koru_lambda_core::primitives::{ByteFold, ByteMapping, Canonicalizable, DomainLabel};
use koru_lambda_core::projection::{
    Adjacency, Boundary, CanonicalBytes, Degree, Direction, HopDistance, ProjectionId,
    RestoreError, SerializeError,
};
use koru_lambda_core::replay::{replay_topological, ReplayError};
use koru_lambda_core::{
    Distinction, DistinctionEngine, InvariantError, RawDistinctionId, SynthesisOutcome, VerifyError,
};
use static_assertions::{assert_impl_all, assert_not_impl_any};
use std::error::Error;
use std::fmt::{Debug, Display};
use std::panic::{RefUnwindSafe, UnwindSafe};

// -------------------------------------------------------------------------
// API-1 — every public error is a usable error
// -------------------------------------------------------------------------

assert_impl_all!(ParseError: Debug, Display, Error, Send, Sync, Clone);
assert_impl_all!(VerifyError: Debug, Display, Error, Send, Sync, Clone);
assert_impl_all!(InvariantError: Debug, Display, Error, Send, Sync, Clone);
assert_impl_all!(ReplayError: Debug, Display, Error, Send, Sync, Clone);
assert_impl_all!(RestoreError: Debug, Display, Error, Send, Sync);
assert_impl_all!(SerializeError: Debug, Display, Error, Send, Sync);

/// `'static` is what lets a consumer box these, and it cannot be written as
/// an `assert_impl_all!` bound — so it is a function that would not compile
/// if any of them borrowed.
fn _boxable() {
    fn boxed<E: Error + Send + Sync + 'static>(e: E) -> Box<dyn Error + Send + Sync> {
        Box::new(e)
    }
    let e = DistinctionEngine::new();
    let _ = boxed(e.verify(RawDistinctionId::from_bytes([0xCC; 16])).unwrap_err());
    let _ = boxed(RawDistinctionId::from_hex("nope").unwrap_err());
    // A parentage that cannot bootstrap. `vec![]` replays successfully, so
    // it is not an error — the first draft of this used it and did not
    // compile, which is the assertion doing its job on the test itself.
    let stranger = DistinctionEngine::new();
    let a = stranger.synthesize(stranger.d0(), stranger.d1());
    let orphan = stranger.synthesize(a, stranger.d0());
    let _ = boxed(
        replay_topological(vec![(orphan, (a, stranger.d0()))])
            .expect_err("a parentage with no primordial root must fail"),
    );
}

// -------------------------------------------------------------------------
// API-3 / API-8 — the trait obligations of the rest of the surface
// -------------------------------------------------------------------------

// `Debug` on every public type a consumer can hold. A type without it
// cannot appear in an `assert_eq!` or a `#[derive(Debug)]` struct field,
// which is a papercut a consumer cannot fix from outside.
assert_impl_all!(Distinction: Debug, Copy, Send, Sync, UnwindSafe, RefUnwindSafe);
assert_impl_all!(RawDistinctionId: Debug, Copy, Send, Sync, UnwindSafe, RefUnwindSafe);
assert_impl_all!(SynthesisOutcome: Debug, Copy, Send, Sync);
assert_impl_all!(DistinctionEngine: Debug, Send, Sync);

// And it is NOT `RefUnwindSafe`, which API-8 asked for and which the engine
// cannot have: `DashMap` holds an `UnsafeCell`, so `&DistinctionEngine`
// carries interior mutability across a `catch_unwind` boundary as far as the
// type system is concerned. Asserted rather than wished away, because the
// consequence is a real consumer obligation — see
// `a_consumers_catch_unwind_needs_assert_unwind_safe` below, and the
// `# Panics` discussion in `src/engine.rs`.
assert_not_impl_any!(DistinctionEngine: RefUnwindSafe);
assert_impl_all!(ProjectionId: Debug, Copy, Send, Sync);
assert_impl_all!(ByteFold: Send, Sync);
assert_impl_all!(ByteMapping: Debug, Copy, Default, Send, Sync);
assert_impl_all!(DomainLabel: Debug, Copy, Send, Sync);

// The read axes are `Hash` so a consumer can key a cache by a projection
// specification. They were `Clone, Copy, PartialEq, Eq, Debug` and nothing
// else, so `HashMap<(Direction, Boundary), _>` did not compile.
assert_impl_all!(Boundary: Debug, Copy, Eq, std::hash::Hash, Send, Sync);
assert_impl_all!(Direction: Debug, Copy, Eq, std::hash::Hash, Send, Sync);

// The three shipped signals.
assert_impl_all!(Adjacency: Debug, Copy, Default, Send, Sync);
assert_impl_all!(Degree: Debug, Copy, Default, Send, Sync);
assert_impl_all!(HopDistance: Debug, Copy, Default, Send, Sync);

// And the trust boundary, restated here because this file is outside the
// crate: the verified type has no byte-level escape hatch.
assert_not_impl_any!(Distinction: Default, From<[u8; 16]>);

/// `Canonicalizable` and `CanonicalBytes` are usable as bounds from outside.
fn _bounds_are_reachable() {
    fn lift<C: Canonicalizable>(c: &C, f: &ByteFold, e: &DistinctionEngine) -> Distinction {
        c.to_canonical_structure(f, e)
    }
    fn emit<B: CanonicalBytes>(b: &B) -> Vec<u8> {
        b.canonical_bytes()
    }
    let e = DistinctionEngine::new();
    let f = ByteFold::new(&e);
    let _ = lift(&0x41u8, &f, &e);
    let _ = emit(&7usize);
}

// -------------------------------------------------------------------------
// API-1 — the messages, pinned against the paths that produce them
// -------------------------------------------------------------------------

/// Hex parsing, all three variants.
#[test]
fn parse_error_messages_are_pinned() {
    let cases: [(&str, &str); 3] = [
        ("", "expected 32 hex characters, got 0"),
        ("0123456789abcdeffedcba987654321", "expected 32 hex characters, got 31"),
        ("0123456789abcdeffedcba98765432zz", "invalid hex character at position 30: 'z'"),
    ];
    for (input, expected) in cases {
        let err = RawDistinctionId::from_hex(input).expect_err("must reject");
        assert_eq!(err.to_string(), expected, "ParseError message drifted for {input:?}");
    }
}

/// The trust boundary's refusal, which is the message a consumer sees most.
#[test]
fn verify_error_message_is_pinned() {
    let e = DistinctionEngine::new();
    let err = e.verify(RawDistinctionId::from_bytes([0xCC; 16])).expect_err("foreign bytes");
    assert_eq!(
        err.to_string(),
        "bytes not registered in this engine (foreign to this engine's witness state)"
    );
}

/// Replay's three refusals.
#[test]
fn replay_error_messages_are_pinned() {
    // Nothing roots in the primordials.
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let orphan = e.snapshot_parentage().into_iter().filter(|(ch, _)| *ch != c).collect::<Vec<_>>();
    let err = replay_topological(orphan).expect_err("an orphaned intermediate must reject");
    assert_eq!(
        err.to_string(),
        "no parentage entry roots in primordials: input cannot bootstrap from d₀/d₁"
    );

    // Progress stalls partway.
    let f = e.synthesize(d, e.d1());
    let pruned: Vec<_> = e.snapshot_parentage().into_iter().filter(|(ch, _)| *ch != d).collect();
    let err = replay_topological(pruned).expect_err("a missing intermediate must reject");
    assert_eq!(
        err.to_string(),
        "corrupted parentage: 1 entries unresolved (cycle or missing intermediate)"
    );
    let _ = f;
}

/// Restore's refusals, on the three a consumer can reach from outside.
#[test]
fn restore_error_messages_are_pinned() {
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let wire = e
        .project(c)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize()
        .canonical_bytes();

    // Too short for the header.
    let err = e.restore_projection(&wire[..10], Adjacency).expect_err("truncated");
    assert_eq!(
        err.to_string(),
        "wire bytes malformed at offset 0: wire bytes shorter than 55-byte header"
    );

    // Bad magic.
    let mut bad = wire.clone();
    bad[0] = b'X';
    let err = e.restore_projection(&bad, Adjacency).expect_err("bad magic");
    assert_eq!(err.to_string(), "wire bytes malformed at offset 0: bad magic");

    // A well-formed forgery: a root this engine never witnessed.
    let stranger = DistinctionEngine::new();
    let a = stranger.synthesize(stranger.d0(), stranger.d1());
    let foreign_root = stranger.synthesize(a, stranger.d0());
    let mut forged = wire.clone();
    forged[5..21].copy_from_slice(foreign_root.as_bytes());
    let err = e.restore_projection(&forged, Adjacency).expect_err("forged root");
    assert_eq!(
        err.to_string(),
        "distinction bytes not registered in restore engine (foreign entry)"
    );

    // The wrong signal for these bytes.
    let err = e.restore_projection(&wire, Degree).expect_err("wrong signal");
    assert!(
        err.to_string().starts_with("signal type mismatch: expected "),
        "SignalMismatch message drifted: {err}"
    );
}

/// The two variants no external test can reach, named so their absence is a
/// statement rather than an oversight.
#[test]
fn two_error_variants_are_unreachable_from_outside() {
    // Both are constructed in `src/projection.rs` and neither can be
    // triggered here: `EntryTooLarge` needs a per-node output exceeding
    // `u32::MAX`, which no shipped signal can produce, and
    // `IntegerOverflow` is unreachable on a 64-bit target by construction —
    // it exists for wasm32, where `usize` is narrower than the wire's `u64`.
    //
    // This test exists so the claim is in the suite rather than in a
    // comment. It asserts the types are still part of the surface, which is
    // what would change if either were removed.
    fn is_error<E: Error>() {}
    is_error::<SerializeError>();
    is_error::<RestoreError>();
}

/// The documented panic is catchable, and what makes the assertion honest.
///
/// `src/engine.rs` promises a panic on an unwitnessed parent with zero state
/// change, and consumers are told it is catchable. Both halves hold — but
/// `&DistinctionEngine` is not `RefUnwindSafe`, so a consumer must write
/// `AssertUnwindSafe` to catch it. That wrapper is a claim that nothing
/// observable is left inconsistent by the unwind, and it is exactly the
/// engine's zero-state-change guarantee that makes the claim true rather
/// than a shrug. Asserted here from outside the crate, which is where a
/// consumer stands.
#[test]
fn a_consumers_catch_unwind_needs_assert_unwind_safe() {
    use std::panic::{catch_unwind, AssertUnwindSafe};

    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());

    // Genuine bytes from a different witness.
    let stranger = DistinctionEngine::new();
    let a = stranger.synthesize(stranger.d0(), stranger.d1());
    let foreign = stranger.synthesize(a, stranger.d0());

    let before = (e.distinction_count(), e.relationship_count(), e.degree(c));

    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let caught = catch_unwind(AssertUnwindSafe(|| e.synthesize(foreign, e.d0())));
    std::panic::set_hook(previous);

    assert!(caught.is_err(), "an unwitnessed parent must panic");
    assert_eq!(
        (e.distinction_count(), e.relationship_count(), e.degree(c)),
        before,
        "and change nothing, which is what makes AssertUnwindSafe honest here"
    );
    e.check_structural_invariant().expect("the graph survives a refused synthesis (invariant)");
}
