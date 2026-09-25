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

// -- Native half. The wasm half lives in the bindings crate; both
// assert against `koru_lambda_core::vectors`, which is what makes the
// cross-target agreement checkable rather than self-referential.

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use koru_lambda_core::projection::Direction;
    use koru_lambda_core::vectors::{
        ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX as EXPECTED_CANONICAL_BYTES_HEX,
        C_HEX as EXPECTED_C_HEX, D_HEX as EXPECTED_D_HEX, E_HEX as EXPECTED_E_HEX,
    };
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
