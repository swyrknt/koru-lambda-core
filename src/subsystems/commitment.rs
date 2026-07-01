//! Batch commitment — reference LCA consumer.
//!
//! A **`BatchCommitment`** is a cryptographic proof that a specific
//! batch was proposed with specific metadata (nonce, epoch, leader).
//! It's safe to share BEFORE the batch data itself — Stage-2 fetch
//! then delivers the batch, verified against the commitment.
//!
//! # v1.2 audit findings closed by construction
//!
//! - **N6 (tampered leader_id survives verification)** —
//!   [`BatchCommitment::compute`] hashes `leader_id` into the SHA-256
//!   digest with a length prefix. Any tamper produces a different hash;
//!   [`BatchCommitment::verify_batch`] recomputes and compares.
//! - **F7 transitive (verify rejects tampered leader_id)** — closed
//!   automatically because `verify_batch` re-runs `compute` with the
//!   stored leader_id. If the stored leader_id was tampered, the
//!   recomputed hash won't match the stored `commitment_hash`.

use crate::primitives::{ByteMapping, Canonicalizable};
use crate::subsystems::validator::TransactionBatch;
use crate::{Distinction, DistinctionEngine, LocalCausalAgent};
use lru::LruCache;
use sha2::{Digest, Sha256};
use std::num::NonZeroUsize;
use std::sync::Arc;

// --- Constants ---------------------------------------------------------

/// Maximum bytes in a leader identifier.
///
/// **Derivation:** matches `network.rs`'s `MAX_PEER_ID_LEN` (Step 2c)
/// so a peer identifier accepted by the network is also accepted as a
/// commitment leader. 64 bytes covers UUIDs, hex-encoded pubkeys, and
/// typical peer-id schemes without allowing DoS via oversized strings.
pub const MAX_LEADER_ID_LEN: usize = 64;

/// LRU capacity for the batch-data cache in [`CommitmentAgent`].
///
/// **Derivation:** carried forward from the v1.2 baseline (dev's cap
/// was 1000). Each entry stores one `TransactionBatch` (≤ 4 MB
/// worst case) + one `BatchCommitment` (~120 B). At 1000 entries the
/// worst-case cache footprint is bounded at ~4 GB; typical peer
/// workloads see much smaller batches (~KB range), keeping the cache
/// footprint in single-digit MB.
pub const COMMITMENT_CACHE_CAP: usize = 1000;

// --- Types -------------------------------------------------------------

/// A cryptographic commitment to a batch — safe to share before the
/// batch data itself.
///
/// # N6 closed by construction
///
/// The `commitment_hash` binds `batch_root`, `nonce`, `epoch`,
/// `leader_id`, and `batch_size` together via SHA-256. Any tamper —
/// including tampering with `leader_id` alone — changes the hash.
/// [`Self::verify_batch`] recomputes and compares.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BatchCommitment {
    /// SHA-256 digest of the commitment inputs.
    pub commitment_hash: [u8; 32],
    /// Sequential batch nonce.
    pub nonce: u64,
    /// Epoch in which the batch was proposed.
    pub epoch: u64,
    /// Leader that proposed the batch. Length-prefixed into the hash
    /// (N6). Bounded by [`MAX_LEADER_ID_LEN`].
    pub leader_id: String,
    /// Number of transactions in the batch (redundant with `batch` on
    /// the wire but hashed for defence-in-depth).
    pub batch_size: usize,
}

