//! wasm-bindgen-test smoke suite for `src/wasm.rs`.
//!
//! Run via `wasm-pack test --node --features wasm`. Gated on
//! `target_arch = "wasm32"` AND `feature = "wasm"` so the default
//! `cargo test --workspace` path never picks it up.

#![cfg(all(target_arch = "wasm32", feature = "wasm"))]

use koru_lambda_core::wasm::{id_from_hex, id_to_hex, WasmEngine};
use wasm_bindgen::JsValue;
use wasm_bindgen_test::wasm_bindgen_test;

// Default runner is Node.js — `wasm-pack test --node` sets that up. No
// `wasm_bindgen_test_configure!(run_in_node)` call needed (that macro
// arm doesn't exist; only `run_in_browser` requires the opt-in).

// -------------------------------------------------------------------
// Basic API smoke
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn primordials_are_16_bytes_and_distinct() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let d1 = e.d1();
    assert_eq!(d0.len(), 16);
    assert_eq!(d1.len(), 16);
    assert_ne!(d0, d1);
    // d0 == [0; 16]; d1 == [1, 0, 0, ...]
    assert_eq!(&d0[..], &[0u8; 16][..]);
    assert_eq!(d1[0], 1);
    for byte in &d1[1..] {
        assert_eq!(*byte, 0);
    }
}

#[wasm_bindgen_test]
fn synthesize_verify_has_degree_round_trip() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let d1 = e.d1();
    let child = e.synthesize(&d0, &d1).expect("synthesize primordials");
    assert_eq!(child.len(), 16);
    assert!(e.has(&child).expect("has(child)"));
    // Verify is the trust boundary — child bytes echo back.
    let echoed = e.verify(&child).expect("verify child");
    assert_eq!(echoed, child);
    // Substrate: primordials are +1 (genesis) plus each parent bump.
    // d0 participated in 1 novel synthesis → degree = 0 (fetch_add count)
    // + 1 (genesis) + parent bump of 1 = 2.
    assert!(e.degree(&d0).expect("degree(d0)") >= 1);
    // Counts.
    assert_eq!(e.distinction_count(), 3);
    assert_eq!(e.relationship_count(), 3); // (3-2)*2 + 1 = 3
}

// -------------------------------------------------------------------
// Axiom-4 falsifier — the load-bearing Logic Enforcer probe.
//
// Bytes from engineA must NOT be accepted by engineB (both engines have
// synthesized the same child, but the substrate's identity is
// engine-witnessed, not byte-inherent — Cond D disclosure). We check
// the negative path: engineB gets a fresh identity FROM engineA that
// engineB has never witnessed.
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn axiom4_foreign_bytes_rejected_by_synthesize() {
    let a = WasmEngine::new();
    let b = WasmEngine::new();

    // Generate two levels in engine A so `x2` is witnessed by A but
    // never by B. (`synthesize(d0, d1)` produces identical child bytes
    // in both engines by content addressing — we need a distinction
    // that only A has synthesized.)
    let x1 = a.synthesize(&a.d0(), &a.d1()).expect("A: x1");
    let x2 = a.synthesize(&x1, &a.d0()).expect("A: x2");
    assert!(!b.has(&x2).expect("B has(x2)")); // engineB has not seen x2

    // synthesize on B with A's identity must throw VerifyError.
    let err = b.synthesize(&x2, &b.d1()).expect_err("engineB.synthesize(foreign, d1) must reject");
    // JsError renders as `Error: VerifyError on a: ...`; the presence
    // of the Err branch IS the assertion. We accept it via unwrap_err.
    let _: JsValue = err.into();
}

#[wasm_bindgen_test]
fn axiom4_foreign_bytes_rejected_by_verify() {
    let a = WasmEngine::new();
    let b = WasmEngine::new();
    let x1 = a.synthesize(&a.d0(), &a.d1()).expect("A: x1");
    let x2 = a.synthesize(&x1, &a.d0()).expect("A: x2");
    let err = b.verify(&x2).expect_err("engineB.verify(foreign) must reject");
    let _: JsValue = err.into();
}

// -------------------------------------------------------------------
// synthesizeNovel returns TS-narrowable POJO
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn synthesize_novel_returns_kind_discriminant() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let d1 = e.d1();

    let first = e.synthesize_novel(&d0, &d1).expect("first synthesize_novel");
    // First observation → kind: "novel"
    let kind = js_sys::Reflect::get(&first, &JsValue::from_str("kind"))
        .expect("read kind")
        .as_string()
        .expect("kind is string");
    assert_eq!(kind, "novel");
    let distinction =
        js_sys::Reflect::get(&first, &JsValue::from_str("distinction")).expect("read distinction");
    // Uint8Array — check by roundtripping through js_sys::Uint8Array.
    let arr = js_sys::Uint8Array::from(distinction);
    assert_eq!(arr.length(), 16);

    // Second synthesis of the same (d0, d1) — Existing.
    let second = e.synthesize_novel(&d0, &d1).expect("second synthesize_novel");
    let kind2 = js_sys::Reflect::get(&second, &JsValue::from_str("kind"))
        .expect("read kind")
        .as_string()
        .expect("kind is string");
    assert_eq!(kind2, "existing");
}

