//! Falsification Test: WASM Consistency
//!
//! Validates that the WASM FFI layer preserves system guarantees:
//!
//! * **Determinism**: WASM ≡ native (same inputs → same outputs).
//! * **Axiom preservation**: all five axioms hold across the FFI boundary.
//! * **LocalCausalAgent compliance**: subsystems maintain
//!   `ΔNew = ΔLocal ⊕ ΔAction` through WASM.
//! * **Byzantine resistance**: WASM cannot bypass commitment or atomic
//!   failure semantics.
//!
//! # Sub-branch #10 rewrite (CHECKLIST 1.8 / W13)
//!
//! v1.2.0 wrote these as host-side `#[test]`s under
//! `#[cfg(feature = "wasm")]`. They compiled under
//! `--features wasm` but **could not run** — `wasm_bindgen::JsValue`
//! aborts on `non-wasm32` targets ("function not implemented on
//! non-wasm32 targets"). The tests have therefore never exercised
//! anything since the WASM rewrite landed.
//!
//! Sub-branch #10 converts them to `#[wasm_bindgen_test]`. The
//! recommended driver is:
//!
//! ```sh
//! wasm-pack test --node --features wasm
//! ```
//!
//! `cargo test --features wasm` will still compile this file (the
//! `#[wasm_bindgen_test]` attribute degrades to a no-op on non-wasm
//! targets), but the tests only execute under a real wasm runtime.

#![cfg(feature = "wasm")]

use koru_lambda_core::wasm::*;
use koru_lambda_core::DistinctionEngine;
use std::sync::Arc;
use wasm_bindgen_test::wasm_bindgen_test;

/// Lowercase 8-byte hex preview of a 16-byte ID, used for diagnostic
/// prints under the wasm-bindgen-test console adapter. Replaces the
/// v1.2.0 `&str[..16]` slicing that broke after the bytes-canonical
/// migration.
fn hex_prefix(bytes: &[u8]) -> String {
    bytes.iter().take(8).map(|b| format!("{:02x}", b)).collect()
}

#[wasm_bindgen_test]
fn falsify_wasm_native_divergence() {
    let native = Arc::new(DistinctionEngine::new());
    let wasm = WasmEngine::new();

    assert_eq!(wasm.d0_id(), native.d0().as_bytes().to_vec());
    assert_eq!(wasm.d1_id(), native.d1().as_bytes().to_vec());

    let d0 = wasm.d0_id();
    let d1 = wasm.d1_id();
    let wasm_d2 = wasm.synthesize(&d0, &d1).unwrap();
    let native_d2 = native.synthesize(native.d0(), native.d1());
    assert_eq!(wasm_d2, native_d2.as_bytes().to_vec());

    let wasm_d3 = wasm.synthesize(&wasm_d2, &d0).unwrap();
    let native_d3 = native.synthesize(&native_d2, native.d0());
    assert_eq!(wasm_d3, native_d3.as_bytes().to_vec());

    // hex_prefix only used in this test's diagnostic line; reference
    // it once so dead-code warnings stay quiet.
    let _ = hex_prefix(&wasm_d3);
}

#[wasm_bindgen_test]
fn falsify_wasm_axiom_violations() {
    let wasm = WasmEngine::new();
    let d0 = wasm.d0_id();
    let d1 = wasm.d1_id();

    assert_ne!(d0, d1);
    assert_eq!(wasm.distinction_count(), 2);

    let s1 = wasm.synthesize(&d0, &d1).unwrap();
    let s2 = wasm.synthesize(&d0, &d1).unwrap();
    assert_eq!(s1, s2);

    let ab = wasm.synthesize(&d0, &d1).unwrap();
    let ba = wasm.synthesize(&d1, &d0).unwrap();
    assert_eq!(ab, ba);

    let aa = wasm.synthesize(&d0, &d0).unwrap();
    assert_eq!(aa, d0);
}

#[wasm_bindgen_test]
fn falsify_wasm_subsystem_local_causal_violations() {
    let engine = WasmEngine::new();

    let mut agent = WasmNetworkAgent::new(&engine);
    let r0 = agent.current_root();
    agent.join_peer("v0").unwrap();
    let r1 = agent.current_root();
    assert_ne!(r1, r0);
    agent.join_peer("v1").unwrap();
    let r2 = agent.current_root();
    assert_ne!(r2, r1);

    let mut validator = WasmValidator::new(&engine);
    let v0 = validator.current_root();
    let batch = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1]}],
        "previous_root": id_to_hex(&v0).unwrap()
    })
    .to_string();
    let v1 = validator.validate_batch(&batch).unwrap();
    assert_ne!(v1, v0);

    let commitment = WasmCommitmentAgent::new(&engine);
    let c0 = commitment.current_root();
    assert_eq!(c0.len(), 16);
}

#[wasm_bindgen_test]
fn falsify_wasm_byzantine_commitment_bypass() {
    let engine = WasmEngine::new();
    let mut agent = WasmNetworkAgent::new(&engine);
    agent.join_peer("attacker").unwrap();

    let batch = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
        "previous_root": id_to_hex(&agent.consensus_root()).unwrap()
    })
    .to_string();

    let legit_hash = agent.propose_commitment(&batch).unwrap();
    let fake_hash = vec![0xdeu8; 32];
    assert!(agent.finalize_batch(&batch, &fake_hash).is_err());

    assert!(!agent.check_commitment(&legit_hash, 999, 0, "attacker", 1).unwrap());
    assert!(!agent.check_commitment(&legit_hash, 0, 999, "attacker", 1).unwrap());
}

