/// Network consensus using structural proof-of-causality (SPoC).
///
/// Implements deterministic leader election and batch validation without
/// traditional voting rounds. Leader is computed from epoch and validator set.
///
/// Fixed-time window (2 seconds) for leader to propose next batch.
use crate::primitives::Canonicalizable;
use crate::subsystems::commitment::{BatchCommitment, CommitmentAgent};
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::subsystems::validator::{BatchValidationResult, ConsensusValidator, TransactionBatch};
use crate::{Distinction, DistinctionEngine};
use lru::LruCache;
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;
use std::sync::Arc;

/// Peer identity in the network
///
/// Maximum byte length accepted for a peer id.
///
/// N1 cap (CHECKLIST 1.6 / Phase 6 sub-branch #7). Phase 1.5 probe
/// `audit_network_foreign_peers` Section A demonstrated that a 1 MB
/// peer id synthesized 1,000,002 permanent distinctions in ~1.85 s.
/// 64 bytes comfortably holds a hex-encoded SHA-256 (64) or a base64
/// public-key fingerprint and forces operators using longer ids to
/// hash them first.
pub const MAX_PEER_ID_LEN: usize = 64;

/// Maximum number of pending commitments retained in-memory.
///
/// N11 cap (CHECKLIST 1.6 / Phase 6 sub-branch #7). Commitments older
/// than the LRU window are evicted; the entire pending set is also
/// cleared on epoch advance (commitments cannot be finalized across
/// epoch boundaries). Sized to match the existing two-stage gossip
/// LRU in `commitment.rs`.
pub const MAX_PENDING_COMMITMENTS: usize = 256;

/// Each peer is represented as a distinction, Peer relationships are
/// structural.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    /// Peer's unique identifier (typically a public key hash)
    pub id: String,
    /// Peer represented as a distinction in the synthesis space
    pub distinction: Distinction,
}

