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

/// A batch lift over borrows compiles. TST-7's pass case.
///
/// The trait defect it guards was expressible rather than runtime: with
/// `to_canonical_structure(self, &DistinctionEngine)` the loop in
/// `tests/trybuild/batch_lift_compiles.rs` does not compile, because
/// `for record in &batch` yields `&Record` and a by-value method needs a
/// clone. The only real consumer stopped using the trait for exactly that
/// reason, and nothing caught it — compile-fail probes assert the absence of
/// paths, and no probe asserted the presence of this one.
#[test]
fn a_batch_lift_over_borrows_compiles() {
    let t = trybuild::TestCases::new();
    t.pass("tests/trybuild/batch_lift_compiles.rs");
}
