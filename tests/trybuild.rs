//! `trybuild` compile-fail harness for E02-S02 discipline probes.
//!
//! Currently one probe: `dyn Signal` must not be object-safe. See
//! `tests/trybuild/dyn_signal_unsafe.rs` for the compile-fail source
//! and PROJECTION_SPEC §3.1 for the design rationale.

// Native-only: `trybuild` is not wasm32-compatible (spawns rustc as a
// subprocess). Gated so `wasm-pack test` compiles without dragging its
// build tree onto wasm.
#![cfg(not(target_arch = "wasm32"))]

#[test]
fn dyn_signal_is_object_unsafe() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/trybuild/dyn_signal_unsafe.rs");
}
