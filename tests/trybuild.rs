//! `trybuild` compile-fail harness for v2.0.0 projection API discipline
//! probes.
//!
//! Two probes. `dyn Signal` must not be object-safe — see
//! `tests/trybuild/dyn_signal_unsafe.rs` and `docs/SPEC.md` §6. And a
//! `Distinction` must not be constructible without an engine, which is the
//! trust boundary stated as the absence of four removed paths.

// Native-only: `trybuild` is not wasm32-compatible (spawns rustc as a
// subprocess). Gated so `wasm-pack test` compiles without dragging its
// build tree onto wasm.
#![cfg(not(target_arch = "wasm32"))]

#[test]
fn dyn_signal_is_object_unsafe() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/dyn_signal_unsafe.rs");
}

/// The trust boundary, asserted as the absence of four paths.
///
/// `Distinction::from_hex`, `FromStr`, `Pod` and `Zeroable` all existed and
/// all produced a distinction without an engine. A runtime test cannot assert
/// that a trait is *not* implemented, so the assertion is a compile failure.
#[test]
fn a_distinction_cannot_be_made_without_an_engine() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/distinction_has_no_hatches.rs");
}
