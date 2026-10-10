//! Tier 4 — wire_format_roundtrip. Content addressing at both scales:
//!
//! 1. Distinction-level: `to_hex` → wire → `from_hex` → `verify` →
//!    identical bytes back inside the trust boundary.
//! 2. Projection-level (Cond D, live): two engines running the same
//!    synthesis history produce byte-identical `canonical_bytes` and
//!    identical `ProjectionId`.
//!
//! ```text
//! cargo run --example 04_wire_format_roundtrip
//! ```

use koru_lambda_core::projection::{Adjacency, Direction};
use koru_lambda_core::{Distinction, DistinctionEngine, RawDistinctionId};

fn run_history(engine: &DistinctionEngine) -> Distinction {
    let d0 = engine.d0();
    let d1 = engine.d1();
    let c = engine.synthesize(d0, d1);
    let d = engine.synthesize(c, d0);
    let _e = engine.synthesize(d, c);
    c
}

fn main() {
    // ---- (1) distinction wire round-trip ------------------------------
    let engine = DistinctionEngine::new();
    let child = run_history(&engine);

    let wire = child.to_hex();
    let raw = RawDistinctionId::from_hex(&wire).expect("hex parses");
    let verified = engine.verify(raw).expect("engine has these bytes");

    assert_eq!(verified.as_bytes(), child.as_bytes());
    println!("distinction wire round-trip:");
    println!("  child  = {wire}");
    println!("  verify(from_hex(wire)) == child  ✓");
    println!();

    // ---- (2) projection Cond D — twin engines -------------------------
    let engine_a = DistinctionEngine::new();
    let engine_b = DistinctionEngine::new();
    let c_a = run_history(&engine_a);
    let c_b = run_history(&engine_b);
    assert_eq!(c_a.as_bytes(), c_b.as_bytes(), "content addressing broken");

    let proj_a = engine_a
        .project(c_a)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize();
    let proj_b = engine_b
        .project(c_b)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize();

    let bytes_a = proj_a.canonical_bytes();
    let bytes_b = proj_b.canonical_bytes();
    let id_a = proj_a.projection_id();
    let id_b = proj_b.projection_id();

    assert_eq!(bytes_a, bytes_b, "Cond D violated: canonical bytes diverged");
    assert_eq!(id_a.as_bytes(), id_b.as_bytes(), "Cond D violated: projection id diverged");

    println!("projection Cond D (two engines, same history):");
    println!("  engine A canonical_bytes.len = {}", bytes_a.len());
    println!("  engine B canonical_bytes.len = {}", bytes_b.len());
    println!("  bytewise A == B                    ✓");
    println!("  projection_id A == B               ✓");
    println!();
    println!("projection id (both engines) = {}", hex::encode(id_a.as_bytes()));
}
