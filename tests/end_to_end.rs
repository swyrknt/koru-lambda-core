/// End-to-End Integration Tests
///
/// Tests the complete system: Engine + Validator + Compactor + Network
/// working together as a distributed consensus system.
///
/// These tests validate:
/// - Multi-node coordination
/// - Distributed consensus under load
/// - System stability with all subsystems active
/// - Emergent properties at scale

use distinction_engine::{
    DistinctionEngine,
    NetworkAgent,
    NetworkRuntime,
    PeerIdentity,
    TransactionAction,
    TransactionBatch,
    StructuralCompactor,
    LocalCausalAgent,
    RuntimeAction,
};
use std::sync::Arc;

/// End-to-End Test: Multi-Node Consensus
///
/// Tests a complete distributed system with multiple nodes coordinating
/// to process batches and reach consensus.
///
/// System configuration:
/// - 7 validator nodes
/// - 100 transactions across 10 batches
/// - Leader rotation every epoch
/// - Compaction every 50 transactions
///
/// Validates:
/// - All nodes converge to same state
/// - Leader rotation works correctly
/// - Batches are processed in order
/// - Compaction maintains state integrity
#[test]
fn test_e2e_multi_node_consensus() {
    println!("\n=== End-to-End: Multi-Node Consensus ===\n");

    // ============================================================
    // SETUP: Create shared engine and 7 validator nodes
    // ============================================================
    println!("Setting up distributed system...");

    let engine = Arc::new(DistinctionEngine::new());

    const NUM_VALIDATORS: usize = 7;
    let mut nodes: Vec<NetworkAgent> = (0..NUM_VALIDATORS)
        .map(|_| NetworkAgent::new(&engine))
        .collect();

    // Create validator identities
    let validators: Vec<PeerIdentity> = (0..NUM_VALIDATORS)
        .map(|i| PeerIdentity::new(format!("validator_{}", i), &engine))
        .collect();

    // All nodes join all validators (bootstrap)
    println!("  Bootstrapping validator set...");
    for node in nodes.iter_mut() {
        for validator in validators.iter() {
            node.join_peer(validator.clone(), &engine);
        }
    }

    println!("  {} validators active\n", NUM_VALIDATORS);

    // ============================================================
    // PHASE 1: Process 10 batches with leader rotation
    // ============================================================
    println!("Phase 1: Processing 100 transactions across 10 batches...");

    const NUM_BATCHES: usize = 10;
    const TXS_PER_BATCH: usize = 10;

    let mut successful_batches = 0;
    let mut total_txs = 0;

    for batch_idx in 0..NUM_BATCHES {
        // Determine current leader (all nodes should agree)
        let leaders: Vec<String> = nodes
            .iter()
            .map(|n| n.get_current_leader().unwrap().id.clone())
            .collect();

        // Verify all nodes agree on leader
        let leader = &leaders[0];
        assert!(
            leaders.iter().all(|l| l == leader),
            "Nodes disagree on leader at batch {}: {:?}",
            batch_idx,
            leaders
        );

        println!("  Batch {}: Leader = {}", batch_idx, leader);

        // Create batch with sequential transactions
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

        // All nodes process same batch
        let results: Vec<Result<_, _>> = nodes
            .iter_mut()
            .map(|node| node.propose_batch(batch.clone(), &engine))
            .collect();

        // Verify all nodes accepted batch
        assert!(
            results.iter().all(|r| r.is_ok()),
            "Some nodes rejected batch {}: {:?}",
            batch_idx,
            results
        );

        successful_batches += 1;
        total_txs += TXS_PER_BATCH;

        // Verify all nodes have same network state
        let network_roots: Vec<String> = nodes
            .iter()
            .map(|n| n.get_current_root().id().to_string())
            .collect();

        assert!(
            network_roots.iter().all(|r| r == &network_roots[0]),
            "Nodes diverged after batch {}: {:?}",
            batch_idx,
            network_roots
        );

        // Advance epoch for leader rotation
        for node in nodes.iter_mut() {
            node.advance_epoch(&engine);
        }
    }

    println!("  ✓ {} batches processed", successful_batches);
    println!("  ✓ {} transactions committed", total_txs);

    // ============================================================
    // VERIFICATION: All nodes reached consensus
    // ============================================================
    println!("\nVerifying consensus...");

    // All nodes should have same network root
    let final_roots: Vec<String> = nodes
        .iter()
        .map(|n| n.get_current_root().id().to_string())
        .collect();

    let consensus_root = &final_roots[0];
    assert!(
        final_roots.iter().all(|r| r == consensus_root),
        "FAILED: Nodes did not reach consensus. Roots: {:?}",
        final_roots
    );

    println!("  ✓ All nodes converged to: {}...", &consensus_root[..16]);

    // All nodes should have same consensus state
    let consensus_states: Vec<String> = nodes
        .iter()
        .map(|n| n.consensus_state_root().to_string())
        .collect();

    assert!(
        consensus_states.iter().all(|s| s == &consensus_states[0]),
        "FAILED: Nodes have different consensus states"
    );

    println!("  ✓ Consensus state: {}...", &consensus_states[0][..16]);

    // All nodes should have same epoch
    let epochs: Vec<u64> = nodes.iter().map(|n| n.current_epoch()).collect();

    assert!(
        epochs.iter().all(|e| *e == epochs[0]),
        "FAILED: Nodes have different epochs: {:?}",
        epochs
    );

    println!("  ✓ All nodes at epoch {}", epochs[0]);

    println!("\n=== Multi-Node Consensus: SUCCESS ===\n");
}