impl PeerIdentity {
    /// Create a new peer identity from an ID string.
    ///
    /// Returns `Err` if the id is empty or longer than
    /// `MAX_PEER_ID_LEN` bytes:
    ///
    /// * **Empty (N2):** an empty id folds the byte-loop zero times,
    ///   returning `d0` as the peer's distinction. Two peers with
    ///   empty ids would collapse onto each other AND onto the
    ///   primordial — primordial impersonation. *(audit
    ///   `audit_network_foreign_peers` Section B)*
    /// * **Oversized (N1):** the byte-by-byte fold runs once per
    ///   byte, so an unbounded id is an unbounded permanent-state
    ///   amplifier on every `join_peer` call. *(audit
    ///   `audit_network_foreign_peers` Section A)*
    pub fn new(id: String, engine: &Arc<DistinctionEngine>) -> Result<Self, String> {
        if id.is_empty() {
            return Err("peer id must not be empty".to_string());
        }
        if id.len() > MAX_PEER_ID_LEN {
            return Err(format!(
                "peer id length {} exceeds MAX_PEER_ID_LEN ({})",
                id.len(),
                MAX_PEER_ID_LEN
            ));
        }

        // Canonicalize peer ID into a distinction
        let id_bytes = id.as_bytes();
        let distinction = id_bytes.iter().fold(engine.d0().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

        Ok(Self { id, distinction })
    }

    /// Get peer's distinction ID as a 32-character hex string.
    pub fn distinction_id(&self) -> String {
        self.distinction.to_hex()
    }
}

/// Network events that can be synthesized into network state
///
/// Each event is canonicalizable and becomes part of the causal chain.
#[derive(Debug, Clone)]
pub enum NetworkAction {
    /// New peer joined the validator set
    PeerJoined { peer: PeerIdentity },
    /// Leader proposed a new batch for validation
    BatchProposed { batch: TransactionBatch },
    /// Epoch advanced (triggers leader rotation)
    EpochAdvanced { new_epoch: u64 },
}

impl Canonicalizable for NetworkAction {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        match self {
            NetworkAction::PeerJoined { peer } => {
                // Synthesize: network_event_type ⊕ peer_distinction
                let join_marker = engine.d1().clone(); // d1 = "join" event
                engine.synthesize(&join_marker, &peer.distinction)
            },
            NetworkAction::BatchProposed { batch } => {
                // Synthesize: batch_marker ⊕ previous_root_distinction.
                //
                // N5: parse previous_root as a 32-char hex Distinction id
                // rather than folding the first 8 ASCII bytes. The old
                // code caused causal-chain collisions between any two
                // roots sharing a hex prefix (Phase 1.5 probe
                // `audit_network_foreign_peers` Section D demonstrated
                // `deadbeefAAAA…` and `deadbeefBBBB…` collapsing to the
                // same action distinction).
                //
                // Malformed previous_root values (wrong length, non-hex
                // characters) fall through a deterministic sentinel
                // (`d0 ⊕ d1`). The sentinel cannot collide with a valid
                // `from_hex` output unless an attacker can also provide
                // hex matching the d0⊕d1 SHA256 prefix — at which point
                // the input was well-formed and went through the parse
                // path anyway. The validator's separate `previous_root`
                // String check rejects malformed inputs at the consensus
                // layer, so the fallback is never reached on the happy
                // path; keeping the trait infallible avoids touching
                // every `Canonicalizable` impl.
                let batch_marker = engine.d0().clone();

                let root_distinction = match Distinction::from_hex(&batch.previous_root) {
                    Ok(d) => d,
                    Err(_) => engine.synthesize(engine.d0(), engine.d1()),
                };

                engine.synthesize(&batch_marker, &root_distinction)
            },
            NetworkAction::EpochAdvanced { new_epoch } => {
                // Synthesize: epoch_marker ⊕ epoch_value
                let epoch_bytes = new_epoch.to_le_bytes();
                let epoch_distinction =
                    epoch_bytes.iter().fold(engine.d1().clone(), |acc, &byte| {
                        let byte_d = byte.to_canonical_structure(engine);
                        engine.synthesize(&acc, &byte_d)
                    });

                epoch_distinction
            },
        }
    }
}

/// Structural Network Agent Implementation
///
/// Manages network consensus through pure causal synthesis. The network state
/// is a distinction that evolves as events are synthesized into it.
///
/// Key Innovation: No external consensus mechanism needed. The synthesis
/// operation itself IS the consensus - deterministic synthesis ensures all
/// nodes arrive at identical state given identical inputs.
pub struct NetworkAgent {
    /// Current network state as a distinction
    local_root: Distinction,
    /// Validator for batch verification
    validator: ConsensusValidator,
    /// Commitment agent for two-stage gossip
    commitment_agent: CommitmentAgent,
    /// Ordered set of validator peers
    validator_set: Vec<PeerIdentity>,
    /// Current consensus epoch
    current_epoch: u64,
    /// Fixed-Time Window in milliseconds (default: 2000ms)
    ftw_duration_ms: u64,
    /// Number of network events processed
    events_processed: u64,
    /// Pending commitments awaiting finalization (N11).
    ///
    /// Bounded LRU keyed on commitment_hash. Capacity
    /// `MAX_PENDING_COMMITMENTS`; the entire set is also cleared on
    /// epoch advance (commitments cannot be finalized across epoch
    /// boundaries). Replaces the v1.2.0 unbounded `HashMap`.
    pending_commitments: LruCache<[u8; 32], BatchCommitment>,
}

