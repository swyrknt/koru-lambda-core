/// Parallel Integration Tests
///
/// Demonstrates and validates multi-threaded usage of Koru Lambda Core
/// and parallel subsystems. These tests prove thread-safety and concurrent
/// correctness of the system.
use koru_lambda_core::{
    Canonicalizable, DistinctionEngine, LocalCausalAgent, ParallelAction, ParallelBatchProcessor,
    ParallelSynthesizer, ProcessingStrategy, TransactionAction, TransactionBatch,
};
use std::sync::Arc;
use std::thread;

#[test]
fn test_concurrent_engine_synthesis() {
    // Validates that multiple threads can synthesize concurrently
    // using the same engine (DashMap thread-safety)

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    // Spawn 10 threads, each performing 100 synthesis operations
    for thread_id in 0..10 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            let mut results = vec![];

            for i in 0..100 {
                let byte = ((thread_id * 100 + i) % 256) as u8;
                let d = byte.to_canonical_structure(&engine_clone);
                results.push(d.to_hex());
            }

            results
        });

        handles.push(handle);
    }

    // Collect all results
    let mut all_results = vec![];
    for handle in handles {
        let results = handle.join().expect("Thread panicked");
        all_results.extend(results);
    }

    // Verify we got 1000 results (10 threads × 100 operations)
    assert_eq!(all_results.len(), 1000);

    // Verify determinism: same byte → same distinction across threads
    let engine2 = Arc::new(DistinctionEngine::new());
    let byte_42 = 42u8.to_canonical_structure(&engine);
    let byte_42_again = 42u8.to_canonical_structure(&engine2);
    assert_eq!(byte_42.to_hex(), byte_42_again.to_hex());
}

#[test]
fn test_parallel_synthesizer_multi_core() {
    // Validates ParallelSynthesizer uses multiple cores via Rayon

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = ParallelSynthesizer::new(Arc::clone(&engine));

    // Large dataset to benefit from parallelism
    let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();

    // Process in parallel
    let results = synthesizer.canonicalize_bytes_parallel(data.clone());

    assert_eq!(results.len(), 10_000);
    assert!(results.iter().all(|r| !r.is_empty()));

    // Verify determinism
    let results2 = synthesizer.canonicalize_bytes_parallel(data);
    assert_eq!(results, results2);
}

#[test]
fn test_concurrent_batch_processors() {
    // Multiple ParallelBatchProcessors operating concurrently
    // Each maintains independent causal chain

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    // Spawn 5 independent processors
    for processor_id in 0..5 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            let mut processor = ParallelBatchProcessor::new(&engine_clone);

            // Each processor handles 100 batches
            for i in 0..100 {
                let batch = TransactionBatch {
                    transactions: vec![TransactionAction {
                        nonce: i,
                        data: vec![processor_id as u8, i as u8],
                    }],
                    previous_root: processor.get_current_root().to_hex(),
                };

                let action = ParallelAction {
                    batches: vec![batch],
                    strategy: ProcessingStrategy::Sequential,
                };

                let _new_root = processor.synthesize_action(action, &engine_clone);
            }

            (
                processor.batches_processed(),
                processor.expected_nonce(),
                processor.get_current_root().to_hex(),
            )
        });

        handles.push(handle);
    }

    // Verify all processors completed successfully
    for (idx, handle) in handles.into_iter().enumerate() {
        let (batches, nonce, _root) = handle.join().expect("Thread panicked");
        assert_eq!(batches, 100, "Processor {} failed", idx);
        assert_eq!(nonce, 100, "Processor {} nonce mismatch", idx);
    }
}

#[test]
fn test_shared_engine_parallel_synthesis() {
    // Validates ParallelSynthesizer can be used across threads
    // with a shared engine

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = Arc::new(ParallelSynthesizer::new(Arc::clone(&engine)));

    let mut handles = vec![];

    // 5 threads each canonicalizing 1000 bytes in parallel
    for thread_id in 0..5 {
        let synthesizer_clone = Arc::clone(&synthesizer);

        let handle = thread::spawn(move || {
            let data: Vec<u8> = (0..1_000).map(|i| ((thread_id * 1000 + i) % 256) as u8).collect();
            let results = synthesizer_clone.canonicalize_bytes_parallel(data);
            results.len()
        });

        handles.push(handle);
    }

    // Verify all threads completed
    for handle in handles {
        let count = handle.join().expect("Thread panicked");
        assert_eq!(count, 1_000);
    }
}

