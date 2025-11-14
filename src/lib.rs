pub mod engine;
pub mod primitives;
pub mod subsystems;

pub use engine::{Distinction, DistinctionEngine, Relationship};
pub use primitives::{ByteMapping, Canonicalizable};
pub use subsystems::{
    CompactionAction, CompactionStats, StructuralCompactor, ThermalState,
    ConsensusValidator, TransactionAction, TransactionBatch, BatchValidationResult,
    LocalCausalAgent, synthesize_causal_action,
    NetworkAction, NetworkAgent, NetworkStats, PeerIdentity,
    ParallelAction, ParallelBatchProcessor, ParallelSynthesizer, ProcessingStrategy,
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_axiom_irreflexivity() {
        let engine = DistinctionEngine::new();
        let d1 = engine.d1().clone();

        let result = engine.synthesize(&d1, &d1);

        // Axiom: A distinction synthesized with itself yields itself
        assert_eq!(result.id(), d1.id());
        assert_eq!(engine.distinction_count(), 2); // No new distinctions
        assert_eq!(engine.relationship_count(), 1); // No new relationships
    }

    #[test]
    fn test_axiom_symmetry() {
        let engine1 = DistinctionEngine::new();
        let engine2 = DistinctionEngine::new();

        let d0_1 = engine1.d0().clone();
        let d1_1 = engine1.d1().clone();
        let d0_2 = engine2.d0().clone();
        let d1_2 = engine2.d1().clone();

        let c_ab = engine1.synthesize(&d0_1, &d1_1);
        let c_ba = engine2.synthesize(&d1_2, &d0_2);

        // Axiom: Symmetry - order independence in synthesis
        assert_eq!(c_ab.id(), c_ba.id());
    }

    #[test]
    fn test_axiom_synthesis() {
        let engine = DistinctionEngine::new();

        assert_eq!(engine.relationship_count(), 1);
        assert_eq!(engine.distinction_count(), 2);

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let c = engine.synthesize(&d0, &d1);

        // Axiom: Synthesis creates a new distinction
        assert_eq!(engine.distinction_count(), 3);

        let (distinctions, relationships) = engine.get_state_snapshot();

        // Verify the new distinction exists
        assert!(distinctions.iter().any(|d| d.id() == c.id()));

        // Verify relationships with canonical ordering
        let rel_c_d0 = if d0.id() < c.id() {
            (d0.id().to_string(), c.id().to_string())
        } else {
            (c.id().to_string(), d0.id().to_string())
        };

        let rel_c_d1 = if d1.id() < c.id() {
            (d1.id().to_string(), c.id().to_string())
        } else {
            (c.id().to_string(), d1.id().to_string())
        };

        // The original relationship ("0", "1") plus two new relationships = 3 total
        assert!(relationships.contains(&rel_c_d0));
        assert!(relationships.contains(&rel_c_d1));
        assert_eq!(engine.relationship_count(), 3);
    }

    #[test]
    fn test_axiom_idempotency() {
        let engine = DistinctionEngine::new();

        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();

        let c1 = engine.synthesize(&d0, &d1);
        let distinctions_1 = engine.distinction_count();
        let relationships_1 = engine.relationship_count();

        // Synthesize again with the same inputs
        let c2 = engine.synthesize(&d0, &d1);

        // Axiom: Timeless consistency - repeated synthesis yields identical results
        assert_eq!(c1.id(), c2.id());
        assert_eq!(engine.distinction_count(), distinctions_1);
        assert_eq!(engine.relationship_count(), relationships_1);
    }

    #[test]
    fn test_byte_mapping() {
        let engine = DistinctionEngine::new();

        // Test that same byte produces same distinction
        let d1 = ByteMapping::map_byte_to_distinction(42, &engine);
        let d2 = ByteMapping::map_byte_to_distinction(42, &engine);
        assert_eq!(d1.id(), d2.id());

        // Test that different bytes produce different distinctions
        let d3 = ByteMapping::map_byte_to_distinction(43, &engine);
        assert_ne!(d1.id(), d3.id());
    }

    #[test]
    fn test_canonicalizable_trait() {
        let engine = DistinctionEngine::new();

        // Test the Canonicalizable trait implementation for u8
        let byte: u8 = 255;
        let d1 = byte.to_canonical_structure(&engine);
        let d2 = ByteMapping::map_byte_to_distinction(255, &engine);

        // Should produce the same distinction
        assert_eq!(d1.id(), d2.id());
    }
}
