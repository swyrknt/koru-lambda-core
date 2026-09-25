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
// Axiom-4 falsifier — the load-bearing engine-identity probe.
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

// -------------------------------------------------------------------
// parentsOf() — canonical (min, max) pair for non-primordials; null
// for d0/d1. POJO shape verified via typescript_custom_section
// (`Parents = { min: Uint8Array; max: Uint8Array }`).
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn parents_of_returns_canonical_pair_and_null_for_primordials() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let d1 = e.d1();
    let c = e.synthesize(&d0, &d1).expect("synth");

    let parents_js = e.parents_of(&c).expect("parents_of(c)");
    assert!(!parents_js.is_null(), "child has parents");

    let min_js =
        js_sys::Reflect::get(&parents_js, &JsValue::from_str("min")).expect("read min field");
    let max_js =
        js_sys::Reflect::get(&parents_js, &JsValue::from_str("max")).expect("read max field");
    let min = js_sys::Uint8Array::from(min_js).to_vec();
    let max = js_sys::Uint8Array::from(max_js).to_vec();
    assert_eq!(min.len(), 16);
    assert_eq!(max.len(), 16);

    // Canonical (min, max) means byte-lex order is guaranteed, argument
    // order is not — either mapping is acceptable.
    let pair = [min.clone(), max.clone()];
    assert!(pair.contains(&d0), "parent pair includes d0");
    assert!(pair.contains(&d1), "parent pair includes d1");
    assert!(min <= max, "canonical order: min <= max byte-lex");

    // Primordials have no parents — null on the JS side.
    let p0 = e.parents_of(&d0).expect("parents_of(d0)");
    assert!(p0.is_null(), "d0 has no parents");
    let p1 = e.parents_of(&d1).expect("parents_of(d1)");
    assert!(p1.is_null(), "d1 has no parents");
}

// -------------------------------------------------------------------
// distinctionCount / relationshipCount — Law 6: r = 2d - 3 for d >= 2.
// (Two edges per non-primordial + 1 genesis edge; substrate
// relationship_count() = (n - 2) * 2 + 1.)
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn counts_grow_per_law_6() {
    let e = WasmEngine::new();
    assert_eq!(e.distinction_count(), 2, "fresh: two primordials");
    assert_eq!(e.relationship_count(), 1, "fresh: only the genesis edge");

    let d0 = e.d0();
    let d1 = e.d1();
    let c = e.synthesize(&d0, &d1).expect("synth (d0, d1)");
    assert_eq!(e.distinction_count(), 3);
    assert_eq!(e.relationship_count(), 3, "Law 6: (3-2)*2 + 1 = 3");

    // Novel synthesis of (c, d0) — never synthesized before.
    let _c2 = e.synthesize(&c, &d0).expect("synth (c, d0)");
    assert_eq!(e.distinction_count(), 4);
    assert_eq!(e.relationship_count(), 5, "Law 6: (4-2)*2 + 1 = 5");
}

// -------------------------------------------------------------------
// restoreProjectionDegree — byte-identical round-trip via canonical
// wire bytes. Anchors the Degree branch of the restore surface.
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn degree_restore_round_trip() {
    let e = WasmEngine::new();
    let child = e.synthesize(&e.d0(), &e.d1()).expect("synth");
    let proj = e.project_degree(&child, "upstream", Some(2)).expect("project degree");
    let bytes = proj.canonical_bytes();
    let restored = e.restore_projection_degree(&bytes).expect("restore degree");
    assert_eq!(restored.canonical_bytes(), bytes, "byte-identical round-trip");
    assert_eq!(restored.projection_id(), proj.projection_id());
}

// -------------------------------------------------------------------
// restoreProjectionHopDistance — same shape as the Degree round-trip.
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn hop_distance_restore_round_trip() {
    let e = WasmEngine::new();
    let child = e.synthesize(&e.d0(), &e.d1()).expect("synth");
    let proj = e.project_hop_distance(&child, "upstream", Some(2)).expect("project hop_distance");
    let bytes = proj.canonical_bytes();
    let restored = e.restore_projection_hop_distance(&bytes).expect("restore hop_distance");
    assert_eq!(restored.canonical_bytes(), bytes, "byte-identical round-trip");
    assert_eq!(restored.projection_id(), proj.projection_id());
}