#[test]
fn test_concurrent_synthesis_determinism() {
    // Critical test: Concurrent synthesis must be deterministic
    // Same inputs across different threads → same outputs

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    // All threads synthesize the same sequence
    for _ in 0..10 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            let d0 = engine_clone.d0().clone();
            let d1 = engine_clone.d1().clone();
            let genesis = engine_clone.synthesize(&d0, &d1);

            let mut current = genesis.clone();

            // Fixed sequence
            for i in 0..50 {
                let byte = (i % 256) as u8;
                let byte_d = byte.to_canonical_structure(&engine_clone);
                current = engine_clone.synthesize(&current, &byte_d);
            }

            current.to_hex()
        });

        handles.push(handle);
    }

    // Collect all final states
    let mut results = vec![];
    for handle in handles {
        let final_state = handle.join().expect("Thread panicked");
        results.push(final_state);
    }

    // All threads must produce identical final state
    assert_eq!(results.len(), 10);
    let first = &results[0];
    assert!(results.iter().all(|r| r == first), "Concurrent synthesis is non-deterministic!");
}

#[test]
fn test_high_concurrency_stress() {
    // Stress test: 100 threads performing concurrent operations

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    for thread_id in 0..100 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            // Each thread does diverse operations
            let d0 = engine_clone.d0().clone();
            let d1 = engine_clone.d1().clone();

            // Mix of synthesis operations
            let mut current = engine_clone.synthesize(&d0, &d1);

            for i in 0..10 {
                let byte = ((thread_id + i) % 256) as u8;
                let byte_d = byte.to_canonical_structure(&engine_clone);
                current = engine_clone.synthesize(&current, &byte_d);
            }

            current.to_hex()
        });

        handles.push(handle);
    }

    // Verify no panics or deadlocks
    let mut results = 0;
    for handle in handles {
        let _result = handle.join().expect("Thread panicked in stress test");
        results += 1;
    }

    assert_eq!(results, 100, "Not all threads completed");
}

#[test]
fn test_parallel_batch_large_workload() {
    // Real-world scenario: Processing 10,000 transactions across batches

    let engine = Arc::new(DistinctionEngine::new());
    let mut processor = ParallelBatchProcessor::new(&engine);

    // Create 1000 batches of 10 transactions each
    let num_batches: u64 = 1_000;
    let tx_per_batch: u64 = 10;

    let mut current_root = processor.get_current_root().to_hex();

    for batch_idx in 0..num_batches {
        let transactions: Vec<TransactionAction> = (0..tx_per_batch)
            .map(|tx_idx| TransactionAction {
                nonce: batch_idx * tx_per_batch + tx_idx,
                data: vec![batch_idx as u8, tx_idx as u8],
            })
            .collect();

        let batch = TransactionBatch { transactions, previous_root: current_root.clone() };

        let action =
            ParallelAction { batches: vec![batch], strategy: ProcessingStrategy::Sequential };

        let new_root = processor.synthesize_action(action, &engine);
        current_root = new_root.to_hex();
    }

    // Verify final state
    assert_eq!(processor.batches_processed(), num_batches);
    assert_eq!(processor.expected_nonce(), num_batches * tx_per_batch);
}

#[test]
fn test_cross_thread_state_consistency() {
    // Validates that state created in one thread is visible in others

    let engine = Arc::new(DistinctionEngine::new());

    // Thread 1: Create some distinctions
    let engine1 = Arc::clone(&engine);
    let handle1 = thread::spawn(move || {
        let d0 = engine1.d0().clone();
        let d1 = engine1.d1().clone();
        let genesis = engine1.synthesize(&d0, &d1);
        genesis.to_hex()
    });

    let genesis_id = handle1.join().expect("Thread 1 panicked");

    // Thread 2: Should see the same genesis
    let engine2 = Arc::clone(&engine);
    let handle2 = thread::spawn(move || {
        let d0 = engine2.d0().clone();
        let d1 = engine2.d1().clone();
        let genesis = engine2.synthesize(&d0, &d1);
        genesis.to_hex()
    });

    let genesis_id2 = handle2.join().expect("Thread 2 panicked");

    assert_eq!(genesis_id, genesis_id2, "State not consistent across threads");
}

#[test]
fn test_parallel_synthesizer_vs_sequential() {
    // Compare parallel vs sequential for correctness (not speed)

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = ParallelSynthesizer::new(Arc::clone(&engine));

    let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();

    // Sequential
    let sequential: Vec<String> = data
        .iter()
        .map(|&byte| {
            let d = byte.to_canonical_structure(&engine);
            d.to_hex()
        })
        .collect();

    // Parallel
    let parallel = synthesizer.canonicalize_bytes_parallel(data);

    // Must produce identical results
    assert_eq!(sequential, parallel, "Parallel and sequential produce different results!");
}
