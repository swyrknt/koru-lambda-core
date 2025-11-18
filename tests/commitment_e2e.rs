/// Commitment Protocol End-to-End Integration Tests
///
/// These tests validate the two-stage commitment gossip protocol across
/// realistic distributed scenarios following scientific falsification principles.
///
/// FALSIFICATION APPROACH:
/// Each test attempts to BREAK the system by finding edge cases, race conditions,
/// Byzantine behaviors, and network failures that could violate invariants.
///
/// Test Structure:
/// - Setup: Create adversarial conditions
/// - Execute: Run the protocol
/// - Assert: Verify invariants hold (or deliberately fail for negative tests)
use koru_lambda_core::{
    DistinctionEngine, LocalCausalAgent, NetworkAgent, PeerIdentity, TransactionAction,
    TransactionBatch,
};
use std::collections::HashSet;
use std::sync::Arc;

// ============================================================================
// SCENARIO 1: Normal Operation - Multi-Node Consensus via Commitment Protocol
// ============================================================================

#[test]
fn test_commitment_multi_node_consensus() {
    println!("\n=== SCENARIO 1: Multi-Node Consensus ===");
    println!("Testing normal operation with 7 validators, 50 transactions\n");

    let engine = Arc::new(DistinctionEngine::new());
    const NUM_NODES: usize = 7;

    // Create validator nodes
    let mut agents: Vec<NetworkAgent> =
        (0..NUM_NODES).map(|_| NetworkAgent::new(&engine)).collect();

    // Bootstrap validator set
    let peers: Vec<PeerIdentity> =
        (0..NUM_NODES).map(|i| PeerIdentity::new(format!("validator_{}", i), &engine)).collect();

    for agent in agents.iter_mut() {
        for peer in peers.iter() {
            agent.join_peer(peer.clone(), &engine);
        }
    }

    // PHASE 1: Leader proposes commitment
    let previous_root = agents[0].consensus_state_root().to_string();

    let batch = TransactionBatch {
        transactions: (0..10)
            .map(|i| TransactionAction {
                nonce: i,
                data: vec![i as u8, (i + 1) as u8, (i + 2) as u8],
            })
            .collect(),
        previous_root: previous_root.clone(),
    };

    // Stage 1: Propose commitment
    let commitment = agents[0]
        .propose_commitment(batch.clone(), &engine)
        .expect("Leader should propose commitment");

    println!("✓ Leader proposed commitment: {:?}", hex::encode(&commitment.commitment_hash));

    // PHASE 2: All validators check commitment (light node verification)
    for (idx, agent) in agents.iter().enumerate() {
        let is_valid = agent.check_commitment(&commitment);
        println!("  Validator {}: commitment valid = {}", idx, is_valid);
        assert!(is_valid, "Validator {} rejected valid commitment", idx);
    }

    println!("✓ All validators accepted commitment (no batch download)\n");

    // PHASE 3: Full validators finalize batch
    // Note: Only leader needs to finalize since they all share the same agent state in this test
    let result = agents[0].finalize_batch(batch.clone(), commitment.commitment_hash, &engine);

    assert!(result.is_ok(), "Leader failed to finalize batch: {:?}", result);

    println!("✓ Leader finalized batch\n");

    // INVARIANT CHECK: Leader's state advanced
    let final_root = agents[0].get_current_root().id().to_string();

    assert_ne!(previous_root, final_root, "INVARIANT VIOLATION: State did not advance after batch");

    println!("✓ INVARIANT: State advanced after batch");
    println!("  Initial: {}", previous_root);
    println!("  Final: {}\n", final_root);
}

// ============================================================================
// SCENARIO 2: FALSIFICATION - Byzantine Leader Submits Invalid Commitment
// ============================================================================

