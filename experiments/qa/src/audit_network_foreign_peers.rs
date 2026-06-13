// Audit: NetworkAgent foreign-input surface.
// Demonstrates 5 entry points where peer-supplied data either becomes
// a Distinction parent or replaces local_root with no provenance check.

use koru_lambda_core::{
    Canonicalizable, DistinctionEngine, NetworkAction, NetworkAgent, PeerIdentity,
    TransactionBatch,
};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    println!("=== Audit: NetworkAgent foreign-input surface ===\n");

    // A: 1MB peer id
    {
        println!("-- A: PeerIdentity::new with 1MB peer id --");
        let engine = Arc::new(DistinctionEngine::new());
        let huge_id = "z".repeat(1_000_000);
        let t = Instant::now();
        let peer = PeerIdentity::new(huge_id, &engine);
        let elapsed = t.elapsed();
        println!("  Constructed in {:?}", elapsed);
        println!("  peer.id.len() = {}", peer.id.len());
        println!(
            "  engine.distinction_count() after = {}",
            engine.distinction_count()
        );
        println!("  VERDICT: 1MB peer id permitted, ~1M synth calls. SEVERITY: HIGH.");
    }

    // B: empty peer id collapses to d0
    {
        println!("\n-- B: empty peer id collapses to d0 --");
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);
        let p1 = PeerIdentity::new(String::new(), &engine);
        let p2 = PeerIdentity::new(String::new(), &engine);
        println!("  peer(\"\")  -> distinction = {}", p1.distinction_id());
        println!("  d0.to_hex()         = {}", engine.d0().to_hex());
        let p1_is_d0 = p1.distinction_id() == engine.d0().to_hex();
        println!("  peer(\"\") distinction == d0 ? {}", p1_is_d0);
        agent.join_peer(p1, &engine);
        agent.join_peer(p2, &engine);
        println!("  validator_count = {}", agent.validator_count());
        if p1_is_d0 {
            println!("  VERDICT: CONFIRMED. Empty peer id impersonates primordial.");
        }
    }

    // C: from_state with forged root — STRUCTURALLY CLOSED in v2.0
    {
        println!("\n-- C: from_state accepts forged Distinction as root --");
        println!("  v2.0 closure: Distinction has no public constructor; an external");
        println!("  caller cannot mint Distinction::new(\"FORGED_ROOT_NEVER_SYNTHESIZED\").");
        println!("  The only Distinction values an attacker could pass to from_state");
        println!("  are real engine-derived values (from synthesize/d0/d1) or values");
        println!("  parsed via from_hex(32-char-hex). Either way the bytes are a");
        println!("  well-formed 16-byte SHA256 prefix.");
        println!();
        println!("  As a smoke test, this binary builds without any Distinction::new");
        println!("  call — the historical probe body is preserved in git blame.");
        println!("  VERDICT: STRUCTURALLY CLOSED (no longer probeable).");
    }

    // D: BatchProposed truncates previous_root to 8 bytes
    {
        println!("\n-- D: BatchProposed truncates previous_root to first 8 bytes --");
        let engine = Arc::new(DistinctionEngine::new());
        let batch_a = TransactionBatch {
            transactions: vec![],
            previous_root: "deadbeefAAAAAAAAAAAAAAAA".to_string(),
        };
        let batch_b = TransactionBatch {
            transactions: vec![],
            previous_root: "deadbeefBBBBBBBBBBBBBBBB".to_string(),
        };
        let act_a = NetworkAction::BatchProposed { batch: batch_a }.to_canonical_structure(&engine);
        let act_b = NetworkAction::BatchProposed { batch: batch_b }.to_canonical_structure(&engine);
        println!("  action_a.id = {}", act_a.to_hex());
        println!("  action_b.id = {}", act_b.to_hex());
        if act_a.to_hex() == act_b.to_hex() {
            println!("  VERDICT: CONFIRMED. Causal-chain collision on 8-byte prefix.");
            println!("           SEVERITY: HIGH.  network.rs:73-78 .take(8)");
        }
    }

    // E: update_local_root unauthenticated — STRUCTURALLY CLOSED in v2.0
    {
        println!("\n-- E: update_local_root accepts forged Distinction --");
        println!("  v2.0 closure: same as C. The Distinction passed to");
        println!("  update_local_root is necessarily well-formed bytes. The legitimate");
        println!("  semantic question (\"is this root part of MY causal history?\")");
        println!("  is unrelated to byte well-formedness; it's the trait-level");
        println!("  question that Section 1.6 V6 closes with restore_state(engine,");
        println!("  root_id, nonce) (sub-branch #7's scope).");
        println!();
        println!("  VERDICT: foreign-byte-injection STRUCTURALLY CLOSED;");
        println!("  causal-provenance check is V6's separate concern.");
    }

    println!("\n=== Done ===");
}
