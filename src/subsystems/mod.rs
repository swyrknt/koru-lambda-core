/// Subsystems Module
///
/// Contains parallel observers/subsystems that build on the distinction engine.
/// All subsystems implement the LocalCausalAgent trait to enforce:
/// - Locality (anchored to local Δ root)
/// - Causality (ΔNew = ΔLocal ⊕ ΔAction)
/// - Determinism (all inputs canonicalizable)
pub mod commitment;
pub mod compactor;
pub mod local_agent;
pub mod network;
pub mod parallel;
pub mod validator;

// Re-export key types
pub use commitment::{BatchCommitment, BatchDataRequest, BatchDataResponse, CommitmentAgent};
pub use compactor::{CompactionAction, CompactionStats, StructuralCompactor, ThermalState};
pub use local_agent::{synthesize_causal_action, LocalCausalAgent};
pub use network::{NetworkAction, NetworkAgent, NetworkStats, PeerIdentity};
pub use parallel::BatchSynthesizer;
pub use validator::{
    BatchValidationResult, ConsensusValidator, TransactionAction, TransactionBatch,
};