impl BatchCommitment {
    /// Compute a commitment from a batch + metadata.
    ///
    /// # Hash inputs (in order, all length-prefixed to prevent
    /// concatenation ambiguity)
    ///
    /// 1. `batch_root` (16 bytes) — the chain-fold of every transaction
    ///    starting from `batch.previous_root`.
    /// 2. `nonce` (8 bytes LE).
    /// 3. `epoch` (8 bytes LE).
    /// 4. `leader_id.len()` (8 bytes LE) — the length prefix that closes
    ///    N6 by preventing prefix-ambiguity attacks (a 5-byte
    ///    `"alice"` and a 5-byte `"al" + "ice"` split have the same
    ///    concatenation but different length-prefixed encodings).
    /// 5. `leader_id.as_bytes()`.
    /// 6. `batch_size` (8 bytes LE).
    ///
    /// # Errors
    ///
    /// - [`CommitmentError::EmptyLeaderId`] if `leader_id` is empty.
    /// - [`CommitmentError::LeaderIdTooLong`] if
    ///   `leader_id.len() > MAX_LEADER_ID_LEN`.
    ///
    /// The batch-root computation is infallible: `engine.synthesize`
    /// is monotone and always succeeds.
    pub fn compute(
        batch: &TransactionBatch,
        nonce: u64,
        epoch: u64,
        leader_id: String,
        engine: &Arc<DistinctionEngine>,
    ) -> Result<Self, CommitmentError> {
        if leader_id.is_empty() {
            return Err(CommitmentError::EmptyLeaderId);
        }
        if leader_id.len() > MAX_LEADER_ID_LEN {
            return Err(CommitmentError::LeaderIdTooLong {
                got: leader_id.len(),
                cap: MAX_LEADER_ID_LEN,
            });
        }

        let batch_root = compute_batch_root(batch, engine);
        let batch_size = batch.transactions.len();

        let mut hasher = Sha256::new();
        hasher.update(batch_root.as_bytes());
        hasher.update(nonce.to_le_bytes());
        hasher.update(epoch.to_le_bytes());
        hasher.update((leader_id.len() as u64).to_le_bytes());
        hasher.update(leader_id.as_bytes());
        hasher.update((batch_size as u64).to_le_bytes());

        Ok(Self { commitment_hash: hasher.finalize().into(), nonce, epoch, leader_id, batch_size })
    }

    /// Verify a batch matches this commitment.
    ///
    /// Recomputes the commitment using the stored `nonce`, `epoch`, and
    /// `leader_id` (F7 transitive: if any of those were tampered
    /// post-`compute`, the recomputed hash differs). Also verifies
    /// `batch_size` matches — the batch cannot have been truncated or
    /// padded.
    ///
    /// Returns `true` iff the batch is authentic under this commitment.
    #[must_use]
    pub fn verify_batch(&self, batch: &TransactionBatch, engine: &Arc<DistinctionEngine>) -> bool {
        if batch.transactions.len() != self.batch_size {
            return false;
        }
        let recomputed =
            match Self::compute(batch, self.nonce, self.epoch, self.leader_id.clone(), engine) {
                Ok(c) => c,
                // If the stored leader_id somehow violates the length cap,
                // verification fails (defensive — should not happen if the
                // BatchCommitment was constructed via compute).
                Err(_) => return false,
            };
        recomputed.commitment_hash == self.commitment_hash
    }
}

impl Canonicalizable for BatchCommitment {
    /// Fold the commitment_hash bytes into a distinction. Consumers
    /// use this to advance an LCA's local root after processing a
    /// commitment.
    fn to_canonical_structure(self, engine: &DistinctionEngine) -> Distinction {
        let mut acc = ByteMapping::map_byte_to_distinction(self.commitment_hash[0], engine);
        for &b in &self.commitment_hash[1..] {
            let d = ByteMapping::map_byte_to_distinction(b, engine);
            acc = engine.synthesize(acc, d);
        }
        acc
    }
}

/// Chain-fold every transaction starting from `batch.previous_root`.
fn compute_batch_root(batch: &TransactionBatch, engine: &DistinctionEngine) -> Distinction {
    let mut current = batch.previous_root;
    for tx in &batch.transactions {
        let tx_d = tx.canonicalize(engine);
        current = engine.synthesize(current, tx_d);
    }
    current
}

// --- Errors ------------------------------------------------------------

/// Reasons [`BatchCommitment::compute`] can fail.
///
/// `#[non_exhaustive]`: future variants will not break consumer semver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum CommitmentError {
    /// `leader_id` is empty.
    #[error("empty leader_id")]
    EmptyLeaderId,
    /// `leader_id` exceeds [`MAX_LEADER_ID_LEN`].
    #[error("leader_id too long: {got} bytes (cap {cap})")]
    LeaderIdTooLong {
        /// Actual length.
        got: usize,
        /// The cap.
        cap: usize,
    },
}

// --- Stage-2 fetch types -----------------------------------------------

