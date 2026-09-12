//! `trybuild` compile-fail harness for E02-S02 discipline probes.
//!
//! Currently one probe: `dyn Signal` must not be object-safe. See
//! `tests/trybuild/dyn_signal_unsafe.rs` for the compile-fail source
//! and PROJECTION_SPEC §3.1 for the design rationale.

#[test]
fn dyn_signal_is_object_unsafe() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/dyn_signal_unsafe.rs");
}
