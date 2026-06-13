use std::sync::Arc;
/// WASM Bindings for Koru Lambda Core
///
/// FFI layer exposing the core engine and subsystems to JavaScript/WASM.
///
/// Universal bindings that work in:
/// - Browsers (JavaScript/TypeScript)
/// - Node.js/Deno/Bun (Server-side JS)
/// - Any WASM runtime (Go wazero, Kotlin WasmEdge, etc.)
///
/// Philosophy:
/// - One artifact, infinite platforms
/// - Thin wrappers around core Rust subsystems
/// - The subsystems themselves implement LocalCausalAgent (not these bindings)
use wasm_bindgen::prelude::*;

use crate::subsystems::{
    BatchCommitment,
    CommitmentAgent,
    ConsensusValidator,
    LocalCausalAgent, // Used to access subsystem methods
    NetworkAgent,
    PeerIdentity,
    TransactionBatch,
};
use crate::DistinctionEngine;

/// WASM-friendly Engine wrapper
#[wasm_bindgen]
pub struct WasmEngine {
    inner: Arc<DistinctionEngine>,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Create a new engine with primordial distinctions (Δ₀, Δ₁)
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { inner: Arc::new(DistinctionEngine::new()) }
    }

    /// Get current distinction count
    #[wasm_bindgen(js_name = distinctionCount)]
    pub fn distinction_count(&self) -> usize {
        self.inner.distinction_count()
    }

    /// Get current relationship count
    #[wasm_bindgen(js_name = relationshipCount)]
    pub fn relationship_count(&self) -> usize {
        self.inner.relationship_count()
    }

    /// Get primordial distinction Δ₀ ID as raw bytes
    /// Note: Primordial IDs ("0", "1") return as UTF-8 bytes, synthesized IDs return as binary
    #[wasm_bindgen(js_name = d0Id)]
    pub fn d0_id(&self) -> Vec<u8> {
        id_to_bytes(self.inner.d0().to_hex())
    }

    /// Get primordial distinction Δ₁ ID as raw bytes
    /// Note: Primordial IDs ("0", "1") return as UTF-8 bytes, synthesized IDs return as binary
    #[wasm_bindgen(js_name = d1Id)]
    pub fn d1_id(&self) -> Vec<u8> {
        id_to_bytes(self.inner.d1().to_hex())
    }

    /// Synthesize two distinctions by their IDs (as strings)
    /// Returns the ID of the resulting distinction as raw bytes
    #[wasm_bindgen]
    pub fn synthesize(&self, id_a: &str, id_b: &str) -> Result<Vec<u8>, JsValue> {
        // O(1) lookup using the engine's internal map, replacing the slow O(N) snapshot/linear search.
        let a = self
            .inner
            .get_distinction_by_id(id_a)
            .ok_or_else(|| JsValue::from_str(&format!("Distinction not found: {}", id_a)))?;

        let b = self
            .inner
            .get_distinction_by_id(id_b)
            .ok_or_else(|| JsValue::from_str(&format!("Distinction not found: {}", id_b)))?;

        let result = self.inner.synthesize(&a, &b);
        Ok(id_to_bytes(result.to_hex()))
    }

    /// Batch synthesis benchmark - runs n iterations inside WASM
    /// Eliminates FFI overhead by keeping the loop in Rust
    /// Returns number of completed operations
    #[wasm_bindgen(js_name = benchmarkSynthesis)]
    pub fn benchmark_synthesis(&self, iterations: u32) -> u32 {
        let d0 = self.inner.d0();
        let d1 = self.inner.d1();

        for _ in 0..iterations {
            let _ = self.inner.synthesize(d0, d1);
        }

        iterations
    }
}

/// WASM binding for NetworkAgent subsystem
///
/// Thin wrapper exposing NetworkAgent to JavaScript.
/// The NetworkAgent itself implements LocalCausalAgent:
/// - Locality: Anchored to local network root
/// - Causality: ΔNew = ΔNetwork_Root ⊕ ΔNetwork_Action
/// - Determinism: All network actions are canonicalizable
#[wasm_bindgen]
pub struct WasmNetworkAgent {
    inner: NetworkAgent, // The actual subsystem (implements LocalCausalAgent)
    engine: Arc<DistinctionEngine>,
}

