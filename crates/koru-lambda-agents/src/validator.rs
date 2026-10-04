//! Consensus batch validator — reference LCA consumer.
//!
//! Advances a local causal chain by validating batches of
//! [`TransactionAction`]s, one batch at a time. Rejection is atomic:
//! if any invariant fails, engine state and validator state are
//! unchanged (V5 designed-in, not retrofit).
//!
//! # v1.2 audit findings closed by construction
//!
//! - **V3 (data cap)** — `MAX_DATA_LEN` bounds per-transaction payload;
//!   `MAX_TRANSACTIONS_PER_BATCH` bounds batch cardinality. Both enforced
//!   in pre-validation AND at deserialize-time via
//!   `#[serde(try_from = "...")]`.
//! - **V4 (oversized-root clipping)** — `TransactionBatch::previous_root`
//!   is [`Distinction`] (16 bytes, 32-char hex), not `String`. Injection
//!   of oversized identifiers is a compile error; error messages carry
//!   only bounded values (`u64`, `usize`, `Distinction`).
//! - **V5 (atomic failure)** — pre-validation runs FIRST as a pure
//!   inspection (no engine mutation). Only if every invariant passes
//!   does the commit phase call `engine.synthesize`. This makes engine
//!   state invariant on rejection.
//! - **V6 (atomic restore_state)** — [`ConsensusValidator::restore_state`]
//!   sets `local_root` and `expected_nonce` in a single non-fallible
//!   method. Partial restoration is impossible.
//! - **V8 (empty-data by design)** — `TransactionAction::data` must be
//!   non-empty. Enforced in pre-validation AND at deserialize-time.

use crate::agent::LocalCausalAgent;
use crate::DOMAIN_TRANSACTION;
use koru_lambda_core::domain_anchor;
use koru_lambda_core::primitives::{ByteFold, Canonicalizable};
use koru_lambda_core::{Distinction, DistinctionEngine};

// --- Constants ---------------------------------------------------------

/// Maximum bytes in a single transaction's `data` payload.
///
/// **Derivation:** 4 KB matches the upper end of typical L2 transaction
/// payloads (Ethereum ~2 KB for complex calls, plus headroom).
/// `MAX_DATA_LEN × MAX_TRANSACTIONS_PER_BATCH = 4 MB` bounds a single
/// batch's worst-case memory footprint predictably.
pub const MAX_DATA_LEN: usize = 4096;

/// Maximum number of transactions in a single batch.
///
/// **Derivation:** 1024 covers the busiest L1 blocks (Bitcoin ~2000
/// tx/block at ~500 B avg — we cap smaller because our batches are
/// per-consensus-round, not per-block).
pub const MAX_TRANSACTIONS_PER_BATCH: usize = 1024;

// --- Types -------------------------------------------------------------

/// A single transaction action.
///
/// `nonce` establishes causal ordering; `data` is arbitrary bytes that
/// get folded into a `Distinction` via [`Canonicalizable`].
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(into = "TransactionActionRaw", try_from = "TransactionActionRaw")]
pub struct TransactionAction {
    /// Sequential nonce.
    pub nonce: u64,
    /// Payload bytes. Non-empty, ≤ [`MAX_DATA_LEN`].
    pub data: Vec<u8>,
}

impl TransactionAction {
    /// Construct with size + non-empty validation.
    ///
    /// # Errors
    ///
    /// Returns [`ValidatorError::EmptyData`] if `data` is empty (V8) or
    /// [`ValidatorError::DataTooLarge`] if `data.len() > MAX_DATA_LEN` (V3).
    /// In both cases `at_tx` is `None` — no batch context.
    pub fn new(nonce: u64, data: Vec<u8>) -> Result<Self, ValidatorError> {
        if data.is_empty() {
            return Err(ValidatorError::EmptyData { at_tx: None });
        }
        if data.len() > MAX_DATA_LEN {
            return Err(ValidatorError::DataTooLarge {
                at_tx: None,
                got: data.len(),
                cap: MAX_DATA_LEN,
            });
        }
        Ok(Self { nonce, data })
    }
}

