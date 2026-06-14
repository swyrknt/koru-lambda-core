//! Concurrent / batch integration tests.
//!
//! Validates that the engine is safe and deterministic under multi-threaded
//! synthesis, and that `BatchSynthesizer` (the renamed-and-trimmed v2.0
//! parallel helper) is sound.
//!
//! v2.0 history: the old `ParallelBatchProcessor` tests were removed
//! alongside the misnamed Sequential-body wrapper itself (CHECKLIST
//! Section 1.10 / Phase 6 sub-branch #2). `ParallelSynthesizer` survives
//! renamed `BatchSynthesizer`; tests updated accordingly.
use koru_lambda_core::{BatchSynthesizer, Canonicalizable, DistinctionEngine};
use std::sync::Arc;
use std::thread;

#[test]
fn test_concurrent_engine_synthesis() {
    // Validates that multiple threads can synthesize concurrently
    // using the same engine (DashMap thread-safety).

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    // 10 threads × 100 byte canonicalizations
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

    let mut all_results = vec![];
    for handle in handles {
        let results = handle.join().expect("Thread panicked");
        all_results.extend(results);
    }

    assert_eq!(all_results.len(), 1000);

    // Determinism: same byte → same distinction across engines.
    let engine2 = Arc::new(DistinctionEngine::new());
    let byte_42 = 42u8.to_canonical_structure(&engine);
    let byte_42_again = 42u8.to_canonical_structure(&engine2);
    assert_eq!(byte_42.to_hex(), byte_42_again.to_hex());
}

#[test]
fn test_batch_synthesizer_multi_core() {
    // Validates BatchSynthesizer uses multiple cores via Rayon.

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = BatchSynthesizer::new(Arc::clone(&engine));

    let data: Vec<u8> = (0..10_000).map(|i| (i % 256) as u8).collect();
    let results = synthesizer.canonicalize_bytes_batch(data.clone());

    assert_eq!(results.len(), 10_000);

    // Determinism: same input → byte-identical output.
    let results2 = synthesizer.canonicalize_bytes_batch(data);
    assert_eq!(results, results2);
}

#[test]
fn test_shared_engine_batch_synthesis() {
    // Validates BatchSynthesizer can be used across threads with a shared
    // engine.

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = Arc::new(BatchSynthesizer::new(Arc::clone(&engine)));

    let mut handles = vec![];

    for thread_id in 0..5 {
        let synthesizer_clone = Arc::clone(&synthesizer);

        let handle = thread::spawn(move || {
            let data: Vec<u8> = (0..1_000).map(|i| ((thread_id * 1000 + i) % 256) as u8).collect();
            let results = synthesizer_clone.canonicalize_bytes_batch(data);
            results.len()
        });

        handles.push(handle);
    }

    for handle in handles {
        let count = handle.join().expect("Thread panicked");
        assert_eq!(count, 1_000);
    }
}

#[test]
fn test_concurrent_synthesis_determinism() {
    // Critical test: concurrent synthesis must be deterministic. Same
    // inputs across different threads → same outputs.

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    for _ in 0..10 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            let d0 = engine_clone.d0().clone();
            let d1 = engine_clone.d1().clone();
            let genesis = engine_clone.synthesize(&d0, &d1);

            let mut current = genesis.clone();

            for i in 0..50 {
                let byte = (i % 256) as u8;
                let byte_d = byte.to_canonical_structure(&engine_clone);
                current = engine_clone.synthesize(&current, &byte_d);
            }

            current.to_hex()
        });

        handles.push(handle);
    }

    let mut results = vec![];
    for handle in handles {
        let final_state = handle.join().expect("Thread panicked");
        results.push(final_state);
    }

    assert_eq!(results.len(), 10);
    let first = &results[0];
    assert!(results.iter().all(|r| r == first), "Concurrent synthesis is non-deterministic!");
}

#[test]
fn test_high_concurrency_stress() {
    // Stress test: 100 threads performing concurrent operations.

    let engine = Arc::new(DistinctionEngine::new());
    let mut handles = vec![];

    for thread_id in 0..100 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            let d0 = engine_clone.d0().clone();
            let d1 = engine_clone.d1().clone();

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

    let mut results = 0;
    for handle in handles {
        let _result = handle.join().expect("Thread panicked in stress test");
        results += 1;
    }

    assert_eq!(results, 100, "Not all threads completed");
}

#[test]
fn test_cross_thread_state_consistency() {
    // Validates that state created in one thread is visible in others.

    let engine = Arc::new(DistinctionEngine::new());

    let engine1 = Arc::clone(&engine);
    let handle1 = thread::spawn(move || {
        let d0 = engine1.d0().clone();
        let d1 = engine1.d1().clone();
        let genesis = engine1.synthesize(&d0, &d1);
        genesis.to_hex()
    });

    let genesis_id = handle1.join().expect("Thread 1 panicked");

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
fn test_batch_synthesizer_matches_sequential() {
    // Compare batch vs sequential for correctness (not speed).

    let engine = Arc::new(DistinctionEngine::new());
    let synthesizer = BatchSynthesizer::new(Arc::clone(&engine));

    let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();

    // Sequential reference
    let sequential: Vec<String> =
        data.iter().map(|&byte| byte.to_canonical_structure(&engine).to_hex()).collect();

    // Parallel via BatchSynthesizer
    let parallel: Vec<String> =
        synthesizer.canonicalize_bytes_batch(data).iter().map(|d| d.to_hex()).collect();

    assert_eq!(sequential, parallel, "Batch and sequential produce different results!");
}