#[wasm_bindgen]
impl WasmNetworkAgent {
    /// Create new network agent subsystem
    #[wasm_bindgen(constructor)]
    pub fn new(engine: &WasmEngine) -> Self {
        Self {
            inner: NetworkAgent::new(&engine.inner), // Subsystem creation
            engine: engine.inner.clone(),
        }
    }

    /// Get network agent's current root as raw bytes (from LocalCausalAgent trait)
    #[wasm_bindgen(js_name = currentRoot)]
    pub fn current_root(&self) -> Vec<u8> {
        id_to_bytes(self.inner.get_current_root().to_hex())
    }

    /// Get consensus state root as raw bytes (managed by validator inside agent)
    #[wasm_bindgen(js_name = consensusRoot)]
    pub fn consensus_root(&self) -> Vec<u8> {
        id_to_bytes(&self.inner.consensus_state_root())
    }

    /// Join peer - agent synthesizes NetworkAction::PeerJoined
    /// Returns new root as raw bytes
    #[wasm_bindgen(js_name = joinPeer)]
    pub fn join_peer(&mut self, peer_id: &str) -> Result<Vec<u8>, JsValue> {
        let peer = PeerIdentity::new(peer_id.to_string(), &self.engine);
        let new_root = self.inner.join_peer(peer, &self.engine);
        Ok(id_to_bytes(new_root.to_hex()))
    }

    /// Bulk join multiple peers - eliminates FFI overhead for batch operations
    /// Returns final root as raw bytes
    #[wasm_bindgen(js_name = joinPeers)]
    pub fn join_peers(&mut self, peer_ids: Vec<String>) -> Result<Vec<u8>, JsValue> {
        let mut new_root = self.inner.get_current_root().clone();
        for peer_id in peer_ids {
            let peer = PeerIdentity::new(peer_id, &self.engine);
            new_root = self.inner.join_peer(peer, &self.engine);
        }
        Ok(id_to_bytes(new_root.to_hex()))
    }

    /// Advance epoch - agent synthesizes NetworkAction::EpochAdvanced
    /// Returns new root as raw bytes
    #[wasm_bindgen(js_name = advanceEpoch)]
    pub fn advance_epoch(&mut self) -> Result<Vec<u8>, JsValue> {
        let new_root = self.inner.advance_epoch(&self.engine);
        Ok(id_to_bytes(new_root.to_hex()))
    }

    /// Get current epoch
    #[wasm_bindgen(js_name = currentEpoch)]
    pub fn current_epoch(&self) -> u64 {
        self.inner.current_epoch()
    }

    /// Get validator count
    #[wasm_bindgen(js_name = validatorCount)]
    pub fn validator_count(&self) -> usize {
        self.inner.validator_count()
    }

    /// Get deterministic leader (pure function of epoch + validator set)
    #[wasm_bindgen(js_name = getLeader)]
    pub fn get_leader(&self) -> Option<String> {
        self.inner.get_current_leader().map(|p| p.id.clone())
    }

    /// Batch leader election benchmark - runs n iterations inside WASM
    /// Eliminates FFI overhead by keeping the loop in Rust
    #[wasm_bindgen(js_name = benchmarkLeaderElection)]
    pub fn benchmark_leader_election(&self, iterations: u32) -> u32 {
        for _ in 0..iterations {
            let _ = self.inner.get_current_leader();
        }
        iterations
    }