impl NetworkAgent {
    /// Create a new network agent at genesis
    ///
    /// Initializes with empty validator set and epoch 0.
    pub fn new(engine: &Arc<DistinctionEngine>) -> Self {
        // Genesis network state: d0 ⊕ d1 (same as consensus genesis)
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        Self {
            local_root: genesis,
            validator: ConsensusValidator::new(engine),
            commitment_agent: CommitmentAgent::new(engine),
            validator_set: Vec::new(),
            current_epoch: 0,
            ftw_duration_ms: 2000, // 2 seconds per design doc
            events_processed: 0,
            pending_commitments: LruCache::new(
                NonZeroUsize::new(MAX_PENDING_COMMITMENTS).expect("cap > 0"),
            ),
        }
    }

    /// Create network agent from existing state
    pub fn from_state(
        root: Distinction,
        validator: ConsensusValidator,
        commitment_agent: CommitmentAgent,
        validator_set: Vec<PeerIdentity>,
        epoch: u64,
    ) -> Self {
        Self {
            local_root: root,
            validator,
            commitment_agent,
            validator_set,
            current_epoch: epoch,
            ftw_duration_ms: 2000,
            events_processed: 0,
            pending_commitments: LruCache::new(
                NonZeroUsize::new(MAX_PENDING_COMMITMENTS).expect("cap > 0"),
            ),
        }
    }

    /// Add a new peer to the validator set
    ///
    /// This is a network event that gets synthesized into the network state.
    pub fn join_peer(
        &mut self,
        peer: PeerIdentity,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        // N7: dedupe on the joint (id, distinction) key — the same key
        // basis the deterministic leader hash uses. v1.2.0 deduped on
        // `id` alone, so two peers sharing an id but with different
        // (corrupted/swapped) distinctions could BOTH be kept out of
        // the set, AND inversely one peer's distinction could be
        // shadow-paired with a different peer's id without rejection.
        if self.validator_set.iter().any(|p| p.id == peer.id && p.distinction == peer.distinction) {
            return self.local_root.clone();
        }

        // Add to validator set
        self.validator_set.push(peer.clone());

        // Synthesize join event into network state
        let action = NetworkAction::PeerJoined { peer };
        self.synthesize_action(action, engine)
    }

    /// STAGE 1: Propose Commitment (Two-Stage Gossip Protocol)
    ///
    /// Leader computes lightweight commitment hash, caches the batch data,
    /// and returns the commitment for gossiping.
    ///
    /// Returns: BatchCommitment (80 bytes to gossip)
    ///
    /// The runtime (Go/Kotlin/Swift) broadcasts this commitment to all nodes.
    /// Light nodes verify via `check_commitment()` without downloading batch.
    /// Full nodes fetch batch via Stage 2 when needed.
    pub fn propose_commitment(
        &mut self,
        batch: TransactionBatch,
        _engine: &Arc<DistinctionEngine>,
    ) -> Result<BatchCommitment, String> {
        // Compute commitment hash
        let nonce = self.validator.expected_nonce();
        let epoch = self.current_epoch;
        let leader_id = self
            .get_current_leader()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let commitment = BatchCommitment::compute(&batch, nonce, epoch, leader_id);

        // Cache batch data for Stage 2 fetches
        self.commitment_agent.cache_batch(batch.clone(), commitment.clone());
        // N11: bounded LRU push; evicts the least-recently-touched
        // commitment if capacity is exceeded.
        self.pending_commitments.put(commitment.commitment_hash, commitment.clone());

        Ok(commitment)
    }

    /// STAGE 1: Check Commitment (Light Node "Ping" Check)
    ///
    /// Verifies commitment against expected nonce/epoch WITHOUT downloading batch.
    ///
    /// This is how light clients achieve consensus participation:
    /// - Receive 80-byte commitment via gossip
    /// - Verify nonce/epoch match local expectations
    /// - Accept/reject without bandwidth cost
    ///
    /// Returns: true if commitment is valid for current state
    pub fn check_commitment(&self, commitment: &BatchCommitment) -> bool {
        let expected_nonce = self.validator.expected_nonce();
        let expected_epoch = self.current_epoch;

        commitment.verify(expected_nonce, expected_epoch)
    }

