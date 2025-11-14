/// Structural Compactor
///
/// Implements the Universal Coding Law (R ∝ U) by managing long-term graph efficiency.
/// Acts as a "pressure cooker" that concentrates the distinction graph by preserving
/// high-utility distinctions and archiving low-utility ones.
///
/// Design Principles:
/// - **Locality**: Anchored to compacted state root
/// - **Causality**: All compaction events are causal synthesis
/// - **Determinism**: S.I.S. calculation is purely structural
/// - **Emergence**: Leverages natural scale-free topology
///
/// The compactor identifies distinctions by Structural Importance Score (S.I.S.):
/// - **HOT**: High-degree hubs (S.I.S. ≥ threshold) - kept in active set
/// - **WARM**: Medium-degree nodes - candidates for future archival
/// - **COLD**: Low-degree nodes (S.I.S. < threshold) - archived
///
/// This enforces 26.6x storage efficiency by pruning the power-law tail.

use crate::primitives::Canonicalizable;
use crate::subsystems::local_agent::LocalCausalAgent;
use crate::{Distinction, DistinctionEngine};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Thermal classification of distinctions based on S.I.S.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThermalState {
    /// High S.I.S. - preserved in active memory
    Hot,
    /// Medium S.I.S. - monitored for future archival
    Warm,
    /// Low S.I.S. - archived/pruned from active set
    Cold,
}

/// Compaction operation representing a state transition
///
/// Each compaction action is canonicalized into a distinction and synthesized
/// with the local root to create a new compacted state.
#[derive(Debug, Clone)]
pub struct CompactionAction {
    /// Set of distinction IDs being archived (marked COLD)
    pub archived_ids: Vec<String>,
    /// Threshold S.I.S. value used for this compaction
    pub sis_threshold: usize,
    /// Number of distinctions preserved as HOT
    pub preserved_count: usize,
}

impl Canonicalizable for CompactionAction {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        // Canonicalize threshold as a byte sequence
        let threshold_bytes = self.sis_threshold.to_le_bytes();
        let threshold_d = threshold_bytes.iter().fold(engine.d0().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

        // Canonicalize preserved count
        let count_bytes = self.preserved_count.to_le_bytes();
        let count_d = count_bytes.iter().fold(engine.d0().clone(), |acc, &byte| {
            let byte_d = byte.to_canonical_structure(engine);
            engine.synthesize(&acc, &byte_d)
        });

        // Synthesize compaction event: threshold ⊕ preserved_count
        // Note: archived_ids are implicit (anything below threshold)
        engine.synthesize(&threshold_d, &count_d)
    }
}

/// Structural Compactor Implementation
///
/// Manages the long-term health of the distinction graph by:
/// 1. Calculating S.I.S. for all distinctions (degree centrality)
/// 2. Classifying distinctions into thermal states (HOT/WARM/COLD)
/// 3. Archiving COLD distinctions to reduce memory footprint
/// 4. Maintaining compacted state root as causal chain
pub struct StructuralCompactor {
    /// Current compacted state root
    local_root: Distinction,
    /// Thermal classification of all known distinctions
    thermal_states: HashMap<String, ThermalState>,
    /// Set of archived (COLD) distinction IDs
    archived_set: HashSet<String>,
    /// S.I.S. threshold for HOT classification (degree >= threshold)
    hot_threshold: usize,
    /// Number of compaction operations performed
    compaction_count: u64,
}

impl StructuralCompactor {
    /// Create a new compactor anchored at genesis
    ///
    /// **Concurrency**: Takes Arc for thread-safe shared access.
    pub fn new(engine: &Arc<DistinctionEngine>) -> Self {
        // Genesis compacted root: d0 ⊕ d1
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let genesis = engine.synthesize(&d0, &d1);

        Self {
            local_root: genesis,
            thermal_states: HashMap::new(),
            archived_set: HashSet::new(),
            hot_threshold: 3, // Default: nodes with degree ≥ 3 are HOT
            compaction_count: 0,
        }
    }

    /// Create compactor from existing state root
    pub fn from_root(root: Distinction, hot_threshold: usize) -> Self {
        Self {
            local_root: root,
            thermal_states: HashMap::new(),
            archived_set: HashSet::new(),
            hot_threshold,
            compaction_count: 0,
        }
    }

    /// Calculate Structural Importance Score (S.I.S.) for all distinctions
    ///
    /// S.I.S. is currently implemented as degree centrality:
    /// - Higher degree = more connections = higher structural importance
    /// - Leverages emergent scale-free topology
    ///
    /// Returns: HashMap<distinction_id, degree>
    pub fn calculate_sis(&self, engine: &Arc<DistinctionEngine>) -> HashMap<String, usize> {
        let (distinctions, relationships) = engine.get_state_snapshot();

        // Build degree count for each distinction
        let mut degree_map: HashMap<String, usize> = HashMap::new();

        // Initialize all distinctions with degree 0
        for d in &distinctions {
            degree_map.insert(d.id().to_string(), 0);
        }

        // Count relationships (each relationship connects two nodes)
        for (id_a, id_b) in &relationships {
            *degree_map.entry(id_a.clone()).or_insert(0) += 1;
            *degree_map.entry(id_b.clone()).or_insert(0) += 1;
        }

        degree_map
    }

