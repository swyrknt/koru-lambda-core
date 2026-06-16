//! WASM Bindings for Koru Lambda Core
//!
//! Bytes-on-wire FFI layer exposing the core engine and subsystems to
//! JavaScript / TypeScript runtimes (browsers, Node, Deno, Bun, Go
//! `wazero`, Kotlin `WasmEdge`, etc.).
//!
//! # Wire format (Decision 5.5)
//!
//! Every distinction ID crossing the JS / WASM boundary is a
//! `Uint8Array` of length 16 (the canonical 16-byte SHA-256 prefix).
//! There is no hex on the wire. The v1.2.0 `id_to_bytes` heuristic
//! that fell through to UTF-8 bytes for short / primordial IDs is gone
//! — primordials Δ₀, Δ₁ are real 16-byte IDs (`[0u8; 16]` and
//! `[1, 0, ..., 0]`) and ride the same Uint8Array path.
//!
//! Hex is a *display* format. The `idToHex` / `idFromHex` JS helpers
//! convert between bytes and the human-readable 32-char lowercase hex
//! at JS boundaries (logs, URLs, JSON debugging). Anything that wants
//! to *use* a distinction passes bytes; anything that wants to *show*
//! one calls `idToHex`.
//!
//! # Panic safety (Decision 5.6, W5)
//!
//! `console_error_panic_hook` is wired up unconditionally under the
//! `wasm` feature via `#[wasm_bindgen(start)] fn _wasm_start()`. Any
//! panic in Rust code is forwarded to `console.error` with a readable
//! stack trace instead of the opaque
//! `RuntimeError: unreachable executed` that wasm-bindgen produces by
//! default.

use std::sync::Arc;
use wasm_bindgen::prelude::*;

use crate::subsystems::{
    BatchCommitment, CommitmentAgent, ConsensusValidator, LocalCausalAgent, NetworkAgent,
    PeerIdentity, TransactionBatch,
};
use crate::{Distinction, DistinctionEngine};

// =============================================================================
// Startup hook (W5 / Decision 5.6)
// =============================================================================

/// WASM module start. Wires up the panic-to-`console.error` hook so
/// Rust panics surface as readable stack traces in the JS console.
///
/// `set_once` is safe to call multiple times across module loads.
#[wasm_bindgen(start)]
pub fn _wasm_start() {
    console_error_panic_hook::set_once();
}

// =============================================================================
// Hex conversion helpers (Decision 5.5)
// =============================================================================
//
// `Distinction::to_hex` / `from_hex` live in `src/distinction_hex.rs`
// (foundation). The WASM layer re-exports them as `idToHex` /
// `idFromHex` for JS callers that need to display or parse IDs at the
// human-facing edge (logging, JSON debug, URL params). The substrate
// itself never sees hex.

/// Convert a distinction ID (16 raw bytes) to its 32-character
/// lowercase hex representation.
///
/// Returns an error if `bytes` is not exactly 16 bytes.
#[wasm_bindgen(js_name = idToHex)]
pub fn id_to_hex(bytes: &[u8]) -> Result<String, JsValue> {
    let arr: [u8; 16] =
        bytes.try_into().map_err(|_| JsValue::from_str("idToHex: expected 16 bytes"))?;
    Ok(Distinction::from_bytes_internal(arr).to_hex())
}

/// Convert a 32-character lowercase hex distinction ID into 16 raw
/// bytes.
///
/// Returns an error if `s` is not 32 chars of `[0-9a-f]`.
#[wasm_bindgen(js_name = idFromHex)]
pub fn id_from_hex(s: &str) -> Result<Vec<u8>, JsValue> {
    Distinction::from_hex(s)
        .map(|d| d.as_bytes().to_vec())
        .map_err(|e| JsValue::from_str(&format!("idFromHex: {}", e)))
}

// =============================================================================
// Internal byte → Distinction helper
// =============================================================================

