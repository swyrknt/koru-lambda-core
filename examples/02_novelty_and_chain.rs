//! Tier 2 — novelty_and_chain. Build a 3-generation lattice and
//! demonstrate `SynthesisOutcome::{Novel, Existing}` at the operator
//! return.
//!
//! ```text
//! cargo run --example 02_novelty_and_chain
//! ```
//!
//! Law-7 (saturation) says re-synthesizing an existing pair returns the
//! same distinction; the novelty bit lets a caller react to that
//! without a follow-up `has()` query.

use koru_lambda_core::{DistinctionEngine, SynthesisOutcome};

fn main() {
    let engine = DistinctionEngine::new();
    let d0 = engine.d0();
    let d1 = engine.d1();

    // Three generations.
    let c = engine.synthesize(d0, d1);
    let d = engine.synthesize(c, d0);
    let e = engine.synthesize(d, c);

    println!("lattice:");
    println!("  c = synth(d0, d1) = {}", c.to_hex());
    println!("  d = synth(c,  d0) = {}", d.to_hex());
    println!("  e = synth(d,  c ) = {}", e.to_hex());
    println!();

    // (d0, d1) is already in the graph — expect Existing.
    // `SynthesisOutcome` is `#[non_exhaustive]`; the wildcard forwards-
    // compatibly handles variants added in future substrate versions.
    match engine.synthesize_novel(d0, d1) {
        SynthesisOutcome::Novel(_) => panic!("Law-7 broken: (d0, d1) should saturate"),
        SynthesisOutcome::Existing(child) => {
            println!("re-synth(d0, d1) -> Existing({})", child.to_hex());
        },
        other => panic!("unhandled outcome variant: {other:?}"),
    }

    // (d0, d) is a fresh pair — expect Novel.
    match engine.synthesize_novel(d0, d) {
        SynthesisOutcome::Novel(child) => {
            println!("  synth(d0, d ) -> Novel(   {})", child.to_hex());
        },
        SynthesisOutcome::Existing(_) => panic!("(d0, d) was not previously synthesized"),
        other => panic!("unhandled outcome variant: {other:?}"),
    }

    println!();
    println!("distinctions  = {}", engine.distinction_count());
    println!("relationships = {}", engine.relationship_count());
}
