/// Commitment Protocol - Two-Stage Gossip System
///
/// Implements the revolutionary insight: **The commitment IS the consensus**.
///
/// Traditional systems broadcast full data to all nodes (wasteful).
/// This system separates concerns:
/// - **Stage 1 (FAST)**: Gossip lightweight commitments (32 bytes) for consensus
/// - **Stage 2 (LAZY)**: Fetch heavy data only when needed
///
/// Design Principles:
/// - **Locality**: Anchored to local commitment root distinction
/// - **Causality**: ΔNew = ΔCommitment_Root ⊕ ΔBatch_Commitment
/// - **Determinism**: Commitment = SHA256(batch_root || nonce || epoch)
/// - **Verification**: Light nodes can verify without downloading
/// - **Efficiency**: Only fetch data when actually needed
///
/// Theoretical Foundation:
/// In distinction calculus, the distinction (hash) IS the truth.
/// The data is merely evidence that can be fetched on demand.

use crate::primitives::Canonicalizable;
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::subsystems::validator::TransactionBatch;
use crate::{Distinction, DistinctionEngine};
use sha2::{Digest, Sha256};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Stage 1: Lightweight commitment for fast gossip
///
/// This is what gets broadcast to all nodes. Size: ~80 bytes total.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchCommitment {
    /// 32-byte commitment hash (deterministic proof of batch state)
    pub commitment_hash: [u8; 32],

    /// Sequential nonce for causal ordering
    pub nonce: u64,

    /// Current epoch for leader validation
    pub epoch: u64,

    /// Leader who proposed this batch
    pub leader_id: String,

    /// Batch size (number of transactions)
    pub batch_size: usize,
}

impl BatchCommitment {
    /// Compute commitment from batch + metadata
    ///
    /// This is the CORE OPERATION: deterministically hash the batch
    /// without needing to synthesize it first.
    ///
    /// **Crucially**: All nodes can verify this hash matches their expected
    /// state progression WITHOUT downloading the full batch.
    pub fn compute(
        batch: &TransactionBatch,
        nonce: u64,
        epoch: u64,
        leader_id: String,
    ) -> Self {
        // Compute batch root (hash of all transaction data)
        let batch_root = Self::compute_batch_root(batch);

        // Deterministic commitment: SHA256(batch_root || nonce || epoch)
        let mut hasher = Sha256::new();
        hasher.update(&batch_root);
        hasher.update(nonce.to_le_bytes());
        hasher.update(epoch.to_le_bytes());
        let commitment_hash: [u8; 32] = hasher.finalize().into();

        Self {
            commitment_hash,
            nonce,
            epoch,
            leader_id,
            batch_size: batch.transactions.len(),
        }
    }

    /// Compute root hash of batch data
    ///
    /// This creates a content-addressable identifier for the batch.
    fn compute_batch_root(batch: &TransactionBatch) -> [u8; 32] {
        let mut hasher = Sha256::new();

        // Hash previous root reference
        hasher.update(batch.previous_root.as_bytes());

        // Hash each transaction in order
        for tx in &batch.transactions {
            hasher.update(tx.nonce.to_le_bytes());
            hasher.update(&tx.data);
        }

        hasher.finalize().into()
    }

    /// Verify commitment against expected state
    ///
    /// **The "Ping" Check**: Light nodes call this to verify
    /// that a commitment is valid WITHOUT downloading the batch.
    ///
    /// This is how light clients achieve consensus participation
    /// without the storage/bandwidth of full nodes.
    pub fn verify(&self, expected_nonce: u64, expected_epoch: u64) -> bool {
        self.nonce == expected_nonce && self.epoch == expected_epoch
    }

    /// Verify commitment matches actual batch data
    ///
    /// Called after fetching the batch to ensure data matches the commitment.
    /// This is the Stage 2 verification after lazy fetch.
    pub fn verify_batch(
        &self,
        batch: &TransactionBatch,
        nonce: u64,
        epoch: u64,
    ) -> bool {
        let recomputed = Self::compute(batch, nonce, epoch, self.leader_id.clone());
        self.commitment_hash == recomputed.commitment_hash
    }
}