    /// STAGE 2: Finalize Batch (Full Validator Execution)
    ///
    /// Applies the full batch after fetching data and verifying it matches commitment.
    ///
    /// Called by full validators who need to execute transactions.
    /// Runtime fetches batch data from peers, verifies hash matches commitment,
    /// then calls this to finalize state transition.
    ///
    /// Returns: New network root on success
    pub fn finalize_batch(
        &mut self,
        batch: TransactionBatch,
        commitment_hash: [u8; 32],
        engine: &Arc<DistinctionEngine>,
    ) -> Result<Distinction, String> {
        // Verify we have pending commitment. Clone out so the LRU
        // borrow is released before we call other &mut self methods.
        let commitment = self
            .pending_commitments
            .get(&commitment_hash)
            .cloned()
            .ok_or_else(|| "No pending commitment for hash".to_string())?;

        // Verify batch data matches commitment
        if !commitment.verify_batch(&batch, self.validator.expected_nonce(), self.current_epoch) {
            return Err("Batch data does not match commitment hash".to_string());
        }

        // Validate batch structurally
        let result = self.validator.validate_batch(batch.clone(), engine);

        match result {
            BatchValidationResult::Valid(_new_state_root) => {
                // Batch is valid - synthesize into network state
                let action = NetworkAction::BatchProposed { batch };
                let new_network_root = self.synthesize_action(action, engine);

                // Remove from pending
                self.pending_commitments.pop(&commitment_hash);

                Ok(new_network_root)
            },
            BatchValidationResult::Rejected(reason) => Err(reason),
        }
    }

    /// Advance to next epoch (triggers leader rotation)
    ///
    /// This happens after FTW expires or batch is accepted.
    pub fn advance_epoch(&mut self, engine: &Arc<DistinctionEngine>) -> Distinction {
        self.current_epoch += 1;

        // N11: commitments do not survive an epoch boundary. The
        // commitment hash binds `epoch`, so any pending commitments
        // are now un-finalizable; dropping them releases their slots
        // and keeps the LRU's working set bounded by traffic in the
        // current epoch.
        self.pending_commitments.clear();

        let action = NetworkAction::EpochAdvanced { new_epoch: self.current_epoch };

        self.synthesize_action(action, engine)
    }

    /// Deterministically elect current leader based on epoch + validator set
    ///
    /// Deterministic function of current state.
    /// All nodes compute identical leader given identical epoch + validator set.
    ///
    /// This is forkless by construction (symmetry ensures order independence).
    pub fn get_current_leader(&self) -> Option<&PeerIdentity> {
        if self.validator_set.is_empty() {
            return None;
        }

        // Deterministic leader election: hash(epoch || validator_set_root) % set_size
        let mut hasher = Sha256::new();

        // Hash epoch
        hasher.update(self.current_epoch.to_le_bytes());

        // Hash validator set (in order)
        for peer in &self.validator_set {
            hasher.update(peer.distinction_id().as_bytes());
        }

        let hash = hasher.finalize();
        let hash_value = u64::from_le_bytes([
            hash[0], hash[1], hash[2], hash[3], hash[4], hash[5], hash[6], hash[7],
        ]);

        let leader_index = (hash_value as usize) % self.validator_set.len();
        Some(&self.validator_set[leader_index])
    }

    /// Get current epoch
    pub fn current_epoch(&self) -> u64 {
        self.current_epoch
    }

    /// Get number of validators in the set
    pub fn validator_count(&self) -> usize {
        self.validator_set.len()
    }

    /// Get validator set
    pub fn validator_set(&self) -> &[PeerIdentity] {
        &self.validator_set
    }

    /// Get current consensus state root as a 32-character hex string.
    pub fn consensus_state_root(&self) -> String {
        self.validator.state_root_id()
    }