/// Construct a `Distinction` from a JS-supplied `Uint8Array`. Rejects
/// any length other than 16.
fn distinction_from_bytes(bytes: &[u8]) -> Result<Distinction, JsValue> {
    let arr: [u8; 16] = bytes
        .try_into()
        .map_err(|_| JsValue::from_str("distinction id must be exactly 16 bytes"))?;
    Ok(Distinction::from_bytes_internal(arr))
}

// =============================================================================
// WasmEngine
// =============================================================================

/// WASM-friendly wrapper around `DistinctionEngine`.
#[wasm_bindgen]
pub struct WasmEngine {
    inner: Arc<DistinctionEngine>,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Create a new engine seeded with primordial distinctions Δ₀, Δ₁.
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { inner: Arc::new(DistinctionEngine::new()) }
    }

    /// Current distinction count (including Δ₀, Δ₁).
    #[wasm_bindgen(js_name = distinctionCount)]
    pub fn distinction_count(&self) -> usize {
        self.inner.distinction_count()
    }

    /// Current relationship count.
    #[wasm_bindgen(js_name = relationshipCount)]
    pub fn relationship_count(&self) -> usize {
        self.inner.relationship_count()
    }

    /// Primordial Δ₀ as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = d0Id)]
    pub fn d0_id(&self) -> Vec<u8> {
        self.inner.d0().as_bytes().to_vec()
    }

    /// Primordial Δ₁ as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = d1Id)]
    pub fn d1_id(&self) -> Vec<u8> {
        self.inner.d1().as_bytes().to_vec()
    }

    /// Synthesize two distinctions by their canonical 16-byte IDs.
    ///
    /// Both arguments and the return value are `Uint8Array` of length
    /// 16. Returns a JS error if either input is the wrong length.
    #[wasm_bindgen]
    pub fn synthesize(&self, id_a: &[u8], id_b: &[u8]) -> Result<Vec<u8>, JsValue> {
        let a = distinction_from_bytes(id_a)?;
        let b = distinction_from_bytes(id_b)?;
        let result = self.inner.synthesize(&a, &b);
        Ok(result.as_bytes().to_vec())
    }

    /// Batch synthesis benchmark — runs `iterations` Δ₀⊕Δ₁ folds
    /// inside WASM (no FFI overhead per call). Returns the iteration
    /// count for parity with the JS-side timer.
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

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// WasmNetworkAgent
// =============================================================================

/// WASM wrapper around `NetworkAgent`.
#[wasm_bindgen]
pub struct WasmNetworkAgent {
    inner: NetworkAgent,
    engine: Arc<DistinctionEngine>,
}

#[wasm_bindgen]
impl WasmNetworkAgent {
    #[wasm_bindgen(constructor)]
    pub fn new(engine: &WasmEngine) -> Self {
        Self { inner: NetworkAgent::new(&engine.inner), engine: engine.inner.clone() }
    }

    /// Current network root as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = currentRoot)]
    pub fn current_root(&self) -> Vec<u8> {
        self.inner.get_current_root().as_bytes().to_vec()
    }

    /// Consensus state root (validator-held) as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = consensusRoot)]
    pub fn consensus_root(&self) -> Vec<u8> {
        // `consensus_state_root` returns hex; round-trip back through
        // bytes for the WASM-facing API. The validator's internal
        // Distinction lives in bytes; this conversion is a thin
        // adapter at the boundary.
        Distinction::from_hex(&self.inner.consensus_state_root())
            .map(|d| d.as_bytes().to_vec())
            .expect("consensus_state_root is always 32-char hex")
    }

    /// Join a peer. Returns the new root as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = joinPeer)]
    pub fn join_peer(&mut self, peer_id: &str) -> Result<Vec<u8>, JsValue> {
        let peer = PeerIdentity::new(peer_id.to_string(), &self.engine)
            .map_err(|e| JsValue::from_str(&e))?;
        let new_root = self.inner.join_peer(peer, &self.engine);
        Ok(new_root.as_bytes().to_vec())
    }

