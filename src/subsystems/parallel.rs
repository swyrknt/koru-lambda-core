//! Batch synthesis helper.
//!
//! Performs concurrent synthesis operations using Rayon across a slice of
//! parent-id pairs or a byte stream. Useful for batch canonicalization and
//! data ingestion.
//!
//! v2.0 history: this module formerly contained `ParallelBatchProcessor`,
//! `ProcessingStrategy`, and `ParallelAction`. Phase 1 audit (parallel.md)
//! identified `ParallelBatchProcessor` as a misnamed Sequential-body
//! wrapper that duplicated `ConsensusValidator` with an unused
//! `worker_count` field. All of it was removed in v2.0 (CHECKLIST
//! Section 1.10 / Phase 6 sub-branch #2). The genuine concurrent helper
//! formerly known as `ParallelSynthesizer` survives, renamed
//! `BatchSynthesizer` — parallelism is an implementation choice, not
//! user-facing API surface.

use crate::{Distinction, DistinctionEngine};
use rayon::prelude::*;
use std::sync::Arc;

/// Concurrent synthesis helper.
///
/// Holds a shared `Arc<DistinctionEngine>` and exposes methods that
/// distribute work across Rayon's thread pool. The engine itself is
/// thread-safe via DashMap; this helper is a convenience wrapper that
/// avoids boilerplate when ingesting many independent pairs or bytes.
pub struct BatchSynthesizer {
    engine: Arc<DistinctionEngine>,
}

impl BatchSynthesizer {
    /// Creates a new batch synthesizer over the given engine.
    #[must_use]
    pub fn new(engine: Arc<DistinctionEngine>) -> Self {
        Self { engine }
    }

    /// Synthesizes a batch of `(parent_a_hex, parent_b_hex)` pairs in parallel.
    ///
    /// Returns one `Option<Distinction>` per input pair:
    /// - `Some(distinction)` if both parents are registered in the engine
    /// - `None` if either parent id is malformed hex or not registered
    ///
    /// The explicit `Option` replaces the v1.2.0 "silent empty-string
    /// fallback" pattern (parallel.md audit finding) — missing parents
    /// now surface as `None` instead of an empty-id distinction.
    #[must_use]
    pub fn synthesize_batch(&self, pairs: Vec<(String, String)>) -> Vec<Option<Distinction>> {
        pairs
            .into_par_iter()
            .map(|(id_a, id_b)| {
                let d_a = self.engine.get_distinction_by_id(&id_a)?;
                let d_b = self.engine.get_distinction_by_id(&id_b)?;
                Some(self.engine.synthesize(&d_a, &d_b))
            })
            .collect()
    }

    /// Canonicalizes a slice of bytes in parallel.
    ///
    /// Useful for data ingestion: returns one `Distinction` per input byte
    /// (each byte canonicalized via `ByteMapping`). All returned distinctions
    /// are valid; `ByteMapping` cannot fail.
    #[must_use]
    pub fn canonicalize_bytes_batch(&self, bytes: Vec<u8>) -> Vec<Distinction> {
        use crate::Canonicalizable;

        bytes
            .into_par_iter()
            .map(|byte| byte.to_canonical_structure(&self.engine))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_batch_synthesizer_canonicalize_bytes_deterministic() {
        let engine = Arc::new(DistinctionEngine::new());
        let synthesizer = BatchSynthesizer::new(engine.clone());

        let bytes: Vec<u8> = (0..100).collect();
        let results = synthesizer.canonicalize_bytes_batch(bytes);

        assert_eq!(results.len(), 100);

        // Determinism: same input -> byte-identical output.
        let bytes2: Vec<u8> = (0..100).collect();
        let results2 = synthesizer.canonicalize_bytes_batch(bytes2);
        assert_eq!(results, results2);
    }

    #[test]
    fn test_batch_synthesizer_missing_parent_yields_none() {
        let engine = Arc::new(DistinctionEngine::new());
        let synthesizer = BatchSynthesizer::new(engine.clone());

        // Register d0 ⊕ d1 so we have at least one synthesized distinction
        // we can name by hex.
        let real = engine.synthesize(engine.d0(), engine.d1());
        let real_hex = real.to_hex();
        let d0_hex = engine.d0().to_hex();

        // Pair 0: both real → Some(_)
        // Pair 1: second parent is well-formed hex but not registered → None
        //   (`f` × 32 = `[0xff; 16]`, vanishingly unlikely to be a SHA256
        //   prefix and not a primordial)
        // Pair 2: second parent is malformed hex → None
        let pairs = vec![
            (d0_hex.clone(), real_hex.clone()),
            (d0_hex.clone(), "f".repeat(32)),
            (d0_hex, "not-hex-at-all".to_string()),
        ];

        let results = synthesizer.synthesize_batch(pairs);
        assert_eq!(results.len(), 3);
        assert!(results[0].is_some(), "real pair must synthesize");
        assert!(results[1].is_none(), "unregistered parent must yield None");
        assert!(results[2].is_none(), "malformed hex must yield None");
    }
}
