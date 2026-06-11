// Audit: NetworkAgent foreign-input surface.
// Demonstrates 5 entry points where peer-supplied data either becomes
// a Distinction parent or replaces local_root with no provenance check.

use koru_lambda_core::{
    Canonicalizable, CommitmentAgent, ConsensusValidator, Distinction, DistinctionEngine,
    LocalCausalAgent, NetworkAction, NetworkAgent, PeerIdentity, TransactionBatch,
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
        println!("  d0.id()         = {}", engine.d0().id());
        let p1_is_d0 = p1.distinction_id() == engine.d0().id();
        println!("  peer(\"\") distinction == d0 ? {}", p1_is_d0);
        agent.join_peer(p1, &engine);
        agent.join_peer(p2, &engine);
        println!("  validator_count = {}", agent.validator_count());
        if p1_is_d0 {
            println!("  VERDICT: CONFIRMED. Empty peer id impersonates primordial.");
        }
    }

    // C: from_state with forged root
    {
        println!("\n-- C: from_state accepts forged Distinction as root --");
        let engine = Arc::new(DistinctionEngine::new());
        let forged = Distinction::new("FORGED_ROOT_NEVER_SYNTHESIZED".to_string());
        let agent = NetworkAgent::from_state(
            forged,
            ConsensusValidator::new(&engine),
            CommitmentAgent::new(&engine),
            Vec::new(),
            42,
        );
        let stats = agent.get_stats();
        println!("  network_root in stats = {}", stats.network_root);
        println!(
            "  Present in engine? {}",
            engine.get_distinction_by_id(&stats.network_root).is_some()
        );
        if stats.network_root == "FORGED_ROOT_NEVER_SYNTHESIZED" {
            println!("  VERDICT: CONFIRMED. SEVERITY: HIGH.");
        }
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
        println!("  action_a.id = {}", act_a.id());
        println!("  action_b.id = {}", act_b.id());
        if act_a.id() == act_b.id() {
            println!("  VERDICT: CONFIRMED. Causal-chain collision on 8-byte prefix.");
            println!("           SEVERITY: HIGH.  network.rs:73-78 .take(8)");
        }
    }

    // E: update_local_root unauthenticated
    {
        println!("\n-- E: update_local_root accepts forged Distinction --");
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);
        let before = agent.get_current_root().id().to_string();
        agent.update_local_root(Distinction::new("ATTACKER_PICKED_ROOT".to_string()));
        let after = agent.get_current_root().id().to_string();
        println!("  before: {}", &before[..16]);
        println!("  after : {}", after);
        if after == "ATTACKER_PICKED_ROOT" {
            println!("  VERDICT: CONFIRMED. SEVERITY: HIGH.");
        }
    }

    println!("\n=== Done ===");
}