    /// Bulk join — eliminates FFI overhead for batch operations.
    /// Returns the final root as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = joinPeers)]
    pub fn join_peers(&mut self, peer_ids: Vec<String>) -> Result<Vec<u8>, JsValue> {
        let mut new_root = self.inner.get_current_root().clone();
        for peer_id in peer_ids {
            let peer =
                PeerIdentity::new(peer_id, &self.engine).map_err(|e| JsValue::from_str(&e))?;
            new_root = self.inner.join_peer(peer, &self.engine);
        }
        Ok(new_root.as_bytes().to_vec())
    }

    /// Advance one epoch. Returns the new root as a 16-byte
    /// `Uint8Array`.
    #[wasm_bindgen(js_name = advanceEpoch)]
    pub fn advance_epoch(&mut self) -> Result<Vec<u8>, JsValue> {
        let new_root = self.inner.advance_epoch(&self.engine);
        Ok(new_root.as_bytes().to_vec())
    }

    #[wasm_bindgen(js_name = currentEpoch)]
    pub fn current_epoch(&self) -> u64 {
        self.inner.current_epoch()
    }

    #[wasm_bindgen(js_name = validatorCount)]
    pub fn validator_count(&self) -> usize {
        self.inner.validator_count()
    }

    /// Deterministic leader id (string). Returns `null` if no
    /// validators are joined.
    #[wasm_bindgen(js_name = getLeader)]
    pub fn get_leader(&self) -> Option<String> {
        self.inner.get_current_leader().map(|p| p.id.clone())
    }

    /// Batch leader election benchmark.
    #[wasm_bindgen(js_name = benchmarkLeaderElection)]
    pub fn benchmark_leader_election(&self, iterations: u32) -> u32 {
        for _ in 0..iterations {
            let _ = self.inner.get_current_leader();
        }
        iterations
    }

    /// Stage 1 propose. Returns the 32-byte commitment hash as a
    /// `Uint8Array`.
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

    /// Stage 1 light-node check.
    ///
    /// # W10 closure (CHECKLIST 1.8 / Phase 6 sub-branch #10)
    ///
    /// Mirrors the FFI F7 fix from sub-branch #8: the constructed
    /// `BatchCommitment` now carries the real `leader_id` and
    /// `batch_size` instead of v1.2.0's empty / zero "Frankenstein"
    /// values. The verify is metadata-only by design (nonce + epoch);
    /// full hash verification requires the batch payload.
    #[wasm_bindgen(js_name = checkCommitment)]
    pub fn check_commitment(
        &self,
        hash_bytes: &[u8],
        nonce: u64,
        epoch: u64,
        leader_id: &str,
        batch_size: u64,
    ) -> Result<bool, JsValue> {
        if hash_bytes.len() != 32 {
            return Err(JsValue::from_str("commitment hash must be 32 bytes"));
        }
        if leader_id.is_empty() {
            // Mirrors PeerIdentity::new's N2 rejection.
            return Err(JsValue::from_str("leader_id must not be empty"));
        }

        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_bytes);

        let commitment = BatchCommitment {
            commitment_hash: hash,
            nonce,
            epoch,
            leader_id: leader_id.to_string(),
            batch_size: batch_size as usize,
        };

        Ok(self.inner.check_commitment(&commitment))
    }

    /// Stage 2 finalize. `hash_bytes` is a 32-byte `Uint8Array`.
    /// Returns the new root as a 16-byte `Uint8Array`.
    #[wasm_bindgen(js_name = finalizeBatch)]
    pub fn finalize_batch(
        &mut self,
        batch_json: &str,
        hash_bytes: &[u8],
    ) -> Result<Vec<u8>, JsValue> {
        let batch: TransactionBatch = serde_json::from_str(batch_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid batch JSON: {}", e)))?;
        if hash_bytes.len() != 32 {
            return Err(JsValue::from_str("commitment hash must be 32 bytes"));
        }
        let mut hash = [0u8; 32];
        hash.copy_from_slice(hash_bytes);

        let result = self
            .inner
            .finalize_batch(batch, hash, &self.engine)
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(result.as_bytes().to_vec())
    }
}