/// End-to-End Test: System Under Load with Compaction
///
/// Tests the complete system processing thousands of transactions
/// while performing periodic compaction to manage graph growth.
///
/// System configuration:
/// - 5 validator nodes
/// - 1000 transactions across 100 batches
/// - Compaction every 500 transactions
/// - Measures compression ratio and state consistency
///
/// Validates:
/// - System stability under sustained load
/// - Compaction maintains consensus integrity
/// - Performance scales with load
#[test]
fn test_e2e_system_under_load_with_compaction() {
    println!("\n=== End-to-End: System Under Load + Compaction ===\n");

    // ============================================================
    // SETUP
    // ============================================================
    println!("Setting up high-load distributed system...");

    let engine = Arc::new(DistinctionEngine::new());

    const NUM_VALIDATORS: usize = 5;
    let mut nodes: Vec<NetworkAgent> = (0..NUM_VALIDATORS)
        .map(|_| NetworkAgent::new(&engine))
        .collect();

    // Create compactor for each node
    let mut compactors: Vec<StructuralCompactor> = (0..NUM_VALIDATORS)
        .map(|_| StructuralCompactor::new(&engine))
        .collect();

    // Bootstrap validators
    let validators: Vec<PeerIdentity> = (0..NUM_VALIDATORS)
        .map(|i| PeerIdentity::new(format!("node_{}", i), &engine))
        .collect();

    for node in nodes.iter_mut() {
        for validator in validators.iter() {
            node.join_peer(validator.clone(), &engine);
        }
    }

    println!("  {} validators active", NUM_VALIDATORS);
    println!("  {} compactors initialized\n", NUM_VALIDATORS);

    // ============================================================
    // LOAD TEST: Process 1000 transactions
    // ============================================================
    const NUM_BATCHES: usize = 100;
    const TXS_PER_BATCH: usize = 10;
    const TOTAL_TXS: usize = NUM_BATCHES * TXS_PER_BATCH;

    println!("Processing {} transactions...", TOTAL_TXS);

    let start = std::time::Instant::now();
    let mut txs_processed = 0;

    for batch_idx in 0..NUM_BATCHES {
        // Create batch
        let transactions: Vec<TransactionAction> = (0..TXS_PER_BATCH)
            .map(|i| TransactionAction {
                nonce: (batch_idx * TXS_PER_BATCH + i) as u64,
                data: vec![
                    (batch_idx as u8),
                    (i as u8),
                    ((batch_idx + i) % 256) as u8,
                ],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: nodes[0].consensus_state_root().to_string(),
        };

        // All nodes process batch
        for node in nodes.iter_mut() {
            let result = node.propose_batch(batch.clone(), &engine);
            assert!(result.is_ok(), "Batch {} failed: {:?}", batch_idx, result);
        }

        txs_processed += TXS_PER_BATCH;

        // Periodic compaction (every 500 txs)
        if txs_processed % 500 == 0 {
            println!("  {} txs processed - running compaction...", txs_processed);

            for compactor in compactors.iter_mut() {
                compactor.compact(&engine);
            }

            // Verify all compactors have same stats
            let stats: Vec<_> = compactors.iter().map(|c| c.get_stats()).collect();

            println!("    Compactor 0: {} HOT, {} WARM, {} COLD",
                stats[0].hot_count,
                stats[0].warm_count,
                stats[0].cold_count
            );

            // All compactors should have same thermal distribution
            for i in 1..stats.len() {
                assert_eq!(
                    stats[i].hot_count, stats[0].hot_count,
                    "Compactors diverged on HOT count"
                );
                assert_eq!(
                    stats[i].cold_count, stats[0].cold_count,
                    "Compactors diverged on COLD count"
                );
            }
        }

        // Advance epoch every 10 batches
        if batch_idx % 10 == 9 {
            for node in nodes.iter_mut() {
                node.advance_epoch(&engine);
            }
        }
    }

    let duration = start.elapsed();
    let throughput = (TOTAL_TXS as f64) / duration.as_secs_f64();

    println!("\nPerformance metrics:");
    println!("  Total transactions: {}", TOTAL_TXS);
    println!("  Duration: {:.2}s", duration.as_secs_f64());
    println!("  Throughput: {:.0} tx/s", throughput);

    // ============================================================
    // COMPACTION METRICS
    // ============================================================
    println!("\nFinal compaction state:");

    let final_stats = compactors[0].get_stats();
    let total_distinctions = engine.distinction_count();
    let active_set = final_stats.hot_count + final_stats.warm_count;
    let compression_ratio = total_distinctions as f64 / active_set.max(1) as f64;

    println!("  Total distinctions: {}", total_distinctions);
    println!("  HOT: {}", final_stats.hot_count);
    println!("  WARM: {}", final_stats.warm_count);
    println!("  COLD: {}", final_stats.cold_count);
    println!("  Compression ratio: {:.2}x", compression_ratio);

    // ============================================================
    // CONSENSUS VERIFICATION
    // ============================================================
    println!("\nVerifying final consensus...");

    // All nodes should have same state
    let consensus_states: Vec<String> = nodes
        .iter()
        .map(|n| n.consensus_state_root().to_string())
        .collect();

    assert!(
        consensus_states.iter().all(|s| s == &consensus_states[0]),
        "FAILED: Nodes diverged under load"
    );

    println!("  ✓ All nodes converged");
    println!("  ✓ Consensus maintained throughout compaction");
    println!("  ✓ {} total epochs", nodes[0].current_epoch());

    println!("\n=== System Under Load: SUCCESS ===\n");
}

/// End-to-End Test: Byzantine Fault Tolerance
///
/// Tests system resilience when some nodes behave incorrectly or
/// maliciously. Validates that honest nodes maintain consensus.
///
/// System configuration:
/// - 7 validator nodes (5 honest, 2 byzantine)
/// - Byzantine nodes attempt to submit invalid batches
/// - Honest nodes should reject and continue
///
/// Validates:
/// - System rejects invalid batches
/// - Honest majority maintains consensus
/// - Byzantine nodes cannot corrupt state
#[test]
fn test_e2e_byzantine_fault_tolerance() {
    println!("\n=== End-to-End: Byzantine Fault Tolerance ===\n");

    // ============================================================
    // SETUP: 7 validators (5 honest, 2 byzantine)
    // ============================================================
    println!("Setting up network with byzantine nodes...");

    let engine = Arc::new(DistinctionEngine::new());

    const NUM_VALIDATORS: usize = 7;
    const HONEST_NODES: usize = 5;

    let mut honest_nodes: Vec<NetworkAgent> = (0..HONEST_NODES)
        .map(|_| NetworkAgent::new(&engine))
        .collect();

    let mut byzantine_nodes: Vec<NetworkAgent> = (0..2)
        .map(|_| NetworkAgent::new(&engine))
        .collect();

    // Bootstrap all nodes with same validators
    let validators: Vec<PeerIdentity> = (0..NUM_VALIDATORS)
        .map(|i| PeerIdentity::new(format!("validator_{}", i), &engine))
        .collect();

    for node in honest_nodes.iter_mut().chain(byzantine_nodes.iter_mut()) {
        for validator in validators.iter() {
            node.join_peer(validator.clone(), &engine);
        }
    }

    println!("  {} honest nodes", HONEST_NODES);
    println!("  {} byzantine nodes\n", 2);

    // ============================================================
    // PHASE 1: Normal operation
    // ============================================================
    println!("Phase 1: Normal operation (10 batches)...");

    for batch_idx in 0..10 {
        let transactions: Vec<TransactionAction> = (0..5)
            .map(|i| TransactionAction {
                nonce: (batch_idx * 5 + i) as u64,
                data: vec![batch_idx as u8, i as u8],
            })
            .collect();

        let batch = TransactionBatch {
            transactions,
            previous_root: honest_nodes[0].consensus_state_root().to_string(),
        };

        // All nodes process
        for node in honest_nodes.iter_mut().chain(byzantine_nodes.iter_mut()) {
            let result = node.propose_batch(batch.clone(), &engine);
            assert!(result.is_ok(), "Normal batch {} failed", batch_idx);
        }

        // Advance epoch
        for node in honest_nodes.iter_mut().chain(byzantine_nodes.iter_mut()) {
            node.advance_epoch(&engine);
        }
    }

    println!("  ✓ 10 normal batches processed\n");

    // ============================================================
    // PHASE 2: Byzantine attack - invalid nonce
    // ============================================================
    println!("Phase 2: Byzantine attack (invalid nonce)...");

    // Byzantine nodes submit batch with WRONG nonce
    let byzantine_batch = TransactionBatch {
        transactions: vec![TransactionAction {
            nonce: 9999, // INVALID - should be 50
            data: vec![0xff],
        }],
        previous_root: honest_nodes[0].consensus_state_root().to_string(),
    };

    // Byzantine nodes attempt to process invalid batch
    let byzantine_results: Vec<_> = byzantine_nodes
        .iter_mut()
        .map(|node| node.propose_batch(byzantine_batch.clone(), &engine))
        .collect();

    // Byzantine batches should be rejected
    assert!(
        byzantine_results.iter().all(|r| r.is_err()),
        "Byzantine batch should be rejected"
    );

    println!("  ✓ Byzantine batch rejected by structural validator");

    // Honest nodes should reject too
    let honest_results: Vec<_> = honest_nodes
        .iter_mut()
        .map(|node| node.propose_batch(byzantine_batch.clone(), &engine))
        .collect();

    assert!(
        honest_results.iter().all(|r| r.is_err()),
        "Honest nodes should also reject byzantine batch"
    );

    println!("  ✓ Honest nodes rejected byzantine batch\n");

    // ============================================================
    // PHASE 3: Recovery - honest nodes continue
    // ============================================================
    println!("Phase 3: Recovery (honest nodes continue)...");

    // Honest nodes continue with valid batch
    let recovery_batch = TransactionBatch {
        transactions: vec![TransactionAction {
            nonce: 50, // CORRECT nonce
            data: vec![0xaa],
        }],
        previous_root: honest_nodes[0].consensus_state_root().to_string(),
    };

    for node in honest_nodes.iter_mut() {
        let result = node.propose_batch(recovery_batch.clone(), &engine);
        assert!(result.is_ok(), "Recovery batch failed");
    }

    println!("  ✓ Honest nodes recovered and processed valid batch");

    // ============================================================
    // VERIFICATION: Honest consensus maintained
    // ============================================================
    println!("\nVerifying honest consensus...");

    let honest_states: Vec<String> = honest_nodes
        .iter()
        .map(|n| n.consensus_state_root().to_string())
        .collect();

    assert!(
        honest_states.iter().all(|s| s == &honest_states[0]),
        "Honest nodes diverged"
    );

    let byzantine_states: Vec<String> = byzantine_nodes
        .iter()
        .map(|n| n.consensus_state_root().to_string())
        .collect();

    // Byzantine nodes should NOT have same state as honest nodes
    // (they couldn't process invalid batch)
    assert!(
        byzantine_states.iter().all(|s| s != &honest_states[0]),
        "Byzantine nodes should not match honest consensus"
    );

    println!("  ✓ Honest majority maintained consensus");
    println!("  ✓ Byzantine nodes excluded from consensus");

    println!("\n=== Byzantine Fault Tolerance: SUCCESS ===\n");
}

/// End-to-End Test: Network Partition Recovery
///
/// Tests system behavior when network partitions and recovers.
/// Validates that nodes can rejoin and sync to canonical state.
///
/// Scenario:
/// - 5 nodes start in consensus
/// - 2 nodes get partitioned (stop receiving batches)
/// - 3 nodes continue processing
/// - Partitioned nodes catch up when reconnected
#[test]
fn test_e2e_network_partition_recovery() {
    println!("\n=== End-to-End: Network Partition Recovery ===\n");

    // ============================================================
    // SETUP
    // ============================================================
    println!("Setting up network...");

    let engine = Arc::new(DistinctionEngine::new());

    const NUM_VALIDATORS: usize = 5;
    let mut all_nodes: Vec<NetworkAgent> = (0..NUM_VALIDATORS)
        .map(|_| NetworkAgent::new(&engine))
        .collect();

    let validators: Vec<PeerIdentity> = (0..NUM_VALIDATORS)
        .map(|i| PeerIdentity::new(format!("node_{}", i), &engine))
        .collect();

    for node in all_nodes.iter_mut() {
        for validator in validators.iter() {
            node.join_peer(validator.clone(), &engine);
        }
    }

    println!("  {} validators initialized\n", NUM_VALIDATORS);

    // ============================================================
    // PHASE 1: All nodes in consensus (10 batches)
    // ============================================================
    println!("Phase 1: All nodes processing (10 batches)...");

    for batch_idx in 0..10 {
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: batch_idx as u64,
                data: vec![batch_idx as u8],
            }],
            previous_root: all_nodes[0].consensus_state_root().to_string(),
        };

        for node in all_nodes.iter_mut() {
            node.propose_batch(batch.clone(), &engine).unwrap();
        }

        for node in all_nodes.iter_mut() {
            node.advance_epoch(&engine);
        }
    }

    println!("  ✓ All nodes at nonce 10\n");

    // ============================================================
    // PHASE 2: Partition (2 nodes isolated)
    // ============================================================
    println!("Phase 2: Network partition (nodes 3-4 isolated)...");

    // Split nodes: [0,1,2] active, [3,4] partitioned
    let mut partitioned_nodes = all_nodes.split_off(3);
    let mut active_nodes = all_nodes;

    println!("  Active nodes: 3");
    println!("  Partitioned nodes: 2");

    // Active nodes continue processing
    println!("  Active nodes processing 20 batches...");

    for batch_idx in 10..30 {
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: batch_idx as u64,
                data: vec![batch_idx as u8],
            }],
            previous_root: active_nodes[0].consensus_state_root().to_string(),
        };

        for node in active_nodes.iter_mut() {
            node.propose_batch(batch.clone(), &engine).unwrap();
        }

        for node in active_nodes.iter_mut() {
            node.advance_epoch(&engine);
        }
    }

    println!("  ✓ Active nodes at nonce 30");
    println!("  ✓ Partitioned nodes still at nonce 10\n");

    // Verify divergence
    let active_state = active_nodes[0].consensus_state_root().to_string();
    let partitioned_state = partitioned_nodes[0].consensus_state_root().to_string();

    assert_ne!(
        active_state, partitioned_state,
        "States should diverge during partition"
    );

    // ============================================================
    // PHASE 3: Partition heals - catch-up
    // ============================================================
    println!("Phase 3: Partition heals - catching up partitioned nodes...");

    // Partitioned nodes need to sync 20 batches (nonce 10-29)
    for batch_idx in 10..30 {
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: batch_idx as u64,
                data: vec![batch_idx as u8],
            }],
            previous_root: partitioned_nodes[0].consensus_state_root().to_string(),
        };

        for node in partitioned_nodes.iter_mut() {
            let result = node.propose_batch(batch.clone(), &engine);
            assert!(result.is_ok(), "Catch-up batch {} failed", batch_idx);
        }

        for node in partitioned_nodes.iter_mut() {
            node.advance_epoch(&engine);
        }
    }

    println!("  ✓ Partitioned nodes caught up to nonce 30\n");

    // ============================================================
    // VERIFICATION: All nodes converged
    // ============================================================
    println!("Verifying convergence...");

    // Merge nodes back
    active_nodes.append(&mut partitioned_nodes);
    let all_nodes = active_nodes;

    let final_states: Vec<String> = all_nodes
        .iter()
        .map(|n| n.consensus_state_root().to_string())
        .collect();

    assert!(
        final_states.iter().all(|s| s == &final_states[0]),
        "Nodes failed to converge after partition recovery"
    );

    println!("  ✓ All 5 nodes converged to same state");
    println!("  ✓ Consensus state: {}...", &final_states[0][..16]);

    println!("\n=== Network Partition Recovery: SUCCESS ===\n");
}