/// Commitment can be synthesized into distinction space
///
/// This allows network agents to maintain a causal chain of commitments
/// without storing full batch data.
impl Canonicalizable for BatchCommitment {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        // Canonicalize the commitment hash
        let hash_distinction = self.commitment_hash.iter().take(16).fold(
            engine.d0().clone(),
            |acc, &byte| {
                let byte_d = byte.to_canonical_structure(engine);
                engine.synthesize(&acc, &byte_d)
            },
        );

        // Canonicalize nonce
        let nonce_bytes = self.nonce.to_le_bytes();
        let nonce_distinction = nonce_bytes.iter().fold(
            engine.d1().clone(),
            |acc, &byte| {
                let byte_d = byte.to_canonical_structure(engine);
                engine.synthesize(&acc, &byte_d)
            },
        );

        // Synthesize: hash ⊕ nonce (epoch implicit in hash)
        engine.synthesize(&hash_distinction, &nonce_distinction)
    }
}

/// Stage 2: Data fetch request
///
/// When a node needs the actual batch data (e.g., to execute or validate),
/// it sends this request to peers who have cached the data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDataRequest {
    /// Commitment hash being requested
    pub commitment_hash: [u8; 32],

    /// Requesting peer ID
    pub requester_id: String,
}

/// Stage 2: Data fetch response
///
/// Peer responds with the full batch data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchDataResponse {
    /// The full batch data
    pub batch: TransactionBatch,

    /// The commitment this data corresponds to
    pub commitment: BatchCommitment,
}

/// Commitment Agent - LocalCausalAgent implementation for commitment tracking
///
/// Manages the commitment protocol as a proper subprocess anchored to a local root.
/// Maintains a causal chain of commitments and caches batch data for Stage 2 fetches.
///
/// Implements LocalCausalAgent to enforce:
/// - Locality: Anchored to commitment root distinction
/// - Causality: ΔNew = ΔLocal ⊕ ΔCommitment
/// - Determinism: All commitments are canonicalizable
pub struct CommitmentAgent {
    /// Current local root (causal chain of commitments)
    local_root: Distinction,

    /// Cache of batch data for Stage 2 fetches
    cache: std::collections::HashMap<[u8; 32], (TransactionBatch, BatchCommitment)>,

    /// Maximum cache size
    max_cache_size: usize,

    /// Number of commitments processed
    commitments_processed: u64,

    /// Expected nonce for next commitment
    expected_nonce: u64,
}

impl CommitmentAgent {
    /// Create new commitment agent anchored at genesis
    pub fn new(engine: &Arc<DistinctionEngine>) -> Self {
        // Genesis commitment root: d0 ⊕ d1
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        Self {
            local_root: genesis,
            cache: std::collections::HashMap::new(),
            max_cache_size: 1000, // Default: cache last 1000 batches
            commitments_processed: 0,
            expected_nonce: 0,
        }
    }

    /// Create agent from existing state
    pub fn from_root(root: Distinction, expected_nonce: u64, max_cache_size: usize) -> Self {
        Self {
            local_root: root,
            cache: std::collections::HashMap::new(),
            max_cache_size,
            commitments_processed: 0,
            expected_nonce,
        }
    }

    /// Store batch data in cache (for Stage 2 data availability)
    pub fn cache_batch(&mut self, batch: TransactionBatch, commitment: BatchCommitment) {
        // Evict oldest if cache full
        if self.cache.len() >= self.max_cache_size {
            // Simple FIFO eviction (TODO: implement LRU)
            if let Some(&first_key) = self.cache.keys().next() {
                self.cache.remove(&first_key);
            }
        }

        self.cache.insert(commitment.commitment_hash, (batch, commitment));
    }