// =============================================================================
// WasmValidator
// =============================================================================

/// WASM wrapper around `ConsensusValidator`.
#[wasm_bindgen]
pub struct WasmValidator {
    inner: ConsensusValidator,
    engine: Arc<DistinctionEngine>,
}

#[wasm_bindgen]
impl WasmValidator {
    #[wasm_bindgen(constructor)]
    pub fn new(engine: &WasmEngine) -> Self {
        Self { inner: ConsensusValidator::new(&engine.inner), engine: engine.inner.clone() }
    }

    #[wasm_bindgen(js_name = currentRoot)]
    pub fn current_root(&self) -> Vec<u8> {
        self.inner.get_current_root().as_bytes().to_vec()
    }

    #[wasm_bindgen(js_name = expectedNonce)]
    pub fn expected_nonce(&self) -> u64 {
        self.inner.expected_nonce()
    }

    #[wasm_bindgen(js_name = validateBatch)]
    pub fn validate_batch(&mut self, batch_json: &str) -> Result<Vec<u8>, JsValue> {
        let batch: TransactionBatch = serde_json::from_str(batch_json)
            .map_err(|e| JsValue::from_str(&format!("Invalid batch JSON: {}", e)))?;
        match self.inner.validate_batch(batch, &self.engine) {
            crate::subsystems::BatchValidationResult::Valid(new_root) => {
                Ok(new_root.as_bytes().to_vec())
            },
            crate::subsystems::BatchValidationResult::Rejected(reason) => {
                Err(JsValue::from_str(&reason))
            },
        }
    }

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

// =============================================================================
// WasmCommitmentAgent
// =============================================================================

/// WASM wrapper around `CommitmentAgent`.
#[wasm_bindgen]
pub struct WasmCommitmentAgent {
    inner: CommitmentAgent,
}

#[wasm_bindgen]
impl WasmCommitmentAgent {
    #[wasm_bindgen(constructor)]
    pub fn new(engine: &WasmEngine) -> Self {
        Self { inner: CommitmentAgent::new(&engine.inner) }
    }

    #[wasm_bindgen(js_name = currentRoot)]
    pub fn current_root(&self) -> Vec<u8> {
        self.inner.get_current_root().as_bytes().to_vec()
    }

    #[wasm_bindgen(js_name = expectedNonce)]
    pub fn expected_nonce(&self) -> u64 {
        self.inner.expected_nonce()
    }

    #[wasm_bindgen(js_name = commitmentsProcessed)]
    pub fn commitments_processed(&self) -> u64 {
        self.inner.commitments_processed()
    }
}

// =============================================================================
// Tests
// =============================================================================
//
// W13 closure (CHECKLIST 1.8 / Phase 6 sub-branch #10): tests in this
// module use `#[wasm_bindgen_test]` rather than `#[test]` so they
// actually exercise the WASM runtime under `wasm-pack test --node`.
// Under host `cargo test --features wasm`, wasm-bindgen-test still
// generates compilable test functions; they just don't execute as
// native tests (the harness is the wasm runtime). The tests here
// therefore deliberately avoid host-only invariants.

#[cfg(test)]
mod tests {
    use super::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    /// Bytes formatter for assertion messages; replaces the
    /// host-side `&str[..16]` slicing that the v1.2.0 tests used to
    /// print hex prefixes.
    fn to_hex_prefix(bytes: &[u8]) -> String {
        bytes.iter().take(8).map(|b| format!("{:02x}", b)).collect()
    }

    #[wasm_bindgen_test]
    fn primordial_consistency() {
        let native = DistinctionEngine::new();
        let wasm = WasmEngine::new();

        // Bytes equality — both sides emit the same canonical 16-byte ID.
        assert_eq!(wasm.d0_id(), native.d0().as_bytes().to_vec());
        assert_eq!(wasm.d1_id(), native.d1().as_bytes().to_vec());
        assert_eq!(wasm.distinction_count(), native.distinction_count());
        assert_eq!(wasm.relationship_count(), native.relationship_count());
    }

