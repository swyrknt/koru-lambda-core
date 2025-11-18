/// Performance Validation Tests
///
/// Simple performance tests that validate the system meets design targets
/// without requiring criterion framework complexity.
///
/// Design Targets (adjusted for cross-platform consistency):
/// - 10,000+ synthesis operations/second
/// - 800+ batch validations/second
/// - 90,000+ bytes/s canonicalization
/// - Sub-20-microsecond leader election
/// - 2,500+ tx/s distributed consensus
use koru_lambda_core::{
    Canonicalizable, DistinctionEngine, NetworkAgent, PeerIdentity, StructuralCompactor,
    TransactionAction, TransactionBatch,
};
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_core_synthesis_performance() {
    println!("\n=== Performance: Core Synthesis ===\n");

    let engine = Arc::new(DistinctionEngine::new());

    const NUM_OPERATIONS: usize = 10_000;

    let start = Instant::now();

    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut current = engine.synthesize(&d0, &d1);

    for i in 0..NUM_OPERATIONS {
        let byte = ((i % 256) as u8).to_canonical_structure(&engine);
        current = engine.synthesize(&current, &byte);
    }

    let duration = start.elapsed();
    let ops_per_sec = (NUM_OPERATIONS as f64) / duration.as_secs_f64();

    println!("Operations: {}", NUM_OPERATIONS);
    println!("Duration: {:.3}s", duration.as_secs_f64());
    println!("Throughput: {:.0} ops/s", ops_per_sec);

    // Target: 10,000+ ops/s
    assert!(
        ops_per_sec > 10_000.0,
        "Core synthesis throughput too low: {:.0} ops/s (target: 10,000+ ops/s)",
        ops_per_sec
    );

    println!("\n✓ Target met: {} ops/s > 10,000 ops/s\n", ops_per_sec as u64);
}

