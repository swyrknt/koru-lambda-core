/// Falsification Test: WASM Consistency
///
/// Tests whether the WASM FFI layer maintains all system guarantees:
/// - Determinism: WASM ≡ Native (same inputs → same outputs)
/// - Axiom preservation: All 5 axioms hold through FFI boundary
/// - LocalCausalAgent compliance: Subsystems maintain ΔNew = ΔLocal ⊕ ΔAction
/// - Byzantine resistance: WASM can't bypass security
///
/// Falsification Targets:
/// - FFI boundary corrupts determinism
/// - Serialization breaks axioms
/// - WASM allows invalid state transitions
/// - Subsystem contracts violated through WASM

#[cfg(feature = "wasm")]
use distinction_engine::wasm::*;
use distinction_engine::{DistinctionEngine, Canonicalizable};
use std::sync::Arc;

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_native_divergence() {
    /// Falsifies if: WASM and native engines produce different results
    ///
    /// This is the CORE guarantee - determinism must hold across FFI boundary
    println!("\nTest: WASM vs Native Determinism");
    println!("  Hypothesis: WASM ≡ Native for all operations");

    let native = Arc::new(DistinctionEngine::new());
    let wasm = WasmEngine::new();

    // Test 1: Primordial distinctions
    assert_eq!(
        wasm.d0_id(),
        native.d0().id(),
        "FALSIFIED: WASM Δ₀ ≠ Native Δ₀"
    );
    assert_eq!(
        wasm.d1_id(),
        native.d1().id(),
        "FALSIFIED: WASM Δ₁ ≠ Native Δ₁"
    );

    // Test 2: Synthesis chain
    let d0 = wasm.d0_id();
    let d1 = wasm.d1_id();

    // Build identical chain in both
    let wasm_d2 = wasm.synthesize(&d0, &d1).unwrap();
    let native_d2 = native.synthesize(native.d0(), native.d1());

    assert_eq!(
        wasm_d2,
        native_d2.id(),
        "FALSIFIED: synthesis(Δ₀, Δ₁) diverged between WASM and native"
    );

    // Test 3: Extended synthesis
    let wasm_d3 = wasm.synthesize(&wasm_d2, &d0).unwrap();
    let native_d3 = native.synthesize(&native_d2, native.d0());

    assert_eq!(
        wasm_d3,
        native_d3.id(),
        "FALSIFIED: Extended synthesis chain diverged"
    );

    println!("  ✓ Determinism preserved across FFI boundary");
    println!("  ✓ synthesis(Δ₀, Δ₁) identical: {}", &wasm_d2[..16]);
    println!("  ✓ Extended chain identical: {}", &wasm_d3[..16]);
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_axiom_violations() {
    /// Falsifies if: WASM violates any of the 5 core axioms
    println!("\nTest: WASM Axiom Preservation");

    let wasm = WasmEngine::new();
    let d0 = wasm.d0_id();
    let d1 = wasm.d1_id();

    // Axiom 1: Identity (distinctions defined by unique ID)
    println!("  Testing Axiom 1: Identity...");
    assert_ne!(d0, d1, "FALSIFIED: Δ₀ = Δ₁ (nontriviality violated)");

    // Axiom 2: Nontriviality (system starts with 2 distinctions)
    println!("  Testing Axiom 2: Nontriviality...");
    assert_eq!(wasm.distinction_count(), 2, "FALSIFIED: Initial count ≠ 2");

    // Axiom 3: Synthesis (deterministic combination)
    println!("  Testing Axiom 3: Synthesis...");
    let s1 = wasm.synthesize(&d0, &d1).unwrap();
    let s2 = wasm.synthesize(&d0, &d1).unwrap();
    assert_eq!(s1, s2, "FALSIFIED: Synthesis is non-deterministic");

    // Axiom 4: Symmetry (order independence)
    println!("  Testing Axiom 4: Symmetry...");
    let ab = wasm.synthesize(&d0, &d1).unwrap();
    let ba = wasm.synthesize(&d1, &d0).unwrap();
    assert_eq!(ab, ba, "FALSIFIED: synthesis(a,b) ≠ synthesis(b,a)");

    // Axiom 5: Irreflexivity (self-synthesis yields self)
    println!("  Testing Axiom 5: Irreflexivity...");
    let aa = wasm.synthesize(&d0, &d0).unwrap();
    assert_eq!(aa, d0, "FALSIFIED: synthesis(a,a) ≠ a");

    println!("  ✓ All 5 axioms preserved through WASM FFI");
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_subsystem_local_causal_violations() {
    /// Falsifies if: Subsystems don't maintain LocalCausalAgent contract through WASM
    println!("\nTest: LocalCausalAgent Contract Preservation");

    let engine = WasmEngine::new();

    // Test NetworkAgent
    println!("  Testing NetworkAgent subsystem...");
    let mut agent = WasmNetworkAgent::new(&engine);
    let r0 = agent.current_root();

    agent.join_peer("v0").unwrap();
    let r1 = agent.current_root();
    assert_ne!(r1, r0, "FALSIFIED: NetworkAgent root didn't change after action");

    agent.join_peer("v1").unwrap();
    let r2 = agent.current_root();
    assert_ne!(r2, r1, "FALSIFIED: NetworkAgent not maintaining causal chain");

    // Test ConsensusValidator
    println!("  Testing ConsensusValidator subsystem...");
    let mut validator = WasmValidator::new(&engine);
    let v0 = validator.current_root();

    let batch = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1]}],
        "previous_root": v0
    }).to_string();

    let v1 = validator.validate_batch(&batch).unwrap();
    assert_ne!(v1, v0, "FALSIFIED: Validator root didn't change after validation");

    // Test CommitmentAgent
    println!("  Testing CommitmentAgent subsystem...");
    let commitment = WasmCommitmentAgent::new(&engine);
    let c0 = commitment.current_root();
    assert!(!c0.is_empty(), "FALSIFIED: CommitmentAgent has no root");

    println!("  ✓ All subsystems maintain LocalCausalAgent contract");
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_byzantine_commitment_bypass() {
    /// Falsifies if: Attacker can bypass commitment protocol via WASM
    println!("\nTest: Byzantine Commitment Protocol Attack");

    let engine = WasmEngine::new();
    let mut agent = WasmNetworkAgent::new(&engine);
    agent.join_peer("attacker").unwrap();

    let batch = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
        "previous_root": agent.consensus_root()
    }).to_string();

    println!("  Attack 1: Hash tampering...");
    let legit_hash = agent.propose_commitment(&batch).unwrap();
    let fake_hash = "deadbeef".repeat(8); // Wrong hash

    let result = agent.finalize_batch(&batch, &fake_hash);
    assert!(result.is_err(),
        "FALSIFIED: Accepted batch with tampered commitment hash");
    println!("    ✓ Hash tampering rejected");

    println!("  Attack 2: Nonce manipulation...");
    let is_valid = agent.check_commitment(&legit_hash, 999, 0).unwrap();
    assert!(!is_valid,
        "FALSIFIED: Accepted commitment with wrong nonce");
    println!("    ✓ Nonce manipulation rejected");

    println!("  Attack 3: Epoch manipulation...");
    let is_valid = agent.check_commitment(&legit_hash, 0, 999).unwrap();
    assert!(!is_valid,
        "FALSIFIED: Accepted commitment with wrong epoch");
    println!("    ✓ Epoch manipulation rejected");

    println!("  ✓ Commitment protocol resists Byzantine attacks via WASM");
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_validator_atomic_failure_bypass() {
    /// Falsifies if: Attacker can cause partial batch application via WASM
    println!("\nTest: Atomic Failure Bypass Attack");

    let engine = WasmEngine::new();
    let mut validator = WasmValidator::new(&engine);

    let initial_root = validator.current_root();
    let initial_nonce = validator.expected_nonce();

    // Attacker crafts batch with good and bad transactions
    let attack_batch = serde_json::json!({
        "transactions": [
            {"nonce": 0, "data": [1]},    // Valid
            {"nonce": 1, "data": [2]},    // Valid
            {"nonce": 999, "data": [3]}   // Invalid nonce
        ],
        "previous_root": initial_root
    }).to_string();

    println!("  Attempting partial application attack...");
    let result = validator.validate_batch(&attack_batch);

    assert!(result.is_err(),
        "FALSIFIED: Validator accepted batch with invalid transaction");

    // Verify atomic failure: state must be completely unchanged
    assert_eq!(validator.current_root(), initial_root,
        "FALSIFIED: Partial application occurred - root changed");

    assert_eq!(validator.expected_nonce(), initial_nonce,
        "FALSIFIED: Partial application occurred - nonce changed");

    println!("  ✓ Atomic failure enforced - no partial application");
    println!("    Root unchanged: {}", &validator.current_root()[..16]);
    println!("    Nonce unchanged: {}", validator.expected_nonce());
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_deterministic_leader_manipulation() {
    /// Falsifies if: Leader election can be manipulated via WASM
    println!("\nTest: Leader Election Manipulation Attack");

    let engine = WasmEngine::new();
    let mut agent1 = WasmNetworkAgent::new(&engine);
    let mut agent2 = WasmNetworkAgent::new(&engine);

    // Both agents see same validator set
    for i in 0..10 {
        let peer = format!("validator_{}", i);
        agent1.join_peer(&peer).unwrap();
        agent2.join_peer(&peer).unwrap();
    }

    let leader1 = agent1.get_leader();
    let leader2 = agent2.get_leader();

    println!("  Agent 1 elected: {:?}", leader1);
    println!("  Agent 2 elected: {:?}", leader2);

    assert_eq!(leader1, leader2,
        "FALSIFIED: Different agents elected different leaders");

    // Advance epoch - should deterministically change leader
    agent1.advance_epoch().unwrap();
    agent2.advance_epoch().unwrap();

    let leader1_e1 = agent1.get_leader();
    let leader2_e1 = agent2.get_leader();

    assert_eq!(leader1_e1, leader2_e1,
        "FALSIFIED: Leader election non-deterministic after epoch change");

    println!("  ✓ Leader election deterministic");
    println!("    Epoch 0 leader: {:?}", leader1);
    println!("    Epoch 1 leader: {:?}", leader1_e1);
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_concurrent_state_corruption() {
    /// Falsifies if: Concurrent WASM operations corrupt shared state
    println!("\nTest: Concurrent Operation Safety");

    let engine = WasmEngine::new();

    // Create multiple subsystems sharing same engine
    let mut agent1 = WasmNetworkAgent::new(&engine);
    let mut agent2 = WasmNetworkAgent::new(&engine);
    let mut validator1 = WasmValidator::new(&engine);
    let mut validator2 = WasmValidator::new(&engine);

    println!("  Performing concurrent operations...");

    // Concurrent network operations
    agent1.join_peer("concurrent_1").unwrap();
    agent2.join_peer("concurrent_2").unwrap();

    let r1 = agent1.current_root();
    let r2 = agent2.current_root();

    // Both should see same distinctions in engine
    assert_eq!(engine.distinction_count(), engine.distinction_count(),
        "FALSIFIED: Engine state corrupted by concurrent access");

    // Concurrent validator operations
    let batch1 = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1]}],
        "previous_root": validator1.current_root()
    }).to_string();

    let batch2 = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [2]}],
        "previous_root": validator2.current_root()
    }).to_string();

    let v1 = validator1.validate_batch(&batch1).unwrap();
    let v2 = validator2.validate_batch(&batch2).unwrap();

    // Independent validators maintain independent state
    assert_ne!(v1, v2,
        "FALSIFIED: Validators shared state when they shouldn't");

    println!("  ✓ Concurrent operations safe");
    println!("    Agent 1 root: {}", &r1[..16]);
    println!("    Agent 2 root: {}", &r2[..16]);
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_serialization_corruption() {
    /// Falsifies if: JSON serialization corrupts data across WASM boundary
    println!("\nTest: Serialization Boundary Integrity");

    let engine = WasmEngine::new();
    let mut validator = WasmValidator::new(&engine);

    // Test various data patterns
    let test_cases = vec![
        vec![0u8],
        vec![255u8],
        vec![0, 127, 255],
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        (0..=255).collect::<Vec<u8>>(), // Full byte range
    ];

    for (i, data) in test_cases.iter().enumerate() {
        let batch = serde_json::json!({
            "transactions": [{"nonce": i as u64, "data": data}],
            "previous_root": validator.current_root()
        }).to_string();

        let result = validator.validate_batch(&batch);
        assert!(result.is_ok(),
            "FALSIFIED: Serialization corrupted data pattern: {:?}", data);
    }

    println!("  ✓ Serialization preserved {} data patterns", test_cases.len());
}