    #[wasm_bindgen_test]
    fn synthesis_determinism() {
        let native = DistinctionEngine::new();
        let wasm = WasmEngine::new();

        let wasm_result = wasm.synthesize(&wasm.d0_id(), &wasm.d1_id()).unwrap();
        let native_result = native.synthesize(native.d0(), native.d1());
        assert_eq!(wasm_result, native_result.as_bytes().to_vec());
    }

    #[wasm_bindgen_test]
    fn axiom_symmetry() {
        let wasm = WasmEngine::new();
        let d0 = wasm.d0_id();
        let d1 = wasm.d1_id();
        let ab = wasm.synthesize(&d0, &d1).unwrap();
        let ba = wasm.synthesize(&d1, &d0).unwrap();
        assert_eq!(ab, ba, "symmetry violated");
    }

    #[wasm_bindgen_test]
    fn axiom_irreflexivity() {
        let wasm = WasmEngine::new();
        let d0 = wasm.d0_id();
        let result = wasm.synthesize(&d0, &d0).unwrap();
        assert_eq!(result, d0, "irreflexivity violated");
    }

    #[wasm_bindgen_test]
    fn synthesize_rejects_wrong_length_inputs() {
        let wasm = WasmEngine::new();
        let too_short = vec![0u8; 15];
        let too_long = vec![0u8; 17];
        assert!(wasm.synthesize(&too_short, &wasm.d1_id()).is_err());
        assert!(wasm.synthesize(&wasm.d0_id(), &too_long).is_err());
    }

    #[wasm_bindgen_test]
    fn id_to_hex_and_back_roundtrip() {
        let wasm = WasmEngine::new();
        let d0 = wasm.d0_id();
        let hex = id_to_hex(&d0).unwrap();
        assert_eq!(hex.len(), 32);
        let bytes = id_from_hex(&hex).unwrap();
        assert_eq!(bytes, d0);
    }

    #[wasm_bindgen_test]
    fn id_to_hex_rejects_wrong_length() {
        assert!(id_to_hex(&[0u8; 15]).is_err());
        assert!(id_to_hex(&[0u8; 17]).is_err());
    }

    #[wasm_bindgen_test]
    fn id_from_hex_rejects_invalid() {
        assert!(id_from_hex("not-hex").is_err());
        assert!(id_from_hex("deadbeef").is_err()); // wrong length
        assert!(id_from_hex(&"z".repeat(32)).is_err()); // invalid chars
    }

    #[wasm_bindgen_test]
    fn network_agent_local_causal_compliance() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        let initial_root = agent.current_root();

        let new_root = agent.join_peer("validator_0").expect("peer join ok");
        assert_ne!(new_root, initial_root, "NetworkAgent root must change after action");
        assert_eq!(agent.current_root(), new_root);
    }