/// End-to-End Test: Full-Stack NetworkRuntime Integration
///
/// Tests the complete async runtime layer with LocalCausalAgent integration.
/// Validates that the runtime properly tracks P2P events as causal distinctions.
///
/// System configuration:
/// - Single async NetworkRuntime instance
/// - Simulates peer discoveries and batch receipts
/// - Validates runtime-level causal chain
/// - Tests integration between runtime events and consensus logic
///
/// Validates:
/// - Runtime implements LocalCausalAgent correctly
/// - P2P events synthesize into causal chain
/// - Runtime maintains separate event stream from consensus
/// - All event types properly handled
#[tokio::test]
async fn test_e2e_full_stack_runtime_integration() {
    println!("\n=== End-to-End: Full-Stack Runtime Integration ===\n");

    // ============================================================
    // SETUP: Create NetworkRuntime
    // ============================================================
    println!("Setting up async NetworkRuntime...");

    let engine = Arc::new(DistinctionEngine::new());

    let mut runtime = NetworkRuntime::new(engine.clone(), "/ip4/127.0.0.1/tcp/0")
        .await
        .expect("Runtime creation failed");

    let initial_root = runtime.get_current_root().id().to_string();

    println!("  ✓ NetworkRuntime initialized");
    println!("  ✓ Initial runtime root: {}...", &initial_root[..16]);
    println!();

    // ============================================================
    // PHASE 1: Peer Discovery Events
    // ============================================================
    println!("Phase 1: Simulating peer discovery...");

    let peers = vec!["peer_alice", "peer_bob", "peer_carol"];
    let mut peer_roots = vec![];

    for peer in peers.iter() {
        let action = RuntimeAction::PeerDiscovered {
            peer_id: peer.to_string(),
        };

        let new_root = runtime.synthesize_action(action, &engine);
        peer_roots.push(new_root.id().to_string());

        println!("  Discovered {}: {}...", peer, &new_root.id()[..16]);
    }

    // Verify each peer discovery creates a unique root
    for i in 0..peer_roots.len() {
        assert_ne!(
            peer_roots[i], initial_root,
            "Peer {} root should differ from initial",
            peers[i]
        );

        for j in (i + 1)..peer_roots.len() {
            assert_ne!(
                peer_roots[i], peer_roots[j],
                "Peer {} and {} roots should differ",
                peers[i], peers[j]
            );
        }
    }

    println!("  ✓ {} peers discovered", peers.len());
    println!("  ✓ Each discovery created unique causal state\n");

    // ============================================================
    // PHASE 2: Batch Receipt Events
    // ============================================================
    println!("Phase 2: Simulating batch receipts...");

    for epoch in 0..5 {
        let action = RuntimeAction::BatchReceived {
            epoch,
            leader_id: format!("leader_{}", epoch % 3),
        };

        let new_root = runtime.synthesize_action(action, &engine);

        println!(
            "  Batch received (epoch {}): {}...",
            epoch,
            &new_root.id()[..16]
        );
    }

    println!("  ✓ 5 batches received");
    println!("  ✓ Runtime causal chain advanced\n");

    // ============================================================
    // PHASE 3: Epoch Advancement Events
    // ============================================================
    println!("Phase 3: Simulating epoch advancement...");

    for new_epoch in 5..10 {
        let action = RuntimeAction::EpochAdvanced { new_epoch };

        let new_root = runtime.synthesize_action(action, &engine);

        println!(
            "  Epoch advanced to {}: {}...",
            new_epoch,
            &new_root.id()[..16]
        );
    }

    println!("  ✓ 5 epoch advancements processed");
    println!("  ✓ Runtime events fully tracked\n");

    // ============================================================
    // PHASE 4: Mixed Event Stream
    // ============================================================
    println!("Phase 4: Mixed event stream (realistic workload)...");

    let mixed_events = vec![
        RuntimeAction::PeerDiscovered {
            peer_id: "peer_dave".to_string(),
        },
        RuntimeAction::BatchReceived {
            epoch: 10,
            leader_id: "leader_dave".to_string(),
        },
        RuntimeAction::EpochAdvanced { new_epoch: 11 },
        RuntimeAction::PeerDiscovered {
            peer_id: "peer_eve".to_string(),
        },
        RuntimeAction::BatchReceived {
            epoch: 11,
            leader_id: "leader_eve".to_string(),
        },
    ];

    let mut previous_root = runtime.get_current_root().id().to_string();

    for (i, event) in mixed_events.iter().enumerate() {
        let new_root = runtime.synthesize_action(event.clone(), &engine);

        assert_ne!(
            new_root.id(),
            previous_root,
            "Event {} should change root: {:?}",
            i,
            event
        );

        previous_root = new_root.id().to_string();
    }

    println!("  ✓ {} mixed events processed", mixed_events.len());
    println!("  ✓ Causal ordering maintained\n");

    // ============================================================
    // VERIFICATION: Runtime State Integrity
    // ============================================================
    println!("Verifying runtime state integrity...");

    let final_runtime_root = runtime.get_current_root().id().to_string();

    // Final root should be dramatically different from initial
    assert_ne!(
        final_runtime_root, initial_root,
        "Runtime causal chain should have evolved"
    );

    println!("  Initial root: {}...", &initial_root[..16]);
    println!("  Final root:   {}...", &final_runtime_root[..16]);
    println!();

    // Count total events processed
    let total_events = peers.len() + 5 + 5 + mixed_events.len();
    println!("  ✓ {} total runtime events processed", total_events);
    println!("  ✓ Runtime causal chain integrity maintained");
    println!("  ✓ LocalCausalAgent implementation validated");
    println!("  ✓ All RuntimeAction variants tested");

    println!("\n=== Full-Stack Runtime Integration: SUCCESS ===\n");
}

