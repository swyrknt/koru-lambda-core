//! Tier 1 — hello_synth. Predict a child id with a plain SHA-256 call,
//! *then* ask the engine, and confirm they agree.
//!
//! ```text
//! cargo run --example 01_hello_synth
//! ```
//!
//! The point of this order is that the prediction cannot be a readback.
//! Nothing here calls into the engine before the hash is already computed,
//! so if the two sixteen-byte values agree, the rule is the rule: a
//! distinction's identity is `SHA-256(min ‖ max)` truncated to the leading
//! sixteen bytes, and nothing else is consulted. No registry, no counter,
//! no clock, no node identity. The last block prints a command you can
//! paste into a shell to check it without this crate at all.

use koru_lambda_core::DistinctionEngine;
use sha2::{Digest, Sha256};

/// The content-addressing rule, written out independently of the crate:
/// SHA-256 over the canonically ordered pair, leading sixteen bytes.
///
/// `src/engine.rs` orders with `if a.0 <= b.0`, which is bytewise
/// lexicographic on the sixteen-byte arrays. `min`/`max` here is the same
/// comparison, and that is the whole of the canonical ordering — the pair
/// is unordered, so the operator has to pick one arrangement and commit.
fn predict(a: [u8; 16], b: [u8; 16]) -> [u8; 16] {
    let (first, second) = if a <= b { (a, b) } else { (b, a) };
    let mut h = Sha256::new();
    h.update(first);
    h.update(second);
    let mut out = [0u8; 16];
    out.copy_from_slice(&h.finalize()[..16]);
    out
}

fn main() {
    // ---- predict, before any engine exists ----------------------------
    let d0_bytes = [0u8; 16];
    let d1_bytes = {
        let mut b = [0u8; 16];
        b[0] = 1;
        b
    };
    let predicted_child = predict(d0_bytes, d1_bytes);
    let predicted_grandchild = predict(predicted_child, d0_bytes);

    println!("predicted, with a plain SHA-256 call and no engine:");
    println!("  child      = {}", hex::encode(predicted_child));
    println!("  grandchild = {}", hex::encode(predicted_grandchild));
    println!();

    // ---- now ask the engine -------------------------------------------
    let engine = DistinctionEngine::new();
    let d0 = engine.d0();
    let d1 = engine.d1();
    let child = engine.synthesize(d0, d1);
    let grandchild = engine.synthesize(child, d0);

    println!("d0    = {}", d0.to_hex());
    println!("d1    = {}", d1.to_hex());
    println!("child = {}", child.to_hex());
    println!();
    println!("distinctions  = {}", engine.distinction_count());
    println!("relationships = {}", engine.relationship_count());
    println!();

    // ---- confirm ------------------------------------------------------
    assert_eq!(*d0.as_bytes(), d0_bytes, "d0 is not [0x00; 16]");
    assert_eq!(*d1.as_bytes(), d1_bytes, "d1 is not [0x01, 0x00 * 15]");
    assert_eq!(*child.as_bytes(), predicted_child, "the prediction missed the child");
    assert_eq!(
        *grandchild.as_bytes(),
        predicted_grandchild,
        "the prediction missed the grandchild — the rule does not compose"
    );

    println!("confirmed:");
    println!("  engine child      == predicted  ✓");
    println!("  engine grandchild == predicted  ✓  (the rule composes: the child's");
    println!("                                      own bytes are the next input)");
    println!();

    // ---- check it by hand ---------------------------------------------
    //
    // Printed rather than described. The argument is only as good as the
    // reader's ability to run it without this crate.
    let (lo, hi) = if d0_bytes <= d1_bytes { (d0_bytes, d1_bytes) } else { (d1_bytes, d0_bytes) };
    println!("check it yourself — no Rust, no npm:");
    println!(
        "  printf '{}' | shasum -a 256 | cut -c1-32",
        hex::encode([lo, hi].concat()).as_bytes().chunks(2).fold(String::new(), |mut s, pair| {
            s.push_str("\\x");
            s.push_str(std::str::from_utf8(pair).expect("hex is ascii"));
            s
        })
    );
    println!("  => {}", child.to_hex());
}