    /// Propose commitment (Stage 1) - returns commitment hash as raw bytes
    #[wasm_bindgen(js_name = proposeCommitment)]
    pub fn propose_commitment(&mut self, batch_json: &str) -> Result<Vec<u8>, JsValue> {
        let batch: TransactionBatch = serde_json::from_str(batch_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid batch JSON: {}", e)))?;

        let commitment = self
            .inner
            .propose_commitment(batch, &self.engine)
            .map_err(|e| JsValue::from_str(&e))?;

        Ok(commitment.commitment_hash.to_vec())
    }

    /// Check commitment (Light node verification)
    /// Accepts hash as raw bytes (Uint8Array)
    #[wasm_bindgen(js_name = checkCommitment)]
    pub fn check_commitment(
        &self,
        hash_bytes: &[u8],
        nonce: u64,
        epoch: u64,
    ) -> Result<bool, JsValue> {
        if hash_bytes.len() != 32 {
            return Err(JsValue::from_str("Hash must be 32 bytes"));
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_bytes);

        let commitment = BatchCommitment {
            commitment_hash: hash,
            nonce,
            epoch,
            leader_id: String::new(),
            batch_size: 0,
        };

        Ok(self.inner.check_commitment(&commitment))
    }

    /// Finalize batch (Stage 2)
    /// Accepts hash as raw bytes, returns new root as raw bytes
    #[wasm_bindgen(js_name = finalizeBatch)]
    pub fn finalize_batch(
        &mut self,
        batch_json: &str,
        hash_bytes: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        let batch: TransactionBatch = serde_json::from_str(batch_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid batch JSON: {}", e)))?;

        if hash_bytes.len() != 32 {
            return Err(JsValue::from_str("Hash must be 32 bytes"));
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_bytes);

        let result = self
            .inner
            .finalize_batch(batch, hash, &self.engine)
            .map_err(|e| JsValue::from_str(&e))?;

        Ok(id_to_bytes(result.to_hex()))
    }
}

/// WASM binding for ConsensusValidator subsystem
///
/// Thin wrapper exposing ConsensusValidator to JavaScript.
/// The ConsensusValidator itself implements LocalCausalAgent:
/// - Locality: Anchored to consensus state root
/// - Causality: ΔNew = ΔState_Root ⊕ ΔTransaction
/// - Determinism: All transactions are canonicalizable
#[wasm_bindgen]
pub struct WasmValidator {
    inner: ConsensusValidator, // The actual subsystem (implements LocalCausalAgent)
    engine: Arc<DistinctionEngine>,
}

#[wasm_bindgen]
impl WasmValidator {
    /// Create new consensus validator subsystem
    #[wasm_bindgen(constructor)]
    pub fn new(engine: &WasmEngine) -> Self {
        Self {
            inner: ConsensusValidator::new(&engine.inner), // Subsystem creation
            engine: engine.inner.clone(),
        }
    }

    /// Get validator's current root as raw bytes (from LocalCausalAgent trait)
    #[wasm_bindgen(js_name = currentRoot)]
    pub fn current_root(&self) -> Vec<u8> {
        id_to_bytes(self.inner.get_current_root().to_hex())
    }

    /// Get expected nonce for next transaction
    #[wasm_bindgen(js_name = expectedNonce)]
    pub fn expected_nonce(&self) -> u64 {
        self.inner.expected_nonce()
    }

    /// Validate batch - validator performs atomic causal synthesis
    /// Returns new root as raw bytes
    #[wasm_bindgen(js_name = validateBatch)]
    pub fn validate_batch(&mut self, batch_json: &str) -> Result<Vec<u8>, JsValue> {
        let batch: TransactionBatch = serde_json::from_str(batch_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid batch JSON: {}", e)))?;

        match self.inner.validate_batch(batch, &self.engine) {
            crate::subsystems::BatchValidationResult::Valid(new_root) => {
                Ok(id_to_bytes(new_root.to_hex()))
            },
            crate::subsystems::BatchValidationResult::Rejected(reason) => {
                Err(JsValue::from_str(&reason))
            },
        }
    }

    /// Batch validation benchmark - validates n single-tx batches inside WASM
    /// Eliminates FFI overhead by keeping the loop in Rust
    #[wasm_bindgen(js_name = benchmarkValidation)]
    pub fn benchmark_validation(&mut self, iterations: u32) -> u32 {
        for _ in 0..iterations {
            let batch = TransactionBatch {
                transactions: vec![crate::subsystems::TransactionAction {
                    nonce: self.inner.expected_nonce(),
                    data: vec![1, 2, 3],
                }],
                previous_root: self.inner.get_current_root().to_hex(),
            };
            let _ = self.inner.validate_batch(batch, &self.engine);
        }
        iterations
    }
}

/// WASM binding for CommitmentAgent subsystem
///
/// Thin wrapper exposing CommitmentAgent to JavaScript.
/// The CommitmentAgent itself implements LocalCausalAgent:
/// - Locality: Anchored to commitment root
/// - Causality: ΔNew = ΔCommitment_Root ⊕ ΔBatch_Commitment
/// - Determinism: All commitments are canonicalizable
#[wasm_bindgen]
pub struct WasmCommitmentAgent {
    inner: CommitmentAgent, // The actual subsystem (implements LocalCausalAgent)
}

#[wasm_bindgen]
impl WasmCommitmentAgent {
    /// Create new commitment agent subsystem
    #[wasm_bindgen(constructor)]
    pub fn new(engine: &WasmEngine) -> Self {
        Self {
            inner: CommitmentAgent::new(&engine.inner), // Subsystem creation
        }
    }

    /// Get commitment agent's current root as raw bytes (from LocalCausalAgent trait)
    #[wasm_bindgen(js_name = currentRoot)]
    pub fn current_root(&self) -> Vec<u8> {
        id_to_bytes(self.inner.get_current_root().to_hex())
    }

    /// Get expected nonce for next commitment
    #[wasm_bindgen(js_name = expectedNonce)]
    pub fn expected_nonce(&self) -> u64 {
        self.inner.expected_nonce()
    }

    /// Get total commitments processed by this subsystem
    #[wasm_bindgen(js_name = commitmentsProcessed)]
    pub fn commitments_processed(&self) -> u64 {
        self.inner.commitments_processed()
    }
}

/// Convert distinction ID to bytes.
///
/// Post step 3 of the foundation sub-branch, every Distinction's `.to_hex()`
/// is a 32-character (16-byte) lowercase hex string. The legacy 64-char
/// branch is retained for safety while sub-branch #10 (impl/wasm-bytes-on-wire)
/// rewrites WASM to bytes-canonical.
fn id_to_bytes(id: impl AsRef<str>) -> Vec<u8> {
    let id = id.as_ref();
    if id.len() == 32 {
        return hex::decode(id).unwrap_or_else(|_| id.as_bytes().to_vec());
    }
    if id.len() == 64 {
        return hex::decode(id).unwrap_or_else(|_| id.as_bytes().to_vec());
    }
    // Primordial or short IDs - return as UTF-8.
    id.as_bytes().to_vec()
}

/// Helper for hex decoding (simple implementation)
mod hex {
    pub fn decode(s: &str) -> Result<Vec<u8>, String> {
        if s.len() % 2 != 0 {
            return Err("Hex string must have even length".to_string());
        }

        (0..s.len())
            .step_by(2)
            .map(|i| {
                u8::from_str_radix(&s[i..i + 2], 16).map_err(|e| format!("Invalid hex: {}", e))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to convert bytes to hex string for comparison
    fn to_hex(bytes: &[u8]) -> String {
        bytes.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Test: WASM engine exposes same primordial distinctions as native
    /// Falsifies if: WASM creates different Δ₀, Δ₁ than native engine
    #[test]
    fn test_wasm_engine_primordial_consistency() {
        let native_engine = DistinctionEngine::new();
        let wasm_engine = WasmEngine::new();

        // WASM must have same primordial IDs as native (compare hex)
        assert_eq!(to_hex(&wasm_engine.d0_id()), native_engine.d0().to_hex());
        assert_eq!(to_hex(&wasm_engine.d1_id()), native_engine.d1().to_hex());

        // Initial counts must match
        assert_eq!(wasm_engine.distinction_count(), native_engine.distinction_count());
        assert_eq!(wasm_engine.relationship_count(), native_engine.relationship_count());
    }

    /// Test: WASM synthesis produces identical results to native
    /// Falsifies if: WASM synthesis diverges from native (breaks determinism)
    #[test]
    fn test_wasm_synthesis_determinism() {
        let native_engine = DistinctionEngine::new();
        let wasm_engine = WasmEngine::new();

        // Get IDs as hex strings for synthesis input
        let d0_id = native_engine.d0().to_hex();
        let d1_id = native_engine.d1().to_hex();

        // Synthesize via WASM (returns bytes)
        let wasm_result =
            wasm_engine.synthesize(d0_id, d1_id).expect("WASM synthesis should succeed");

        // Synthesize via native
        let native_result = native_engine.synthesize(native_engine.d0(), native_engine.d1());

        // Results MUST be identical (determinism) - compare hex
        assert_eq!(to_hex(&wasm_result), native_result.to_hex());
    }

    /// Test: WASM respects Axiom of Symmetry
    /// Falsifies if: synthesize(a, b) ≠ synthesize(b, a) in WASM
    #[test]
    fn test_wasm_axiom_symmetry() {
        let native_engine = DistinctionEngine::new();
        let wasm_engine = WasmEngine::new();
        let d0 = native_engine.d0().to_hex();
        let d1 = native_engine.d1().to_hex();

        let ab = wasm_engine.synthesize(d0, d1).unwrap();
        let ba = wasm_engine.synthesize(d1, d0).unwrap();

        // Symmetry - order independence
        assert_eq!(ab, ba, "WASM synthesis violates symmetry axiom");
    }

    /// Test: WASM respects Axiom of Irreflexivity
    /// Falsifies if: synthesize(a, a) ≠ a in WASM
    #[test]
    fn test_wasm_axiom_irreflexivity() {
        let native_engine = DistinctionEngine::new();
        let wasm_engine = WasmEngine::new();
        let d0 = native_engine.d0().to_hex();

        let result = wasm_engine.synthesize(d0, d0).unwrap();

        // Irreflexivity - self-synthesis yields self
        assert_eq!(to_hex(&result), d0, "WASM synthesis violates irreflexivity axiom");
    }

    /// Test: NetworkAgent subsystem maintains LocalCausalAgent contract through WASM
    /// Falsifies if: Subsystem root doesn't change after action synthesis
    #[test]
    fn test_wasm_network_agent_local_causal_compliance() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);

        let initial_root = agent.current_root();

        // Perform causal action: join peer
        let new_root = agent.join_peer("validator_0").expect("Peer join should succeed");

        // LocalCausalAgent contract: ΔNew ≠ ΔOld (action changes state)
        assert_ne!(new_root, initial_root, "NetworkAgent didn't update root after action");

        // Verify agent's current_root matches the returned root
        assert_eq!(
            agent.current_root(),
            new_root,
            "Agent's current_root inconsistent with returned root"
        );
    }

    /// Test: ConsensusValidator subsystem maintains LocalCausalAgent contract through WASM
    /// Falsifies if: Validator doesn't synthesize transactions causally
    #[test]
    fn test_wasm_validator_local_causal_compliance() {
        let engine = WasmEngine::new();
        let mut validator = WasmValidator::new(&engine);

        let initial_root = validator.current_root();
        let initial_nonce = validator.expected_nonce();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": to_hex(&initial_root)
        });

        let new_root =
            validator.validate_batch(&batch.to_string()).expect("Valid batch should succeed");

        // LocalCausalAgent contract: ΔNew ≠ ΔOld
        assert_ne!(new_root, initial_root, "Validator didn't update root after validation");

        // Nonce should increment
        assert_eq!(
            validator.expected_nonce(),
            initial_nonce + 1,
            "Validator nonce didn't increment"
        );
    }

    /// Test: CommitmentAgent subsystem maintains LocalCausalAgent contract through WASM
    /// Falsifies if: Commitment agent doesn't track commitments causally
    #[test]
    fn test_wasm_commitment_agent_local_causal_compliance() {
        let engine = WasmEngine::new();
        let agent = WasmCommitmentAgent::new(&engine);

        // Verify initial state
        assert_eq!(agent.expected_nonce(), 0);
        assert_eq!(agent.commitments_processed(), 0);

        let initial_root = agent.current_root();
        assert!(!initial_root.is_empty(), "Commitment agent should have root");
    }

    /// Test: WASM two-stage commitment protocol maintains integrity
    /// Falsifies if: Commitment hash doesn't match batch data
    #[test]
    fn test_wasm_commitment_protocol_integrity() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);

        // Add validators for leader election
        agent.join_peer("validator_0").unwrap();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": to_hex(&agent.consensus_root())
        })
        .to_string();