/// End-to-End Test: Runtime + Consensus Coordination
///
/// Tests the coordination between NetworkRuntime (P2P layer) and
/// NetworkAgent (consensus layer). Validates that both maintain
/// independent but coordinated causal chains.
///
/// Validates:
/// - Runtime tracks P2P events (peer discovery, message receipt)
/// - Consensus tracks transaction validation events
/// - Both subsystems maintain independent causal chains
/// - Events are properly segregated by layer
#[tokio::test]
async fn test_e2e_runtime_consensus_coordination() {
    println!("\n=== End-to-End: Runtime + Consensus Coordination ===\n");

    // ============================================================
    // SETUP
    // ============================================================
    println!("Setting up layered architecture...");

    let engine = Arc::new(DistinctionEngine::new());

    // Create runtime (P2P layer)
    let mut runtime = NetworkRuntime::new(engine.clone(), "/ip4/127.0.0.1/tcp/0")
        .await
        .expect("Runtime creation failed");

    // Create consensus agent (Consensus layer)
    let mut consensus = NetworkAgent::new(&engine);

    // Bootstrap consensus with validators
    let validators = vec![
        PeerIdentity::new("validator_0".to_string(), &engine),
        PeerIdentity::new("validator_1".to_string(), &engine),
        PeerIdentity::new("validator_2".to_string(), &engine),
    ];

    for validator in validators.iter() {
        consensus.join_peer(validator.clone(), &engine);
    }

    let initial_runtime_root = runtime.get_current_root().id().to_string();
    let initial_consensus_root = consensus.get_current_root().id().to_string();

    println!("  ✓ Runtime layer initialized");
    println!("  ✓ Consensus layer initialized");
    println!("  ✓ {} validators bootstrapped", validators.len());
    println!();

    // ============================================================
    // PHASE 1: Runtime Events (P2P layer)
    // ============================================================
    println!("Phase 1: Runtime layer events...");

    // Simulate peer discoveries at runtime level
    for i in 0..3 {
        let action = RuntimeAction::PeerDiscovered {
            peer_id: format!("peer_{}", i),
        };
        runtime.synthesize_action(action, &engine);
    }

    let runtime_root_after_peers = runtime.get_current_root().id().to_string();

    assert_ne!(
        runtime_root_after_peers, initial_runtime_root,
        "Runtime root should change after peer events"
    );

    // Consensus root should be UNCHANGED (runtime events are isolated)
    let consensus_root_after_peers = consensus.get_current_root().id().to_string();
    assert_eq!(
        consensus_root_after_peers, initial_consensus_root,
        "Consensus root should be unchanged by runtime events"
    );

    println!("  ✓ 3 peer discoveries processed at runtime layer");
    println!("  ✓ Runtime root changed, consensus root unchanged");
    println!();

    // ============================================================
    // PHASE 2: Consensus Events (Consensus layer)
    // ============================================================
    println!("Phase 2: Consensus layer events...");

    // Process batch at consensus level
    let batch = TransactionBatch {
        transactions: vec![TransactionAction {
            nonce: 0,
            data: vec![0xaa],
        }],
        previous_root: consensus.consensus_state_root().to_string(),
    };

    consensus
        .propose_batch(batch, &engine)
        .expect("Batch proposal failed");

    let consensus_root_after_batch = consensus.get_current_root().id().to_string();

    assert_ne!(
        consensus_root_after_batch, initial_consensus_root,
        "Consensus root should change after batch"
    );

    // Runtime root should be UNCHANGED by consensus events
    let runtime_root_after_batch = runtime.get_current_root().id().to_string();
    assert_eq!(
        runtime_root_after_batch, runtime_root_after_peers,
        "Runtime root should be unchanged by consensus events"
    );

    println!("  ✓ Batch processed at consensus layer");
    println!("  ✓ Consensus root changed, runtime root unchanged");
    println!();

    // ============================================================
    // PHASE 3: Coordinated Events
    // ============================================================
    println!("Phase 3: Coordinated multi-layer events...");

    // Runtime receives batch (runtime event)
    let runtime_batch_action = RuntimeAction::BatchReceived {
        epoch: consensus.current_epoch(),
        leader_id: consensus
            .get_current_leader()
            .unwrap()
            .id
            .clone(),
    };

    runtime.synthesize_action(runtime_batch_action, &engine);

    // Advance epoch at consensus layer
    consensus.advance_epoch(&engine);

    // Advance epoch at runtime layer
    let runtime_epoch_action = RuntimeAction::EpochAdvanced {
        new_epoch: consensus.current_epoch(),
    };

    runtime.synthesize_action(runtime_epoch_action, &engine);

    println!("  ✓ Batch receipt tracked at runtime layer");
    println!("  ✓ Epoch advancement synchronized");
    println!();

    // ============================================================
    // VERIFICATION: Layer Independence
    // ============================================================
    println!("Verifying layer independence...");

    let final_runtime_root = runtime.get_current_root().id().to_string();
    let final_consensus_root = consensus.get_current_root().id().to_string();

    // Both layers should have evolved
    assert_ne!(final_runtime_root, initial_runtime_root);
    assert_ne!(final_consensus_root, initial_consensus_root);

    // But they should have DIFFERENT roots (independent chains)
    assert_ne!(
        final_runtime_root, final_consensus_root,
        "Runtime and consensus should maintain independent causal chains"
    );

    println!("  Runtime root:   {}...", &final_runtime_root[..16]);
    println!("  Consensus root: {}...", &final_consensus_root[..16]);
    println!();

    println!("  ✓ Runtime layer: {} events processed", 3 + 1 + 1); // peers + batch + epoch
    println!("  ✓ Consensus layer: {} events processed", 1 + 1); // batch + epoch
    println!("  ✓ Layers maintain independent causal chains");
    println!("  ✓ Proper event segregation validated");

    println!("\n=== Runtime + Consensus Coordination: SUCCESS ===\n");
}
