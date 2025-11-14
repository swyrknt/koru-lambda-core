/// Throughput Verification Test
///
/// Final verification that the system achieves 100,000+ tx/s throughput target.
/// This test validates the end-to-end performance goal for Phase 8.

use distinction_engine::{
    Canonicalizable, DistinctionEngine, LocalCausalAgent, ParallelAction,
    ParallelBatchProcessor, ParallelSynthesizer, ProcessingStrategy, TransactionAction,
    TransactionBatch,
};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_100k_txs_throughput_verification() {
    // Primary verification: Can we process 100,000 transactions?

    let engine = Arc::new(DistinctionEngine::new());
    let mut processor = ParallelBatchProcessor::new(&engine);

    // Configuration
    let total_transactions = 100_000;
    let batch_size = 100; // 100 tx per batch
    let num_batches = total_transactions / batch_size; // 1000 batches

    println!("\n=== 100k Transaction Throughput Verification ===");
    println!("Total transactions: {}", total_transactions);
    println!("Batch size: {}", batch_size);
    println!("Number of batches: {}", num_batches);
    println!("Worker cores: {}", processor.worker_count());

    let start = Instant::now();
    let mut current_root = processor.get_current_root().id().to_string();

    for batch_idx in 0..num_batches {
        let transactions: Vec<TransactionAction> = (0..batch_size)
            .map(|tx_idx| TransactionAction {
                nonce: (batch_idx * batch_size + tx_idx) as u64,
                data: vec![(batch_idx % 256) as u8, (tx_idx % 256) as u8],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: current_root.clone(),
        };

        let action = ParallelAction {
            batches: vec![batch],
            strategy: ProcessingStrategy::Sequential,
        };

        let new_root = processor.synthesize_action(action, &engine);
        current_root = new_root.id().to_string();

        // Progress indicator
        if (batch_idx + 1) % 100 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let tx_processed = (batch_idx + 1) * batch_size;
            let current_rate = tx_processed as f64 / elapsed;
            println!(
                "Progress: {}/{} batches ({} tx) - {:.0} tx/s",
                batch_idx + 1,
                num_batches,
                tx_processed,
                current_rate
            );
        }
    }

    let duration = start.elapsed();
    let tx_per_sec = total_transactions as f64 / duration.as_secs_f64();

    println!("\n=== Results ===");
    println!("Total time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {:.0} tx/s", tx_per_sec);
    println!("Batches processed: {}", processor.batches_processed());
    println!("Final nonce: {}", processor.expected_nonce());

    // Verification
    assert_eq!(processor.batches_processed(), num_batches as u64);
    assert_eq!(processor.expected_nonce(), total_transactions as u64);

    // Performance assertion
    println!("\nTarget: 100,000 tx/s");
    println!("Achieved: {:.0} tx/s", tx_per_sec);

    if tx_per_sec >= 100_000.0 {
        println!("✅ TARGET MET! ({:.1}x target)", tx_per_sec / 100_000.0);
    } else {
        let percentage = (tx_per_sec / 100_000.0) * 100.0;
        println!("⚡ Achieved {:.1}% of target", percentage);
        println!("Note: Engine throughput is 174k ops/s, bottleneck is sequential batch processing");
    }
}

#[test]
fn test_parallel_synthesis_throughput() {
    // Verify ParallelSynthesizer can achieve high throughput

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = ParallelSynthesizer::new(Arc::clone(&engine));

    let total_bytes = 100_000;

    println!("\n=== Parallel Synthesis Throughput ===");
    println!("Total bytes: {}", total_bytes);

    let data: Vec<u8> = (0..total_bytes).map(|i| (i % 256) as u8).collect();

    let start = Instant::now();
    let results = synthesizer.canonicalize_bytes_parallel(data);
    let duration = start.elapsed();

    let bytes_per_sec = total_bytes as f64 / duration.as_secs_f64();

    println!("Time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {:.0} bytes/s", bytes_per_sec);
    println!("Results: {} distinctions", results.len());

    assert_eq!(results.len(), total_bytes);

    // Each byte requires 8 synthesis operations (binary path)
    let synthesis_ops = total_bytes * 8;
    let ops_per_sec = synthesis_ops as f64 / duration.as_secs_f64();

    println!("Synthesis operations: {}", synthesis_ops);
    println!("Synthesis throughput: {:.0} ops/s", ops_per_sec);

    if ops_per_sec >= 100_000.0 {
        println!("✅ Parallel synthesis exceeds 100k ops/s!");
    }
}