    /// Get network statistics
    pub fn get_stats(&self) -> NetworkStats {
        NetworkStats {
            current_epoch: self.current_epoch,
            validator_count: self.validator_set.len(),
            events_processed: self.events_processed,
            consensus_nonce: self.validator.expected_nonce(),
            network_root: self.local_root.to_hex(),
        }
    }

    /// Get Fixed-Time Window duration
    pub fn ftw_duration(&self) -> u64 {
        self.ftw_duration_ms
    }

    /// Get expected transaction nonce from the internal validator.
    /// This is the authoritative, canonical nonce required by the LCA contract.
    pub fn consensus_validator_expected_nonce(&self) -> u64 {
        self.validator.expected_nonce()
    }

    /// Restore the internal validator to a previously-persisted state
    /// atomically.
    ///
    /// V6 (CHECKLIST 1.6 / Phase 6 sub-branch #7). Replaces the v1.2.0
    /// `restore_consensus_validator_nonce(nonce)` setter, which left
    /// `local_root` unchanged and so allowed `(root, nonce)`
    /// inconsistency. The new entry point constructs a fresh validator
    /// via `ConsensusValidator::restore_state`, which verifies the
    /// supplied root is registered in `engine`. Fabricated roots are
    /// rejected.
    pub fn restore_consensus_validator_state(
        &mut self,
        engine: &Arc<DistinctionEngine>,
        root_id: Distinction,
        nonce: u64,
    ) -> Result<(), String> {
        self.validator = ConsensusValidator::restore_state(engine, root_id, nonce)?;
        Ok(())
    }
}

/// Network statistics
#[derive(Debug, Clone)]
pub struct NetworkStats {
    pub current_epoch: u64,
    pub validator_count: usize,
    pub events_processed: u64,
    pub consensus_nonce: u64,
    pub network_root: String,
}

impl LocalCausalAgent for NetworkAgent {
    type ActionData = NetworkAction;

    fn get_current_root(&self) -> &Distinction {
        &self.local_root
    }

    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        // Causal synthesis: ΔNew = ΔNetwork_Root ⊕ ΔNetwork_Action
        let action_distinction = action_data.to_canonical_structure(engine);
        let new_root = engine.synthesize(&self.local_root, &action_distinction);

        // Update local state
        self.local_root = new_root.clone();
        self.events_processed += 1;