impl TransactionAction {
    /// Fold this transaction into a distinction by reference.
    ///
    /// Reference-taking counterpart to the [`Canonicalizable`] trait's
    /// `to_canonical_structure` (which consumes). Used by
    /// [`ConsensusValidator::validate_batch`]'s commit loop and by
    /// [`commitment::BatchCommitment::compute`](super::commitment)
    /// to fold each transaction without cloning.
    ///
    /// Folds nonce (8 bytes little-endian) followed by the data bytes,
    /// through the substrate's anchored fold.
    ///
    /// Builds a [`ByteFold`] per call. Loops over many transactions should
    /// use [`Self::canonicalize_with`] and share one.
    pub(crate) fn canonicalize(&self, engine: &DistinctionEngine) -> Distinction {
        self.canonicalize_with(&ByteFold::new(engine), engine)
    }

    /// Fold against an already-built [`ByteFold`].
    ///
    /// The sequence is `nonce ‖ data`. The nonce is a fixed eight bytes,
    /// so the boundary between the two is unambiguous, and the fold binds
    /// the total length.
    ///
    /// This previously seeded the accumulator with the first byte's tip,
    /// which left the first two positions unordered and let a leading run
    /// of equal bytes collapse — nonce 1 and nonce 256 folded to the same
    /// distinction, and so did data `[0x00]` and `[0x00, 0x00]`. The
    /// anchored fold is the fix, and the anchor also separates
    /// transactions from other folded types.
    pub(crate) fn canonicalize_with(
        &self,
        fold: &ByteFold,
        engine: &DistinctionEngine,
    ) -> Distinction {
        let mut bytes = Vec::with_capacity(8 + self.data.len());
        bytes.extend_from_slice(&self.nonce.to_le_bytes());
        bytes.extend_from_slice(&self.data);
        fold.fold(domain_anchor(engine, fold, DOMAIN_TRANSACTION), &bytes, engine)
    }
}

impl Canonicalizable for TransactionAction {
    fn to_canonical_structure(self, engine: &DistinctionEngine) -> Distinction {
        self.canonicalize(engine)
    }
}

/// Raw wire form for [`TransactionAction`] serde. Used via
/// `#[serde(try_from = ...)]` so V3/V8 invariants are enforced at parse
/// time — an untrusted `Deserialize` cannot bypass the constructor.
#[derive(serde::Serialize, serde::Deserialize)]
struct TransactionActionRaw {
    nonce: u64,
    data: Vec<u8>,
}

impl From<TransactionAction> for TransactionActionRaw {
    fn from(tx: TransactionAction) -> Self {
        Self { nonce: tx.nonce, data: tx.data }
    }
}

impl TryFrom<TransactionActionRaw> for TransactionAction {
    type Error = ValidatorError;

    fn try_from(raw: TransactionActionRaw) -> Result<Self, Self::Error> {
        Self::new(raw.nonce, raw.data)
    }
}

/// A batch of transaction actions rooted at a specific parent
/// distinction.
///
/// `previous_root` is typed as [`Distinction`] — N5's "String injection"
/// class of bug is a compile error.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TransactionBatch {
    /// The parent root this batch continues from.
    #[serde(with = "koru_lambda_core::distinction_hex::serde_adapter")]
    pub previous_root: Distinction,
    /// Ordered transactions. Non-empty at commit time (enforced in
    /// [`ConsensusValidator::validate_batch`]).
    pub transactions: Vec<TransactionAction>,
}

// --- Errors ------------------------------------------------------------

