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

/// Represents a transaction action in the system
///
/// Transactions are canonicalized into distinctions for structural validation.
/// The nonce ensures causal ordering, Prevents replay attacks.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TransactionAction {
    /// Sequential number ensuring causal ordering
    pub nonce: u64,
    /// Arbitrary transaction data (will be canonicalized)
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

    /// Create validator from existing state root
    pub fn from_root(root: Distinction, expected_nonce: u64) -> Self {
        Self { local_root: root, expected_nonce }
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
        if batch.previous_root != self.local_root.to_hex() {
            return BatchValidationResult::Rejected(format!(
                "Invalid previous root: expected {}, got {}",
                self.local_root.to_hex(),
                batch.previous_root
            ));
        }

        // 2. Validate batch is non-empty.
        if batch.transactions.is_empty() {
            return BatchValidationResult::Rejected("Empty batch".to_string());
        }

        // 3. Pre-validate the full nonce sequence (V5 fix). Walking once
        // here means a malformed nonce in tx N+1 is detected BEFORE any
        // of txs 0..=N synthesize into the engine.
        let mut expected = self.expected_nonce;
        for (idx, tx) in batch.transactions.iter().enumerate() {
            if tx.nonce != expected {
                return BatchValidationResult::Rejected(format!(
                    "Invalid nonce at tx {}: expected {}, got {}",
                    idx, expected, tx.nonce
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

    /// Set current expected nonce (used for state restoration/import).
    /// WARNING: Use only during initialization/import.
    pub fn set_expected_nonce(&mut self, nonce: u64) {
        self.expected_nonce = nonce;
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
    fn test_set_expected_nonce() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut validator = ConsensusValidator::new(&engine);

        // Initial nonce should be 0
        assert_eq!(validator.expected_nonce(), 0);

        // Set nonce to 42 (simulating state restoration)
        validator.set_expected_nonce(42);
        assert_eq!(validator.expected_nonce(), 42);

        // Now a batch with nonce 42 should be valid
        let batch = TransactionBatch {
            transactions: vec![TransactionAction { nonce: 42, data: vec![1, 2, 3] }],
            previous_root: validator.state_root_id().to_string(),
        };

        let result = validator.validate_batch(batch, &engine);
        assert!(matches!(result, BatchValidationResult::Valid(_)));
        assert_eq!(validator.expected_nonce(), 43);
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
}