        new_root
    }

    fn update_local_root(&mut self, new_root: Distinction) {
        self.local_root = new_root;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_agent_genesis() {
        let engine = Arc::new(DistinctionEngine::new());
        let agent = NetworkAgent::new(&engine);

        assert_eq!(agent.current_epoch(), 0);
        assert_eq!(agent.validator_count(), 0);
        assert_eq!(agent.ftw_duration(), 2000);
        assert!(!agent.get_current_root().to_hex().is_empty());
    }

    #[test]
    fn test_peer_identity_creation() {
        let engine = Arc::new(DistinctionEngine::new());

        let peer1 = PeerIdentity::new("peer_1".to_string(), &engine).unwrap();
        let peer2 = PeerIdentity::new("peer_1".to_string(), &engine).unwrap();
        let peer3 = PeerIdentity::new("peer_2".to_string(), &engine).unwrap();

        // Same ID → same distinction (determinism)
        assert_eq!(peer1.distinction_id(), peer2.distinction_id());

        // Different ID → different distinction
        assert_ne!(peer1.distinction_id(), peer3.distinction_id());
    }

    #[test]
    fn peer_identity_rejects_empty_id() {
        // N2: empty peer id MUST be rejected. The legacy fold returned
        // d0 (primordial impersonation).
        let engine = Arc::new(DistinctionEngine::new());
        let result = PeerIdentity::new(String::new(), &engine);
        assert!(result.is_err());
    }

    #[test]
    fn peer_identity_rejects_oversized_id() {
        // N1: oversized peer id MUST be rejected. The legacy fold ran
        // unbounded synths into the engine on every join.
        let engine = Arc::new(DistinctionEngine::new());
        let oversized = "a".repeat(MAX_PEER_ID_LEN + 1);

        let dist_before = engine.distinction_count();
        let result = PeerIdentity::new(oversized, &engine);
        assert!(result.is_err());
        // Crucially: the fold never ran, so the engine is unchanged.
        assert_eq!(engine.distinction_count(), dist_before);
    }

    #[test]
    fn peer_identity_accepts_id_at_cap_boundary() {
        // N1 boundary: id length exactly MAX_PEER_ID_LEN is accepted.
        let engine = Arc::new(DistinctionEngine::new());
        let at_cap = "a".repeat(MAX_PEER_ID_LEN);
        let result = PeerIdentity::new(at_cap, &engine);
        assert!(result.is_ok());
    }

    #[test]
    fn test_peer_joining() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let initial_root = agent.get_current_root().to_hex();

        // Join first peer
        let peer1 = PeerIdentity::new("validator_1".to_string(), &engine).unwrap();
        let new_root = agent.join_peer(peer1.clone(), &engine);

        // Network root should change
        assert_ne!(new_root.to_hex(), initial_root);
        assert_eq!(agent.validator_count(), 1);

        // Join second peer
        let peer2 = PeerIdentity::new("validator_2".to_string(), &engine).unwrap();
        agent.join_peer(peer2, &engine);

        assert_eq!(agent.validator_count(), 2);
    }

    #[test]
    fn test_deterministic_leader_election() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        // Add validators
        for i in 0..5 {
            let peer = PeerIdentity::new(format!("validator_{}", i), &engine).unwrap();
            agent.join_peer(peer, &engine);
        }

        // Get leader at epoch 0
        let leader1 = agent.get_current_leader().unwrap();
        let leader1_id = leader1.id.clone();

        // Leader should be deterministic
        let leader_check = agent.get_current_leader().unwrap();
        assert_eq!(leader_check.id, leader1_id);

        // Create identical agent with same state
        let mut agent2 = NetworkAgent::new(&engine);
        for i in 0..5 {
            let peer = PeerIdentity::new(format!("validator_{}", i), &engine).unwrap();
            agent2.join_peer(peer, &engine);
        }

        // Should elect same leader (determinism)
        let leader2 = agent2.get_current_leader().unwrap();
        assert_eq!(leader2.id, leader1_id);

        // Advance epoch - leader should change
        agent.advance_epoch(&engine);
        let _leader3 = agent.get_current_leader().unwrap();

        // Leader rotation should occur (different epoch)
        // Note: May be same peer by chance with small set, but epoch changed
        assert_eq!(agent.current_epoch(), 1);
    }

    #[test]
    fn test_epoch_advancement() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        assert_eq!(agent.current_epoch(), 0);

        let initial_root = agent.get_current_root().to_hex();

        // Advance epoch
        let new_root = agent.advance_epoch(&engine);

        assert_eq!(agent.current_epoch(), 1);
        assert_ne!(new_root.to_hex(), initial_root);

        // Advance again
        agent.advance_epoch(&engine);
        assert_eq!(agent.current_epoch(), 2);
    }

    #[test]
    fn test_network_action_canonical() {
        let engine = Arc::new(DistinctionEngine::new());

        let peer = PeerIdentity::new("peer_1".to_string(), &engine).unwrap();

        let action1 = NetworkAction::PeerJoined { peer: peer.clone() };
        let action2 = NetworkAction::PeerJoined { peer: peer.clone() };

        let d1 = action1.to_canonical_structure(&engine);
        let d2 = action2.to_canonical_structure(&engine);

        // Same action → same distinction (determinism)
        assert_eq!(d1.to_hex(), d2.to_hex());
    }

    #[test]
    fn test_local_causal_agent_compliance() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let initial_root = agent.get_current_root().to_hex();

        // Synthesize an action
        let peer = PeerIdentity::new("peer_1".to_string(), &engine).unwrap();
        let action = NetworkAction::PeerJoined { peer };

        let new_root = agent.synthesize_action(action, &engine);

        // Root should change (causal synthesis)
        assert_ne!(new_root.to_hex(), initial_root);
        assert_eq!(new_root.to_hex(), agent.get_current_root().to_hex());

        // Events counter should increment
        let stats = agent.get_stats();
        assert_eq!(stats.events_processed, 1);
    }

    #[test]
    fn test_consensus_validator_nonce_access() {
        // V6: restore_consensus_validator_state is the atomic
        // replacement for the v1.2.0 nonce-only setter. Use the
        // agent's existing genesis root so the registered-root
        // precondition holds.
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        assert_eq!(agent.consensus_validator_expected_nonce(), 0);

        let current_root = agent.get_current_root().clone();
        agent
            .restore_consensus_validator_state(&engine, current_root, 100)
            .expect("registered root must be accepted");
        assert_eq!(agent.consensus_validator_expected_nonce(), 100);

        let stats = agent.get_stats();
        assert_eq!(stats.consensus_nonce, 100);
    }

    #[test]
    fn restore_consensus_validator_state_rejects_fabricated_root() {
        // V6: fabricated bytes (well-formed hex, never synthesized)
        // must be refused. Closes the partial-update window at the
        // network agent surface.
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let fabricated =
            Distinction::from_hex(&"f".repeat(32)).expect("32 hex chars parse to a Distinction");

        let result = agent.restore_consensus_validator_state(&engine, fabricated, 100);
        assert!(result.is_err());
        // Validator state must remain at genesis.
        assert_eq!(agent.consensus_validator_expected_nonce(), 0);
    }

    // === N5 regression tests (Phase 6 sub-branch #6) ===

    #[test]
    fn batch_proposed_well_formed_hex_prefix_collision_closed() {
        // N5 mirror of audit_network_foreign_peers Section D, but with
        // VALID 32-char lowercase hex. Two roots sharing an 8-char hex
        // prefix MUST produce distinct action distinctions.
        let engine = DistinctionEngine::new();

        let root_c = "deadbeefaaaaaaaaaaaaaaaaaaaaaaaa".to_string();
        let root_d = "deadbeefbbbbbbbbbbbbbbbbbbbbbbbb".to_string();
        let act_c = NetworkAction::BatchProposed {
            batch: TransactionBatch { transactions: vec![], previous_root: root_c },
        }
        .to_canonical_structure(&engine);
        let act_d = NetworkAction::BatchProposed {
            batch: TransactionBatch { transactions: vec![], previous_root: root_d },
        }
        .to_canonical_structure(&engine);

        assert_ne!(
            act_c.as_bytes(),
            act_d.as_bytes(),
            "well-formed roots with shared hex prefix must not collide (N5)"
        );
    }

    #[test]
    fn batch_proposed_distinct_full_hex_roots_produce_distinct_action_ids() {
        // N5: two completely different 32-char hex roots produce
        // distinct action distinctions.
        let engine = DistinctionEngine::new();

        let act_a = NetworkAction::BatchProposed {
            batch: TransactionBatch { transactions: vec![], previous_root: "a".repeat(32) },
        }
        .to_canonical_structure(&engine);
        let act_b = NetworkAction::BatchProposed {
            batch: TransactionBatch { transactions: vec![], previous_root: "b".repeat(32) },
        }
        .to_canonical_structure(&engine);

        assert_ne!(act_a.as_bytes(), act_b.as_bytes());
    }

    #[test]
    fn batch_proposed_malformed_previous_root_falls_through_sentinel() {
        // Malformed previous_root (empty, short, non-hex) all canonicalize
        // through the deterministic sentinel. Documenting this fallback
        // contract; downstream `previous_root` String check in the
        // validator rejects the batch before this would be observed in
        // the happy path.
        let engine = DistinctionEngine::new();

        let act_empty = NetworkAction::BatchProposed {
            batch: TransactionBatch { transactions: vec![], previous_root: String::new() },
        }
        .to_canonical_structure(&engine);
        let act_short = NetworkAction::BatchProposed {
            batch: TransactionBatch { transactions: vec![], previous_root: "deadbeef".to_string() },
        }
        .to_canonical_structure(&engine);
        let act_nonhex = NetworkAction::BatchProposed {
            batch: TransactionBatch {
                transactions: vec![],
                previous_root: "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz".to_string(),
            },
        }
        .to_canonical_structure(&engine);

        // All three collapse to the same sentinel-derived distinction.
        assert_eq!(act_empty.as_bytes(), act_short.as_bytes());
        assert_eq!(act_empty.as_bytes(), act_nonhex.as_bytes());

        // A well-formed root must not collide with the sentinel bucket.
        let act_valid = NetworkAction::BatchProposed {
            batch: TransactionBatch {
                transactions: vec![],
                previous_root: "00000000000000000000000000000001".to_string(),
            },
        }
        .to_canonical_structure(&engine);
        assert_ne!(
            act_empty.as_bytes(),
            act_valid.as_bytes(),
            "valid previous_root must not collide with sentinel bucket"
        );
    }

    // === N7 / N11 regression tests (Phase 6 sub-branch #7) ===

    #[test]
    fn join_peer_dedupes_on_joint_id_and_distinction() {
        // N7: legitimate re-join with identical (id, distinction)
        // is idempotent; the second call must not mutate the
        // validator_set.
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let peer = PeerIdentity::new("validator_1".to_string(), &engine).unwrap();
        agent.join_peer(peer.clone(), &engine);
        let after_first = agent.validator_count();

        agent.join_peer(peer, &engine);
        assert_eq!(agent.validator_count(), after_first, "duplicate join must be idempotent");
    }

    #[test]
    fn pending_commitments_bounded_by_lru_cap() {
        // N11: proposing more than MAX_PENDING_COMMITMENTS commitments
        // must not grow the in-memory set without bound. v1.2.0 used an
        // unbounded HashMap.
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        // Need at least one validator so propose_commitment can name a leader.
        let peer = PeerIdentity::new("validator_1".to_string(), &engine).unwrap();
        agent.join_peer(peer, &engine);

        // Push twice the cap; each batch is distinct via a unique
        // transaction payload so commitment_hash differs.
        for i in 0..(MAX_PENDING_COMMITMENTS * 2) {
            let batch = TransactionBatch {
                transactions: vec![crate::subsystems::validator::TransactionAction {
                    nonce: 0,
                    data: (i as u32).to_le_bytes().to_vec(),
                }],
                previous_root: agent.consensus_state_root(),
            };
            let _ = agent.propose_commitment(batch, &engine).expect("propose ok");
        }

        assert_eq!(
            agent.pending_commitments.len(),
            MAX_PENDING_COMMITMENTS,
            "LRU must cap at MAX_PENDING_COMMITMENTS"
        );
    }

    #[test]
    fn advance_epoch_clears_pending_commitments() {
        // N11: commitments are epoch-bound (their hash includes epoch),
        // so any pending commitment is unfinalizable after an epoch
        // boundary. advance_epoch must drop them.
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let peer = PeerIdentity::new("validator_1".to_string(), &engine).unwrap();
        agent.join_peer(peer, &engine);

        let batch = TransactionBatch {
            transactions: vec![crate::subsystems::validator::TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: agent.consensus_state_root(),
        };
        let _ = agent.propose_commitment(batch, &engine).expect("propose ok");
        assert_eq!(agent.pending_commitments.len(), 1);

        agent.advance_epoch(&engine);
        assert_eq!(
            agent.pending_commitments.len(),
            0,
            "epoch advance must clear pending commitments (N11)"
        );
    }
}