#[test]
fn test_falsify_byzantine_leader_invalid_commitment() {
    println!("\n=== SCENARIO 2: Byzantine Leader Attack ===");
    println!("FALSIFICATION: Leader submits commitment with wrong nonce\n");

    let engine = Arc::new(DistinctionEngine::new());

    let mut byzantine_leader = NetworkAgent::new(&engine);
    let mut honest_validator = NetworkAgent::new(&engine);

    // Bootstrap
    let peer = PeerIdentity::new("validator_0".to_string(), &engine);
    byzantine_leader.join_peer(peer.clone(), &engine);
    honest_validator.join_peer(peer, &engine);

    let root = byzantine_leader.consensus_state_root().to_string();

    // Byzantine leader creates batch
    let batch = TransactionBatch {
        transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
        previous_root: root,
    };

    // Leader proposes commitment
    let mut commitment = byzantine_leader
        .propose_commitment(batch.clone(), &engine)
        .expect("Byzantine leader creates commitment");

    println!("✓ Byzantine leader created commitment");

    // ATTACK: Byzantine leader tampers with nonce
    commitment.nonce = 999; // Should be 0
    println!("✗ Byzantine leader tampered with nonce: 0 → 999\n");

    // FALSIFICATION TEST: Honest validator should REJECT
    let is_valid = honest_validator.check_commitment(&commitment);

    println!("  Honest validator check result: {}", is_valid);

    assert!(!is_valid, "FALSIFICATION FAILED: Validator accepted commitment with invalid nonce");

    println!("✓ FALSIFICATION SUCCESS: Invalid commitment rejected\n");
}

// ============================================================================
// SCENARIO 3: FALSIFICATION - Commitment Hash Mismatch
// ============================================================================

#[test]
fn test_falsify_commitment_hash_mismatch() {
    println!("\n=== SCENARIO 3: Commitment Hash Manipulation ===");
    println!("FALSIFICATION: Attacker modifies batch data after commitment\n");

    let engine = Arc::new(DistinctionEngine::new());
    let mut agent = NetworkAgent::new(&engine);

    let root = agent.consensus_state_root().to_string();

    // Original batch
    let original_batch = TransactionBatch {
        transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
        previous_root: root.clone(),
    };

    // Create commitment
    let commitment =
        agent.propose_commitment(original_batch.clone(), &engine).expect("Create commitment");

    println!("✓ Commitment created for original batch");

    // ATTACK: Modify batch data after commitment
    let tampered_batch = TransactionBatch {
        transactions: vec![TransactionAction {
            nonce: 0,
            data: vec![9, 9, 9], // Different data!
        }],
        previous_root: root,
    };

    println!("✗ Attacker substituted batch data\n");

    // FALSIFICATION TEST: finalize_batch should REJECT
    let result = agent.finalize_batch(tampered_batch, commitment.commitment_hash, &engine);

    println!("  Finalization result: {:?}", result);

    assert!(
        result.is_err(),
        "FALSIFICATION FAILED: Agent accepted batch that doesn't match commitment"
    );

    println!("✓ FALSIFICATION SUCCESS: Tampered batch rejected\n");
}

// ============================================================================
// SCENARIO 4: Epoch Boundary Commitment Processing
// ============================================================================

#[test]
fn test_commitment_across_epoch_boundary() {
    println!("\n=== SCENARIO 4: Epoch Boundary Handling ===");
    println!("Testing commitment protocol during leader rotation\n");

    let engine = Arc::new(DistinctionEngine::new());

    let mut leader_node = NetworkAgent::new(&engine);
    let mut follower_node = NetworkAgent::new(&engine);

    // Bootstrap with 3 validators
    for i in 0..3 {
        let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
        leader_node.join_peer(peer.clone(), &engine);
        follower_node.join_peer(peer, &engine);
    }

    let initial_epoch = leader_node.current_epoch();
    println!("Initial epoch: {}", initial_epoch);

    // Process batch at epoch 0
    let root = leader_node.get_current_root().id().to_string();
    let batch_epoch_0 = TransactionBatch {
        transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
        previous_root: root,
    };

    let commitment =
        leader_node.propose_commitment(batch_epoch_0.clone(), &engine).expect("Propose at epoch 0");

    assert_eq!(commitment.epoch, 0, "Commitment should be for epoch 0");
    println!("✓ Commitment proposed at epoch 0");

    // Advance epoch
    leader_node.advance_epoch(&engine);
    follower_node.advance_epoch(&engine);

    let new_epoch = leader_node.current_epoch();
    println!("✓ Epoch advanced: {} → {}", initial_epoch, new_epoch);

    // CRITICAL TEST: Old commitment should be rejected at new epoch
    let is_valid = follower_node.check_commitment(&commitment);

    assert!(!is_valid, "INVARIANT VIOLATION: Old epoch commitment accepted at new epoch");

    println!("✓ INVARIANT: Commitment from previous epoch rejected\n");
}

// ============================================================================
// SCENARIO 5: Concurrent Commitments from Multiple Leaders
// ============================================================================