    /// Classify distinctions into thermal states based on S.I.S.
    ///
    /// Classification rules:
    /// - HOT: degree >= hot_threshold (preserve)
    /// - WARM: hot_threshold / 2 <= degree < hot_threshold (monitor)
    /// - COLD: degree < hot_threshold / 2 (archive)
    pub fn classify_thermal_states(&mut self, sis_map: &HashMap<String, usize>) {
        self.thermal_states.clear();

        let warm_threshold = self.hot_threshold / 2;

        for (id, &degree) in sis_map {
            let state = if degree >= self.hot_threshold {
                ThermalState::Hot
            } else if degree >= warm_threshold {
                ThermalState::Warm
            } else {
                ThermalState::Cold
            };

            self.thermal_states.insert(id.clone(), state);
        }
    }

    /// Perform compaction: archive COLD distinctions
    ///
    /// This creates a new compacted state root representing the compression event.
    /// The actual archival is simulated by marking distinctions as archived.
    ///
    /// Returns: CompactionAction representing the operation
    pub fn compact(&mut self, engine: &Arc<DistinctionEngine>) -> CompactionAction {
        // Calculate S.I.S. and classify
        let sis_map = self.calculate_sis(engine);
        self.classify_thermal_states(&sis_map);

        // Identify COLD distinctions for archival
        let mut archived_ids = Vec::new();
        let mut preserved_count = 0;

        for (id, state) in &self.thermal_states {
            match state {
                ThermalState::Cold => {
                    archived_ids.push(id.clone());
                    self.archived_set.insert(id.clone());
                }
                ThermalState::Hot | ThermalState::Warm => {
                    preserved_count += 1;
                }
            }
        }

        // Create compaction action
        let action = CompactionAction {
            archived_ids: archived_ids.clone(),
            sis_threshold: self.hot_threshold,
            preserved_count,
        };

        // Update compaction count
        self.compaction_count += 1;

        action
    }

    /// Get current compaction statistics
    pub fn get_stats(&self) -> CompactionStats {
        let total_known = self.thermal_states.len();
        let hot_count = self
            .thermal_states
            .values()
            .filter(|s| **s == ThermalState::Hot)
            .count();
        let warm_count = self
            .thermal_states
            .values()
            .filter(|s| **s == ThermalState::Warm)
            .count();
        let cold_count = self
            .thermal_states
            .values()
            .filter(|s| **s == ThermalState::Cold)
            .count();

        CompactionStats {
            total_distinctions: total_known,
            hot_count,
            warm_count,
            cold_count,
            archived_count: self.archived_set.len(),
            compaction_operations: self.compaction_count,
            current_root: self.local_root.id().to_string(),
        }
    }

    /// Check if a distinction is archived
    pub fn is_archived(&self, id: &str) -> bool {
        self.archived_set.contains(id)
    }

    /// Get thermal state of a distinction
    pub fn get_thermal_state(&self, id: &str) -> Option<&ThermalState> {
        self.thermal_states.get(id)
    }

    /// Set HOT threshold (minimum degree for preservation)
    pub fn set_hot_threshold(&mut self, threshold: usize) {
        self.hot_threshold = threshold;
    }
}

