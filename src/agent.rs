//! [`LocalCausalAgent`] — the substrate's reference consumer pattern.
//!
//! The substrate is timeless: it knows about distinctions and parents,
//! not about order, history, or perspective. Time is what consumers do.
//! The **Local Causal Agent (LCA)** pattern captures the canonical way
//! a consumer adds time to the timeless substrate:
//!
//! 1. The LCA anchors to a **local root distinction** — its perspective.
//! 2. State transitions are **causal syntheses** from `(local_root,
//!    canonical_action_data)`.
//! 3. The LCA **updates its perspective forward** as its causal chain
//!    advances.
//!
//! `LocalCausalAgent` lives at substrate level (not `subsystems/`)
//! because the trait IS the canonical consumer contract — it formalizes
//! what it means to "use" the substrate. See `THEORY.md` §13-15 and
//! DESIGN.md Decision 2.
//!
//! # Not the only legal pattern
//!
//! The axioms constrain `synthesize`, not consumer shape —
//! multi-perspective and non-root-anchored consumers can use the
//! substrate directly. LCA is the **reference** because every consumer
//! we've shipped (ALIS, koru-protocol, the reference subsystems) uses
//! it, and it captures "time is what consumers do" as a composable
//! trait.

use crate::primitives::Canonicalizable;
use crate::{Distinction, DistinctionEngine};
use std::sync::Arc;

/// The Local Causal Agent contract.
///
/// Implementers carry a local root distinction (their perspective) and
/// advance it forward via causal syntheses with action data. See module
/// docs for the full pattern.
pub trait LocalCausalAgent {
    /// The type of action data this agent processes. Must be
    /// [`Canonicalizable`] so it can be lifted into the substrate.
    type ActionData: Canonicalizable;

    /// The agent's current local root — its perspective at this moment.
    ///
    /// Returned by value because [`Distinction`] is `Copy` (16 bytes —
    /// two registers on x86-64/ARM64).
    fn get_current_root(&self) -> Distinction;

    /// Lift `action` via [`Canonicalizable`], synthesize with the
    /// current local root, advance the root forward, and return the
    /// new distinction. The default impl does exactly this via
    /// [`synthesize_causal_action`] +
    /// [`update_local_root`](LocalCausalAgent::update_local_root);
    /// override only for finer control.
    #[must_use]
    fn synthesize_action(
        &mut self,
        action: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        let new_root = synthesize_causal_action(self.get_current_root(), action, engine);
        self.update_local_root(new_root);
        new_root
    }

    /// Advance the local root forward to `new_root`. Convention (not
    /// trait-enforced): `new_root` is a distinction whose parents
    /// include the current root — i.e., the LCA stays monotonically
    /// forward in the synthesis graph.
    fn update_local_root(&mut self, new_root: Distinction);
}