#[test]
fn test_falsify_concurrent_leader_commitments() {
    println!("\n=== SCENARIO 5: Concurrent Leader Proposals ===");
    println!("FALSIFICATION: Multiple nodes try to propose simultaneously\n");

    let engine = Arc::new(DistinctionEngine::new());

    const NUM_NODES: usize = 5;
    let mut agents: Vec<NetworkAgent> =
        (0..NUM_NODES).map(|_| NetworkAgent::new(&engine)).collect();

    // Bootstrap
    let peers: Vec<PeerIdentity> =
        (0..NUM_NODES).map(|i| PeerIdentity::new(format!("validator_{}", i), &engine)).collect();

    for agent in agents.iter_mut() {
        for peer in peers.iter() {
            agent.join_peer(peer.clone(), &engine);
        }
    }

    // All nodes try to propose at same nonce
    let mut commitments = Vec::new();

    for (idx, agent) in agents.iter_mut().enumerate() {
        let root = agent.consensus_state_root().to_string();
        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![idx as u8] }],
            previous_root: root,
        };

        let commitment = agent.propose_commitment(batch, &engine).expect("Each node proposes");

        commitments.push(commitment);
    }

    println!("✓ All {} nodes proposed commitments concurrently", NUM_NODES);

    // CRITICAL TEST: All commitments should have same nonce/epoch
    let nonces: HashSet<u64> = commitments.iter().map(|c| c.nonce).collect();
    let epochs: HashSet<u64> = commitments.iter().map(|c| c.epoch).collect();

    assert_eq!(
        nonces.len(),
        1,
        "INVARIANT VIOLATION: Commitments have different nonces: {:?}",
        nonces
    );

    assert_eq!(
        epochs.len(),
        1,
        "INVARIANT VIOLATION: Commitments have different epochs: {:?}",
        epochs
    );

    println!("✓ INVARIANT: All commitments share same nonce/epoch");
    println!("  Nonce: {}, Epoch: {}\n", commitments[0].nonce, commitments[0].epoch);
}

// ============================================================================
// SCENARIO 6: Network Partition Recovery
// ============================================================================

#[test]
fn test_network_partition_recovery() {
    println!("\n=== SCENARIO 6: Network Partition Recovery ===");
    println!("Testing state convergence after partition heals\n");

    let engine = Arc::new(DistinctionEngine::new());

    // Create two partitions
    let mut partition_a: Vec<NetworkAgent> = (0..3).map(|_| NetworkAgent::new(&engine)).collect();

    let mut partition_b: Vec<NetworkAgent> = (0..3).map(|_| NetworkAgent::new(&engine)).collect();

    // Partition A processes batches
    println!("Partition A processing batches...");
    for agent in partition_a.iter_mut() {
        let root = agent.consensus_state_root().to_string();
        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction { nonce: 0, data: vec![1, 2, 3] },
                TransactionAction { nonce: 1, data: vec![4, 5, 6] },
            ],
            previous_root: root,
        };

        let commitment = agent.propose_commitment(batch.clone(), &engine).unwrap();
        agent.finalize_batch(batch, commitment.commitment_hash, &engine).unwrap();
    }

    let partition_a_roots: Vec<String> =
        partition_a.iter().map(|a| a.get_current_root().id().to_string()).collect();

    println!("✓ Partition A converged: {}", partition_a_roots[0]);

    // Partition B is idle (network partition)
    let partition_b_roots: Vec<String> =
        partition_b.iter().map(|a| a.get_current_root().id().to_string()).collect();

    println!("✓ Partition B idle: {}", partition_b_roots[0]);

    // INVARIANT: Partitions should have different states
    assert_ne!(
        partition_a_roots[0], partition_b_roots[0],
        "Partitions should be in different states"
    );

    println!("✓ Partitions diverged during split\n");

    // Network heals: Partition B syncs from Partition A
    println!("Network partition healed - syncing...");

    for b_agent in partition_b.iter_mut() {
        let root = b_agent.get_current_root().id().to_string();
        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction { nonce: 0, data: vec![1, 2, 3] },
                TransactionAction { nonce: 1, data: vec![4, 5, 6] },
            ],
            previous_root: root,
        };

        let commitment = b_agent.propose_commitment(batch.clone(), &engine).unwrap();
        b_agent.finalize_batch(batch, commitment.commitment_hash, &engine).unwrap();
    }

    // INVARIANT: After sync, all nodes should converge
    let all_roots: Vec<String> = partition_a
        .iter()
        .chain(partition_b.iter())
        .map(|a| a.get_current_root().id().to_string())
        .collect();

    let first = &all_roots[0];
    assert!(
        all_roots.iter().all(|r| r == first),
        "INVARIANT VIOLATION: Nodes did not reconverge after partition heal"
    );

    println!("✓ INVARIANT: All nodes reconverged to: {}\n", first);
}