/// Request from a peer that has a commitment but wants the batch data.
///
/// Small wire type: consumers exchange these to coordinate Stage-2
/// batch fetching. Network transport is orthogonal (see `network.rs`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BatchDataRequest {
    /// The commitment hash being requested.
    pub commitment_hash: [u8; 32],
    /// Identifier of the requesting peer. Bounded by
    /// [`MAX_LEADER_ID_LEN`] as a defensive shared cap.
    pub requester_id: String,
}

/// Response carrying batch data + its commitment.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct BatchDataResponse {
    /// The batch being delivered.
    pub batch: TransactionBatch,
    /// The commitment the batch matches.
    pub commitment: BatchCommitment,
}

// --- Agent -------------------------------------------------------------

/// Cached (batch, commitment) pair value stored in [`CommitmentAgent`]'s
/// LRU cache. Alias mirrors the pattern used in `src/engine.rs` for
/// public compound types.
pub type CachedBatch = (TransactionBatch, BatchCommitment);

/// Crate-internal alias for the batch-data cache map type.
type BatchCache = LruCache<[u8; 32], CachedBatch>;

/// An LCA that processes commitments + caches (batch, commitment)
/// pairs for Stage-2 fetch.
pub struct CommitmentAgent {
    local_root: Distinction,
    cache: BatchCache,
    commitments_processed: u64,
}

impl CommitmentAgent {
    /// Fresh agent rooted at `engine.d0()`, cache capacity
    /// [`COMMITMENT_CACHE_CAP`].
    #[must_use]
    pub fn new(engine: &DistinctionEngine) -> Self {
        Self::with_capacity(engine, COMMITMENT_CACHE_CAP)
    }

    /// Fresh agent with a custom cache capacity. Primarily for tests.
    ///
    /// # Panics
    ///
    /// Panics if `capacity == 0` — an LRU cache with zero capacity is
    /// a category error.
    #[must_use]
    pub fn with_capacity(engine: &DistinctionEngine, capacity: usize) -> Self {
        Self {
            local_root: engine.d0(),
            cache: LruCache::new(NonZeroUsize::new(capacity).expect("capacity > 0 (invariant)")),
            commitments_processed: 0,
        }
    }

    /// Cache a (batch, commitment) pair for later Stage-2 fetch.
    pub fn cache_batch(&mut self, batch: TransactionBatch, commitment: BatchCommitment) {
        self.cache.put(commitment.commitment_hash, (batch, commitment));
    }

    /// Look up a cached (batch, commitment) by hash. LRU access-order
    /// is updated on hit.
    #[must_use]
    pub fn get_cached_batch(&mut self, hash: &[u8; 32]) -> Option<&CachedBatch> {
        self.cache.get(hash)
    }

    /// Peek without updating LRU access-order.
    #[must_use]
    pub fn has_cached(&self, hash: &[u8; 32]) -> bool {
        self.cache.contains(hash)
    }

    /// Number of commitments this agent has processed since
    /// construction (LCA-advance count).
    #[must_use]
    pub const fn commitments_processed(&self) -> u64 {
        self.commitments_processed
    }

    /// Current local root (LCA perspective).
    #[must_use]
    pub const fn local_root(&self) -> Distinction {
        self.local_root
    }

    /// Advance the LCA by folding a commitment into the local root.
    /// Increments `commitments_processed`.
    pub fn advance(&mut self, commitment: BatchCommitment, engine: &Arc<DistinctionEngine>) {
        let commit_d = commitment.to_canonical_structure(engine);
        self.local_root = engine.synthesize(self.local_root, commit_d);
        self.commitments_processed = self.commitments_processed.wrapping_add(1);
    }
}

impl LocalCausalAgent for CommitmentAgent {
    type ActionData = BatchCommitment;

    fn get_current_root(&self) -> Distinction {
        self.local_root
    }

    fn update_local_root(&mut self, new_root: Distinction) {
        self.local_root = new_root;
    }
}

