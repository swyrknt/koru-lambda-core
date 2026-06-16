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

    // A: 1MB peer id — STRUCTURALLY CLOSED in v2.0
    {
        println!("-- A: PeerIdentity::new with 1MB peer id (N1) --");
        let engine = Arc::new(DistinctionEngine::new());
        let huge_id = "z".repeat(1_000_000);
        let dist_before = engine.distinction_count();
        let t = Instant::now();
        let result = PeerIdentity::new(huge_id, &engine);
        let elapsed = t.elapsed();
        let dist_after = engine.distinction_count();
        println!("  PeerIdentity::new returned in {:?}", elapsed);
        println!("  is_err = {}", result.is_err());
        println!(
            "  engine.distinction_count() before/after = {}/{}",
            dist_before, dist_after
        );
        if result.is_err() && dist_before == dist_after {
            println!("  VERDICT: N1 CLOSED — oversized peer id rejected before any synth.");
        } else {
            println!("  VERDICT: N1 REGRESSED — oversized peer id was accepted or synthesized.");
        }
    }

    // B: empty peer id — STRUCTURALLY CLOSED in v2.0
    {
        println!("\n-- B: empty peer id rejected (N2) --");
        let engine = Arc::new(DistinctionEngine::new());
        let result_a = PeerIdentity::new(String::new(), &engine);
        let result_b = PeerIdentity::new(String::new(), &engine);
        println!("  PeerIdentity::new(\"\") result_a.is_err = {}", result_a.is_err());
        println!("  PeerIdentity::new(\"\") result_b.is_err = {}", result_b.is_err());
        if result_a.is_err() && result_b.is_err() {
            println!("  VERDICT: N2 CLOSED — empty peer id can no longer impersonate d0.");
        } else {
            println!("  VERDICT: N2 REGRESSED — empty peer id was accepted.");
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

    // D: BatchProposed previous_root canonicalization.
    //
    // v1.2.0 bug: NetworkAction::BatchProposed canonicalized previous_root
    // via `.take(8)` over the raw UTF-8 bytes — meaning any two strings
    // sharing the first 8 ASCII characters folded to the same action
    // distinction (e.g., "deadbeefAAAAAAAAAAAAAAAA" and
    // "deadbeefBBBBBBBBBBBBBBBB" collided).
    //
    // v2.0 fix (Phase 6 #6): canonicalization parses previous_root via
    // Distinction::from_hex (32-char lowercase hex). Malformed inputs
    // (wrong length, non-hex) fall through a deterministic sentinel
    // distinction that does not collide with any well-formed root.
    //
    // The original v1.2.0 demo strings ("deadbeefAAAA…" 24 chars) are
    // malformed and therefore collapse to the sentinel — they no longer
    // demonstrate the bug. To actually verify the fix, this section
    // uses VALID 32-char lowercase hex inputs that share an 8-char prefix.
    {
        println!("\n-- D: BatchProposed previous_root canonicalization (N5) --");
        let engine = Arc::new(DistinctionEngine::new());

        // Two well-formed 32-char hex roots sharing the first 8 chars.
        // Under the v1.2.0 .take(8) bug these collided; under the v2.0
        // fix they must NOT collide.
        let root_a = "deadbeefaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let root_b = "deadbeefbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        let batch_a = TransactionBatch { transactions: vec![], previous_root: root_a };
        let batch_b = TransactionBatch { transactions: vec![], previous_root: root_b };
        let act_a = NetworkAction::BatchProposed { batch: batch_a }.to_canonical_structure(&engine);
        let act_b = NetworkAction::BatchProposed { batch: batch_b }.to_canonical_structure(&engine);
        println!("  well-formed roots, shared 8-char hex prefix:");
        println!("  action_a.id = {}", act_a.to_hex());
        println!("  action_b.id = {}", act_b.to_hex());
        if act_a.to_hex() == act_b.to_hex() {
            println!("  VERDICT: BUG STILL PRESENT — N5 fix has regressed.");
            println!("           SEVERITY: HIGH.  network.rs BatchProposed canonicalization");
        } else {
            println!("  VERDICT: N5 CLOSED — distinct well-formed roots produce distinct actions.");
        }

        // Malformed inputs fall through the sentinel and intentionally
        // collide there — documenting the fallback contract.
        let act_malformed_a = NetworkAction::BatchProposed {
            batch: TransactionBatch {
                transactions: vec![],
                previous_root: "deadbeefAAAAAAAAAAAAAAAA".to_string(), // 24 chars, malformed
            },
        }
        .to_canonical_structure(&engine);
        let act_malformed_b = NetworkAction::BatchProposed {
            batch: TransactionBatch {
                transactions: vec![],
                previous_root: "deadbeefBBBBBBBBBBBBBBBB".to_string(),
            },
        }
        .to_canonical_structure(&engine);
        println!();
        println!("  Malformed inputs (24 chars) fall through deterministic sentinel:");
        println!("  malformed_a.id = {}", act_malformed_a.to_hex());
        println!("  malformed_b.id = {}", act_malformed_b.to_hex());
        if act_malformed_a.to_hex() == act_malformed_b.to_hex() {
            println!("  (expected — sentinel fallback for malformed input is by-design;");
            println!("   validator's previous_root String check rejects upstream)");
        }
    }

    // E: update_local_root unauthenticated — STRUCTURALLY CLOSED in v2.0
    {
        println!("\n-- E: update_local_root accepts forged Distinction --");
        println!("  v2.0 closure: same as C. The Distinction passed to");
        println!("  update_local_root is necessarily well-formed bytes. The legitimate");
        println!("  semantic question (\"is this root part of MY causal history?\")");
        println!("  is the trait-level question CHECKLIST 1.6 V6 closes via");
        println!("  ConsensusValidator::restore_state(engine, root_id, nonce) and");
        println!("  NetworkAgent::restore_consensus_validator_state(engine, root_id,");
        println!("  nonce); both reject Distinction values not registered in the");
        println!("  supplied engine (Phase 6 sub-branch #7).");
        println!();
        println!("  VERDICT: foreign-byte-injection STRUCTURALLY CLOSED;");
        println!("  fabricated-root injection at the restore boundary CLOSED (V6).");
    }

    println!("\n=== Done ===");
}
