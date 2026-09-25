//! Tier 3 — projection_composition. Materialize all three substrate
//! signal projections (Adjacency, Degree, HopDistance) on one lattice.
//!
//! ```text
//! cargo run --example 03_projection_composition
//! ```
//!
//! Every projection commits to the 4-axis structure
//! `{ root, direction, boundary, signal }` and produces a stable
//! `ProjectionId` + `canonical_bytes` — the same synthesis history on
//! two engines yields byte-identical output (Cond D, live-verified in
//! tier 4).

use koru_lambda_core::projection::Direction;
use koru_lambda_core::{Adjacency, Degree, DistinctionEngine, HopDistance};

fn main() {
    let engine = DistinctionEngine::new();
    let d0 = engine.d0();
    let d1 = engine.d1();

    let c = engine.synthesize(d0, d1);
    let d = engine.synthesize(c, d0);
    let _e = engine.synthesize(d, c); // grows the cone; projections anchor at c

    let adjacency =
        engine.project(c).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();

    let degree =
        engine.project(c).direction(Direction::Upstream).hops(2).signal(Degree).materialize();

    let hop_distance =
        engine.project(c).direction(Direction::Upstream).hops(2).signal(HopDistance).materialize();

    println!("anchor: c = {}", c.to_hex());
    println!("axis:   direction=Upstream, boundary=Hops(2)");
    println!();

    let bytes_a = adjacency.canonical_bytes();
    let bytes_d = degree.canonical_bytes();
    let bytes_h = hop_distance.canonical_bytes();

    let row = |name: &str, len: usize, id_prefix: String, cb_len: usize| {
        println!("{name:<12} len={len:>2}  id={id_prefix}  canonical={cb_len}B");
    };
    row(
        "Adjacency",
        adjacency.output().len(),
        hex_prefix(adjacency.projection_id().as_bytes()),
        bytes_a.len(),
    );
    row(
        "Degree",
        degree.output().len(),
        hex_prefix(degree.projection_id().as_bytes()),
        bytes_d.len(),
    );
    row(
        "HopDistance",
        hop_distance.output().len(),
        hex_prefix(hop_distance.projection_id().as_bytes()),
        bytes_h.len(),
    );
}

fn hex_prefix(bytes: &[u8]) -> String {
    let hex = hex::encode(bytes);
    format!("{}…", &hex[..12])
}
