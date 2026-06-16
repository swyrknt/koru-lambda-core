/// Consensus Validator
///
/// Implements the Structural Proof-of-Causality (SPoC) validation logic.
/// Validates transaction batches using atomic failure semantics:
/// if any transaction in a batch fails, the entire batch is rejected.
///
use crate::primitives::Canonicalizable;
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::{Distinction, DistinctionEngine};
use std::sync::Arc;

/// Maximum byte length accepted for `TransactionAction.data`.
///
/// V3 cap (CHECKLIST 1.6 / Phase 6 sub-branch #7). Phase 1.5 probe
/// `exp_validator_audit` Section B demonstrated that a 100 KB `data`
/// field synthesized 100,002 permanent distinctions in ~149 ms per tx.
/// Sized to comfortably hold typical signed-payload txs (ed25519 sig
/// plus small payload ≈ 100–500 B); larger payloads must be referenced
/// by content hash rather than embedded.
pub const MAX_TX_DATA_BYTES: usize = 4096;

/// Maximum prefix length of `previous_root` echoed in rejection
/// messages.
///
/// V4 cap (CHECKLIST 1.6 / Phase 6 sub-branch #7). v1.2.0 echoed the
/// full untrusted `previous_root` into the rejection reason string,
/// amplifying a 1 MB attacker-supplied root into a 1 MB error payload.
/// 64 chars is two full distinction hex IDs — enough for diagnostics
/// without enabling amplification.
const PREVIOUS_ROOT_DISPLAY_PREFIX: usize = 64;

/// Represents a transaction action in the system
///
/// Transactions are canonicalized into distinctions for structural validation.
/// The nonce ensures causal ordering, prevents replay attacks.
///
/// # Content addressing of empty-data txs (V8, by-design)
///
/// Two `TransactionAction { nonce: n, data: vec![] }` values with the
/// same `nonce` produce the same `to_canonical_structure` output by
/// content addressing — `data.iter().fold(d0, ...)` returns `d0` for
/// an empty iterator, and equal `(nonce, data)` inputs MUST produce
/// equal distinctions (this is the irreflexivity / determinism axiom
/// of the substrate, not a bug). Consumers must therefore not rely on
/// tx-distinction uniqueness for txs that share `(nonce, data)`;
/// distinguish such txs via signature or sender id in `data` itself.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionAction {
    /// Sequential number ensuring causal ordering
    pub nonce: u64,
    /// Arbitrary transaction data (will be canonicalized).
    /// Capped at `MAX_TX_DATA_BYTES`; oversized txs are rejected.
    pub data: Vec<u8>,
}

impl Canonicalizable for TransactionAction {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        // Canonicalize nonce
        let nonce_bytes = self.nonce.to_le_bytes();
        let nonce_distinction = nonce_bytes.iter().fold(engine.d0().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

        // Canonicalize data
        let data_distinction = self.data.iter().fold(engine.d0().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

        // Synthesize transaction: nonce ⊕ data
        engine.synthesize(&nonce_distinction, &data_distinction)
    }
}

/// Batch of transactions for atomic validation
///
/// The batch is the fundamental unit of consensus - it represents a single
/// causal event in the system's evolution.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionBatch {
    /// Transactions in canonical order
    pub transactions: Vec<TransactionAction>,
    /// Reference to previous state root (ensures causal chain)
    pub previous_root: String,
}

/// Result of batch validation
#[derive(Debug, Clone, PartialEq)]
pub enum BatchValidationResult {
    /// Batch valid - new state root
    Valid(Distinction),
    /// Batch rejected - reason for failure
    Rejected(String),
}

/// Consensus Validator Implementation
///
/// Tracks local state root and validates transaction batches atomically.
/// Implements LocalCausalAgent to enforce the causal synthesis pattern.
pub struct ConsensusValidator {
    /// Current local state root
    local_root: Distinction,
    /// Expected nonce for next transaction (prevents replay)
    expected_nonce: u64,
}

impl ConsensusValidator {
    /// Create a new validator anchored at genesis
    ///
    /// Concurrency: Takes Arc for thread-safe shared access.
    pub fn new(engine: &Arc<DistinctionEngine>) -> Self {
        // Genesis state: d0 ⊕ d1 (primordial synthesis)
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        Self { local_root: genesis, expected_nonce: 0 }
    }

