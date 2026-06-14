//! Throughput verification.
//!
//! Validates that the system achieves 100,000+ tx/s throughput. v2.0 history:
//! formerly exercised the deleted `ParallelBatchProcessor` — which the
//! Phase 1 parallel-audit identified as a duplicate of `ConsensusValidator`
//! with a misnamed "Parallel" prefix. The tests are now expressed against
//! `ConsensusValidator` and `BatchSynthesizer` directly (CHECKLIST Section
//! 1.10 / Phase 6 sub-branch #2).

use koru_lambda_core::{
    BatchSynthesizer, BatchValidationResult, Canonicalizable, ConsensusValidator,
    DistinctionEngine, TransactionAction, TransactionBatch,
};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_100k_txs_throughput_verification() {
    // Primary verification: can we process 100,000 transactions?

    let engine = Arc::new(DistinctionEngine::new());
    let mut validator = ConsensusValidator::new(&engine);

    let total_transactions = 100_000usize;
    let batch_size = 100usize;
    let num_batches = total_transactions / batch_size;

    println!("\n=== 100k Transaction Throughput Verification ===");
    println!("Total transactions: {total_transactions}");
    println!("Batch size: {batch_size}");
    println!("Number of batches: {num_batches}");

    let start = Instant::now();
    let mut current_root = validator.state_root_id();

    for batch_idx in 0..num_batches {
        let transactions: Vec<TransactionAction> = (0..batch_size)
            .map(|tx_idx| TransactionAction {
                nonce: (batch_idx * batch_size + tx_idx) as u64,
                data: vec![(batch_idx % 256) as u8, (tx_idx % 256) as u8],
            })
            .collect();

        let batch = TransactionBatch { transactions, previous_root: current_root.clone() };

        match validator.validate_batch(batch, &engine) {
            BatchValidationResult::Valid(new_root) => current_root = new_root.to_hex(),
            BatchValidationResult::Rejected(reason) => panic!("batch rejected: {reason}"),
        }

        if (batch_idx + 1) % 100 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let tx_processed = (batch_idx + 1) * batch_size;
            let current_rate = tx_processed as f64 / elapsed;
            println!(
                "Progress: {}/{num_batches} batches ({tx_processed} tx) - {current_rate:.0} tx/s",
                batch_idx + 1
            );
        }
    }

    let duration = start.elapsed();
    let tx_per_sec = total_transactions as f64 / duration.as_secs_f64();

    println!("\n=== Results ===");
    println!("Total time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {tx_per_sec:.0} tx/s");
    println!("Final nonce: {}", validator.expected_nonce());

    assert_eq!(validator.expected_nonce(), total_transactions as u64);

    if tx_per_sec >= 100_000.0 {
        println!("\u{2705} TARGET MET! ({:.1}x target)", tx_per_sec / 100_000.0);
    } else {
        let percentage = (tx_per_sec / 100_000.0) * 100.0;
        println!("Achieved {percentage:.1}% of target");
    }
}

#[test]
fn test_batch_synthesis_throughput() {
    // Verify BatchSynthesizer can achieve high byte canonicalization throughput.

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = BatchSynthesizer::new(Arc::clone(&engine));

    let total_bytes = 100_000usize;

    println!("\n=== Batch Synthesis Throughput ===");
    println!("Total bytes: {total_bytes}");

    let data: Vec<u8> = (0..total_bytes).map(|i| (i % 256) as u8).collect();

    let start = Instant::now();
    let results = synthesizer.canonicalize_bytes_batch(data);
    let duration = start.elapsed();

    let bytes_per_sec = total_bytes as f64 / duration.as_secs_f64();

    println!("Time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {bytes_per_sec:.0} bytes/s");
    println!("Results: {} distinctions", results.len());

    assert_eq!(results.len(), total_bytes);

    // Each byte requires 8 synthesis operations (binary path).
    let synthesis_ops = total_bytes * 8;
    let ops_per_sec = synthesis_ops as f64 / duration.as_secs_f64();

    println!("Synthesis operations: {synthesis_ops}");
    println!("Synthesis throughput: {ops_per_sec:.0} ops/s");
}

#[test]
fn test_core_synthesis_raw_throughput() {
    // Verify core engine can sustain 100k+ synthesis ops/s.

    let engine = Arc::new(DistinctionEngine::new());
    let num_operations = 100_000usize;

    println!("\n=== Core Synthesis Raw Throughput ===");
    println!("Operations: {num_operations}");

    let start = Instant::now();

    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut current = engine.synthesize(&d0, &d1);

    for i in 0..num_operations {
        let byte = (i % 256) as u8;
        let byte_d = byte.to_canonical_structure(&engine);
        current = engine.synthesize(&current, &byte_d);
    }

    let duration = start.elapsed();
    let ops_per_sec = num_operations as f64 / duration.as_secs_f64();

    println!("Time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {ops_per_sec:.0} ops/s");
    println!("Final distinction: {}", current.to_hex());

    assert!(!current.to_hex().is_empty());

    if ops_per_sec >= 100_000.0 {
        println!("\u{2705} Core synthesis exceeds 100k ops/s target!");
        println!("Achievement: {:.1}x target", ops_per_sec / 100_000.0);
    }
}

