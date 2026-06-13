/// Two-stage batch commitment protocol.
///
/// Stage 1: Broadcast lightweight commitments (32 bytes) for consensus.
/// Stage 2: Fetch full batch data on demand.
///
/// Reduces network overhead by separating consensus from data availability.
use crate::primitives::Canonicalizable;
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::subsystems::validator::TransactionBatch;
use crate::{Distinction, DistinctionEngine};
use lru::LruCache;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// Type alias for batch cache: commitment hash -> (batch, commitment)
type BatchCache = LruCache<[u8; 32], (TransactionBatch, BatchCommitment)>;

/// Lightweight batch commitment (Stage 1 of two-stage protocol).
///
/// Broadcast to all nodes for consensus. Approximately 80 bytes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchCommitment {
    /// 32-byte commitment hash
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
    /// Deterministically hash the batch
    /// without needing to synthesize it first.
    ///
    pub fn compute(batch: &TransactionBatch, nonce: u64, epoch: u64, leader_id: String) -> Self {
        // Compute batch root (hash of all transaction data)
        let batch_root = Self::compute_batch_root(batch);

        // Deterministic commitment: SHA256(batch_root || nonce || epoch)
        let mut hasher = Sha256::new();
        hasher.update(batch_root);
        hasher.update(nonce.to_le_bytes());
        hasher.update(epoch.to_le_bytes());
        let commitment_hash: [u8; 32] = hasher.finalize().into();

        Self { commitment_hash, nonce, epoch, leader_id, batch_size: batch.transactions.len() }
    }

    /// Compute root hash of batch data
    ///
    /// Creates a content-addressable identifier for the batch.
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
    /// that a commitment is valid WITHOUT downloading the batch.
    ///
    pub fn verify(&self, expected_nonce: u64, expected_epoch: u64) -> bool {
        self.nonce == expected_nonce && self.epoch == expected_epoch
    }

    /// Verify commitment matches actual batch data
    ///
    /// Called after fetching the batch to ensure data matches the commitment.
    /// Stage 2 verification after lazy fetch.
    pub fn verify_batch(&self, batch: &TransactionBatch, nonce: u64, epoch: u64) -> bool {
        let recomputed = Self::compute(batch, nonce, epoch, self.leader_id.clone());
        self.commitment_hash == recomputed.commitment_hash
    }
}

/// Commitment can be synthesized into distinction space
///
/// Allows network agents to maintain a causal chain of commitments
/// without storing full batch data.
impl Canonicalizable for BatchCommitment {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        // Canonicalize the commitment hash
        let hash_distinction =
            self.commitment_hash.iter().take(16).fold(engine.d0().clone(), |acc, &byte| {
                let byte_d = byte.to_canonical_structure(engine);
                engine.synthesize(&acc, &byte_d)
            });

        // Canonicalize nonce
        let nonce_bytes = self.nonce.to_le_bytes();
        let nonce_distinction = nonce_bytes.iter().fold(engine.d1().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

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
pub struct CommitmentAgent {
    /// Current local root (causal chain of commitments)
    local_root: Distinction,

    /// Cache of batch data for Stage 2 fetches (LRU handles capacity internally)
    cache: BatchCache,

    /// Number of commitments processed
    commitments_processed: u64,

    /// Expected nonce for next commitment
    expected_nonce: u64,
}

impl CommitmentAgent {
    /// Create new commitment agent
    pub fn new(engine: &Arc<DistinctionEngine>) -> Self {
        // Initialize with genesis
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        Self {
            local_root: genesis,
            cache: LruCache::new(std::num::NonZeroUsize::new(1000).unwrap()),
            commitments_processed: 0,
            expected_nonce: 0,
        }
    }

    /// Create agent from existing state
    pub fn from_root(root: Distinction, expected_nonce: u64, cache_size: usize) -> Self {
        Self {
            local_root: root,
            cache: LruCache::new(std::num::NonZeroUsize::new(cache_size).unwrap()),
            commitments_processed: 0,
            expected_nonce,
        }
    }

    /// Store batch data in cache (for Stage 2 data availability)
    pub fn cache_batch(&mut self, batch: TransactionBatch, commitment: BatchCommitment) {
        // LRU eviction handled automatically by put()
        self.cache.put(commitment.commitment_hash, (batch, commitment));
    }

    /// Retrieve cached batch by commitment hash
    pub fn get_cached_batch(
        &mut self,
        commitment_hash: &[u8; 32],
    ) -> Option<&(TransactionBatch, BatchCommitment)> {
        self.cache.get(commitment_hash)
    }

    /// Check if batch is cached
    pub fn has_cached(&mut self, commitment_hash: &[u8; 32]) -> bool {
        self.cache.contains(commitment_hash)
    }

    /// Get expected nonce for next commitment
    pub fn expected_nonce(&self) -> u64 {
        self.expected_nonce
    }

    /// Get number of commitments processed
    pub fn commitments_processed(&self) -> u64 {
        self.commitments_processed
    }

    /// Get commitment root ID as a 32-character hex string.
    pub fn commitment_root_id(&self) -> String {
        self.local_root.to_hex()
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

        // Update local root via synthesis
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystems::validator::TransactionAction;

    #[test]
    fn test_commitment_computation() {
        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction { nonce: 0, data: vec![1, 2, 3] },
                TransactionAction { nonce: 1, data: vec![4, 5, 6] },
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
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
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
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
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
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::from_root(
            engine.d0().clone(),
            0,
            2, // Small cache size for testing
        );

        let batch1 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1] }],
            previous_root: "root1".to_string(),
        };

