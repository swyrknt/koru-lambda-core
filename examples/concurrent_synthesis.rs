//! Example demonstrating concurrent synthesis using Arc<DistinctionEngine>
//!
//! This example shows how multiple threads can safely synthesize distinctions
//! concurrently using the same shared engine.

use koru_lambda_core::DistinctionEngine;
use std::sync::Arc;
use std::thread;

fn main() {
    // Create shared engine
    let engine = Arc::new(DistinctionEngine::new());

    println!("Concurrent Synthesis Example");
    println!("============================\n");

    // Spawn multiple threads that synthesize concurrently
    let mut handles = vec![];

    for i in 0..4 {
        let engine_clone = Arc::clone(&engine);

        let handle = thread::spawn(move || {
            let d0 = engine_clone.d0().clone();
            let d1 = engine_clone.d1().clone();

            // Each thread performs synthesis
            let result = engine_clone.synthesize(&d0, &d1);

            println!("Thread {} synthesized: {}", i, &result.to_hex()[..16]);
            result
        });

        handles.push(handle);
    }

    // Wait for all threads
    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // Verify all threads produced identical results (determinism)
    let first_id = results[0].to_hex();
    for (i, result) in results.iter().enumerate() {
        assert_eq!(result.to_hex(), first_id);
        println!("Thread {} result matches", i);
    }

    println!("\n✓ All threads produced identical distinctions!");
    println!("Final distinction count: {}", engine.distinction_count());
}