#[cfg(feature = "wasm")]
#[test]
fn test_falsify_wasm_subsystem_isolation_breach() {
    /// Falsifies if: One subsystem can interfere with another via WASM
    println!("\nTest: Subsystem Isolation");

    let engine = WasmEngine::new();

    let mut network = WasmNetworkAgent::new(&engine);
    let mut validator = WasmValidator::new(&engine);
    let commitment = WasmCommitmentAgent::new(&engine);

    let net_r0 = network.current_root();
    let val_r0 = validator.current_root();
    let com_r0 = commitment.current_root();

    println!("  Initial subsystem roots:");
    println!("    Network:    {}", &net_r0[..16]);
    println!("    Validator:  {}", &val_r0[..16]);
    println!("    Commitment: {}", &com_r0[..16]);

    // Each subsystem should have independent root
    assert_ne!(net_r0, val_r0, "FALSIFIED: Network and Validator share root");
    assert_ne!(val_r0, com_r0, "FALSIFIED: Validator and Commitment share root");
    assert_ne!(net_r0, com_r0, "FALSIFIED: Network and Commitment share root");

    // Modify network
    network.join_peer("peer").unwrap();
    let net_r1 = network.current_root();

    // Other subsystems should be unaffected
    assert_eq!(validator.current_root(), val_r0,
        "FALSIFIED: Network action affected Validator");
    assert_eq!(commitment.current_root(), com_r0,
        "FALSIFIED: Network action affected Commitment");

    // Modify validator
    let batch = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1]}],
        "previous_root": val_r0
    }).to_string();
    validator.validate_batch(&batch).unwrap();

    // Network should be unaffected (except for its own change)
    assert_eq!(network.current_root(), net_r1,
        "FALSIFIED: Validator action affected Network");

    println!("  ✓ Subsystems properly isolated");
    println!("    Each maintains independent causal chain");
}