/// Helper: lift `action` into the engine and synthesize it with
/// `local_root` to produce the next causal-chain distinction.
///
/// This is the building block of the LCA pattern. The default
/// `LocalCausalAgent::synthesize_action` calls this then advances the
/// agent's local root forward to the returned distinction.
///
/// Decoupled from the trait so consumers that need finer control
/// (e.g., batching actions before advancing root) can call it
/// directly.
#[must_use]
pub fn synthesize_causal_action<A: Canonicalizable>(
    local_root: Distinction,
    action: A,
    engine: &Arc<DistinctionEngine>,
) -> Distinction {
    let action_d = action.to_canonical_structure(engine);
    engine.synthesize(local_root, action_d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal LCA used in tests. Real consumers (ALIS, koru-protocol)
    /// ship richer implementations; this is just enough to exercise
    /// the trait contract.
    struct TestLca {
        root: Distinction,
    }

    impl TestLca {
        fn new(root: Distinction) -> Self {
            Self { root }
        }
    }

    impl LocalCausalAgent for TestLca {
        type ActionData = u8;

        fn get_current_root(&self) -> Distinction {
            self.root
        }

        fn update_local_root(&mut self, new_root: Distinction) {
            self.root = new_root;
        }
    }

    // ----- LCA pattern invariants (Theory §13-15) -----------------------

    #[test]
    fn lca_byte_identical_advancing_chains() {
        // Two LCAs initialized with the same local root, processing the
        // same canonical action sequence, must produce byte-identical
        // advancing root chains at every step. This is the falsifiable
        // form of "deterministic distributed state without consensus"
        // applied to consumer perspective evolution.
        let engine = Arc::new(DistinctionEngine::new());

        let initial = engine.d0();
        let mut lca_a = TestLca::new(initial);
        let mut lca_b = TestLca::new(initial);

        let actions: Vec<u8> = vec![1, 2, 3, 5, 8, 13, 21, 42, 0xFF, 0x00];

        for &action in &actions {
            let root_a = lca_a.synthesize_action(action, &engine);
            let root_b = lca_b.synthesize_action(action, &engine);
            assert_eq!(
                root_a, root_b,
                "LCA chains diverged on action {action}; substrate broke determinism"
            );
        }

        assert_eq!(lca_a.get_current_root(), lca_b.get_current_root());
    }

    #[test]
    fn lca_monotonicity_previous_root_is_parent() {
        // After an LCA advances via synthesize_action, the new root's
        // parents include the previous root. This is the structural
        // form of "advances forward in the synthesis graph" — the
        // causal chain is recorded as edges in parents_of.
        let engine = Arc::new(DistinctionEngine::new());
        let mut lca = TestLca::new(engine.d0());

        let old_root = lca.get_current_root();
        let new_root = lca.synthesize_action(42, &engine);

        assert_ne!(old_root, new_root, "advance produced same root");
        let (p1, p2) = engine
            .parents_of(new_root)
            .expect("new root is non-primordial and has parents (invariant)");
        assert!(
            p1 == old_root || p2 == old_root,
            "previous root must be one of the new root's parents (LCA forward monotonicity)"
        );
    }

    #[test]
    fn lca_chain_records_in_engine_parents_of() {
        // Five actions in sequence; verify each step's root has the
        // previous root as a parent. This is the LCA chain literally
        // existing in the engine's parents_of map.
        let engine = Arc::new(DistinctionEngine::new());
        let mut lca = TestLca::new(engine.d0());
        let mut chain = vec![lca.get_current_root()];

        for action in 1u8..=5 {
            let new_root = lca.synthesize_action(action, &engine);
            chain.push(new_root);
        }

        for window in chain.windows(2) {
            let prev = window[0];
            let curr = window[1];
            let (p1, p2) =
                engine.parents_of(curr).expect("non-primordial advance has parents (invariant)");
            assert!(p1 == prev || p2 == prev, "step {curr:?} did not record {prev:?} as a parent");
        }
    }

    #[test]
    fn synthesize_causal_action_helper_matches_trait_default() {
        // The trait's default synthesize_action calls
        // synthesize_causal_action then update_local_root. Direct call
        // to the helper must produce the same distinction.
        let engine = Arc::new(DistinctionEngine::new());
        let mut lca = TestLca::new(engine.d0());
        let root = lca.get_current_root();

        let via_helper = synthesize_causal_action(root, 42u8, &engine);
        let via_trait = lca.synthesize_action(42u8, &engine);

        assert_eq!(via_helper, via_trait);
    }

    #[test]
    fn empty_action_sequence_leaves_root_unchanged() {
        // Edge case: an LCA that never advances has its initial root
        // unchanged. (Synthesizing zero actions is a no-op on the LCA
        // and on the engine.)
        let engine = Arc::new(DistinctionEngine::new());
        let lca = TestLca::new(engine.d0());
        assert_eq!(lca.get_current_root(), engine.d0());
        // engine grew by zero (no synthesize calls made).
        assert_eq!(engine.distinction_count(), 2);
    }

    #[test]
    fn update_local_root_overrides_unconditionally() {
        // The trait contract: update_local_root takes any distinction.
        // It does NOT enforce monotonicity — that's the consumer's
        // responsibility. This test confirms the contract: passing d1
        // to an LCA rooted at d0 just sets root to d1.
        let engine = Arc::new(DistinctionEngine::new());
        let mut lca = TestLca::new(engine.d0());
        lca.update_local_root(engine.d1());
        assert_eq!(lca.get_current_root(), engine.d1());
    }
}
