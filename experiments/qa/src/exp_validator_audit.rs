// Validator Audit Probe (companion to audits/validator.md)
//
// PURPOSE
// -------
// Exercise the attack surfaces enumerated in the validator.rs audit:
//   A. Oversized `previous_root` (DoS / memory amp in rejection message)
//   B. Oversized `data: Vec<u8>` in TransactionAction (per-byte fold DoS)
//   C. Out-of-order nonces (atomic-failure semantics — engine leakage)
//   D. Empty distinctions / empty batches
//   E. ByteMapping phantom-parent leakage through validator path
//   F. `from_root` accepting an arbitrary Distinction without provenance check
//   G. `set_expected_nonce` desync from local_root
//
// READ-ONLY against src/.

use koru_lambda_core::{
    BatchValidationResult, ConsensusValidator, DistinctionEngine, TransactionAction,
    TransactionBatch,
};
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

fn relationship_participants(engine: &DistinctionEngine) -> HashSet<String> {
    let mut set = HashSet::new();
    for (a, b) in engine.get_relationships_snapshot() {
        set.insert(a);
        set.insert(b);
    }
    set
}

fn main() {
    println!("=== Validator Audit Probe ===\n");

    // --- A: Oversized previous_root ---
    println!("-- A: 1 MiB previous_root --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);
        let huge_root = "z".repeat(1_000_000);
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: huge_root,
        };
        let t0 = Instant::now();
        let result = validator.validate_batch(batch, &engine);
        let elapsed = t0.elapsed();
        match result {
            BatchValidationResult::Rejected(reason) => {
                println!(
                    "  rejected in {:?}; reason length = {} bytes",
                    elapsed,
                    reason.len()
                );
                println!("  validator.rs:115-119 format!s the full untrusted string into the");
                println!("  error message. 1 MB in -> ~1 MB allocation out.");
            }
            BatchValidationResult::Valid(_) => println!("  UNEXPECTED: validated giant root"),
        }
    }

    // --- B: Oversized TransactionAction.data ---
    println!("\n-- B: 100 000-byte TransactionAction.data --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);
        let prev = validator.state_root_id().to_string();
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![0xAB; 100_000],
            }],
            previous_root: prev,
        };
        let dist_before = engine.distinction_count();
        let rel_before = engine.relationship_count();
        let t0 = Instant::now();
        let result = validator.validate_batch(batch, &engine);
        let elapsed = t0.elapsed();
        let dist_after = engine.distinction_count();
        let rel_after = engine.relationship_count();
        match result {
            BatchValidationResult::Valid(root) => {
                println!(
                    "  validated in {:?}: new root = {}...",
                    elapsed,
                    &root.to_hex()[..16]
                );
                println!(
                    "  engine grew: distinctions +{}, relationships +{}",
                    dist_after - dist_before,
                    rel_after - rel_before
                );
                println!("  one batch with one large tx -> ~data.len() synth calls.");
            }
            BatchValidationResult::Rejected(r) => println!("  rejected: {}", r),
        }
    }

    // --- C: Out-of-order nonces (engine leakage) ---
    println!("\n-- C: out-of-order nonces in a batch --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);
        let prev = validator.state_root_id().to_string();
        let dist_before = engine.distinction_count();
        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction {
                    nonce: 0,
                    data: vec![1, 2, 3],
                },
                TransactionAction {
                    nonce: 2,
                    data: vec![4, 5, 6],
                },
                TransactionAction {
                    nonce: 1,
                    data: vec![7, 8, 9],
                },
            ],
            previous_root: prev,
        };
        let result = validator.validate_batch(batch, &engine);
        let dist_after = engine.distinction_count();
        match result {
            BatchValidationResult::Rejected(reason) => {
                println!("  rejected: {}", reason);
                let leaked = dist_after - dist_before;
                println!(
                    "  engine distinction delta = {}; validator.expected_nonce = {}",
                    leaked,
                    validator.expected_nonce()
                );
                if leaked > 0 {
                    println!("  VERDICT: NODES LEAKED into engine despite atomic rejection.");
                    println!("  validator.rs:131-145 calls engine.synthesize() inside the loop;");
                    println!("  the engine is append-only, so any synth before the failing tx is");
                    println!("  permanent. Only validator state rolls back.");
                }
            }
            BatchValidationResult::Valid(_) => panic!("should have rejected"),
        }
    }

    // --- D: empty batch + empty data ---
    println!("\n-- D: empty batch + empty transaction data --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);
        let prev = validator.state_root_id().to_string();

        let r1 = validator.validate_batch(
            TransactionBatch {
                transactions: vec![],
                previous_root: prev.clone(),
            },
            &engine,
        );
        println!("  empty-batch result: {:?}", r1);

        let r2 = validator.validate_batch(
            TransactionBatch {
                transactions: vec![TransactionAction {
                    nonce: 0,
                    data: vec![],
                }],
                previous_root: prev,
            },
            &engine,
        );
        println!("  empty-data tx result: {:?}", r2);
        println!("  Empty data folds to engine.d0(). Two txs with empty data + same nonce");
        println!("  produce identical tx-distinctions.");
    }

    // --- E: phantom-parent count after validator usage ---
    println!("\n-- E: phantom-parent count after validator usage --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);
        let prev = validator.state_root_id().to_string();
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: (0u8..=255).collect(),
            }],
            previous_root: prev,
        };
        let _ = validator.validate_batch(batch, &engine);

        let registered: HashSet<String> = engine
            .get_distinctions_snapshot()
            .into_iter()
            .map(|d| d.to_hex())
            .collect();
        let referenced = relationship_participants(&engine);
        let phantoms: Vec<&String> = referenced.difference(&registered).collect();
        println!("  distinction_count       = {}", engine.distinction_count());
        println!("  unique parent IDs in r  = {}", referenced.len());
        println!("  phantom IDs (refd, !registered) = {}", phantoms.len());
        if !phantoms.is_empty() {
            println!("  VERDICT: validator path INHERITS the ByteMapping phantom-parent bug.");
        }
    }

    // --- F: from_root accepts arbitrary Distinction — STRUCTURALLY CLOSED in v2.0 ---
    println!("\n-- F: from_root(foreign_distinction, nonce) --");
    {
        println!("  v2.0 closure: Distinction has no public constructor. The only");
        println!("  Distinction values an external caller can pass to from_root are");
        println!("  engine-derived (synthesize/d0/d1) or from_hex-parsed (well-formed");
        println!("  bytes). Foreign-bytes-as-string injection is no longer possible.");
        println!();
        println!("  The original probe body called Distinction::new(\"deadbeef\".repeat(8)).");
        println!("  Against v2.0 it does not compile.");
        println!();
        println!("  Causal-provenance (\"is this root anchored in MY history?\") remains");
        println!("  a separate concern — addressed by V6 (joint restore_state) in");
        println!("  sub-branch #7.");
        println!("  VERDICT: STRUCTURALLY CLOSED.");
    }

    // --- G: set_expected_nonce desync ---
    println!("\n-- G: set_expected_nonce desync --");
    {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);
        validator.set_expected_nonce(100);
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 100,
                data: vec![1, 2, 3],
            }],
            previous_root: validator.state_root_id().to_string(),
        };
        let result = validator.validate_batch(batch, &engine);
        match result {
            BatchValidationResult::Valid(_) => {
                println!("  Accepted a nonce-100 batch against the genesis root.");
                println!(
                    "  set_expected_nonce + local_root are independent fields. No invariant"
                );
                println!("  ties them. No atomic restore-state(root, nonce) API exists.");
            }
            BatchValidationResult::Rejected(r) => println!("  rejected: {}", r),
        }
    }

    println!("\n=== Done ===");
}