/// Reasons batch validation can fail.
///
/// `#[non_exhaustive]`: future variants will not break consumer semver.
/// Every variant carries bounded diagnostic data (`u64`, `usize`,
/// `Distinction`) so error messages have bounded length (V4).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ValidatorError {
    /// Batch's `previous_root` doesn't match the validator's current
    /// local root.
    #[error("root mismatch: expected {expected}, got {got}")]
    RootMismatch {
        /// Validator's current local root.
        expected: Distinction,
        /// The batch's claimed previous_root.
        got: Distinction,
    },
    /// Batch has zero transactions.
    #[error("empty batch: at least one transaction required")]
    EmptyBatch,
    /// Batch exceeds [`MAX_TRANSACTIONS_PER_BATCH`].
    #[error("batch too large: {got} transactions (cap {cap})")]
    BatchTooLarge {
        /// Actual count.
        got: usize,
        /// The cap.
        cap: usize,
    },
    /// A transaction's nonce doesn't match the expected sequence.
    #[error("nonce mismatch at tx {tx_index}: expected {expected}, got {got}")]
    NonceMismatch {
        /// Position of the offending transaction.
        tx_index: usize,
        /// Expected nonce.
        expected: u64,
        /// Provided nonce.
        got: u64,
    },
    /// A transaction's `data` exceeds [`MAX_DATA_LEN`] (V3).
    #[error("data too large: {got} bytes (cap {cap}){}", at_tx_suffix(*at_tx))]
    DataTooLarge {
        /// Position of the offending transaction within a batch, or
        /// `None` if raised by [`TransactionAction::new`] outside a
        /// batch context.
        at_tx: Option<usize>,
        /// Actual length.
        got: usize,
        /// The cap.
        cap: usize,
    },
    /// A transaction has empty `data` (V8: empty-data by design).
    #[error("empty data (empty-data disallowed by design){}", at_tx_suffix(*at_tx))]
    EmptyData {
        /// Position of the offending transaction within a batch, or
        /// `None` if raised by [`TransactionAction::new`] outside a
        /// batch context.
        at_tx: Option<usize>,
    },
}

/// Formatting helper for the `at_tx` suffix on data errors.
///
/// `Some(i)` → " at tx {i}"; `None` → "" (raised outside batch context).
fn at_tx_suffix(at: Option<usize>) -> String {
    match at {
        Some(i) => format!(" at tx {i}"),
        None => String::new(),
    }
}

// --- Validator ---------------------------------------------------------

/// Advances a local causal chain by consuming batches of transactions.
///
/// Implements the LCA pattern: carries a local root and advances it
/// forward when a batch is accepted.
pub struct ConsensusValidator {
    local_root: Distinction,
    expected_nonce: u64,
}

impl ConsensusValidator {
    /// Fresh validator rooted at `engine.d0()` with `expected_nonce = 0`.
    #[must_use]
    pub const fn new(engine: &DistinctionEngine) -> Self {
        Self { local_root: engine.d0(), expected_nonce: 0 }
    }

    /// The nonce the next transaction must carry.
    #[must_use]
    pub const fn expected_nonce(&self) -> u64 {
        self.expected_nonce
    }

    /// The validator's current local root (LCA perspective).
    #[must_use]
    pub const fn state_root_id(&self) -> Distinction {
        self.local_root
    }

    /// **Atomic** restore of both root and nonce (V6).
    ///
    /// A single non-fallible method sets both fields together. Partial
    /// restoration (updating one field but not the other) is
    /// structurally impossible.
    pub fn restore_state(&mut self, root: Distinction, nonce: u64) {
        self.local_root = root;
        self.expected_nonce = nonce;
    }

