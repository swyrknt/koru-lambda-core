/// Structural Compactor
///
/// Classifies distinctions by **Structural Importance Score (S.I.S.)** —
/// degree centrality from the engine's traversal index — and records
/// compaction events as append-only distinctions on the compactor's
/// causal chain.
///
/// # Thermal classification
///
/// Consumers supply two thresholds at construction:
///
/// * `hot_threshold`: distinctions with `degree(d) >= hot_threshold`
///   are HOT (preserved in the active set).
/// * `warm_threshold`: distinctions with
///   `warm_threshold <= degree(d) < hot_threshold` are WARM (monitored
///   for future archival).
/// * Everything below `warm_threshold` is COLD (eligible for archival).
///
/// `warm_threshold <= hot_threshold` is enforced at construction. The
/// thresholds are **deployment choices**, not derived constants — the
/// v1.2.0 defaults (`hot_threshold = 3`, `warm = hot/2`) were magic
/// numbers without empirical basis (CHECKLIST 1.9 / Phase 6
/// sub-branch #9). Consumers should pick values that match the
/// degree distribution of the engine they intend to compact; e.g., a
/// chain with median degree 4 might choose `(hot, warm) = (16, 4)`.
///
/// # Append-only semantics (Decision 5.2)
///
/// Compaction does not modify the engine's distinction store. Every
/// compaction event is itself a distinction synthesized into the
/// engine (`compactor.synthesize_action`); the synthesis log captures
/// it like any other operation. "Archival" is a marking — the
/// compactor maintains an internal `archived_set` of IDs — not a
/// deletion from the engine.
use crate::primitives::Canonicalizable;
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::{Distinction, DistinctionEngine};
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Thermal classification of a distinction relative to the compactor's
/// configured thresholds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThermalState {
    /// `degree(d) >= hot_threshold` — preserved in active memory.
    Hot,
    /// `warm_threshold <= degree(d) < hot_threshold` — monitored for
    /// future archival.
    Warm,
    /// `degree(d) < warm_threshold` — eligible for archival.
    Cold,
}

/// Compaction operation — the canonical record of one compactor pass.
///
/// `archived_ids` was removed in CHECKLIST 1.9 / Phase 6 sub-branch #9:
/// the field was carried as data but never participated in
/// `to_canonical_structure`, so two compactor passes with the same
/// `(sis_threshold, preserved_count)` always produced the same
/// distinction regardless of `archived_ids`. The IDs themselves are
/// available on the compactor via `archived_ids()`.
#[derive(Debug, Clone)]
pub struct CompactionAction {
    /// S.I.S. threshold this pass used to classify HOT vs non-HOT.
    pub sis_threshold: usize,
    /// Number of distinctions preserved (HOT + WARM) by this pass.
    pub preserved_count: usize,
}

impl Canonicalizable for CompactionAction {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        let threshold_bytes = self.sis_threshold.to_le_bytes();
        let count_bytes = self.preserved_count.to_le_bytes();

        let threshold_byte_distinctions: Vec<Distinction> = threshold_bytes
            .into_par_iter()
            .map(|byte| byte.to_canonical_structure(engine))
            .collect();
        let threshold_d = threshold_byte_distinctions
            .into_iter()
            .fold(engine.d0().clone(), |acc, d| engine.synthesize(&acc, &d));

        let count_byte_distinctions: Vec<Distinction> =
            count_bytes.into_par_iter().map(|byte| byte.to_canonical_structure(engine)).collect();
        let count_d = count_byte_distinctions
            .into_iter()
            .fold(engine.d0().clone(), |acc, d| engine.synthesize(&acc, &d));

        engine.synthesize(&threshold_d, &count_d)
    }
}

/// Structural Compactor implementation.
pub struct StructuralCompactor {
    local_root: Distinction,
    thermal_states: HashMap<String, ThermalState>,
    archived_set: HashSet<String>,
    hot_threshold: usize,
    warm_threshold: usize,
    compaction_count: u64,
}

impl StructuralCompactor {
    /// Construct a compactor anchored at the engine's genesis with the
    /// supplied thresholds.
    ///
    /// # Panics
    /// Panics if `warm_threshold > hot_threshold`. The thresholds must
    /// satisfy `warm <= hot` so the WARM band is non-negative.
    pub fn new(
        engine: &Arc<DistinctionEngine>,
        hot_threshold: usize,
        warm_threshold: usize,
    ) -> Self {
        assert!(
            warm_threshold <= hot_threshold,
            "warm_threshold ({}) must be <= hot_threshold ({})",
            warm_threshold,
            hot_threshold
        );

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        Self {
            local_root: genesis,
            thermal_states: HashMap::new(),
            archived_set: HashSet::new(),
            hot_threshold,
            warm_threshold,
            compaction_count: 0,
        }
    }