    /// Restore a validator anchored at a previously-synthesized state.
    ///
    /// V6 (CHECKLIST 1.6 / Phase 6 sub-branch #7). Replaces the v1.2.0
    /// pair `from_root(root) + set_expected_nonce(nonce)` that allowed
    /// the partial-update window: a caller could set
    /// `expected_nonce = 100` on a genesis-rooted validator and submit
    /// a nonce-100 batch that the validator would accept (Phase 1.5
    /// probe `exp_validator_audit` Section G).
    ///
    /// This constructor:
    ///
    /// 1. Sets `local_root` and `expected_nonce` in a single call —
    ///    there is no longer a public API where they can be set
    ///    independently.
    /// 2. Verifies `root_id` is a real distinction registered in the
    ///    supplied engine. Fabricated roots (well-formed bytes that
    ///    were never synthesized) are rejected. This closes the
    ///    "set arbitrary root" half of the foreign-ID attack class at
    ///    the validator boundary, complementing the `pub(crate)`
    ///    constructor that closes it at the type level.
    ///
    /// Note on (root, nonce) consistency: this constructor does NOT
    /// reconstruct the chain to verify that `expected_nonce` is the
    /// nonce that follows `root_id` in some canonical history (the
    /// substrate stores no nonce-to-root mapping). Consumers must
    /// supply consistent values; this API merely refuses fabricated
    /// roots and prevents the partial-update window.
    pub fn restore_state(
        engine: &Arc<DistinctionEngine>,
        root_id: Distinction,
        expected_nonce: u64,
    ) -> Result<Self, String> {
        if engine.degree(&root_id) == 0 {
            return Err(format!(
                "restore_state: root_id {} is not registered in the supplied engine",
                root_id.to_hex()
            ));
        }
        Ok(Self { local_root: root_id, expected_nonce })
    }

    /// Validate a batch of transactions atomically.
    ///
    /// Atomic Failure Semantics: if any transaction fails, the entire
    /// batch is rejected. **The engine is also left unchanged on
    /// rejection** — this validator pre-validates the batch shape
    /// (previous_root linkage + nonce contiguity) BEFORE any
    /// `engine.synthesize` call, so rejected batches never leak
    /// distinctions into the engine.
    ///
    /// # Security (V5 / engine-leak closure)
    ///
    /// Prior to the fix in CHECKLIST 1.5 / Phase 6 sub-branch #6, the
    /// loop here synthesized per-tx state mid-iteration and only
    /// rolled back the validator's `local_root`/`expected_nonce` fields
    /// on rejection. The engine kept the partial-prefix syntheses.
    /// Phase 1.5 probe `exp_validator_audit` Section C demonstrated 4
    /// distinctions leaking into the engine on a 3-tx out-of-order
    /// rejection. After the fix, the engine distinction count delta on
    /// any rejected batch is exactly 0.
    ///
    /// Validation steps:
    /// 1. Verify batch references correct previous root (read-only).
    /// 2. Verify batch is non-empty (read-only).
    /// 3. Pre-validate every nonce is contiguous starting at
    ///    `self.expected_nonce` (read-only).
    /// 4. Only then walk the batch and call `engine.synthesize` per tx.
    ///    Steps 1–3 guarantee step 4 never fails partway through.
    ///
    /// Concurrency: Thread-safe via `Arc<DistinctionEngine>`.
    pub fn validate_batch(
        &mut self,
        batch: TransactionBatch,
        engine: &Arc<DistinctionEngine>,
    ) -> BatchValidationResult {
        // ===== Pre-validation pass (read-only; no engine mutation) =====

        // 1. Verify causal chain: batch must reference current root.
        //    V4: clip echoed previous_root to bounded prefix so a 1 MB
        //    attacker-supplied root does not amplify the error payload.
        if batch.previous_root != self.local_root.to_hex() {
            let supplied = if batch.previous_root.len() > PREVIOUS_ROOT_DISPLAY_PREFIX {
                format!("{}…(truncated)", &batch.previous_root[..PREVIOUS_ROOT_DISPLAY_PREFIX])
            } else {
                batch.previous_root.clone()
            };
            return BatchValidationResult::Rejected(format!(
                "Invalid previous root: expected {}, got {}",
                self.local_root.to_hex(),
                supplied
            ));
        }

        // 2. Validate batch is non-empty.
        if batch.transactions.is_empty() {
            return BatchValidationResult::Rejected("Empty batch".to_string());
        }

        // 3. Pre-validate the full nonce sequence (V5 fix) AND the
        //    per-tx data length cap (V3 fix). Walking once here means a
        //    malformed nonce or oversized payload in tx N+1 is detected
        //    BEFORE any of txs 0..=N synthesize into the engine.
        let mut expected = self.expected_nonce;
        for (idx, tx) in batch.transactions.iter().enumerate() {
            if tx.nonce != expected {
                return BatchValidationResult::Rejected(format!(
                    "Invalid nonce at tx {}: expected {}, got {}",
                    idx, expected, tx.nonce
                ));
            }
            if tx.data.len() > MAX_TX_DATA_BYTES {
                return BatchValidationResult::Rejected(format!(
                    "tx {} data length {} exceeds MAX_TX_DATA_BYTES ({})",
                    idx,
                    tx.data.len(),
                    MAX_TX_DATA_BYTES
                ));
            }
            expected += 1;
        }

        // ===== Commit pass (only reached if pre-validation passed) =====
        //
        // Every iteration here calls engine.synthesize, but pre-validation
        // has proven none will be rejected, so engine mutations are
        // committed regardless. The engine is append-only by design;
        // these synthesized distinctions are part of the canonical chain.
        let mut current_state = self.local_root.clone();
        for tx in batch.transactions.iter() {
            let tx_distinction = tx.to_canonical_structure(engine);
            current_state = engine.synthesize(&current_state, &tx_distinction);
        }

        // All transactions valid — update local state.
        self.local_root = current_state.clone();
        self.expected_nonce = expected;

        BatchValidationResult::Valid(current_state)
    }

