/// Local Causal Agent Trait
///
/// Defines the contract for all subsystems that operate as parallel observers
/// of the distinction engine, enforcing locality, causality, and determinism.
///
/// Design Principles:
/// - **Locality**: Subsystems must anchor to a local Δ root
/// - **Causality**: All state transitions follow ΔNew = ΔLocal ⊕ ΔAction
/// - **Determinism**: All action data must be canonicalizable
/// - **Concurrency**: Uses Arc<DistinctionEngine> for thread-safe shared access

use crate::primitives::Canonicalizable;
use crate::Distinction;
use std::sync::Arc;

/// Contract for subsystems that perform local causal synthesis
///
/// This trait enforces the fundamental constraints:
/// 1. Every action is anchored to a local root distinction
/// 2. New states are synthesized causally from local state + action data
/// 3. All inputs are deterministic (canonicalizable)
/// 4. Engine access is concurrent-safe via Arc
pub trait LocalCausalAgent {
    /// Action data type - must be deterministically canonicalizable
    type ActionData: Canonicalizable;

    /// Get the current local root distinction
    ///
    /// This is the subsystem's "perspective" - the distinction it considers
    /// as its current local state anchor. All synthesis builds from here.
    fn get_current_root(&self) -> &Distinction;

    /// Synthesize a new state from local root + action data
    ///
    /// Enforces the causal chain: ΔNew = ΔLocal_Root ⊕ ΔAction_Data
    ///
    /// This method:
    /// 1. Canonicalizes the action data into a distinction
    /// 2. Synthesizes local_root ⊕ action_distinction
    /// 3. Returns the new distinction representing the state transition
    ///
    /// The subsystem cannot synthesize arbitrary distinctions - it must
    /// always follow the causal chain from its local root.
    ///
    /// **Concurrency**: Takes Arc<DistinctionEngine> for thread-safe access.
    /// Multiple subsystems can synthesize concurrently using the same engine.
    fn synthesize_action(
        &mut self,
        action_data: Self::ActionData,
        engine: &Arc<crate::DistinctionEngine>,
    ) -> Distinction;

    /// Update the local root to a new distinction
    ///
    /// Moves the subsystem's "perspective" forward in the causal chain.
    /// Should only be called after validating the new root is causally
    /// connected to the previous root.
    fn update_local_root(&mut self, new_root: Distinction);
}

/// Helper function for standard causal synthesis pattern
///
/// Provides the canonical implementation that enforces:
/// ΔNew = ΔLocal_Root ⊕ ΔAction_Data
///
/// Most subsystems will use this in their synthesize_action implementation.
///
/// **Concurrency**: Thread-safe via Arc<DistinctionEngine>.
pub fn synthesize_causal_action<A: Canonicalizable>(
    local_root: &Distinction,
    action_data: A,
    engine: &Arc<crate::DistinctionEngine>,
) -> Distinction {
    // Canonicalize action data into a distinction
    let action_distinction = action_data.to_canonical_structure(engine);

    // Causal synthesis: ΔNew = ΔLocal ⊕ ΔAction
    engine.synthesize(local_root, &action_distinction)
}