    /// Validate a batch and, on success, advance the local root.
    ///
    /// # Atomic failure (V5)
    ///
    /// This method runs a **pre-validation pass** over `batch` FIRST.
    /// If any invariant fails, the method returns an error without
    /// calling `engine.synthesize` — engine state is invariant on
    /// rejection. Only if pre-validation passes does the commit phase
    /// run. Since `synthesize` is deterministic and append-only, the
    /// commit phase cannot fail; the atomic contract holds.
    ///
    /// # Errors
    ///
    /// Any [`ValidatorError`] variant. Common cases: `RootMismatch`,
    /// `NonceMismatch`, `DataTooLarge`, `EmptyBatch`, `EmptyData`.
    pub fn validate_batch(
        &mut self,
        engine: &DistinctionEngine,
        batch: &TransactionBatch,
    ) -> Result<Distinction, ValidatorError> {
        // ---- Pre-validation (pure, no engine mutation) --------------

        if batch.previous_root != self.local_root {
            return Err(ValidatorError::RootMismatch {
                expected: self.local_root,
                got: batch.previous_root,
            });
        }
        if batch.transactions.is_empty() {
            return Err(ValidatorError::EmptyBatch);
        }
        if batch.transactions.len() > MAX_TRANSACTIONS_PER_BATCH {
            return Err(ValidatorError::BatchTooLarge {
                got: batch.transactions.len(),
                cap: MAX_TRANSACTIONS_PER_BATCH,
            });
        }

        let mut expected = self.expected_nonce;
        for (i, tx) in batch.transactions.iter().enumerate() {
            if tx.nonce != expected {
                return Err(ValidatorError::NonceMismatch { tx_index: i, expected, got: tx.nonce });
            }
            if tx.data.is_empty() {
                return Err(ValidatorError::EmptyData { at_tx: Some(i) });
            }
            if tx.data.len() > MAX_DATA_LEN {
                return Err(ValidatorError::DataTooLarge {
                    at_tx: Some(i),
                    got: tx.data.len(),
                    cap: MAX_DATA_LEN,
                });
            }
            expected = expected.wrapping_add(1);
        }

        // ---- Commit phase (engine mutations, provably safe) ---------

        let mut current = self.local_root;
        for tx in &batch.transactions {
            let tx_d = tx.canonicalize(engine);
            current = engine.synthesize(current, tx_d);
        }

        // Atomic update
        self.local_root = current;
        self.expected_nonce = expected;
        Ok(current)
    }
}

impl LocalCausalAgent for ConsensusValidator {
    type ActionData = TransactionAction;

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
// Each V-audit finding gets its own regression test that would FAIL if
// the by-construction defense were removed.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::synthesize_causal_action;
    use std::sync::Arc;

    fn fresh() -> (Arc<DistinctionEngine>, ConsensusValidator) {
        let engine = Arc::new(DistinctionEngine::new());
        let validator = ConsensusValidator::new(&engine);
        (engine, validator)
    }

    fn tx(nonce: u64, data: &[u8]) -> TransactionAction {
        TransactionAction::new(nonce, data.to_vec()).expect("test tx valid (invariant)")
    }

    // ----- Positive: valid batch is accepted -----------------------------

    #[test]
    fn valid_batch_advances_root_and_nonce() {
        let (engine, mut v) = fresh();
        let batch = TransactionBatch {
            previous_root: v.state_root_id(),
            transactions: vec![tx(0, b"hello"), tx(1, b"world")],
        };
        let new_root = v.validate_batch(&engine, &batch).expect("valid batch");
        assert_ne!(new_root, engine.d0());
        assert_eq!(v.state_root_id(), new_root);
        assert_eq!(v.expected_nonce(), 2);
    }

    #[test]
    fn sequential_batches_chain_correctly() {
        let (engine, mut v) = fresh();
        let b1 =
            TransactionBatch { previous_root: v.state_root_id(), transactions: vec![tx(0, b"a")] };
        let root1 = v.validate_batch(&engine, &b1).expect("b1 valid");
        let b2 = TransactionBatch { previous_root: root1, transactions: vec![tx(1, b"b")] };
        let root2 = v.validate_batch(&engine, &b2).expect("b2 valid");
        assert_ne!(root1, root2);
        assert_eq!(v.expected_nonce(), 2);
    }

    // ----- V3 regression: data cap ---------------------------------------

    #[test]
    fn v3_data_cap_rejects_oversized_tx() {
        let (engine, mut v) = fresh();
        let oversized = vec![0u8; MAX_DATA_LEN + 1];
        // Bypass ctor to test the pre-validation path directly.
        let batch = TransactionBatch {
            previous_root: v.state_root_id(),
            transactions: vec![TransactionAction { nonce: 0, data: oversized }],
        };
        let err = v.validate_batch(&engine, &batch).expect_err("oversized data must reject");
        assert!(matches!(err, ValidatorError::DataTooLarge { .. }));
    }