    /// Get current expected nonce
    pub fn expected_nonce(&self) -> u64 {
        self.expected_nonce
    }

    /// Get current state root ID as a 32-character hex string.
    pub fn state_root_id(&self) -> String {
        self.local_root.to_hex()
    }
}

impl LocalCausalAgent for ConsensusValidator {
    type ActionData = TransactionAction;

    fn get_current_root(&self) -> &Distinction {
        &self.local_root
    }

    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        // Verify nonce
        if action_data.nonce != self.expected_nonce {
            // Invalid nonce - return current root unchanged
            return self.local_root.clone();
        }

        // Use helper to perform causal synthesis
        let new_root = crate::subsystems::local_agent::synthesize_causal_action(
            &self.local_root,
            action_data,
            engine,
        );

        // Update state
        self.expected_nonce += 1;
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
    fn test_validator_genesis() {
        let engine = Arc::new(DistinctionEngine::new());
        let validator = ConsensusValidator::new(&engine);

        assert_eq!(validator.expected_nonce(), 0);
        assert!(!validator.state_root_id().is_empty());
    }

    #[test]
    fn test_single_transaction_batch() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let genesis_root = validator.state_root_id().to_string();

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: genesis_root.clone(),
        };

        let result = validator.validate_batch(batch, &engine);

        match result {
            BatchValidationResult::Valid(new_root) => {
                // New root should be different from genesis
                assert_ne!(new_root.to_hex(), genesis_root);
                // Validator state should match new root
                assert_eq!(new_root.to_hex(), validator.state_root_id());
                assert_eq!(validator.expected_nonce(), 1);
            },
            BatchValidationResult::Rejected(reason) => {
                panic!("Batch should be valid: {}", reason);
            },
        }
    }

    #[test]
    fn test_atomic_failure_invalid_nonce() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let genesis_root = validator.state_root_id().to_string();

        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction { nonce: 0, data: vec![1, 2, 3] },
                TransactionAction {
                    nonce: 2, // Invalid - should be 1
                    data: vec![4, 5, 6],
                },
            ],
            previous_root: genesis_root,
        };

        let result = validator.validate_batch(batch, &engine);

        match result {
            BatchValidationResult::Valid(_) => {
                panic!("Batch should be rejected due to invalid nonce");
            },
            BatchValidationResult::Rejected(reason) => {
                assert!(reason.contains("Invalid nonce"));
                // State should be unchanged
                assert_eq!(validator.expected_nonce(), 0);
            },
        }
    }

    #[test]
    fn test_atomic_failure_invalid_previous_root() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: "invalid_root".to_string(),
        };

        let result = validator.validate_batch(batch, &engine);

        match result {
            BatchValidationResult::Valid(_) => {
                panic!("Batch should be rejected due to invalid previous root");
            },
            BatchValidationResult::Rejected(reason) => {
                assert!(reason.contains("Invalid previous root"));
                assert_eq!(validator.expected_nonce(), 0);
            },
        }
    }

    #[test]
    fn test_sequential_batches() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        // First batch
        let batch1 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: validator.state_root_id().to_string(),
        };

        let result1 = validator.validate_batch(batch1, &engine);
        assert!(matches!(result1, BatchValidationResult::Valid(_)));

        // Second batch - should reference new root
        let batch2 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 1, data: vec![4, 5, 6] }],
            previous_root: validator.state_root_id().to_string(),
        };

        let result2 = validator.validate_batch(batch2, &engine);
        assert!(matches!(result2, BatchValidationResult::Valid(_)));

        assert_eq!(validator.expected_nonce(), 2);
    }

    #[test]
    fn restore_state_round_trips_root_and_nonce() {
        // V6: replaces the v1.2.0 test_set_expected_nonce. The atomic
        // restore_state API is the only public way to put a validator
        // into a non-genesis configuration; the partial-update window
        // (set_expected_nonce against a stale root) no longer exists.
        let engine = Arc::new(DistinctionEngine::new());
        let mut original = ConsensusValidator::new(&engine);

        // Advance original to nonce 1 by validating a real batch so
        // its local_root becomes a synthesized (registered) distinction.
        let batch0 = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: original.state_root_id(),
        };
        let result0 = original.validate_batch(batch0, &engine);
        let advanced_root = match result0 {
            BatchValidationResult::Valid(d) => d,
            BatchValidationResult::Rejected(r) => panic!("setup batch must validate: {}", r),
        };
        assert_eq!(original.expected_nonce(), 1);

        // restore_state on the advanced root + nonce reconstructs the
        // validator atomically.
        let restored = ConsensusValidator::restore_state(&engine, advanced_root.clone(), 1)
            .expect("registered root must be accepted");
        assert_eq!(restored.expected_nonce(), 1);
        assert_eq!(restored.state_root_id(), advanced_root.to_hex());
    }

    #[test]
    fn restore_state_rejects_fabricated_root() {
        // V6: a well-formed-bytes root that was never synthesized in
        // the engine must be refused. Closes the partial-update
        // window's "set arbitrary root" half.
        let engine = Arc::new(DistinctionEngine::new());

        // Parse a hex string the engine has never synthesized.
        let fabricated = Distinction::from_hex(&"f".repeat(32))
            .expect("32 hex chars parse to a Distinction");
        assert_eq!(
            engine.degree(&fabricated),
            0,
            "fabricated root must not be registered (test precondition)"
        );

        let result = ConsensusValidator::restore_state(&engine, fabricated, 0);
        assert!(result.is_err(), "fabricated root must be rejected");
    }

    // === V5 regression tests (Phase 6 sub-branch #6) ===

    #[test]
    fn validate_batch_rejection_leaves_engine_state_unchanged_for_bad_nonce() {
        // V5: out-of-order batch (nonce 0, 2, 1) must reject without
        // mutating engine state. Phase 1.5 probe `exp_validator_audit`
        // Section C demonstrated 4 distinctions leaking into the engine
        // in v1.2.0.
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let dist_before = engine.distinction_count();
        let rel_before = engine.relationship_count();

        let batch = TransactionBatch {
            transactions: vec![
                TransactionAction { nonce: 0, data: vec![1, 2, 3] },
                TransactionAction { nonce: 2, data: vec![4, 5, 6] },
                TransactionAction { nonce: 1, data: vec![7, 8, 9] },
            ],
            previous_root: validator.state_root_id(),
        };

        let result = validator.validate_batch(batch, &engine);
        assert!(matches!(result, BatchValidationResult::Rejected(_)));

        assert_eq!(
            engine.distinction_count(),
            dist_before,
            "rejection must not leak distinctions (V5)"
        );
        assert_eq!(
            engine.relationship_count(),
            rel_before,
            "rejection must not leak relationships (V5)"
        );
        assert_eq!(validator.expected_nonce(), 0);
    }

    #[test]
    fn validate_batch_rejection_leaves_engine_state_unchanged_for_bad_previous_root() {
        // V5 corollary: previous_root mismatch must also not synthesize.
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let dist_before = engine.distinction_count();
        let rel_before = engine.relationship_count();

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1, 2, 3] }],
            previous_root: "00000000000000000000000000000000".to_string(),
        };

        let result = validator.validate_batch(batch, &engine);
        assert!(matches!(result, BatchValidationResult::Rejected(_)));

        assert_eq!(engine.distinction_count(), dist_before);
        assert_eq!(engine.relationship_count(), rel_before);
        assert_eq!(validator.expected_nonce(), 0);
    }

    #[test]
    fn validate_batch_first_tx_bad_nonce_also_does_not_leak() {
        // Boundary case: even when the failing tx is the first one,
        // engine state must be unchanged.
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let dist_before = engine.distinction_count();

        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 5, data: vec![1] }], // expected 0
            previous_root: validator.state_root_id(),
        };

        let result = validator.validate_batch(batch, &engine);
        assert!(matches!(result, BatchValidationResult::Rejected(_)));
        assert_eq!(engine.distinction_count(), dist_before);
    }

    // === V3 / V4 regression tests (Phase 6 sub-branch #7) ===

    #[test]
    fn validate_batch_rejects_oversized_data_without_leaking() {
        // V3: a tx whose data exceeds MAX_TX_DATA_BYTES must be rejected
        // during pre-validation and must not synthesize anything into
        // the engine. Phase 1.5 probe `exp_validator_audit` Section B
        // demonstrated 100,002 distinctions per 100 KB tx in v1.2.0.
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let dist_before = engine.distinction_count();

        let oversized = vec![0u8; MAX_TX_DATA_BYTES + 1];
        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: oversized }],
            previous_root: validator.state_root_id(),
        };

        let result = validator.validate_batch(batch, &engine);
        match result {
            BatchValidationResult::Rejected(reason) => {
                assert!(
                    reason.contains("exceeds MAX_TX_DATA_BYTES"),
                    "unexpected rejection reason: {}",
                    reason
                );
            },
            other => panic!("expected Rejected, got {:?}", other),
        }
        assert_eq!(
            engine.distinction_count(),
            dist_before,
            "oversized data rejection must not synthesize (V5 invariant holds)"
        );
        assert_eq!(validator.expected_nonce(), 0);
    }

    #[test]
    fn validate_batch_accepts_data_at_cap_boundary() {
        // V3 boundary: data length exactly MAX_TX_DATA_BYTES must be
        // accepted. The cap is `len() > MAX`, not `>=`.
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let at_cap = vec![0u8; MAX_TX_DATA_BYTES];
        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: at_cap }],
            previous_root: validator.state_root_id(),
        };

        let result = validator.validate_batch(batch, &engine);
        assert!(matches!(result, BatchValidationResult::Valid(_)));
        assert_eq!(validator.expected_nonce(), 1);
    }

    #[test]
    fn validate_batch_clips_oversized_previous_root_in_error() {
        // V4: a 1 MB attacker-supplied previous_root must not be
        // copied verbatim into the rejection reason. The error message
        // length is bounded by the display prefix + fixed template.
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        let huge_root = "a".repeat(1_000_000);
        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 0, data: vec![1] }],
            previous_root: huge_root,
        };

        let result = validator.validate_batch(batch, &engine);
        match result {
            BatchValidationResult::Rejected(reason) => {
                // Generous upper bound; the actual message is well under 256 bytes.
                assert!(
                    reason.len() < 1024,
                    "rejection reason should be bounded; was {} bytes",
                    reason.len()
                );
                assert!(reason.contains("truncated"));
            },
            other => panic!("expected Rejected, got {:?}", other),
        }
    }
}