    #[wasm_bindgen_test]
    fn validator_local_causal_compliance() {
        let engine = WasmEngine::new();
        let mut validator = WasmValidator::new(&engine);
        let initial_root = validator.current_root();
        let initial_nonce = validator.expected_nonce();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": id_to_hex(&initial_root).unwrap()
        });

        let new_root = validator.validate_batch(&batch.to_string()).expect("batch ok");
        assert_ne!(new_root, initial_root);
        assert_eq!(validator.expected_nonce(), initial_nonce + 1);
    }

    #[wasm_bindgen_test]
    fn commitment_agent_local_causal_compliance() {
        let engine = WasmEngine::new();
        let agent = WasmCommitmentAgent::new(&engine);
        assert_eq!(agent.expected_nonce(), 0);
        assert_eq!(agent.commitments_processed(), 0);
        let root = agent.current_root();
        assert_eq!(root.len(), 16);
    }

    #[wasm_bindgen_test]
    fn commitment_protocol_integrity() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        agent.join_peer("validator_0").unwrap();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": id_to_hex(&agent.consensus_root()).unwrap()
        })
        .to_string();

        let hash = agent.propose_commitment(&batch).expect("propose ok");
        assert_eq!(hash.len(), 32);

        let is_valid = agent.check_commitment(&hash, 0, 0, "validator_0", 1).expect("check ok");
        assert!(is_valid);

        let result = agent.finalize_batch(&batch, &hash);
        assert!(result.is_ok());
    }

    #[wasm_bindgen_test]
    fn check_commitment_rejects_empty_leader_id() {
        // W10 closure: empty leader_id is no longer a silent accept.
        let engine = WasmEngine::new();
        let agent = WasmNetworkAgent::new(&engine);
        let fake_hash = vec![0u8; 32];
        assert!(agent.check_commitment(&fake_hash, 0, 0, "", 1).is_err());
    }

    #[wasm_bindgen_test]
    fn finalize_rejects_tampered_hash() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        agent.join_peer("validator_0").unwrap();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": id_to_hex(&agent.consensus_root()).unwrap()
        })
        .to_string();
        let _ = agent.propose_commitment(&batch).expect("propose ok");

        let tampered = vec![0u8; 32];
        assert!(agent.finalize_batch(&batch, &tampered).is_err());
    }

    #[wasm_bindgen_test]
    fn check_commitment_rejects_wrong_nonce() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        agent.join_peer("validator_0").unwrap();

        let batch = serde_json::json!({
            "transactions": [{"nonce": 0, "data": [1, 2, 3]}],
            "previous_root": id_to_hex(&agent.consensus_root()).unwrap()
        })
        .to_string();
        let hash = agent.propose_commitment(&batch).expect("propose ok");

        let is_valid = agent.check_commitment(&hash, 999, 0, "validator_0", 1).expect("check ok");
        assert!(!is_valid);
    }

    #[wasm_bindgen_test]
    fn validator_atomic_failure() {
        let engine = WasmEngine::new();
        let mut validator = WasmValidator::new(&engine);

        let initial_root = validator.current_root();
        let bad_batch = serde_json::json!({
            "transactions": [
                {"nonce": 0, "data": [1, 2, 3]},
                {"nonce": 999, "data": [4, 5, 6]}
            ],
            "previous_root": id_to_hex(&initial_root).unwrap()
        })
        .to_string();

        assert!(validator.validate_batch(&bad_batch).is_err());
        assert_eq!(validator.current_root(), initial_root);
        assert_eq!(validator.expected_nonce(), 0);
    }

    #[wasm_bindgen_test]
    fn deterministic_leader_election() {
        let engine = WasmEngine::new();
        let mut agent1 = WasmNetworkAgent::new(&engine);
        let mut agent2 = WasmNetworkAgent::new(&engine);
        for i in 0..5 {
            let peer_id = format!("validator_{}", i);
            agent1.join_peer(&peer_id).unwrap();
            agent2.join_peer(&peer_id).unwrap();
        }
        assert_eq!(agent1.get_leader(), agent2.get_leader());
    }

    #[wasm_bindgen_test]
    fn batch_benchmarks() {
        let engine = WasmEngine::new();
        assert_eq!(engine.benchmark_synthesis(100), 100);

        let mut agent = WasmNetworkAgent::new(&engine);
        for i in 0..3 {
            agent.join_peer(&format!("validator_{}", i)).unwrap();
        }
        assert_eq!(agent.benchmark_leader_election(100), 100);

        let mut validator = WasmValidator::new(&engine);
        assert_eq!(validator.benchmark_validation(10), 10);
    }

    #[wasm_bindgen_test]
    fn bulk_peer_join() {
        let engine = WasmEngine::new();
        let mut agent = WasmNetworkAgent::new(&engine);
        let peer_ids: Vec<String> = (0..5).map(|i| format!("validator_{}", i)).collect();
        assert!(agent.join_peers(peer_ids).is_ok());
        assert_eq!(agent.validator_count(), 5);
    }

    /// Diagnostic helper used for printing IDs in assertion failures.
    #[allow(dead_code)]
    fn _dbg_prefix(bytes: &[u8]) -> String {
        to_hex_prefix(bytes)
    }
}