        // Stage 1: Propose commitment (returns raw bytes)
        let hash = agent.propose_commitment(&batch).expect("Commitment proposal should succeed");

        assert!(!hash.is_empty(), "Commitment hash should not be empty");
        assert_eq!(hash.len(), 32, "Commitment hash should be 32 bytes");

        // Stage 1: Verify commitment (accepts raw bytes)
        let is_valid =
            agent.check_commitment(&hash, 0, 0).expect("Commitment check should succeed");

        assert!(is_valid, "Valid commitment should pass verification");

        // Stage 2: Finalize with correct hash should succeed (accepts raw bytes)
        let result = agent.finalize_batch(&batch, &hash);
        assert!(result.is_ok(), "Finalization with correct hash should succeed");
    }

    /// Test: WASM rejects tampered commitment hash
    /// Falsifies if: WASM accepts batch with wrong commitment hash
    #[test]
    fn test_wasm_commitment_hash_tampering_rejected() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        agent.join_peer("validator_0").unwrap();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": to_hex(&agent.consensus_root())
        })
        .to_string();

        // Propose commitment
        let _correct_hash =
            agent.propose_commitment(&batch).expect("Commitment proposal should succeed");

        // Attacker creates different hash (32 zero bytes)
        let tampered_hash = vec![0u8; 32];

        // Should reject tampered hash
        let result = agent.finalize_batch(&batch, &tampered_hash);
        assert!(
            result.is_err(),
            "FALSIFICATION FAILED: Accepted batch with tampered commitment hash"
        );
    }

    /// Test: WASM rejects commitment with wrong nonce
    /// Falsifies if: WASM accepts out-of-order transactions
    #[test]
    fn test_wasm_commitment_nonce_enforcement() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        agent.join_peer("validator_0").unwrap();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": to_hex(&agent.consensus_root())
        })
        .to_string();

        let hash = agent.propose_commitment(&batch).expect("Commitment proposal should succeed");

        // Check with wrong nonce (expected 0, checking 999)
        let is_valid = agent.check_commitment(&hash, 999, 0).expect("Check should not error");

        assert!(!is_valid, "FALSIFICATION FAILED: Accepted commitment with wrong nonce");
    }

    /// Test: WASM validator enforces atomic failure
    /// Falsifies if: Partial batch application possible
    #[test]
    fn test_wasm_validator_atomic_failure() {
        let engine = WasmEngine::new();
        let mut validator = WasmValidator::new(&engine);

        let initial_root = validator.current_root();

        // Batch with invalid nonce in second transaction
        let bad_batch = serde_json::json!({
            "transactions": [
                {"nonce": 0, "data": [1, 2, 3]},
                {"nonce": 999, "data": [4, 5, 6]}  // Wrong nonce
            ],
            "previous_root": to_hex(&initial_root)
        })
        .to_string();

        let result = validator.validate_batch(&bad_batch);

        // Should reject entire batch
        assert!(result.is_err(), "Invalid batch should be rejected");

        // Root should be unchanged (atomic failure)
        assert_eq!(
            validator.current_root(),
            initial_root,
            "FALSIFICATION FAILED: Partial batch application occurred"
        );

        // Nonce should be unchanged
        assert_eq!(
            validator.expected_nonce(),
            0,
            "FALSIFICATION FAILED: Nonce changed despite batch rejection"
        );
    }

    /// Test: WASM hex decoding correctness
    /// Falsifies if: Hex conversion corrupts data
    #[test]
    fn test_wasm_hex_decoding_correctness() {
        let original = [42u8; 32];
        let encoded = to_hex(&original);
        let decoded = hex::decode(&encoded).expect("Decoding should succeed");

        assert_eq!(decoded.len(), 32);
        assert_eq!(&decoded[..], &original[..], "Hex decoding corrupted data");
    }

    /// Test: WASM deterministic leader election through subsystem
    /// Falsifies if: Same validator set produces different leaders
    #[test]
    fn test_wasm_deterministic_leader_election() {
        let engine = WasmEngine::new();
        let mut agent1 = WasmNetworkAgent::new(&engine);
        let mut agent2 = WasmNetworkAgent::new(&engine);

        // Add same validators to both agents
        for i in 0..5 {
            let peer_id = format!("validator_{}", i);
            agent1.join_peer(&peer_id).unwrap();
            agent2.join_peer(&peer_id).unwrap();
        }

        let leader1 = agent1.get_leader();
        let leader2 = agent2.get_leader();

        // Determinism: same validator set → same leader
        assert_eq!(leader1, leader2, "FALSIFICATION FAILED: Leader election is non-deterministic");
    }

    /// Test: Batch benchmark functions work correctly
    #[test]
    fn test_wasm_batch_benchmarks() {
        let engine = WasmEngine::new();

        // Test synthesis benchmark
        let iterations = engine.benchmark_synthesis(100);
        assert_eq!(iterations, 100);

        // Test leader election benchmark
        let mut agent = WasmNetworkAgent::new(&engine);
        for i in 0..3 {
            agent.join_peer(&format!("validator_{}", i)).unwrap();
        }
        let iterations = agent.benchmark_leader_election(100);
        assert_eq!(iterations, 100);

        // Test validation benchmark
        let mut validator = WasmValidator::new(&engine);
        let iterations = validator.benchmark_validation(10);
        assert_eq!(iterations, 10);
    }

    /// Test: Bulk peer join works correctly
    #[test]
    fn test_wasm_bulk_peer_join() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);

        let peer_ids: Vec<String> = (0..5).map(|i| format!("validator_{}", i)).collect();
        let result = agent.join_peers(peer_ids);

        assert!(result.is_ok());
        assert_eq!(agent.validator_count(), 5);
    }
}