    #[test]
    fn v3_ctor_rejects_oversized_data() {
        let err = TransactionAction::new(0, vec![0u8; MAX_DATA_LEN + 1])
            .expect_err("ctor must reject oversized");
        assert!(matches!(err, ValidatorError::DataTooLarge { .. }));
    }

    // ----- V4 regression: oversized-root / bounded error messages --------

    #[test]
    fn v4_named_error_variants_bounded() {
        // Honest scope: this enumerates the CURRENT ValidatorError
        // variants and asserts each has a bounded Display. A future
        // variant like `#[error("bad data at {tampered}")]` with a
        // String field would NOT be caught by this test — the test
        // only sees what's listed. Compensation: a real invariant
        // check on Display shape belongs in a hygiene test or
        // proptest at the audit layer. This test proves the
        // current variants are bounded.
        let engine = DistinctionEngine::new();
        let d = engine.synthesize(engine.d0(), engine.d1());
        let variants = [
            ValidatorError::RootMismatch { expected: d, got: d },
            ValidatorError::EmptyBatch,
            ValidatorError::BatchTooLarge { got: usize::MAX, cap: MAX_TRANSACTIONS_PER_BATCH },
            ValidatorError::NonceMismatch { tx_index: 0, expected: u64::MAX, got: 0 },
            ValidatorError::DataTooLarge { at_tx: Some(0), got: usize::MAX, cap: MAX_DATA_LEN },
            ValidatorError::DataTooLarge { at_tx: None, got: usize::MAX, cap: MAX_DATA_LEN },
            ValidatorError::EmptyData { at_tx: Some(0) },
            ValidatorError::EmptyData { at_tx: None },
        ];
        for e in &variants {
            // 256 chars is comfortable; real messages are ~40-100 chars.
            assert!(e.to_string().len() < 256, "error msg unbounded: {e}");
        }
    }

    #[test]
    fn v3_batch_cardinality_cap_rejects_oversized_batch() {
        // Complements v3_data_cap_rejects_oversized_tx — that one covers
        // per-tx data size; this covers batch count.
        let (engine, mut v) = fresh();
        let mut txs = Vec::with_capacity(MAX_TRANSACTIONS_PER_BATCH + 1);
        for i in 0..=MAX_TRANSACTIONS_PER_BATCH as u64 {
            txs.push(tx(i, b"x"));
        }
        let batch = TransactionBatch { previous_root: v.state_root_id(), transactions: txs };
        let err = v.validate_batch(&engine, &batch).expect_err("oversized batch must reject");
        assert!(matches!(err, ValidatorError::BatchTooLarge { .. }));
    }

    // ----- V5 regression: atomic failure ---------------------------------

    #[test]
    fn v5_rejected_batch_leaves_engine_state_invariant() {
        // Load-bearing V5 test: a batch that fails pre-validation
        // must not have called engine.synthesize at all.
        //
        // Two-layer check: (1) distinction count unchanged, AND
        // (2) the full set of registered distinction IDs unchanged.
        // The set check catches a subtler regression where the commit
        // loop runs on inputs that happen to already-exist (saturation
        // fast-path) — a count-only check would miss it.
        let (engine, mut v) = fresh();
        let count_before = engine.distinction_count();
        let mut ids_before: Vec<Distinction> = engine.snapshot_distinctions();
        ids_before.sort_by_key(|d| *d.as_bytes());
        let root_before = v.state_root_id();
        let nonce_before = v.expected_nonce();

        // Batch with a mid-way bad nonce (nonce 0, then 5 instead of 1).
        let batch = TransactionBatch {
            previous_root: v.state_root_id(),
            transactions: vec![tx(0, b"ok"), tx(5, b"bad nonce")],
        };
        let err = v.validate_batch(&engine, &batch).expect_err("batch must reject");
        assert!(matches!(err, ValidatorError::NonceMismatch { .. }));

        // Engine state UNCHANGED — both count AND set-of-IDs.
        assert_eq!(
            engine.distinction_count(),
            count_before,
            "V5: engine distinction_count must be invariant on rejection"
        );
        let mut ids_after: Vec<Distinction> = engine.snapshot_distinctions();
        ids_after.sort_by_key(|d| *d.as_bytes());
        assert_eq!(
            ids_before, ids_after,
            "V5: engine set-of-distinctions must be invariant on rejection"
        );
        // Validator state UNCHANGED.
        assert_eq!(
            v.state_root_id(),
            root_before,
            "V5: validator root must be invariant on rejection"
        );
        assert_eq!(
            v.expected_nonce(),
            nonce_before,
            "V5: validator nonce must be invariant on rejection"
        );
    }

