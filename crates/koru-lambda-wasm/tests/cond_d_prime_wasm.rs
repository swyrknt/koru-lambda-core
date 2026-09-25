//! Cross-target attestation, wasm half.
//!
//! The native half lives in the substrate crate. Both assert against
//! `koru_lambda_core::vectors` — one published constant, two
//! independently compiled targets. If the wasm build ever produces
//! different bytes than native for the same synthesis history —
//! endianness, projection ordering, hash divergence, wasm-bindgen
//! serialization drift — this test fails loudly.

#![cfg(target_arch = "wasm32")]

use koru_lambda_core::vectors::{ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX, C_HEX, D_HEX, E_HEX};
use koru_lambda_wasm::WasmEngine;
use wasm_bindgen_test::wasm_bindgen_test;

#[wasm_bindgen_test]
fn cond_d_prime_wasm_matches_published_vectors() {
    let engine = WasmEngine::new();
    let a = engine.d0();
    let b = engine.d1();
    let c = engine.synthesize(&a, &b).expect("(invariant) synthesize c");
    let d = engine.synthesize(&c, &a).expect("(invariant) synthesize d");
    let e_child = engine.synthesize(&d, &b).expect("(invariant) synthesize e_child");

    assert_eq!(
        hex::encode(&c),
        C_HEX,
        "wasm: c bytes drift — native and wasm targets diverged at Axiom-4"
    );
    assert_eq!(hex::encode(&d), D_HEX, "wasm: d bytes drift");
    assert_eq!(hex::encode(&e_child), E_HEX, "wasm: e_child bytes drift");

    let projection =
        engine.project_adjacency(&c, "upstream", Some(2)).expect("(invariant) projectAdjacency");

    assert_eq!(
        hex::encode(projection.canonical_bytes()),
        ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX,
        "wasm: canonicalBytes drift — cross-target wire form broken"
    );
}