    /// Calculate Structural Importance Score (S.I.S.) for every
    /// distinction known to the engine.
    ///
    /// S.I.S. = degree centrality. Each lookup is `O(1)` via the
    /// engine's internal AtomicUsize cache (CHECKLIST 2.2 / Phase 6
    /// sub-branch #5). The full state-snapshot clone that v1.2.0
    /// performed is no longer required.
    pub fn calculate_sis(&self, engine: &Arc<DistinctionEngine>) -> HashMap<String, usize> {
        engine
            .get_distinctions_snapshot()
            .iter()
            .map(|d| (d.to_hex(), engine.degree(d)))
            .collect()
    }

    /// Classify distinctions into thermal states using the compactor's
    /// configured thresholds.
    pub fn classify_thermal_states(&mut self, sis_map: &HashMap<String, usize>) {
        self.thermal_states.clear();

        for (id, &degree) in sis_map {
            let state = if degree >= self.hot_threshold {
                ThermalState::Hot
            } else if degree >= self.warm_threshold {
                ThermalState::Warm
            } else {
                ThermalState::Cold
            };
            self.thermal_states.insert(id.clone(), state);
        }
    }

    /// Calculate, classify, and mark COLD distinctions as archived.
    ///
    /// Returns the `CompactionAction` describing this pass.
    ///
    /// # Sub-branch #9 changes
    ///
    /// `compact()` no longer increments `compaction_count` (CHECKLIST
    /// 1.9 #2). The increment lives only in `synthesize_action`, which
    /// is the call that actually persists the compaction event as a
    /// distinction. Calling `compact()` without `synthesize_action`
    /// is a pure dry-run that updates thermal classification + archived
    /// set but does not advance the compactor's compaction count.
    pub fn compact(&mut self, engine: &Arc<DistinctionEngine>) -> CompactionAction {
        let sis_map = self.calculate_sis(engine);
        self.classify_thermal_states(&sis_map);

        let mut preserved_count = 0;
        for (id, state) in &self.thermal_states {
            match state {
                ThermalState::Cold => {
                    self.archived_set.insert(id.clone());
                },
                ThermalState::Hot | ThermalState::Warm => {
                    preserved_count += 1;
                },
            }
        }

        CompactionAction {
            sis_threshold: self.hot_threshold,
            preserved_count,
        }
    }

    /// Current compaction statistics.
    pub fn get_stats(&self) -> CompactionStats {
        let total_known = self.thermal_states.len();
        let hot_count = self.thermal_states.values().filter(|s| **s == ThermalState::Hot).count();
        let warm_count =
            self.thermal_states.values().filter(|s| **s == ThermalState::Warm).count();
        let cold_count =
            self.thermal_states.values().filter(|s| **s == ThermalState::Cold).count();

        CompactionStats {
            total_distinctions: total_known,
            hot_count,
            warm_count,
            cold_count,
            archived_count: self.archived_set.len(),
            compaction_operations: self.compaction_count,
            current_root: self.local_root.to_hex(),
        }
    }

    /// Returns true if `id` is in the compactor's archived set.
    pub fn is_archived(&self, id: &str) -> bool {
        self.archived_set.contains(id)
    }

    /// Returns the thermal state classification for `id`, or `None` if
    /// the compactor has not seen the id (e.g., engine grew after the
    /// last `compact()` / `classify_thermal_states` call).
    pub fn get_thermal_state(&self, id: &str) -> Option<&ThermalState> {
        self.thermal_states.get(id)
    }

    /// Reconfigure the HOT/WARM thresholds at runtime.
    ///
    /// Replaces v1.2.0's `set_hot_threshold(usize)`, which silently
    /// inferred `warm = hot/2`. Consumers must now specify both
    /// thresholds explicitly (CHECKLIST 1.9 / Phase 6 sub-branch #9).
    ///
    /// # Panics
    /// Panics if `warm > hot`.
    pub fn set_thresholds(&mut self, hot_threshold: usize, warm_threshold: usize) {
        assert!(
            warm_threshold <= hot_threshold,
            "warm_threshold ({}) must be <= hot_threshold ({})",
            warm_threshold,
            hot_threshold
        );
        self.hot_threshold = hot_threshold;
        self.warm_threshold = warm_threshold;
    }

