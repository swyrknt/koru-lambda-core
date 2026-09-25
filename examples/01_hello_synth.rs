//! Tier 1 — hello_synth. Primordials + a single synthesis.
//!
//! ```text
//! cargo run --example 01_hello_synth
//! ```
//!
//! Prints the two primordials, the child of `synth(d0, d1)`, and the
//! engine's post-synthesis distinction / relationship counts.

use koru_lambda_core::DistinctionEngine;

fn main() {
    let engine = DistinctionEngine::new();

    let d0 = engine.d0();
    let d1 = engine.d1();
    let child = engine.synthesize(d0, d1);

    println!("d0    = {}", d0.to_hex());
    println!("d1    = {}", d1.to_hex());
    println!("child = {}", child.to_hex());
    println!();
    println!("distinctions  = {}", engine.distinction_count());
    println!("relationships = {}", engine.relationship_count());
}
