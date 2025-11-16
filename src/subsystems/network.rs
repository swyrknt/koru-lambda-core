/// Structural Network Agent
///
/// Revolutionary P2P consensus layer where the network itself is a distinction
/// that evolves through pure causal synthesis. No traditional gossip, no voting -
/// just structural proof-of-causality (SPoC).
///
/// Design Principles:
/// - **Network-as-Distinction**: The network state is a distinction evolving via synthesis
/// - **Deterministic Leadership**: Leader election is pure function of epoch + validator set
/// - **Fixed-Time Window**: 2-second FTW for leader to propose next batch
/// - **Forkless Consensus**: Structural validation prevents forks automatically
/// - **Emergent Validator Set**: Peers become validators through synthesis topology
///
/// Groundbreaking Insight:
/// The network doesn't "agree" on state - it IS the state. Every node synthesizes
/// the same causal chain because synthesis is deterministic. Divergence is impossible
/// by construction (Axiom: Symmetry).

use crate::primitives::Canonicalizable;
use crate::subsystems::commitment::{BatchCommitment, CommitmentAgent};
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::subsystems::validator::{ConsensusValidator, TransactionBatch, BatchValidationResult};
use crate::{Distinction, DistinctionEngine};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

/// Peer identity in the network
///
/// Each peer is represented as a distinction, making peer relationships
/// structural rather than merely informational.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PeerIdentity {
    /// Peer's unique identifier (typically a public key hash)
    pub id: String,
    /// Peer represented as a distinction in the synthesis space
    pub distinction: Distinction,
}

impl PeerIdentity {
    /// Create a new peer identity from an ID string
    pub fn new(id: String, engine: &Arc<DistinctionEngine>) -> Self {
        // Canonicalize peer ID into a distinction
        let id_bytes = id.as_bytes();
        let distinction = id_bytes.iter().fold(engine.d0().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

        Self { id, distinction }
    }

    /// Get peer's distinction ID
    pub fn distinction_id(&self) -> &str {
        self.distinction.id()
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
            }
            NetworkAction::BatchProposed { batch } => {
                // Synthesize: batch_marker ⊕ previous_root_hash
                let batch_marker = engine.d0().clone(); // d0 = "batch" event

                // Hash previous root for content addressing
                let root_bytes = batch.previous_root.as_bytes();
                let root_distinction = root_bytes.iter().take(8).fold(
                    engine.d0().clone(),
                    |acc, &byte| {
                        let byte_d = byte.to_canonical_structure(engine);
                        engine.synthesize(&acc, &byte_d)
                    },
                );

                engine.synthesize(&batch_marker, &root_distinction)
            }
            NetworkAction::EpochAdvanced { new_epoch } => {
                // Synthesize: epoch_marker ⊕ epoch_value
                let epoch_bytes = new_epoch.to_le_bytes();
                let epoch_distinction = epoch_bytes.iter().fold(
                    engine.d1().clone(),
                    |acc, &byte| {
                        let byte_d = byte.to_canonical_structure(engine);
                        engine.synthesize(&acc, &byte_d)
                    },
                );

                epoch_distinction
            }
        }
    }
}