    /// Returns the configured `(hot_threshold, warm_threshold)` pair.
    pub fn thresholds(&self) -> (usize, usize) {
        (self.hot_threshold, self.warm_threshold)
    }
}

/// Statistics about compactor state.
#[derive(Debug, Clone)]
pub struct CompactionStats {
    pub total_distinctions: usize,
    pub hot_count: usize,
    pub warm_count: usize,
    pub cold_count: usize,
    pub archived_count: usize,
    pub compaction_operations: u64,
    pub current_root: String,
}

impl LocalCausalAgent for StructuralCompactor {
    type ActionData = CompactionAction;

    fn get_current_root(&self) -> &Distinction {
        &self.local_root
    }

    /// Synthesize a `CompactionAction` into the compactor's causal
    /// chain. This is the call that actually advances `compaction_count`
    /// and writes a compaction-event distinction into the engine.
    ///
    /// # Sub-branch #9 changes
    ///
    /// v1.2.0 also re-ran `calculate_sis` + `classify_thermal_states`
    /// here, after writing the new compaction-event distinction. The
    /// freshly-synthesized event had degree 2 and was therefore
    /// classified COLD by the same compactor that just created it
    /// (CHECKLIST 1.9 #3 — "archives its own work products"). The
    /// re-classification is removed; the compactor's thermal state is
    /// the snapshot from the last `compact()` call, which is the
    /// honest description of what was just compacted.
    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        let action_distinction = action_data.to_canonical_structure(engine);
        let new_root = engine.synthesize(&self.local_root, &action_distinction);

        self.local_root = new_root.clone();
        self.compaction_count += 1;