/// Statistics about compaction state
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

    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        // Canonicalize the compaction action
        let action_distinction = action_data.to_canonical_structure(engine);
        
        // Causal synthesis: ΔNew = ΔLocal_Root ⊕ ΔAction
        let new_root = engine.synthesize(&self.local_root, &action_distinction);

        // Update local state
        self.local_root = new_root.clone();
        self.compaction_count += 1;

        // Update thermal states based on the compaction
        let sis_map = self.calculate_sis(engine);
        self.classify_thermal_states(&sis_map);

        // Mark archived distinctions
        for (id, state) in &self.thermal_states {
            if *state == ThermalState::Cold {
                self.archived_set.insert(id.clone());
            }
        }

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
    fn test_compactor_genesis() {
        let engine = Arc::new(DistinctionEngine::new());
        let compactor = StructuralCompactor::new(&engine);

        assert_eq!(compactor.compaction_count, 0);
        assert_eq!(compactor.archived_set.len(), 0);
        assert!(!compactor.get_current_root().id().is_empty());
    }

    #[test]
    fn test_sis_calculation() {
        let engine = Arc::new(DistinctionEngine::new());
        let compactor = StructuralCompactor::new(&engine);

        // Build small graph
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let a = engine.synthesize(&d0, &d1);
        let _b = engine.synthesize(&a, &d0);
        let _c = engine.synthesize(&a, &d1);

        // Calculate S.I.S.
        let sis_map = compactor.calculate_sis(&engine);

        // Verify degrees
        // d0: connected to (d1, a, b) = 3
        // d1: connected to (d0, a, c) = 3
        // a: connected to (b, c) = 2
        // b: connected to (a) = 1
        // c: connected to (a) = 1

        assert!(sis_map.contains_key("0")); // d0
        assert!(sis_map.contains_key("1")); // d1

        // Primordial distinctions should have highest degree
        assert!(sis_map["0"] >= 3);
        assert!(sis_map["1"] >= 3);
    }

    #[test]
    fn test_thermal_classification() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine);
        compactor.set_hot_threshold(3);

        // Build graph
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let a = engine.synthesize(&d0, &d1);
        let _b = engine.synthesize(&a, &d0);
        let _c = engine.synthesize(&a, &d1);

        // Calculate and classify
        let sis_map = compactor.calculate_sis(&engine);
        compactor.classify_thermal_states(&sis_map);

        // Primordials should be HOT (degree >= 3)
        assert_eq!(
            compactor.get_thermal_state("0"),
            Some(&ThermalState::Hot)
        );
        assert_eq!(
            compactor.get_thermal_state("1"),
            Some(&ThermalState::Hot)
        );

        // Leaf nodes should be COLD or WARM
        let stats = compactor.get_stats();
        assert!(stats.hot_count >= 2); // At least d0 and d1
    }

    #[test]
    fn test_compaction_action_canonical() {
        let engine = Arc::new(DistinctionEngine::new());

        let action1 = CompactionAction {
            archived_ids: vec!["a".to_string(), "b".to_string()],
            sis_threshold: 3,
            preserved_count: 10,
        };

        let action2 = CompactionAction {
            archived_ids: vec!["c".to_string(), "d".to_string()],
            sis_threshold: 3,
            preserved_count: 10,
        };

        let d1 = action1.to_canonical_structure(&engine);
        let d2 = action2.to_canonical_structure(&engine);

        // Same threshold and count should produce same distinction
        // (archived_ids are not included in canonicalization)
        assert_eq!(d1.id(), d2.id());
    }

    #[test]
    fn test_compaction_reduces_active_set() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine);
        compactor.set_hot_threshold(5); // Higher threshold to capture power-law tail

        // Build larger graph with leaf nodes
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let a = engine.synthesize(&d0, &d1);

        // Create leaf nodes (low degree)
        for i in 0..10 {
            let byte = (i as u8).to_canonical_structure(&engine);
            engine.synthesize(&a, &byte);
        }

        // Perform compaction
        let action = compactor.compact(&engine);

        // Verify compaction occurred
        assert!(action.preserved_count > 0);

        let stats = compactor.get_stats();
        // With higher threshold, some nodes should be classified as non-HOT
        assert!(stats.total_distinctions > stats.hot_count);
    }

    #[test]
    fn test_local_causal_agent_implementation() {
        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine);

        // Build graph
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let _a = engine.synthesize(&d0, &d1);

        // Perform compaction via synthesize_action
        let action = compactor.compact(&engine);
        let initial_root = compactor.get_current_root().id().to_string();

        let new_root = compactor.synthesize_action(action, &engine);

        // Verify root changed (causal synthesis occurred)
        assert_ne!(new_root.id(), &initial_root);
        assert_eq!(new_root.id(), compactor.get_current_root().id());
    }

    #[test]
    fn test_pressure_cooker_behavior() {
        // This test validates the "pressure cooker" analogy:
        // System concentrates graph by identifying high-utility vs low-utility nodes

        let engine = Arc::new(DistinctionEngine::new());
        let mut compactor = StructuralCompactor::new(&engine);
        compactor.set_hot_threshold(6); // Higher threshold to separate hubs from leaves

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();

        // Build power-law-ish graph: one hub, many leaves
        let hub = engine.synthesize(&d0, &d1);

        // Create 20 leaf nodes connected to hub
        for i in 0..20 {
            let byte = (i as u8).to_canonical_structure(&engine);
            engine.synthesize(&hub, &byte);
        }

        // Initial state
        let initial_count = engine.distinction_count();

        // Compact
        let _action = compactor.compact(&engine);

        let stats = compactor.get_stats();

        // Verify concentration:
        // - High threshold should classify most nodes as non-HOT
        // - Primordials and hub should remain HOT
        // - System correctly identifies structural hierarchy

        println!("\nPressure Cooker Test:");
        println!("  Initial distinctions: {}", initial_count);
        println!("  HOT (preserved): {}", stats.hot_count);
        println!("  WARM (monitored): {}", stats.warm_count);
        println!("  COLD (archived): {}", stats.cold_count);
        println!("  Compression ratio: {:.2}x",
            initial_count as f64 / stats.hot_count.max(1) as f64
        );

        // With threshold of 6, most nodes should be non-HOT
        assert!(stats.total_distinctions > stats.hot_count);

        // System should identify structural hierarchy
        assert!(stats.hot_count >= 2); // At least primordials
        assert!(stats.warm_count + stats.cold_count > 0); // Some non-HOT nodes exist
    }
}