/// Structural Network Agent Implementation
///
/// Manages network consensus through pure causal synthesis. The network state
/// is a distinction that evolves as events are synthesized into it.
///
/// Key Innovation: **No external consensus mechanism needed**. The synthesis
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
    /// Pending commitments awaiting finalization
    pending_commitments: HashMap<[u8; 32], BatchCommitment>,
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
            pending_commitments: HashMap::new(),
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
            pending_commitments: HashMap::new(),
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
        // Check if peer already exists
        if self.validator_set.iter().any(|p| p.id == peer.id) {
            return self.local_root.clone();
        }

        // Add to validator set
        self.validator_set.push(peer.clone());

        // Synthesize join event into network state
        let action = NetworkAction::PeerJoined { peer };
        self.synthesize_action(action, engine)
    }

    /// **STAGE 1: Propose Commitment** (Two-Stage Gossip Protocol)
    ///
    /// Leader computes lightweight commitment hash, caches the batch data,
    /// and returns the commitment for gossiping.
    ///
    /// **Returns:** BatchCommitment (80 bytes to gossip)
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
        let leader_id = self.get_current_leader()
            .map(|p| p.id.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let commitment = BatchCommitment::compute(&batch, nonce, epoch, leader_id);

        // Cache batch data for Stage 2 fetches
        self.commitment_agent.cache_batch(batch.clone(), commitment.clone());
        self.pending_commitments.insert(commitment.commitment_hash, commitment.clone());

        Ok(commitment)
    }

    /// **STAGE 1: Check Commitment** (Light Node "Ping" Check)
    ///
    /// Verifies commitment against expected nonce/epoch WITHOUT downloading batch.
    ///
    /// This is how light clients achieve consensus participation:
    /// - Receive 80-byte commitment via gossip
    /// - Verify nonce/epoch match local expectations
    /// - Accept/reject without bandwidth cost
    ///
    /// **Returns:** true if commitment is valid for current state
    pub fn check_commitment(&self, commitment: &BatchCommitment) -> bool {
        let expected_nonce = self.validator.expected_nonce();
        let expected_epoch = self.current_epoch;

        commitment.verify(expected_nonce, expected_epoch)
    }

    /// **STAGE 2: Finalize Batch** (Full Validator Execution)
    ///
    /// Applies the full batch after fetching data and verifying it matches commitment.
    ///
    /// Called by full validators who need to execute transactions.
    /// Runtime fetches batch data from peers, verifies hash matches commitment,
    /// then calls this to finalize state transition.
    ///
    /// **Returns:** New network root on success
    pub fn finalize_batch(
        &mut self,
        batch: TransactionBatch,
        commitment_hash: [u8; 32],
        engine: &Arc<DistinctionEngine>,
    ) -> Result<Distinction, String> {
        // Verify we have pending commitment
        let commitment = self.pending_commitments.get(&commitment_hash)
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
                self.pending_commitments.remove(&commitment_hash);

                Ok(new_network_root)
            }
            BatchValidationResult::Rejected(reason) => Err(reason),
        }
    }

    /// **DEPRECATED: Old synchronous propose_batch**
    ///
    /// Use `propose_commitment()` + `finalize_batch()` for two-stage protocol.
    /// This method bypasses commitment gossip and is only for backward compatibility.
    #[deprecated(note = "Use propose_commitment() and finalize_batch() instead")]
    pub fn propose_batch(
        &mut self,
        batch: TransactionBatch,
        engine: &Arc<DistinctionEngine>,
    ) -> Result<Distinction, String> {
        // Validate batch structurally
        let result = self.validator.validate_batch(batch.clone(), engine);

        match result {
            BatchValidationResult::Valid(_new_state_root) => {
                // Batch is valid - synthesize into network state
                let action = NetworkAction::BatchProposed { batch };
                let new_network_root = self.synthesize_action(action, engine);

                Ok(new_network_root)
            }
            BatchValidationResult::Rejected(reason) => Err(reason),
        }
    }

    /// Advance to next epoch (triggers leader rotation)
    ///
    /// This happens after FTW expires or batch is accepted.
    pub fn advance_epoch(&mut self, engine: &Arc<DistinctionEngine>) -> Distinction {
        self.current_epoch += 1;

        let action = NetworkAction::EpochAdvanced {
            new_epoch: self.current_epoch,
        };

        self.synthesize_action(action, engine)
    }

    /// Deterministically elect current leader based on epoch + validator set
    ///
    /// **Groundbreaking**: No voting, no communication - pure function of state.
    /// All nodes compute identical leader given identical epoch + validator set.
    ///
    /// This is FORKLESS by construction (Axiom: Symmetry).
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
            hash[0], hash[1], hash[2], hash[3],
            hash[4], hash[5], hash[6], hash[7],
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

    /// Get current consensus state root
    pub fn consensus_state_root(&self) -> &str {
        self.validator.state_root_id()
    }

    /// Get network statistics
    pub fn get_stats(&self) -> NetworkStats {
        NetworkStats {
            current_epoch: self.current_epoch,
            validator_count: self.validator_set.len(),
            events_processed: self.events_processed,
            consensus_nonce: self.validator.expected_nonce(),
            network_root: self.local_root.id().to_string(),
        }
    }

    /// Get Fixed-Time Window duration
    pub fn ftw_duration(&self) -> u64 {
        self.ftw_duration_ms
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
        assert!(!agent.get_current_root().id().is_empty());
    }

    #[test]
    fn test_peer_identity_creation() {
        let engine = Arc::new(DistinctionEngine::new());

        let peer1 = PeerIdentity::new("peer_1".to_string(), &engine);
        let peer2 = PeerIdentity::new("peer_1".to_string(), &engine);
        let peer3 = PeerIdentity::new("peer_2".to_string(), &engine);

        // Same ID → same distinction (determinism)
        assert_eq!(peer1.distinction_id(), peer2.distinction_id());

        // Different ID → different distinction
        assert_ne!(peer1.distinction_id(), peer3.distinction_id());
    }

    #[test]
    fn test_peer_joining() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let initial_root = agent.get_current_root().id().to_string();

        // Join first peer
        let peer1 = PeerIdentity::new("validator_1".to_string(), &engine);
        let new_root = agent.join_peer(peer1.clone(), &engine);

        // Network root should change
        assert_ne!(new_root.id(), &initial_root);
        assert_eq!(agent.validator_count(), 1);

        // Join second peer
        let peer2 = PeerIdentity::new("validator_2".to_string(), &engine);
        agent.join_peer(peer2, &engine);

        assert_eq!(agent.validator_count(), 2);
    }

    #[test]
    fn test_deterministic_leader_election() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        // Add validators
        for i in 0..5 {
            let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
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
            let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
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
    fn test_batch_proposal() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        // Add a validator
        let peer = PeerIdentity::new("leader".to_string(), &engine);
        agent.join_peer(peer, &engine);

        // Create valid batch
        use crate::subsystems::validator::TransactionAction;

        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: agent.consensus_state_root().to_string(),
        };

        // Propose batch
        let result = agent.propose_batch(batch, &engine);

        assert!(result.is_ok());
        assert_eq!(agent.validator.expected_nonce(), 1);
    }

    #[test]
    fn test_epoch_advancement() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        assert_eq!(agent.current_epoch(), 0);

        let initial_root = agent.get_current_root().id().to_string();

        // Advance epoch
        let new_root = agent.advance_epoch(&engine);

        assert_eq!(agent.current_epoch(), 1);
        assert_ne!(new_root.id(), &initial_root);

        // Advance again
        agent.advance_epoch(&engine);
        assert_eq!(agent.current_epoch(), 2);
    }

    #[test]
    fn test_network_action_canonical() {
        let engine = Arc::new(DistinctionEngine::new());

        let peer = PeerIdentity::new("peer_1".to_string(), &engine);

        let action1 = NetworkAction::PeerJoined { peer: peer.clone() };
        let action2 = NetworkAction::PeerJoined { peer: peer.clone() };

        let d1 = action1.to_canonical_structure(&engine);
        let d2 = action2.to_canonical_structure(&engine);

        // Same action → same distinction (determinism)
        assert_eq!(d1.id(), d2.id());
    }

    #[test]
    fn test_local_causal_agent_compliance() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        let initial_root = agent.get_current_root().id().to_string();

        // Synthesize an action
        let peer = PeerIdentity::new("peer_1".to_string(), &engine);
        let action = NetworkAction::PeerJoined { peer };

        let new_root = agent.synthesize_action(action, &engine);

        // Root should change (causal synthesis)
        assert_ne!(new_root.id(), &initial_root);
        assert_eq!(new_root.id(), agent.get_current_root().id());

        // Events counter should increment
        let stats = agent.get_stats();
        assert_eq!(stats.events_processed, 1);
    }
}
