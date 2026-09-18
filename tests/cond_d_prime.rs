//! Cond D′ cross-target falsifier — attests byte-identity of
//! `canonical_bytes()` output between native Rust builds and the wasm
//! build for identical synthesis histories.
//!
//! Cond D (see `tests/cond_d_falsifier.rs`) covers cross-engine
//! byte-equivalence within one compilation target. Cond D′ extends the
//! guarantee into the cross-target axis: if the wasm build ever
//! produces different canonical bytes than native for the same
//! synthesis history — endianness bug, projection ordering drift,
//! hash-algorithm divergence, wasm-bindgen serialization drift — this
//! test fails loudly.
//!
//! Fixture is a small deterministic synthesis chain plus one
//! `Adjacency` projection materialization at `hops(2)` upstream of `c`.
//! Expected canonical bytes captured once from a native run and
//! hard-coded below. Both the native and wasm test assert against the
//! same constants — that IS the cross-target attestation.
//!
//! Fixture sequence:
//!   engine = new()
//!   a = engine.d0()
//!   b = engine.d1()
//!   c       = synthesize(a, b)  — asserted equal to `EXPECTED_C_HEX`
//!   d       = synthesize(c, a)  — asserted equal to `EXPECTED_D_HEX`
//!   e_child = synthesize(d, b)  — asserted equal to `EXPECTED_E_HEX`
//!   proj    = projectAdjacency(c, upstream, hops = 2)
//!   canonical_bytes(proj)       — asserted equal to
//!                                 `EXPECTED_CANONICAL_BYTES_HEX`

// -- Shared fixture --------------------------------------------------

const EXPECTED_C_HEX: &str = "6cc4f0e930b34481d03a4134331852ea";
const EXPECTED_D_HEX: &str = "3309377e376d6ca3c2e3bf9885900260";
const EXPECTED_E_HEX: &str = "5f02c8386c05dc7311e0c73a011e18be";
const EXPECTED_CANONICAL_BYTES_HEX: &str = "4b50524a026cc4f0e930b34481d03a4134331852ea00020000000000000000d0b9d2562fdb2c3f3fa4b8a3258b778103000000000000000000000000000000000000000000000001000000000100000000000000000000000000000001000000006cc4f0e930b34481d03a4134331852ea21000000010000000000000000000000000000000001000000000000000000000000000000";

// -- Native test -----------------------------------------------------

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{EXPECTED_CANONICAL_BYTES_HEX, EXPECTED_C_HEX, EXPECTED_D_HEX, EXPECTED_E_HEX};
    use koru_lambda_core::projection::Direction;
    use koru_lambda_core::{Adjacency, DistinctionEngine};

    #[test]
    fn cond_d_prime_native_matches_fixture() {
        let engine = DistinctionEngine::new();
        let a = engine.d0();
        let b = engine.d1();
        let c = engine.synthesize(a, b);
        let d = engine.synthesize(c, a);
        let e_child = engine.synthesize(d, b);

        assert_eq!(
            hex::encode(c.as_bytes()),
            EXPECTED_C_HEX,
            "native: c bytes drift — Axiom-4 content-addressing broken"
        );
        assert_eq!(hex::encode(d.as_bytes()), EXPECTED_D_HEX, "native: d bytes drift");
        assert_eq!(hex::encode(e_child.as_bytes()), EXPECTED_E_HEX, "native: e_child bytes drift");

        let projection = engine
            .project(c)
            .direction(Direction::Upstream)
            .hops(2)
            .signal(Adjacency)
            .materialize();

        let canonical = projection.canonical_bytes();
        assert_eq!(
            hex::encode(&canonical),
            EXPECTED_CANONICAL_BYTES_HEX,
            "native: canonicalBytes drift — Cond D wire form broken"
        );
    }
}

// -- Wasm test -------------------------------------------------------

#[cfg(all(target_arch = "wasm32", feature = "wasm"))]
mod wasm {
    use super::{EXPECTED_CANONICAL_BYTES_HEX, EXPECTED_C_HEX, EXPECTED_D_HEX, EXPECTED_E_HEX};
    use koru_lambda_core::wasm::WasmEngine;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen_test]
    fn cond_d_prime_wasm_matches_fixture() {
        let engine = WasmEngine::new();
        let a = engine.d0();
        let b = engine.d1();
        let c = engine.synthesize(&a, &b).expect("(invariant) synthesize c");
        let d = engine.synthesize(&c, &a).expect("(invariant) synthesize d");
        let e_child = engine.synthesize(&d, &b).expect("(invariant) synthesize e_child");

        assert_eq!(
            hex::encode(&c),
            EXPECTED_C_HEX,
            "wasm: c bytes drift — Cond D′ violation, native/wasm targets diverged at Axiom-4"
        );
        assert_eq!(hex::encode(&d), EXPECTED_D_HEX, "wasm: d bytes drift");
        assert_eq!(hex::encode(&e_child), EXPECTED_E_HEX, "wasm: e_child bytes drift");

        // Single-iteration materialization — no `.free()` OOM concern
        // at this scale (would matter only in tight synthesis loops).
        let projection = engine
            .project_adjacency(&c, "upstream", Some(2))
            .expect("(invariant) projectAdjacency");

        let canonical = projection.canonical_bytes();
        assert_eq!(
            hex::encode(&canonical),
            EXPECTED_CANONICAL_BYTES_HEX,
            "wasm: canonicalBytes drift — Cond D′ violation, native/wasm targets diverged on wire form"
        );
    }
}