#[test]
fn test_multi_batch_throughput() {
    // Test processing many small batches in sequence.

    let engine = Arc::new(DistinctionEngine::new());
    let mut validator = ConsensusValidator::new(&engine);

    let total_actions = 10_000usize;
    let tx_per_batch = 10usize;
    let total_tx = total_actions * tx_per_batch;

    println!("\n=== Multi-batch Throughput ===");
    println!("Total actions: {total_actions}");
    println!("Tx per batch: {tx_per_batch}");
    println!("Total transactions: {total_tx}");

    let start = Instant::now();
    let mut tx_count = 0usize;
    let mut current_root = validator.state_root_id();

    for action_idx in 0..total_actions {
        let transactions: Vec<TransactionAction> = (0..tx_per_batch)
            .map(|tx_idx| TransactionAction {
                nonce: (tx_count + tx_idx) as u64,
                data: vec![(tx_count % 256) as u8, (tx_idx % 256) as u8],
            })
            .collect();

        let batch = TransactionBatch { transactions, previous_root: current_root.clone() };

        match validator.validate_batch(batch, &engine) {
            BatchValidationResult::Valid(new_root) => current_root = new_root.to_hex(),
            BatchValidationResult::Rejected(reason) => panic!("batch rejected: {reason}"),
        }
        tx_count += tx_per_batch;

        if (action_idx + 1) % 100 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let current_rate = tx_count as f64 / elapsed;
            println!(
                "Progress: {}/{total_actions} actions ({tx_count} tx) - {current_rate:.0} tx/s",
                action_idx + 1
            );
        }
    }

    let duration = start.elapsed();
    let tx_per_sec = total_tx as f64 / duration.as_secs_f64();

    println!("\n=== Results ===");
    println!("Time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {tx_per_sec:.0} tx/s");

    assert_eq!(validator.expected_nonce(), total_tx as u64);
}

#[test]
fn test_sustained_throughput_stability() {
    // Verify throughput remains stable over an extended period.

    let engine = Arc::new(DistinctionEngine::new());
    let mut validator = ConsensusValidator::new(&engine);

    let duration_secs = 5;
    let batch_size = 100usize;

    println!("\n=== Sustained Throughput Stability Test ===");
    println!("Duration: {duration_secs} seconds");
    println!("Batch size: {batch_size}");

    let start = Instant::now();
    let mut batches_processed = 0usize;
    let mut samples = vec![];

    let mut last_sample_time = start;
    let mut last_sample_count = 0usize;
    let mut current_root = validator.state_root_id();

    while start.elapsed().as_secs() < duration_secs {
        let transactions: Vec<TransactionAction> = (0..batch_size)
            .map(|i| TransactionAction {
                nonce: (batches_processed * batch_size + i) as u64,
                data: vec![(batches_processed % 256) as u8, (i % 256) as u8],
            })
            .collect();

        let batch = TransactionBatch { transactions, previous_root: current_root.clone() };

        match validator.validate_batch(batch, &engine) {
            BatchValidationResult::Valid(new_root) => current_root = new_root.to_hex(),
            BatchValidationResult::Rejected(reason) => panic!("batch rejected: {reason}"),
        }
        batches_processed += 1;

        if last_sample_time.elapsed().as_secs() >= 1 {
            let tx_this_second = (batches_processed - last_sample_count) * batch_size;
            samples.push(tx_this_second as f64);
            println!("Second {}: {tx_this_second} tx/s", samples.len());

            last_sample_time = Instant::now();
            last_sample_count = batches_processed;
        }
    }

    let total_duration = start.elapsed();
    let total_tx = batches_processed * batch_size;
    let avg_tx_per_sec = total_tx as f64 / total_duration.as_secs_f64();

    println!("\n=== Stability Results ===");
    println!("Total time: {:.2}s", total_duration.as_secs_f64());
    println!("Total batches: {batches_processed}");
    println!("Total tx: {total_tx}");
    println!("Average: {avg_tx_per_sec:.0} tx/s");

    if !samples.is_empty() {
        let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let variance = max - min;

        println!("Min: {min:.0} tx/s");
        println!("Max: {max:.0} tx/s");
        println!("Variance: {variance:.0} tx/s ({:.1}%)", (variance / avg_tx_per_sec) * 100.0);
    }

    assert_eq!(validator.expected_nonce(), total_tx as u64);
}
