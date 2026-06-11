// Audit: BatchCommitment::compute does not bind leader_id to the hash.
// Demonstrates that an attacker can flip leader_id in flight and verify_batch
// still passes.

use koru_lambda_core::{BatchCommitment, TransactionAction, TransactionBatch};

fn main() {
    println!("=== Audit: BatchCommitment leader_id is not in the hash ===\n");

    let batch = TransactionBatch {
        transactions: vec![TransactionAction {
            nonce: 7,
            data: vec![1, 2, 3],
        }],
        previous_root: "genesis".to_string(),
    };

    let honest = BatchCommitment::compute(&batch, 7, 3, "alice".to_string());
    let forged = BatchCommitment::compute(&batch, 7, 3, "EVE".to_string());

    println!("  honest.commitment_hash = {:x?}", honest.commitment_hash);
    println!("  forged.commitment_hash = {:x?}", forged.commitment_hash);
    println!("  honest.leader_id = {}", honest.leader_id);
    println!("  forged.leader_id = {}", forged.leader_id);
    println!(
        "  hashes equal? {}",
        honest.commitment_hash == forged.commitment_hash
    );

    // verify_batch uses self.leader_id when recomputing -- which is also ignored.
    println!(
        "  honest.verify_batch(&batch, 7, 3) = {}",
        honest.verify_batch(&batch, 7, 3)
    );
    println!(
        "  forged.verify_batch(&batch, 7, 3) = {}",
        forged.verify_batch(&batch, 7, 3)
    );

    if honest.commitment_hash == forged.commitment_hash {
        println!("\n  VERDICT: CONFIRMED. leader_id is a free-form field with no");
        println!("           cryptographic binding to the commitment hash.");
        println!("           commitment.rs:46-58 -- hasher never reads leader_id.");
        println!("           SEVERITY: HIGH. Leader attribution is forgeable.");
    }
}