// -------------------------------------------------------------------
// canonicalBytes() and projectionId() are non-empty, well-shaped, and
// deterministic across engines with identical synthesis history
// (Cond B / Cond D content-addressing testable at the JS boundary).
//
// Note: the ORIGINAL R9 plan text said `sha256(canonicalBytes()) ===
// projectionId()` — that literal equivalence is FALSE against the
// substrate (projection_id hashes only the spec header; canonical_bytes
// serializes header + entries with a different domain tag). This test
// verifies the invariants that DO hold and feed S04's Cond D′ falsifier:
// both are deterministic and well-shaped across engines.
// -------------------------------------------------------------------

fn build_two_isomorphic_engines() -> (WasmEngine, WasmEngine, Vec<u8>) {
    // Two engines that synthesize the same history should produce
    // byte-identical projections.
    let a = WasmEngine::new();
    let b = WasmEngine::new();
    let child_a = a.synthesize(&a.d0(), &a.d1()).expect("A.synth");
    let child_b = b.synthesize(&b.d0(), &b.d1()).expect("B.synth");
    assert_eq!(child_a, child_b, "Axiom 4 — content addressing");
    let _ = a.synthesize(&child_a, &a.d0()).expect("A.synth deeper");
    let _ = b.synthesize(&child_b, &b.d0()).expect("B.synth deeper");
    (a, b, child_a)
}

#[wasm_bindgen_test]
fn adjacency_projection_id_and_canonical_bytes_deterministic() {
    let (a, b, root) = build_two_isomorphic_engines();
    let pa = a.project_adjacency(&root, "upstream", Some(2)).expect("A adjacency");
    let pb = b.project_adjacency(&root, "upstream", Some(2)).expect("B adjacency");
    assert_eq!(pa.canonical_bytes(), pb.canonical_bytes());
    assert_eq!(pa.projection_id(), pb.projection_id());
    assert_eq!(pa.projection_id().len(), 32);
    assert!(!pa.canonical_bytes().is_empty());
}

#[wasm_bindgen_test]
fn degree_projection_id_and_canonical_bytes_deterministic() {
    let (a, b, root) = build_two_isomorphic_engines();
    let pa = a.project_degree(&root, "upstream", Some(2)).expect("A degree");
    let pb = b.project_degree(&root, "upstream", Some(2)).expect("B degree");
    assert_eq!(pa.canonical_bytes(), pb.canonical_bytes());
    assert_eq!(pa.projection_id(), pb.projection_id());
    assert_eq!(pa.projection_id().len(), 32);
}

#[wasm_bindgen_test]
fn hop_distance_projection_id_and_canonical_bytes_deterministic() {
    let (a, b, root) = build_two_isomorphic_engines();
    let pa = a.project_hop_distance(&root, "upstream", Some(2)).expect("A hops");
    let pb = b.project_hop_distance(&root, "upstream", Some(2)).expect("B hops");
    assert_eq!(pa.canonical_bytes(), pb.canonical_bytes());
    assert_eq!(pa.projection_id(), pb.projection_id());
    assert_eq!(pa.projection_id().len(), 32);
}

// -------------------------------------------------------------------
// Projection round-trip via restore_projection_*
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn adjacency_restore_round_trip() {
    let e = WasmEngine::new();
    let child = e.synthesize(&e.d0(), &e.d1()).expect("synth");
    let proj = e.project_adjacency(&child, "upstream", Some(2)).expect("project adjacency");
    let bytes = proj.canonical_bytes();
    let restored = e.restore_projection_adjacency(&bytes).expect("restore adjacency");
    assert_eq!(restored.canonical_bytes(), bytes);
    assert_eq!(restored.projection_id(), proj.projection_id());
}

// -------------------------------------------------------------------
// contains() consistency
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn adjacency_contains_matches_engine_membership() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let d1 = e.d1();
    let child = e.synthesize(&d0, &d1).expect("synth");
    let proj = e.project_adjacency(&child, "upstream", None).expect("saturated project");
    // Root is always in the cone.
    assert!(proj.contains(&child).expect("contains child"));
    // Upstream from child includes both parents.
    assert!(proj.contains(&d0).expect("contains d0"));
    assert!(proj.contains(&d1).expect("contains d1"));
    // A foreign 16-byte value the engine has never seen.
    let foreign = vec![0xCCu8; 16];
    assert!(!proj.contains(&foreign).expect("contains foreign"));
}

// -------------------------------------------------------------------
// Free-function hex round-trip
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn id_to_hex_from_hex_round_trip() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let hex_str = id_to_hex(&d0).expect("id_to_hex(d0)");
    assert_eq!(hex_str, "00000000000000000000000000000000");
    let back = id_from_hex(&hex_str).expect("id_from_hex");
    assert_eq!(back, d0);
}

#[wasm_bindgen_test]
fn id_to_hex_rejects_wrong_length() {
    let short = vec![0u8; 15];
    let err = id_to_hex(&short).expect_err("wrong length rejected");
    let _: JsValue = err.into();
}