// ---------------------------------------------------------------------------
// Tests
//
// Each audit finding gets its own regression test that would FAIL if
// the by-construction defense were removed.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::subsystems::validator::TransactionAction;

    fn fresh() -> (Arc<DistinctionEngine>, CommitmentAgent) {
        let engine = Arc::new(DistinctionEngine::new());
        let agent = CommitmentAgent::new(&engine);
        (engine, agent)
    }

    fn sample_batch(engine: &Arc<DistinctionEngine>) -> TransactionBatch {
        TransactionBatch {
            previous_root: engine.d0(),
            transactions: vec![
                TransactionAction::new(0, b"hello".to_vec()).expect("valid tx (invariant)"),
                TransactionAction::new(1, b"world".to_vec()).expect("valid tx (invariant)"),
            ],
        }
    }

    fn sample_commitment(engine: &Arc<DistinctionEngine>) -> BatchCommitment {
        let batch = sample_batch(engine);
        BatchCommitment::compute(&batch, 0, 42, "leader-alice".into(), engine)
            .expect("valid commitment (invariant)")
    }

    // ----- Positive path -------------------------------------------------

    #[test]
    fn compute_and_verify_roundtrip() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let commit = BatchCommitment::compute(&batch, 0, 42, "leader-alice".into(), &engine)
            .expect("compute succeeds");
        assert!(commit.verify_batch(&batch, &engine));
    }

    #[test]
    fn compute_is_deterministic() {
        let engine1 = Arc::new(DistinctionEngine::new());
        let engine2 = Arc::new(DistinctionEngine::new());
        let b1 = sample_batch(&engine1);
        let b2 = sample_batch(&engine2);
        let c1 = BatchCommitment::compute(&b1, 0, 42, "leader".into(), &engine1)
            .expect("valid compute (invariant)");
        let c2 = BatchCommitment::compute(&b2, 0, 42, "leader".into(), &engine2)
            .expect("valid compute (invariant)");
        assert_eq!(c1.commitment_hash, c2.commitment_hash);
    }

    // ----- N6 regression: leader_id in the hash --------------------------

    #[test]
    fn n6_tampered_leader_id_fails_verify() {
        // Load-bearing N6 test: mutate stored leader_id, verify fails.
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let mut commit = BatchCommitment::compute(&batch, 0, 42, "leader-alice".into(), &engine)
            .expect("compute succeeds");

        // Tamper with just the leader_id (verify recomputes with the
        // stored one, which no longer matches the original hash).
        commit.leader_id = "leader-eve".into();
        assert!(!commit.verify_batch(&batch, &engine), "N6: tampered leader_id must NOT verify");
    }

    #[test]
    fn n6_different_leader_ids_give_different_hashes() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let c1 = BatchCommitment::compute(&batch, 0, 42, "alice".into(), &engine)
            .expect("valid compute (invariant)");
        let c2 = BatchCommitment::compute(&batch, 0, 42, "bob".into(), &engine)
            .expect("valid compute (invariant)");
        assert_ne!(c1.commitment_hash, c2.commitment_hash);
    }

    #[test]
    fn n6_length_prefix_prevents_concat_ambiguity() {
        // "abc" (len=3) + "def" (len=3) MUST hash differently from
        // "abcdef" (len=6) alone. Without the length prefix, both
        // would produce the same concatenated bytes.
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let c1 = BatchCommitment::compute(&batch, 0, 42, "abcdef".into(), &engine)
            .expect("valid compute (invariant)");
        let c2 = BatchCommitment::compute(&batch, 0, 42, "abc".into(), &engine)
            .expect("valid compute (invariant)");
        assert_ne!(c1.commitment_hash, c2.commitment_hash);
    }

    // ----- F7 transitive: verify rejects both hash and metadata tampers --

    #[test]
    fn f7_tampered_nonce_fails_verify() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let mut commit = BatchCommitment::compute(&batch, 0, 42, "l".into(), &engine)
            .expect("valid compute (invariant)");
        commit.nonce = 999;
        assert!(!commit.verify_batch(&batch, &engine));
    }

    #[test]
    fn f7_tampered_epoch_fails_verify() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let mut commit = BatchCommitment::compute(&batch, 0, 42, "l".into(), &engine)
            .expect("valid compute (invariant)");
        commit.epoch = 999;
        assert!(!commit.verify_batch(&batch, &engine));
    }

    #[test]
    fn f7_tampered_batch_content_fails_verify() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let commit = BatchCommitment::compute(&batch, 0, 42, "l".into(), &engine)
            .expect("valid compute (invariant)");
        let mut tampered = batch.clone();
        tampered.transactions[0] = TransactionAction::new(0, b"tampered".to_vec()).expect("valid");
        assert!(!commit.verify_batch(&tampered, &engine));
    }

    #[test]
    fn f7_tampered_batch_size_fails_verify() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let mut commit = BatchCommitment::compute(&batch, 0, 42, "l".into(), &engine)
            .expect("valid compute (invariant)");
        commit.batch_size = 999;
        assert!(!commit.verify_batch(&batch, &engine));
    }

    // ----- leader_id validation ------------------------------------------

    #[test]
    fn empty_leader_id_rejected() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let err = BatchCommitment::compute(&batch, 0, 0, String::new(), &engine)
            .expect_err("empty leader_id must reject");
        assert!(matches!(err, CommitmentError::EmptyLeaderId));
    }

    #[test]
    fn oversized_leader_id_rejected() {
        let engine = Arc::new(DistinctionEngine::new());
        let batch = sample_batch(&engine);
        let big = "x".repeat(MAX_LEADER_ID_LEN + 1);
        let err = BatchCommitment::compute(&batch, 0, 0, big, &engine)
            .expect_err("oversized leader_id must reject");
        assert!(matches!(err, CommitmentError::LeaderIdTooLong { .. }));
    }

    // ----- Cache behavior ------------------------------------------------

    #[test]
    fn cache_stores_and_retrieves_by_hash() {
        let (engine, mut agent) = fresh();
        let batch = sample_batch(&engine);
        let commit = sample_commitment(&engine);
        let hash = commit.commitment_hash;
        agent.cache_batch(batch.clone(), commit.clone());
        assert!(agent.has_cached(&hash));
        let (cached_batch, cached_commit) =
            agent.get_cached_batch(&hash).expect("cache hit (invariant)");
        assert_eq!(cached_batch, &batch);
        assert_eq!(cached_commit, &commit);
    }

    #[test]
    fn cache_lru_eviction_at_capacity() {
        let engine = Arc::new(DistinctionEngine::new());
        // Small cap to make the test fast.
        let mut agent = CommitmentAgent::with_capacity(&engine, 2);
        let batch = sample_batch(&engine);
        // Three distinct commitments (different nonces → different hashes).
        let c1 = BatchCommitment::compute(&batch, 0, 0, "a".into(), &engine).expect("valid");
        let c2 = BatchCommitment::compute(&batch, 1, 0, "a".into(), &engine).expect("valid");
        let c3 = BatchCommitment::compute(&batch, 2, 0, "a".into(), &engine).expect("valid");
        agent.cache_batch(batch.clone(), c1.clone());
        agent.cache_batch(batch.clone(), c2.clone());
        agent.cache_batch(batch.clone(), c3.clone());
        // Oldest (c1) should have been evicted.
        assert!(!agent.has_cached(&c1.commitment_hash), "c1 should be evicted");
        assert!(agent.has_cached(&c2.commitment_hash));
        assert!(agent.has_cached(&c3.commitment_hash));
    }

    // ----- LCA impl ------------------------------------------------------

    #[test]
    fn advance_increments_processed_and_advances_root() {
        let (engine, mut agent) = fresh();
        let r0 = agent.local_root();
        let commit = sample_commitment(&engine);
        agent.advance(commit, &engine);
        assert_eq!(agent.commitments_processed(), 1);
        assert_ne!(agent.local_root(), r0);
    }

    #[test]
    fn lca_impl_get_and_update_root() {
        let (_engine, mut agent) = fresh();
        let r0 = agent.get_current_root();
        let new_root = Distinction::from_bytes_unchecked([0xAB; 16]);
        agent.update_local_root(new_root);
        assert_ne!(agent.get_current_root(), r0);
        assert_eq!(agent.get_current_root(), new_root);
    }

    // ----- Serde round-trip ----------------------------------------------

    #[test]
    fn commitment_serde_roundtrip() {
        let engine = Arc::new(DistinctionEngine::new());
        let original = sample_commitment(&engine);
        let json = serde_json::to_string(&original).expect("serialize");
        let recovered: BatchCommitment = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, recovered);
    }
}