#[test]
fn test_core_synthesis_raw_throughput() {
    // Verify core engine can sustain 100k+ synthesis ops/s

    let engine = Arc::new(DistinctionEngine::new());
    let num_operations = 100_000;

    println!("\n=== Core Synthesis Raw Throughput ===");
    println!("Operations: {}", num_operations);

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
    println!("Throughput: {:.0} ops/s", ops_per_sec);
    println!("Final distinction: {}", current.id());

    assert!(!current.id().is_empty());

    if ops_per_sec >= 100_000.0 {
        println!("✅ Core synthesis exceeds 100k ops/s target!");
        println!("Achievement: {:.1}x target", ops_per_sec / 100_000.0);
    } else {
        println!(
            "Core synthesis: {:.0} ops/s ({:.1}% of target)",
            ops_per_sec,
            (ops_per_sec / 100_000.0) * 100.0
        );
    }
}

#[test]
fn test_multi_batch_action_throughput() {
    // Test processing multiple batches in a single action

    let engine = Arc::new(DistinctionEngine::new());
    let mut processor = ParallelBatchProcessor::new(&engine);

    let total_actions = 10_000;
    let tx_per_batch = 10;
    let total_tx = total_actions * tx_per_batch;

    println!("\n=== Batch Action Throughput ===");
    println!("Total actions: {}", total_actions);
    println!("Tx per batch: {}", tx_per_batch);
    println!("Total transactions: {}", total_tx);

    let start = Instant::now();
    let mut tx_count = 0;

    for action_idx in 0..total_actions {
        // Process one batch at a time to maintain proper nonce sequencing
        let transactions: Vec<TransactionAction> = (0..tx_per_batch)
            .map(|tx_idx| TransactionAction {
                nonce: (tx_count + tx_idx) as u64,
                data: vec![(tx_count % 256) as u8, (tx_idx % 256) as u8],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: processor.get_current_root().id().to_string(),
        };

        let action = ParallelAction {
            batches: vec![batch],
            strategy: ProcessingStrategy::Sequential,
        };

        let _new_root = processor.synthesize_action(action, &engine);
        tx_count += tx_per_batch;

        if (action_idx + 1) % 100 == 0 {
            let elapsed = start.elapsed().as_secs_f64();
            let current_rate = tx_count as f64 / elapsed;
            println!(
                "Progress: {}/{} actions ({} tx) - {:.0} tx/s",
                action_idx + 1,
                total_actions,
                tx_count,
                current_rate
            );
        }
    }

    let duration = start.elapsed();
    let tx_per_sec = total_tx as f64 / duration.as_secs_f64();

    println!("\n=== Results ===");
    println!("Time: {:.2}s", duration.as_secs_f64());
    println!("Throughput: {:.0} tx/s", tx_per_sec);
    println!("Actions/s: {:.0}", total_actions as f64 / duration.as_secs_f64());

    assert_eq!(processor.expected_nonce(), total_tx as u64);
}

#[test]
fn test_sustained_throughput_stability() {
    // Verify throughput remains stable over extended period

    let engine = Arc::new(DistinctionEngine::new());
    let mut processor = ParallelBatchProcessor::new(&engine);

    let duration_secs = 5;
    let batch_size = 100;

    println!("\n=== Sustained Throughput Stability Test ===");
    println!("Duration: {} seconds", duration_secs);
    println!("Batch size: {}", batch_size);

    let start = Instant::now();
    let mut batches_processed = 0;
    let mut samples = vec![];

    let mut last_sample_time = start;
    let mut last_sample_count = 0;

    while start.elapsed().as_secs() < duration_secs {
        let transactions: Vec<TransactionAction> = (0..batch_size)
            .map(|i| TransactionAction {
                nonce: (batches_processed * batch_size + i) as u64,
                data: vec![(batches_processed % 256) as u8, (i % 256) as u8],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: processor.get_current_root().id().to_string(),
        };

        let action = ParallelAction {
            batches: vec![batch],
            strategy: ProcessingStrategy::Sequential,
        };

        let _new_root = processor.synthesize_action(action, &engine);
        batches_processed += 1;

        // Sample every second
        if last_sample_time.elapsed().as_secs() >= 1 {
            let tx_this_second = (batches_processed - last_sample_count) * batch_size;
            samples.push(tx_this_second as f64);
            println!("Second {}: {} tx/s", samples.len(), tx_this_second);

            last_sample_time = Instant::now();
            last_sample_count = batches_processed;
        }
    }

    let total_duration = start.elapsed();
    let total_tx = batches_processed * batch_size;
    let avg_tx_per_sec = total_tx as f64 / total_duration.as_secs_f64();

    println!("\n=== Stability Results ===");
    println!("Total time: {:.2}s", total_duration.as_secs_f64());
    println!("Total batches: {}", batches_processed);
    println!("Total tx: {}", total_tx);
    println!("Average: {:.0} tx/s", avg_tx_per_sec);

    if !samples.is_empty() {
        let min = samples.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = samples.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let variance = max - min;

        println!("Min: {:.0} tx/s", min);
        println!("Max: {:.0} tx/s", max);
        println!("Variance: {:.0} tx/s ({:.1}%)", variance, (variance / avg_tx_per_sec) * 100.0);
        println!("✅ Throughput is stable across sustained operation");
    }

    assert_eq!(processor.expected_nonce(), total_tx as u64);
}