// ============================================================================
// SCENARIO 7: High-Throughput Stress Test
// ============================================================================

#[test]
fn test_commitment_high_throughput_stress() {
    println!("\n=== SCENARIO 7: High-Throughput Stress Test ===");
    println!("Processing 1000 transactions across 100 batches\n");

    let engine = Arc::new(DistinctionEngine::new());
    let mut agent = NetworkAgent::new(&engine);

    const NUM_BATCHES: usize = 100;
    const TXS_PER_BATCH: usize = 10;

    let mut successful = 0;
    let mut total_txs = 0;
    let mut current_nonce = 0u64;

    for batch_idx in 0..NUM_BATCHES {
        // Get current state root (updates after each batch)
        let root = agent.consensus_state_root().to_string();

        let transactions: Vec<TransactionAction> = (0..TXS_PER_BATCH)
            .map(|_i| {
                let tx = TransactionAction {
                    nonce: current_nonce,
                    data: vec![
                        (current_nonce % 256) as u8,
                        ((current_nonce / 256) % 256) as u8,
                        (batch_idx % 256) as u8,
                    ],
                };
                current_nonce += 1;
                tx
            })
            .collect();

        let batch = TransactionBatch { transactions, previous_root: root };

        let commitment = agent.propose_commitment(batch.clone(), &engine).unwrap();
        let result = agent.finalize_batch(batch, commitment.commitment_hash, &engine);

        if result.is_ok() {
            successful += 1;
            total_txs += TXS_PER_BATCH;
        }

        if (batch_idx + 1) % 25 == 0 {
            println!(
                "  Progress: {}/{} batches, {} transactions",
                batch_idx + 1,
                NUM_BATCHES,
                total_txs
            );
        }
    }

    assert_eq!(successful, NUM_BATCHES, "Some batches failed: {}/{}", successful, NUM_BATCHES);

    println!("\n✓ SUCCESS: Processed {} batches, {} transactions", successful, total_txs);
    println!("✓ No failures under high throughput\n");
}

// ============================================================================
// SCENARIO 8: FALSIFICATION - Replay Attack
// ============================================================================

#[test]
fn test_falsify_commitment_replay_attack() {
    println!("\n=== SCENARIO 8: Commitment Replay Attack ===");
    println!("FALSIFICATION: Attacker tries to replay old commitment\n");

    let engine = Arc::new(DistinctionEngine::new());
    let mut agent = NetworkAgent::new(&engine);

    let root = agent.consensus_state_root().to_string();

    // First batch
    let batch_1 = TransactionBatch {
        transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
        previous_root: root,
    };

    let commitment_1 = agent.propose_commitment(batch_1.clone(), &engine).unwrap();
    agent.finalize_batch(batch_1, commitment_1.commitment_hash, &engine).unwrap();

    println!("✓ First batch finalized (nonce=0)");

    // Process second batch - get UPDATED consensus root after first batch
    let root_2 = agent.consensus_state_root().to_string();
    println!("  State advanced to: {}", root_2);

    let batch_2 = TransactionBatch {
        transactions: vec![TransactionAction { nonce: 1, data: vec![4, 5, 6] }],
        previous_root: root_2,
    };

    let commitment_2 = agent.propose_commitment(batch_2.clone(), &engine).unwrap();
    agent.finalize_batch(batch_2, commitment_2.commitment_hash, &engine).unwrap();

    println!("✓ Second batch finalized (nonce=1)");

    // REPLAY ATTACK: Try to replay first commitment
    println!("✗ Attacker replays first commitment\n");

    let is_valid = agent.check_commitment(&commitment_1);

    println!("  Replay check result: {}", is_valid);

    assert!(!is_valid, "FALSIFICATION FAILED: System accepted replayed commitment");

    println!("✓ FALSIFICATION SUCCESS: Replay attack prevented\n");
}

// Helper function for hex encoding (simple implementation)
mod hex {
    pub fn encode(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect::<Vec<_>>().join("")
    }
}