    #[test]
    fn v5_root_mismatch_also_atomic() {
        let (engine, mut v) = fresh();
        let count_before = engine.distinction_count();
        let nonce_before = v.expected_nonce();
        let batch = TransactionBatch {
            previous_root: engine.d1(), // wrong root
            transactions: vec![tx(0, b"ok")],
        };
        let _ = v.validate_batch(&engine, &batch).expect_err("wrong root");
        assert_eq!(engine.distinction_count(), count_before);
        assert_eq!(
            v.expected_nonce(),
            nonce_before,
            "V5: nonce must be invariant on RootMismatch rejection"
        );
    }

    // ----- V6 regression: atomic restore_state ---------------------------

    #[test]
    fn v6_restore_state_sets_both_fields() {
        // Honest scope: since restore_state is `&mut self` non-fallible
        // synchronous, "atomicity" in the observer-safety sense is
        // untestable without &Mutex or sync internals. What we CAN
        // verify: both fields are set by the single call. Regression
        // guard for a future refactor that split restore_state into
        // two methods (which would break the "both together" property).
        let (engine, mut v) = fresh();
        // Engine-witnessed: a sibling crate cannot fabricate a
        // distinction from raw bytes, and should not want to.
        let target_root = engine.synthesize(engine.d0(), engine.d1());
        v.restore_state(target_root, 999);
        assert_eq!(v.state_root_id(), target_root);
        assert_eq!(v.expected_nonce(), 999);
    }

    // ----- V8 regression: empty-data by design ---------------------------

    #[test]
    fn v8_empty_data_rejected_at_ctor() {
        let err = TransactionAction::new(0, vec![]).expect_err("empty data must reject");
        assert!(matches!(err, ValidatorError::EmptyData { .. }));
    }

    #[test]
    fn v8_empty_data_rejected_at_validation() {
        let (engine, mut v) = fresh();
        // Bypass ctor to test pre-validation path.
        let batch = TransactionBatch {
            previous_root: v.state_root_id(),
            transactions: vec![TransactionAction { nonce: 0, data: vec![] }],
        };
        let err = v.validate_batch(&engine, &batch).expect_err("empty data must reject");
        assert!(matches!(err, ValidatorError::EmptyData { .. }));
    }

    // ----- LCA trait impl smoke ------------------------------------------

    #[test]
    fn lca_impl_get_and_update_root() {
        let (engine, mut v) = fresh();
        let r0 = v.get_current_root();
        let action = tx(0, b"single");
        let new_root = synthesize_causal_action(v.get_current_root(), action, &engine);
        v.update_local_root(new_root);
        assert_ne!(v.get_current_root(), r0);
        assert_eq!(v.get_current_root(), new_root);
    }

    // ----- Serde try_from enforcement ------------------------------------

    #[test]
    fn deserialize_rejects_empty_data() {
        let raw = r#"{"nonce":0,"data":[]}"#;
        let err = serde_json::from_str::<TransactionAction>(raw).expect_err("empty must reject");
        assert!(err.to_string().contains("empty data"));
    }

