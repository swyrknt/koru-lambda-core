/// Subsystems Module
///
/// Contains parallel observers/subsystems that build on the distinction engine.
/// All subsystems implement the LocalCausalAgent trait to enforce:
/// - Locality (anchored to local Δ root)
/// - Causality (ΔNew = ΔLocal ⊕ ΔAction)
/// - Determinism (all inputs canonicalizable)

pub mod local_agent;
pub mod validator;

// Re-export key types
pub use local_agent::{synthesize_causal_action, LocalCausalAgent};
pub use validator::{
    BatchValidationResult, ConsensusValidator, TransactionAction, TransactionBatch,
};