        let batch2 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 1, data: vec![2] }],
            previous_root: "root2".to_string(),
        };

        let commit1 = BatchCommitment::compute(&batch1, 0, 0, "leader".to_string());
        let commit2 = BatchCommitment::compute(&batch2, 1, 0, "leader".to_string());

        agent.cache_batch(batch1.clone(), commit1.clone());
        agent.cache_batch(batch2.clone(), commit2.clone());

        // Verify we can retrieve
        assert!(agent.has_cached(&commit1.commitment_hash));
        assert!(agent.has_cached(&commit2.commitment_hash));

        let retrieved = agent.get_cached_batch(&commit1.commitment_hash).unwrap();
        assert_eq!(retrieved.0.transactions[0].data, vec![1]);
    }

    #[test]
    fn test_lru_eviction_order() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::from_root(
            engine.d0().clone(),
            0,
            2, // Cache size of 2 for testing eviction
        );

        // Create 3 batches
        let batch1 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1] }],
            previous_root: "root1".to_string(),
        };
        let batch2 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 1, data: vec![2] }],
            previous_root: "root2".to_string(),
        };
        let batch3 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 2, data: vec![3] }],
            previous_root: "root3".to_string(),
        };

        let commit1 = BatchCommitment::compute(&batch1, 0, 0, "leader".to_string());
        let commit2 = BatchCommitment::compute(&batch2, 1, 0, "leader".to_string());
        let commit3 = BatchCommitment::compute(&batch3, 2, 0, "leader".to_string());

        // Add batch1 and batch2 (cache full)
        agent.cache_batch(batch1.clone(), commit1.clone());
        agent.cache_batch(batch2.clone(), commit2.clone());

        // Access batch1 (makes it recently used)
        let _ = agent.get_cached_batch(&commit1.commitment_hash);

        // Add batch3 (should evict batch2, not batch1 due to LRU)
        agent.cache_batch(batch3.clone(), commit3.clone());

        // batch1 should still be cached (was accessed recently)
        assert!(
            agent.has_cached(&commit1.commitment_hash),
            "batch1 should still be cached (recently accessed)"
        );

        // batch2 should be evicted (least recently used)
        assert!(!agent.has_cached(&commit2.commitment_hash), "batch2 should be evicted (LRU)");

        // batch3 should be cached (just added)
        assert!(agent.has_cached(&commit3.commitment_hash), "batch3 should be cached (just added)");
    }

    #[test]
    fn test_commitment_canonical() {
        let engine = Arc::new(DistinctionEngine::new());

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: "genesis".to_string(),
        };

        let commitment1 = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());
        let commitment2 = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());

        let d1 = commitment1.to_canonical_structure(&engine);
        let d2 = commitment2.to_canonical_structure(&engine);

        // Same commitment -> same distinction
        assert_eq!(d1.to_hex(), d2.to_hex());
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

        let initial_root = agent.get_current_root().to_hex();

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: "genesis".to_string(),
        };

        let commitment = BatchCommitment::compute(&batch, 0, 0, "leader".to_string());

        // Synthesize commitment via LocalCausalAgent
        let new_root = agent.synthesize_action(commitment.clone(), &engine);

        // Verify state changed
        assert_ne!(new_root.to_hex(), initial_root);
        assert_eq!(new_root.to_hex(), agent.get_current_root().to_hex());
        assert_eq!(agent.expected_nonce(), 1);
        assert_eq!(agent.commitments_processed(), 1);
    }

    #[test]
    fn test_commitment_agent_nonce_validation() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::new(&engine);

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: "genesis".to_string(),
        };

        // Invalid nonce (expected 0, got 5)
        let bad_commitment = BatchCommitment::compute(&batch, 5, 0, "leader".to_string());

        let initial_root = agent.get_current_root().to_hex();
        let result = agent.synthesize_action(bad_commitment, &engine);

        // State should be unchanged
        assert_eq!(result.to_hex(), initial_root);
        assert_eq!(agent.expected_nonce(), 0);
        assert_eq!(agent.commitments_processed(), 0);
    }

    #[test]
    fn test_commitment_agent_caching() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = CommitmentAgent::new(&engine);

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
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