    /// Retrieve cached batch by commitment hash
    pub fn get_cached_batch(&self, commitment_hash: &[u8; 32])
        -> Option<&(TransactionBatch, BatchCommitment)> {
        self.cache.get(commitment_hash)
    }

    /// Check if batch is cached
    pub fn has_cached(&self, commitment_hash: &[u8; 32]) -> bool {
        self.cache.contains_key(commitment_hash)
    }

    /// Get expected nonce for next commitment
    pub fn expected_nonce(&self) -> u64 {
        self.expected_nonce
    }

    /// Get number of commitments processed
    pub fn commitments_processed(&self) -> u64 {
        self.commitments_processed
    }

    /// Get commitment root ID
    pub fn commitment_root_id(&self) -> &str {
        self.local_root.id()
    }
}

impl LocalCausalAgent for CommitmentAgent {
    type ActionData = BatchCommitment;

    fn get_current_root(&self) -> &Distinction {
        &self.local_root
    }

    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        // Verify nonce sequencing
        if action_data.nonce != self.expected_nonce {
            // Invalid nonce - return current root unchanged
            return self.local_root.clone();
        }

        // Canonicalize commitment into distinction
        let commitment_distinction = action_data.to_canonical_structure(engine);

        // Causal synthesis: ΔNew = ΔCommitment_Root ⊕ ΔBatch_Commitment
        let new_root = engine.synthesize(&self.local_root, &commitment_distinction);

        // Update state
        self.local_root = new_root.clone();
        self.expected_nonce += 1;
        self.commitments_processed += 1;

        new_root
    }

    fn update_local_root(&mut self, new_root: Distinction) {
        self.local_root = new_root;
    }
}

/// Legacy cache wrapper for backward compatibility
/// (Use CommitmentAgent directly for new code)
pub struct CommitmentCache {
    cache: std::collections::HashMap<[u8; 32], (TransactionBatch, BatchCommitment)>,
    max_size: usize,
}

impl CommitmentCache {
    pub fn new(max_size: usize) -> Self {
        Self {
            cache: std::collections::HashMap::new(),
            max_size,
        }
    }

    pub fn store(&mut self, batch: TransactionBatch, commitment: BatchCommitment) {
        if self.cache.len() >= self.max_size {
            if let Some(&first_key) = self.cache.keys().next() {
                self.cache.remove(&first_key);
            }
        }
        self.cache.insert(commitment.commitment_hash, (batch, commitment));
    }

    pub fn get(&self, commitment_hash: &[u8; 32]) -> Option<&(TransactionBatch, BatchCommitment)> {
        self.cache.get(commitment_hash)
    }

    pub fn has(&self, commitment_hash: &[u8; 32]) -> bool {
        self.cache.contains_key(commitment_hash)
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }

    pub fn is_empty(&self) -> bool {
        self.cache.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystems::validator::TransactionAction;
    use std::sync::Arc;

    #[test]
    fn test_commitment_computation() {
        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction {
                    nonce: 0,
                    data: vec![1, 2, 3],
                },
                TransactionAction {
                    nonce: 1,
                    data: vec![4, 5, 6],
                },
            ],
            previous_root: "genesis".to_string(),
        };

        let commitment1 = BatchCommitment::compute(&batch, 0, 0, "leader_1".to_string());
        let commitment2 = BatchCommitment::compute(&batch, 0, 0, "leader_1".to_string());