#[test]
fn test_batch_validation_performance() {
    println!("\n=== Performance: Batch Validation ===\n");

    let engine = Arc::new(DistinctionEngine::new());
    let mut agent = NetworkAgent::new(&engine);

    // Bootstrap validators
    for i in 0..5 {
        let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
        agent.join_peer(peer, &engine);
    }

    const NUM_BATCHES: usize = 1_000;
    const TXS_PER_BATCH: usize = 10;

    let start = Instant::now();

    for batch_idx in 0..NUM_BATCHES {
        let transactions: Vec<TransactionAction> = (0..TXS_PER_BATCH)
            .map(|i| TransactionAction {
                nonce: (batch_idx * TXS_PER_BATCH + i) as u64,
                data: vec![batch_idx as u8, i as u8],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: agent.consensus_state_root().to_string(),
        };

        let result = {
            let batch_copy = batch;
            let commitment = agent.propose_commitment(batch_copy.clone(), &engine).unwrap();
            agent.finalize_batch(batch_copy, commitment.commitment_hash, &engine)
        };
        assert!(result.is_ok());

        agent.advance_epoch(&engine);
    }

    let duration = start.elapsed();
    let batches_per_sec = (NUM_BATCHES as f64) / duration.as_secs_f64();
    let tx_per_sec = (NUM_BATCHES * TXS_PER_BATCH) as f64 / duration.as_secs_f64();

    println!("Batches: {}", NUM_BATCHES);
    println!("Transactions: {}", NUM_BATCHES * TXS_PER_BATCH);
    println!("Duration: {:.3}s", duration.as_secs_f64());
    println!("Batch throughput: {:.0} batches/s", batches_per_sec);
    println!("Transaction throughput: {:.0} tx/s", tx_per_sec);

    // Target: 800+ batches/s (adjusted for system variance)
    assert!(
        batches_per_sec > 800.0,
        "Batch validation throughput too low: {:.0} batches/s (target: 800+ batches/s)",
        batches_per_sec
    );

    println!("\n✓ Target met: {} batches/s > 800 batches/s", batches_per_sec as u64);
    println!("✓ Transaction rate: {} tx/s\n", tx_per_sec as u64);
}

#[test]
fn test_leader_election_performance() {
    println!("\n=== Performance: Leader Election ===\n");

    let engine = Arc::new(DistinctionEngine::new());
    let mut agent = NetworkAgent::new(&engine);

    // Add 50 validators
    for i in 0..50 {
        let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
        agent.join_peer(peer, &engine);
    }

    const NUM_ELECTIONS: usize = 100_000;

    let start = Instant::now();

    for _ in 0..NUM_ELECTIONS {
        let _leader = agent.get_current_leader();
    }

    let duration = start.elapsed();
    let elections_per_sec = (NUM_ELECTIONS as f64) / duration.as_secs_f64();
    let avg_latency_ns = duration.as_nanos() / NUM_ELECTIONS as u128;

    println!("Elections: {}", NUM_ELECTIONS);
    println!("Duration: {:.3}s", duration.as_secs_f64());
    println!("Throughput: {:.0} elections/s", elections_per_sec);
    println!("Average latency: {} ns", avg_latency_ns);

    // Target: Sub-20-microsecond (< 20,000 ns)
    assert!(
        avg_latency_ns < 20_000,
        "Leader election latency too high: {} ns (target: < 20,000 ns)",
        avg_latency_ns
    );

    println!("\n✓ Target met: {} ns < 20,000 ns (sub-20-microsecond)\n", avg_latency_ns);
}

#[test]
fn test_compaction_performance() {
    println!("\n=== Performance: Graph Compaction ===\n");

    let engine = Arc::new(DistinctionEngine::new());

    // Build large graph
    const GRAPH_SIZE: usize = 10_000;

    println!("Building graph with {} distinctions...", GRAPH_SIZE);

    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut current = engine.synthesize(&d0, &d1);

    for i in 0..GRAPH_SIZE {
        let byte = ((i % 256) as u8).to_canonical_structure(&engine);
        current = engine.synthesize(&current, &byte);
    }

    println!("Graph built: {} distinctions", engine.distinction_count());

    // Perform compaction
    let start = Instant::now();

    let mut compactor = StructuralCompactor::new(&engine);
    compactor.set_hot_threshold(8);
    let _action = compactor.compact(&engine);

    let duration = start.elapsed();

    let stats = compactor.get_stats();

    println!("\nCompaction results:");
    println!("  Duration: {:.1} ms", duration.as_millis());
    println!("  HOT: {}", stats.hot_count);
    println!("  WARM: {}", stats.warm_count);
    println!("  COLD: {}", stats.cold_count);

    // Target: < 100ms for 10,000 node graph
    assert!(
        duration.as_millis() < 100,
        "Compaction too slow: {} ms (target: < 100 ms)",
        duration.as_millis()
    );

    println!("\n✓ Target met: {} ms < 100 ms\n", duration.as_millis());
}

#[test]
fn test_distributed_consensus_throughput() {
    println!("\n=== Performance: Distributed Consensus ===\n");

    let engine = Arc::new(DistinctionEngine::new());

    // Setup 5-node network
    const NUM_NODES: usize = 5;
    let mut nodes: Vec<NetworkAgent> = (0..NUM_NODES).map(|_| NetworkAgent::new(&engine)).collect();

    let validators: Vec<PeerIdentity> =
        (0..NUM_NODES).map(|i| PeerIdentity::new(format!("node_{}", i), &engine)).collect();

    for node in nodes.iter_mut() {
        for validator in validators.iter() {
            node.join_peer(validator.clone(), &engine);
        }
    }

    // Process batches
    const NUM_BATCHES: usize = 1_000;
    const TXS_PER_BATCH: usize = 10;
    const TOTAL_TXS: usize = NUM_BATCHES * TXS_PER_BATCH;

    println!("Processing {} txs across {} nodes...", TOTAL_TXS, NUM_NODES);

    let start = Instant::now();

    for batch_idx in 0..NUM_BATCHES {
        let transactions: Vec<TransactionAction> = (0..TXS_PER_BATCH)
            .map(|i| TransactionAction {
                nonce: (batch_idx * TXS_PER_BATCH + i) as u64,
                data: vec![batch_idx as u8, i as u8],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: nodes[0].consensus_state_root().to_string(),
        };

        // All nodes process
        for node in nodes.iter_mut() {
            let result = {
                let batch_copy = batch.clone();
                let commitment = node.propose_commitment(batch_copy.clone(), &engine).unwrap();
                node.finalize_batch(batch_copy, commitment.commitment_hash, &engine)
            };
            assert!(result.is_ok());
        }

        // Advance epoch
        for node in nodes.iter_mut() {
            node.advance_epoch(&engine);
        }
    }

    let duration = start.elapsed();
    let tx_per_sec = (TOTAL_TXS as f64) / duration.as_secs_f64();

    println!("\nResults:");
    println!("  Total transactions: {}", TOTAL_TXS);
    println!("  Duration: {:.3}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} tx/s", tx_per_sec);
    println!("  Per-node throughput: {:.0} tx/s", tx_per_sec / NUM_NODES as f64);

    // Target: 2,500+ tx/s across 5 nodes (adjusted for system variance)
    assert!(
        tx_per_sec > 2_500.0,
        "Distributed consensus throughput too low: {:.0} tx/s (target: > 2,500 tx/s)",
        tx_per_sec
    );

    println!("\n✓ Distributed consensus validated: {} tx/s\n", tx_per_sec as u64);
}

#[test]
fn test_byte_canonicalization_performance() {
    println!("\n=== Performance: Byte Canonicalization ===\n");

    let engine = Arc::new(DistinctionEngine::new());

    const DATA_SIZE: usize = 100_000;
    let data: Vec<u8> = (0..DATA_SIZE).map(|i| (i % 256) as u8).collect();

    let start = Instant::now();

    for &byte in &data {
        let _ = byte.to_canonical_structure(&engine);
    }

    let duration = start.elapsed();
    let bytes_per_sec = (DATA_SIZE as f64) / duration.as_secs_f64();

    println!("Bytes processed: {}", DATA_SIZE);
    println!("Duration: {:.3}s", duration.as_secs_f64());
    println!("Throughput: {:.0} bytes/s", bytes_per_sec);

    // Target: 90k+ bytes/s (each byte requires full synthesis chain)
    assert!(
        bytes_per_sec > 90_000.0,
        "Byte canonicalization throughput too low: {:.0} bytes/s (target: 90k+ bytes/s)",
        bytes_per_sec
    );

    println!("\n✓ Target met: {} bytes/s > 90k bytes/s\n", bytes_per_sec as u64);
}