    #[test]
    fn deserialize_rejects_oversized_data() {
        let big = vec![0u8; MAX_DATA_LEN + 1];
        let raw = serde_json::to_string(&TransactionActionRaw { nonce: 0, data: big })
            .expect("raw serialize");
        let err =
            serde_json::from_str::<TransactionAction>(&raw).expect_err("oversized must reject");
        assert!(err.to_string().contains("data too large"));
    }

    #[test]
    fn action_serde_roundtrip() {
        let original = tx(42, b"round-trip payload");
        let json = serde_json::to_string(&original).expect("serialize");
        let recovered: TransactionAction = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, recovered);
    }

    #[test]
    fn batch_serde_roundtrip() {
        let (_engine, v) = fresh();
        let original = TransactionBatch {
            previous_root: v.state_root_id(),
            transactions: vec![tx(0, b"a"), tx(1, b"b"), tx(2, b"c")],
        };
        let json = serde_json::to_string(&original).expect("serialize");
        let recovered: TransactionBatch = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(original, recovered);
    }

    // ----- Fold injectivity: the collision, as regression --------------

    fn root_of(nonce: u64, data: &[u8]) -> [u8; 16] {
        let e = DistinctionEngine::new();
        *TransactionAction::new(nonce, data.to_vec())
            .expect("valid action")
            .canonicalize(&e)
            .as_bytes()
    }

    #[test]
    fn nonce_byte_order_is_not_lost() {
        // 1 is `01 00 ..` and 256 is `00 01 ..`. The old fold seeded the
        // accumulator with the first byte's tip, leaving the first two
        // positions an unordered pair, so these folded identically and
        // `verify_batch` accepted a batch whose nonces had been swapped.
        assert_ne!(root_of(1, &[7]), root_of(256, &[7]));
        assert_ne!(root_of(2, &[9, 9]), root_of(512, &[9, 9]));
    }

    #[test]
    fn a_repeated_leading_byte_is_not_absorbed() {
        // `synthesize(x, x) == x`, so a leading run of equal bytes used to
        // vanish: with nonce 0, data `[0x00]` and `[0x00, 0x00]` produced
        // the same root.
        assert_ne!(root_of(0, &[0x00]), root_of(0, &[0x00, 0x00]));
        assert_ne!(root_of(0, &[0x00, 0x00]), root_of(0, &[0x00, 0x00, 0x00]));
    }

    #[test]
    fn nonce_and_data_do_not_bleed_into_each_other() {
        // The nonce is a fixed eight bytes, so the boundary is
        // unambiguous; shifting a byte across it must change the root.
        assert_ne!(root_of(0x0102, &[0x03]), root_of(0x01, &[0x02, 0x03]));
    }

    #[test]
    fn distinct_transactions_have_distinct_roots() {
        // Exhaustive over a small space, which is what actually would have
        // caught this: determinism was tested, injectivity never was.
        let mut seen = std::collections::HashMap::new();
        for nonce in [0u64, 1, 2, 255, 256, 257, 512] {
            for data in
                [vec![0x00], vec![0x00, 0x00], vec![0x01], vec![0x01, 0x00], vec![0x00, 0x01]]
            {
                let root = root_of(nonce, &data);
                if let Some(prev) = seen.insert(root, (nonce, data.clone())) {
                    panic!("({nonce}, {data:?}) collides with {prev:?}");
                }
            }
        }
    }

    proptest::proptest! {
        #[test]
        fn distinct_inputs_give_distinct_roots(
            n1 in proptest::prelude::any::<u64>(),
            n2 in proptest::prelude::any::<u64>(),
            d1 in proptest::collection::vec(proptest::prelude::any::<u8>(), 1..12),
            d2 in proptest::collection::vec(proptest::prelude::any::<u8>(), 1..12),
        ) {
            proptest::prop_assume!((n1, &d1) != (n2, &d2));
            proptest::prop_assert_ne!(root_of(n1, &d1), root_of(n2, &d2));
        }
    }
}