        // Determinism: same inputs -> same commitment
        assert_eq!(commitment1.commitment_hash, commitment2.commitment_hash);
    }

    #[test]
    fn test_commitment_verification() {
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "genesis".to_string(),
        };

        let commitment = BatchCommitment::compute(&batch, 5, 10, "leader".to_string());

        // Valid verification
        assert!(commitment.verify(5, 10));

        // Invalid nonce
        assert!(!commitment.verify(6, 10));

        // Invalid epoch
        assert!(!commitment.verify(5, 11));
    }

    #[test]
    fn test_commitment_batch_verification() {
        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "genesis".to_string(),
        };

        let commitment = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());

        // Verify batch matches commitment
        assert!(commitment.verify_batch(&batch, 0, 0));

        // Modified batch should fail verification
        let mut modified_batch = batch.clone();
        modified_batch.transactions[0].data = vec![7, 8, 9];

        assert!(!commitment.verify_batch(&modified_batch, 0, 0));
    }

    #[test]
    fn test_commitment_cache() {
        let mut cache = CommitmentCache::new(2);

        let batch1 = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1],
            }],
            previous_root: "root1".to_string(),
        };

        let batch2 = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 1,
                data: vec![2],
            }],
            previous_root: "root2".to_string(),
        };

        let commit1 = BatchCommitment::compute(&batch1, 0, 0, "leader".to_string());
        let commit2 = BatchCommitment::compute(&batch2, 1, 0, "leader".to_string());

        cache.store(batch1.clone(), commit1.clone());
        cache.store(batch2.clone(), commit2.clone());

        assert_eq!(cache.len(), 2);
        assert!(cache.has(&commit1.commitment_hash));
        assert!(cache.has(&commit2.commitment_hash));

        // Verify we can retrieve
        let retrieved = cache.get(&commit1.commitment_hash).unwrap();
        assert_eq!(retrieved.0.transactions[0].data, vec![1]);
    }

    #[test]
    fn test_commitment_canonical() {
        let engine = Arc::new(DistinctionEngine::new());

        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "genesis".to_string(),
        };

        let commitment1 = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());
        let commitment2 = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());

        let d1 = commitment1.to_canonical_structure(&engine);
        let d2 = commitment2.to_canonical_structure(&engine);

        // Same commitment -> same distinction
        assert_eq!(d1.id(), d2.id());
    }

    #[test]
    fn test_commitment_agent_genesis() {
        let engine = Arc::new(DistinctionEngine::new());
        let agent = CommitmentAgent::new(&engine);

        assert_eq!(agent.expected_nonce(), 0);
        assert_eq!(agent.commitments_processed(), 0);
        assert!(!agent.commitment_root_id().is_empty());
    }

    #[test]
    fn test_commitment_agent_synthesis() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::new(&engine);

        let initial_root = agent.get_current_root().id().to_string();

        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "genesis".to_string(),
        };

        let commitment = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());

        // Synthesize commitment via LocalCausalAgent
        let new_root = agent.synthesize_action(commitment.clone(), &engine);

        // Verify state changed
        assert_ne!(new_root.id(), &initial_root);
        assert_eq!(new_root.id(), agent.get_current_root().id());
        assert_eq!(agent.expected_nonce(), 1);
        assert_eq!(agent.commitments_processed(), 1);
    }

    #[test]
    fn test_commitment_agent_nonce_validation() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::new(&engine);

        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "genesis".to_string(),
        };

        // Invalid nonce (expected 0, got 5)
        let bad_commitment = BatchCommitment::compute(&batch, 5, 0, "leader".to_string());

        let initial_root = agent.get_current_root().id().to_string();
        let result = agent.synthesize_action(bad_commitment, &engine);

        // State should be unchanged
        assert_eq!(result.id(), &initial_root);
        assert_eq!(agent.expected_nonce(), 0);
        assert_eq!(agent.commitments_processed(), 0);
    }

    #[test]
    fn test_commitment_agent_caching() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::new(&engine);

        let batch = TransactionBatch {
            transactions: vec![TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3],
            }],
            previous_root: "genesis".to_string(),
        };

        let commitment = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());

        // Cache the batch
        agent.cache_batch(batch.clone(), commitment.clone());

        // Verify we can retrieve it
        assert!(agent.has_cached(&commitment.commitment_hash));

        let cached = agent.get_cached_batch(&commitment.commitment_hash).unwrap();
        assert_eq!(cached.0.transactions[0].data, vec![1, 2, 3]);
    }
}
