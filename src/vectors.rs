//! Canonical test vectors for the substrate's byte-level guarantees.
//!
//! These constants are the single source for the cross-target
//! attestation: the native test in this crate and the wasm test in the
//! bindings crate assert against *these* values, not against each
//! other. Two implementations agreeing with one published constant is
//! what makes the agreement checkable; two implementations compared
//! only to each other could drift together.
//!
//! They are published rather than kept in a test file so that an
//! independent implementation can verify itself against the same
//! numbers — see the specification's golden-vector section.
//!
//! Fixture history, in order:
//!
//! ```text
//! engine = new()
//! a = d0()
//! b = d1()
//! c       = synthesize(a, b)   -> C_HEX
//! d       = synthesize(c, a)   -> D_HEX
//! e_child = synthesize(d, b)   -> E_HEX
//! proj    = project(c).direction(Upstream).hops(2).signal(Adjacency)
//! proj.canonical_bytes()       -> ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX
//! ```

/// `synthesize(d0, d1)` as lowercase hex.
pub const C_HEX: &str = "6cc4f0e930b34481d03a4134331852ea";

/// `synthesize(c, d0)` as lowercase hex.
pub const D_HEX: &str = "3309377e376d6ca3c2e3bf9885900260";

/// `synthesize(d, d1)` as lowercase hex.
pub const E_HEX: &str = "5f02c8386c05dc7311e0c73a011e18be";

/// Canonical wire bytes of the `Adjacency` projection anchored at `c`,
/// `Upstream`, `hops(2)`, as lowercase hex.
pub const ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX: &str = "4b50524a026cc4f0e930b34481d03a4134331852ea00020000000000000000d0b9d2562fdb2c3f3fa4b8a3258b778103000000000000000000000000000000000000000000000001000000000100000000000000000000000000000001000000006cc4f0e930b34481d03a4134331852ea21000000010000000000000000000000000000000001000000000000000000000000000000";