#[wasm_bindgen_test]
fn falsify_wasm_validator_atomic_failure_bypass() {
    let engine = WasmEngine::new();
    let mut validator = WasmValidator::new(&engine);
    let initial_root = validator.current_root();
    let initial_nonce = validator.expected_nonce();

    let attack_batch = serde_json::json!({
        "transactions": [
            {"nonce": 0, "data": [1]},
            {"nonce": 1, "data": [2]},
            {"nonce": 999, "data": [3]}
        ],
        "previous_root": id_to_hex(&initial_root).unwrap()
    })
    .to_string();

    assert!(validator.validate_batch(&attack_batch).is_err());
    assert_eq!(validator.current_root(), initial_root);
    assert_eq!(validator.expected_nonce(), initial_nonce);
}

#[wasm_bindgen_test]
fn falsify_wasm_deterministic_leader_manipulation() {
    let engine = WasmEngine::new();
    let mut agent1 = WasmNetworkAgent::new(&engine);
    let mut agent2 = WasmNetworkAgent::new(&engine);
    for i in 0..10 {
        let peer = format!("validator_{}", i);
        agent1.join_peer(&peer).unwrap();
        agent2.join_peer(&peer).unwrap();
    }
    assert_eq!(agent1.get_leader(), agent2.get_leader());

    agent1.advance_epoch().unwrap();
    agent2.advance_epoch().unwrap();
    assert_eq!(agent1.get_leader(), agent2.get_leader());
}

#[wasm_bindgen_test]
fn falsify_wasm_concurrent_state_corruption() {
    let engine = WasmEngine::new();
    let mut agent1 = WasmNetworkAgent::new(&engine);
    let mut agent2 = WasmNetworkAgent::new(&engine);
    let mut validator1 = WasmValidator::new(&engine);
    let mut validator2 = WasmValidator::new(&engine);

    agent1.join_peer("concurrent_1").unwrap();
    agent2.join_peer("concurrent_2").unwrap();
    assert_eq!(agent1.current_root().len(), 16);
    assert_eq!(agent2.current_root().len(), 16);

    let batch1 = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1]}],
        "previous_root": id_to_hex(&validator1.current_root()).unwrap()
    })
    .to_string();
    let batch2 = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [2]}],
        "previous_root": id_to_hex(&validator2.current_root()).unwrap()
    })
    .to_string();
    let v1 = validator1.validate_batch(&batch1).unwrap();
    let v2 = validator2.validate_batch(&batch2).unwrap();
    assert_ne!(v1, v2);
}

#[wasm_bindgen_test]
fn falsify_wasm_serialization_corruption() {
    let engine = WasmEngine::new();
    let mut validator = WasmValidator::new(&engine);

    // Note: V3 caps `data` at MAX_TX_DATA_BYTES (4 KiB). All these
    // patterns are well under the cap.
    let test_cases: Vec<Vec<u8>> = vec![
        vec![0u8],
        vec![255u8],
        vec![0, 127, 255],
        vec![1, 2, 3, 4, 5, 6, 7, 8, 9, 10],
        (0..=255u8).collect(),
    ];

    for (i, data) in test_cases.iter().enumerate() {
        let batch = serde_json::json!({
            "transactions": [{"nonce": i as u64, "data": data}],
            "previous_root": id_to_hex(&validator.current_root()).unwrap()
        })
        .to_string();
        assert!(validator.validate_batch(&batch).is_ok());
    }
}

#[wasm_bindgen_test]
fn falsify_wasm_subsystem_isolation_breach() {
    let engine = WasmEngine::new();
    let mut network = WasmNetworkAgent::new(&engine);
    let mut validator = WasmValidator::new(&engine);
    let commitment = WasmCommitmentAgent::new(&engine);

    let net_r0 = network.current_root();
    let val_r0 = validator.current_root();
    let com_r0 = commitment.current_root();

    assert_ne!(net_r0, val_r0);
    assert_ne!(val_r0, com_r0);
    assert_ne!(net_r0, com_r0);

    network.join_peer("peer").unwrap();
    let net_r1 = network.current_root();
    assert_eq!(validator.current_root(), val_r0);
    assert_eq!(commitment.current_root(), com_r0);

    let batch = serde_json::json!({
        "transactions": [{"nonce": 0, "data": [1]}],
        "previous_root": id_to_hex(&val_r0).unwrap()
    })
    .to_string();
    validator.validate_batch(&batch).unwrap();
    assert_eq!(network.current_root(), net_r1);
}

#[wasm_bindgen_test]
fn id_to_hex_round_trip() {
    let wasm = WasmEngine::new();
    let d0 = wasm.d0_id();
    let hex = id_to_hex(&d0).unwrap();
    assert_eq!(hex.len(), 32);
    let back = id_from_hex(&hex).unwrap();
    assert_eq!(back, d0);
}