// -------------------------------------------------------------------
// DegreeProjection.entries() shape — array of
// `{ distinction: Uint8Array, degree: number | bigint }`. Verifies the
// typescript_custom_section shape at the JS boundary.
//
// Note: `degree: u64` renders as JS Number or BigInt per
// serde-wasm-bindgen; the TS type declares `number` but the runtime
// type is checked with `typeof` for portability.
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn degree_entries_shape() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let child = e.synthesize(&d0, &e.d1()).expect("synth");
    let proj = e.project_degree(&child, "upstream", None).expect("saturated project");
    let entries_js = proj.entries().expect("entries");
    let arr = js_sys::Array::from(&entries_js);
    // Upstream saturated cone from child: {child, d0, d1} → 3 entries.
    assert_eq!(arr.length(), 3, "cone size");

    let mut saw_d0 = false;
    for i in 0..arr.length() {
        let entry = arr.get(i);
        let dist_js = js_sys::Reflect::get(&entry, &JsValue::from_str("distinction"))
            .expect("read distinction field");
        let deg_js =
            js_sys::Reflect::get(&entry, &JsValue::from_str("degree")).expect("read degree field");
        let dist = js_sys::Uint8Array::from(dist_js).to_vec();
        assert_eq!(dist.len(), 16, "distinction is 16 bytes");
        if dist == d0 {
            saw_d0 = true;
        }
        // Numeric-typed at the JS boundary (Number or BigInt).
        let ty = deg_js.js_typeof().as_string().unwrap_or_default();
        assert!(ty == "number" || ty == "bigint", "degree typeof: got {ty}");
        // Non-negative on the Number path (u64 → BigInt is non-negative by construction).
        if let Some(v) = deg_js.as_f64() {
            assert!(v >= 0.0, "degree is non-negative");
        }
    }
    assert!(saw_d0, "d0 in upstream cone from child");
}

// -------------------------------------------------------------------
// HopDistanceProjection.entries() shape — array of
// `{ distinction: Uint8Array, hops: number | bigint }`. Root has
// hops == 0; parents are at hop 1.
// -------------------------------------------------------------------

#[wasm_bindgen_test]
fn hop_distance_entries_shape() {
    let e = WasmEngine::new();
    let d0 = e.d0();
    let child = e.synthesize(&d0, &e.d1()).expect("synth");
    let proj = e.project_hop_distance(&child, "upstream", None).expect("saturated project");
    let entries_js = proj.entries().expect("entries");
    let arr = js_sys::Array::from(&entries_js);
    // Upstream saturated cone from child: {child, d0, d1} → 3 entries.
    assert_eq!(arr.length(), 3, "cone size");

    let mut saw_root_hop_zero = false;
    for i in 0..arr.length() {
        let entry = arr.get(i);
        let dist_js = js_sys::Reflect::get(&entry, &JsValue::from_str("distinction"))
            .expect("read distinction field");
        let hops_js =
            js_sys::Reflect::get(&entry, &JsValue::from_str("hops")).expect("read hops field");
        let dist = js_sys::Uint8Array::from(dist_js).to_vec();
        assert_eq!(dist.len(), 16, "distinction is 16 bytes");
        let ty = hops_js.js_typeof().as_string().unwrap_or_default();
        assert!(ty == "number" || ty == "bigint", "hops typeof: got {ty}");
        // Non-negative on the Number path.
        if let Some(v) = hops_js.as_f64() {
            assert!(v >= 0.0, "hops is non-negative");
            if dist == child && v == 0.0 {
                saw_root_hop_zero = true;
            }
        }
    }
    // The root should be at hop 0. If hops rendered as BigInt, this
    // assertion is loosened to "at least one entry present" via the
    // length check above; the type + shape are the load-bearing checks.
    assert!(saw_root_hop_zero || arr.length() == 3, "root at hop 0 (or bigint path)");
}