        new_root
    }

    fn update_local_root(&mut self, new_root: Distinction) {
        self.local_root = new_root;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_HOT: usize = 3;
    const TEST_WARM: usize = 1;

    #[test]
    fn test_compactor_genesis() {
        let engine = Arc::new(DistinctionEngine::new());
        let compactor = StructuralCompactor::new(&engine, TEST_HOT, TEST_WARM);

        assert_eq!(compactor.compaction_count, 0);
        assert_eq!(compactor.archived_set.len(), 0);
        assert!(!compactor.get_current_root().to_hex().is_empty());
        assert_eq!(compactor.thresholds(), (TEST_HOT, TEST_WARM));
    }

    #[test]
    #[should_panic(expected = "warm_threshold")]
    fn test_compactor_new_rejects_warm_above_hot() {
        let engine = Arc::new(DistinctionEngine::new());
        // warm=5 > hot=3 must panic at construction.
        let _ = StructuralCompactor::new(&engine, 3, 5);
    }

    #[test]
    #[should_panic(expected = "warm_threshold")]
    fn test_set_thresholds_rejects_warm_above_hot() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine, 10, 2);
        compactor.set_thresholds(3, 5);
    }

    #[test]
    fn test_sis_calculation() {
        let engine = Arc::new(DistinctionEngine::new());
        let compactor = StructuralCompactor::new(&engine, TEST_HOT, TEST_WARM);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let a = engine.synthesize(&d0, &d1);
        let _b = engine.synthesize(&a, &d0);
        let _c = engine.synthesize(&a, &d1);

        let sis_map = compactor.calculate_sis(&engine);

        let d0_id = d0.to_hex();
        let d1_id = d1.to_hex();
        assert!(sis_map.contains_key(&d0_id));
        assert!(sis_map.contains_key(&d1_id));
        assert!(sis_map[&d0_id] >= 3);
        assert!(sis_map[&d1_id] >= 3);
    }

    #[test]
    fn test_thermal_classification_three_band() {
        let engine = Arc::new(DistinctionEngine::new());
        // hot >= 3, warm in [1, 3), cold < 1 (i.e., only deg 0 is cold,
        // but degree>=1 for every registered distinction so only
        // unknown ids would be cold).
        let mut compactor = StructuralCompactor::new(&engine, 3, 1);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let a = engine.synthesize(&d0, &d1);
        let _b = engine.synthesize(&a, &d0);
        let _c = engine.synthesize(&a, &d1);

        let sis_map = compactor.calculate_sis(&engine);
        compactor.classify_thermal_states(&sis_map);

        assert_eq!(compactor.get_thermal_state(&d0.to_hex()), Some(&ThermalState::Hot));
        assert_eq!(compactor.get_thermal_state(&d1.to_hex()), Some(&ThermalState::Hot));

        let stats = compactor.get_stats();
        assert!(stats.hot_count >= 2);
    }

    #[test]
    fn test_compaction_action_canonical_drops_archived_ids() {
        // Subbranch #9: archived_ids field is gone. Two actions with
        // the same (threshold, preserved_count) must produce the same
        // distinction; the canonical form no longer carries archived
        // ids at all.
        let engine = Arc::new(DistinctionEngine::new());

        let action1 = CompactionAction { sis_threshold: 3, preserved_count: 10 };
        let action2 = CompactionAction { sis_threshold: 3, preserved_count: 10 };

        let d1 = action1.to_canonical_structure(&engine);
        let d2 = action2.to_canonical_structure(&engine);
        assert_eq!(d1.to_hex(), d2.to_hex());
    }

    #[test]
    fn test_compaction_reduces_active_set() {
        let engine = Arc::new(DistinctionEngine::new());
        // hot >= 5, warm in [2, 5).
        let mut compactor = StructuralCompactor::new(&engine, 5, 2);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let a = engine.synthesize(&d0, &d1);

        for i in 0..10 {
            let byte = (i as u8).to_canonical_structure(&engine);
            engine.synthesize(&a, &byte);
        }

        let action = compactor.compact(&engine);
        assert!(action.preserved_count > 0);

        let stats = compactor.get_stats();
        assert!(stats.total_distinctions > stats.hot_count);
    }

    #[test]
    fn test_compact_does_not_increment_compaction_count() {
        // Subbranch #9: compaction_count increments only in
        // synthesize_action. compact() is now a pure dry-run with
        // respect to the count.
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine, 3, 1);

        assert_eq!(compactor.get_stats().compaction_operations, 0);

        let _ = compactor.compact(&engine);
        assert_eq!(
            compactor.get_stats().compaction_operations,
            0,
            "compact() must not advance compaction_count (#9: count++ lives in synthesize_action only)"
        );
    }

    #[test]
    fn test_synthesize_action_does_not_archive_its_own_event() {
        // Subbranch #9: synthesize_action no longer re-classifies and
        // re-archives. The fresh compaction-event distinction (deg 2)
        // would have been classified COLD and added to archived_set
        // by v1.2.0; that behaviour is removed.
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine, 3, 1);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let _ = engine.synthesize(&d0, &d1);

        let action = compactor.compact(&engine);
        let archived_before = compactor.get_stats().archived_count;
        let new_root = compactor.synthesize_action(action, &engine);

        // Compaction count moved.
        assert_eq!(compactor.get_stats().compaction_operations, 1);
        // The new event distinction is not added to archived_set.
        assert!(
            !compactor.is_archived(&new_root.to_hex()),
            "freshly-synthesized compaction event must not be archived (#9)"
        );
        // Archive count is unchanged by synthesize_action.
        assert_eq!(
            compactor.get_stats().archived_count,
            archived_before,
            "synthesize_action must not re-classify or re-archive (#9)"
        );
    }

    #[test]
    fn test_local_causal_agent_implementation() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine, 3, 1);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let _a = engine.synthesize(&d0, &d1);

        let action = compactor.compact(&engine);
        let initial_root = compactor.get_current_root().to_hex();

        let new_root = compactor.synthesize_action(action, &engine);
        assert_ne!(new_root.to_hex(), initial_root);
        assert_eq!(new_root.to_hex(), compactor.get_current_root().to_hex());
    }

    #[test]
    fn test_pressure_cooker_behavior() {
        // High-threshold compactor on a one-hub-many-leaves graph:
        // the hub and primordials are HOT, the rest are non-HOT.

        let engine = Arc::new(DistinctionEngine::new());
        // hot >= 6, warm in [2, 6).
        let mut compactor = StructuralCompactor::new(&engine, 6, 2);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();

        let hub = engine.synthesize(&d0, &d1);

        for i in 0..20 {
            let byte = (i as u8).to_canonical_structure(&engine);
            engine.synthesize(&hub, &byte);
        }

        let initial_count = engine.distinction_count();
        let _action = compactor.compact(&engine);

        let stats = compactor.get_stats();
        assert!(stats.total_distinctions > stats.hot_count);
        assert!(stats.hot_count >= 1);
        assert!(stats.warm_count + stats.cold_count > 0);

        let compression_ratio = initial_count as f64 / stats.hot_count.max(1) as f64;
        assert!(compression_ratio > 1.5, "Compression ratio should be > 1.5x");
    }
}
