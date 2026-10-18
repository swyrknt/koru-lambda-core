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
//! # Where this trait lives, and what it is
//!
//! This trait is **one witness** of the local-causal-agent pattern, in
//! Rust. It lives here, in `koru-lambda-agents`, and the substrate does
//! not depend on it: `scripts/gates/crate_boundary.sh` asserts there is no
//! path from `koru-lambda-core` to any workspace member, and
//! `api/koru-lambda-core.txt` contains no occurrence of "agent".
//!
//! The pattern itself is described in `THEORY.md#using-the-substrate`,
//! which is explicit that it is **not a type**: "Agent-ness is therefore a
//! reading of the graph, available everywhere, and a trait asserting that
//! only some types have it is the wrong shape." So this trait is a
//! convenience for consumers who want the shape named, not the definition
//! of the shape.
//!
//! *Three claims stood here until 2026-10-18 and all three were false.*
//! It said this trait "lives at substrate level", which `crate_boundary.sh`
//! forbids; it cited `THEORY.md` §13-15, and that document's highest
//! numbered unit is Law 12 — those section numbers exist in a *different*
//! repository's theory document; and it cited "DESIGN.md Decision 2", which
//! `DESIGN.md` does not contain. None were caught by a gate, because
//! `scripts/gates/doc_anchors.sh` reads `git ls-files '*.md'` and never
//! rustdoc.
//!
//! # Not the only legal pattern
//!
//! The axioms constrain `synthesize`, not consumer shape.
//! `THEORY.md#using-the-substrate` names three legal shapes that do not
//! follow this one — multi-root consumers, consumers synthesizing from
//! non-root state, and consumers ignoring perspective entirely — and says
//! all of them "get the substrate's guarantees, because those flow from the
//! axioms and not from the consumer's shape."

use koru_lambda_core::primitives::{ByteFold, Canonicalizable};
use koru_lambda_core::{Distinction, DistinctionEngine};
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
    ///
    /// Takes the [`ByteFold`] rather than building one, because an agent
    /// acting in a loop would otherwise rebuild the 256-tip table on every
    /// action.
    #[must_use]
    fn synthesize_action(
        &mut self,
        action: Self::ActionData,
        fold: &ByteFold,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction {
        let new_root = synthesize_causal_action(self.get_current_root(), action, fold, engine);
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
    fold: &ByteFold,
    engine: &Arc<DistinctionEngine>,
) -> Distinction {
    let action_d = action.to_canonical_structure(fold, engine);
    engine.synthesize(local_root, action_d)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Minimal LCA used in tests. Real consumers
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
        let fold = ByteFold::new(&engine);

        for &action in &actions {
            let root_a = lca_a.synthesize_action(action, &fold, &engine);
            let root_b = lca_b.synthesize_action(action, &fold, &engine);
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
        let new_root = lca.synthesize_action(42, &ByteFold::new(&engine), &engine);

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

        let fold = ByteFold::new(&engine);
        for action in 1u8..=5 {
            let new_root = lca.synthesize_action(action, &fold, &engine);
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

        let fold = ByteFold::new(&engine);
        let via_helper = synthesize_causal_action(root, 42u8, &fold, &engine);
        let via_trait = lca.synthesize_action(42u8, &fold, &engine);

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
