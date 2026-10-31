//! [`Lca`] — the consumer surface, and where the force lives.
//!
//! The substrate publishes an algebra: `synthesize`, `project`, `parents_of`,
//! `verify` and the rest. It is public, and it stays public, because tests,
//! tools and conforming implementations in other languages all read it, and
//! because `docs/SPEC.md` specifies *it* and not this type.
//!
//! This crate's consumer-facing functions do not take an engine. They take an
//! `&mut Lca`. **That is the force, and it is at this boundary** — not in the
//! type system of the substrate, which has no way to express it, and not in
//! `#[doc(hidden)]`, which hides items from rustdoc without making them
//! unreachable. A consumer that wants to interoperate with the agents here
//! holds an `Lca`; one that would rather call the algebra directly still can,
//! and gets no help from this crate.
//!
//! # Why this type is here and not in the substrate
//!
//! The substrate's own membership test, recorded when the agent contract left
//! `src/`: delete a construct, and if two conforming implementations still
//! agree on every byte, it does not belong in the crate identity depends on.
//! `Lca` fails that test — delete it and nothing about any identifier
//! changes. `docs/SPEC.md` §2a is also explicit that a consumer advancing no
//! root is still conforming, so a substrate whose consumer API forced
//! advancing would be stricter than its own specification.
//!
//! # Pure wrapper
//!
//! Every method here is an algebra call plus a root assignment. It holds no
//! cache, derives nothing lazily, and registers nothing on the consumer's
//! behalf. `lca_matches_algebra` in
//! `tests/multi_agent_composition.rs` is the proof: it asserts that
//! [`Lca::advance`] and [`Lca::observe`] produce the same distinctions as the
//! equivalent direct calls, over a hundred cases. Without that test the
//! claim would be unverified, and a wrapper that diverged from the algebra
//! would be worse than no wrapper.

use crate::agent::synthesize_causal_action;
use koru_lambda_core::engine::ParentPair;
use koru_lambda_core::primitives::{ByteFold, Canonicalizable};
use koru_lambda_core::projection::{NeedsDirection, ProjectionBuilder};
use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

/// A local causal agent: one witness, one root, one discipline for advancing
/// it.
///
/// Holds the engine it is bound to, so an `Lca` is a perspective *on a
/// particular witness* and cannot be pointed at another. That is deliberate:
/// an identifier from a different engine is not comparable without `verify`,
/// and a type that let the engine vary per call would make the trust boundary
/// a convention again.
pub struct Lca {
    engine: Arc<DistinctionEngine>,
    /// Shared, because the 256-tip table is identical for every engine
    /// (content addressing) and N agents holding N copies is waste. The
    /// trait's `synthesize_action` takes the fold by reference for the same
    /// reason — an agent in a loop must not rebuild it per action.
    fold: Arc<ByteFold>,
    root: Distinction,
}

impl Lca {
    /// Bind to `engine`, anchored at `seed`, building this agent's fold.
    ///
    /// Use [`Lca::with_fold`] when several agents share one engine, so the
    /// tip table is built once rather than once per agent.
    ///
    /// # Panics
    ///
    /// If `seed` is not a distinction this engine has witnessed. A root the
    /// witness does not hold is a programmer error, not a runtime condition
    /// (`docs/SPEC.md` §3), and catching it at construction is better than
    /// catching it on the first advance.
    #[must_use]
    pub fn new(engine: &Arc<DistinctionEngine>, seed: Distinction) -> Self {
        let fold = Arc::new(ByteFold::new(engine));
        Self::with_fold(engine, fold, seed)
    }

    /// Bind to `engine` with a fold built elsewhere.
    ///
    /// # Panics
    ///
    /// If `seed` is not witnessed by `engine` — see [`Lca::new`].
    #[must_use]
    pub fn with_fold(
        engine: &Arc<DistinctionEngine>,
        fold: Arc<ByteFold>,
        seed: Distinction,
    ) -> Self {
        assert!(
            engine.has(seed),
            "Lca::with_fold: the seed is not registered in this engine — it was \
             not produced by this engine's operator. Cross-engine values and \
             unverified bytes must pass through `verify` first."
        );
        Self { engine: Arc::clone(engine), fold, root: seed }
    }

    /// Lift `action` and synthesize it with the current root, advancing
    /// forward. Returns the new root.
    ///
    /// This is the LCA discipline in one call: `synthesize(root,
    /// canonical(action))`, then replace the root with the result. The
    /// previous root is a parent of the new one, so the step is recoverable
    /// from parentage (`docs/SPEC.md` §2a).
    pub fn advance<A: Canonicalizable>(&mut self, action: A) -> Distinction {
        let next = synthesize_causal_action(self.root, action, &self.fold, &self.engine);
        self.root = next;
        next
    }

    /// Synthesize the current root with a distinction this engine already
    /// holds, advancing forward. Returns the new root.
    ///
    /// # This is hand-off, and it is not `advance`
    ///
    /// `observe` does **not** lift its argument. `advance` folds an action's
    /// bytes into a new node and synthesizes with *that*; if `observe` did
    /// the same, it would fold `other`'s sixteen bytes as a payload and the
    /// resulting node's parent would be the lift — **not `other`**. Then
    /// `parents_of(result)` would not contain `other`, and the observation
    /// would be unrecoverable from the graph, which is the one thing it
    /// exists to be.
    ///
    /// So this synthesizes directly, which makes it exactly the hand-off that
    /// `agent_handoff_needs_no_protocol` tests: one agent's output becomes a
    /// parent of another's next state. The observed party is unaffected — its
    /// own root does not move, and it can advance independently afterwards.
    ///
    /// # Panics
    ///
    /// If `other` is not witnessed by this engine.
    pub fn observe(&mut self, other: Distinction) -> Distinction {
        let next = self.engine.synthesize(self.root, other);
        self.root = next;
        next
    }

    /// The current root — this agent's perspective.
    #[must_use]
    pub const fn root(&self) -> Distinction {
        self.root
    }

    /// Replace the root unconditionally.
    ///
    /// The LCA convention is that a new root carries the old one among its
    /// parents, and this method does **not** enforce it — the same unchecked
    /// contract [`crate::agent::LocalCausalAgent::update_local_root`] carries,
    /// for the same reason: a consumer restoring a checkpoint is moving
    /// sideways on purpose, and a type that forbade it would make replay
    /// impossible. Whether a given move was forward is a question for the
    /// graph: compare `parents_of(new)` against the old root.
    pub fn restore_root(&mut self, root: Distinction) {
        self.root = root;
    }

    /// Every ancestor of the current root, nearest first.
    ///
    /// Breadth-first over `parents_of`, deduplicated, terminating at the
    /// primordials. Cost is O(cone) in the ancestry's size — this walks the
    /// graph rather than reading a cached field, because nothing is cached.
    /// For a long chain that is the whole chain; call it at a quiescent
    /// moment, not inside a loop.
    #[must_use]
    pub fn ancestry(&self) -> Vec<Distinction> {
        let mut seen = HashSet::new();
        let mut order = Vec::new();
        let mut queue = VecDeque::from([self.root]);
        while let Some(d) = queue.pop_front() {
            if let Some((a, b)) = self.engine.parents_of(d) {
                for parent in [a, b] {
                    if seen.insert(parent) {
                        order.push(parent);
                        queue.push_back(parent);
                    }
                }
            }
        }
        order
    }

    /// The root's parents, or `None` if it is a primordial.
    #[must_use]
    pub fn parents(&self) -> Option<ParentPair> {
        self.engine.parents_of(self.root)
    }

    /// The root's degree — incident edges, per `docs/SPEC.md` §6.1.
    #[must_use]
    pub fn degree(&self) -> usize {
        self.engine.degree(self.root)
    }

    /// Begin a projection rooted at this agent's perspective.
    ///
    /// Root-scoped by construction: an `Lca` projects from where it stands,
    /// which is the common case and removes the chance of passing a root the
    /// engine has never witnessed. To project from somewhere else, use the
    /// algebra directly — that is a different question than "what can I see
    /// from here".
    pub fn project(&self) -> ProjectionBuilder<'_, NeedsDirection> {
        self.engine.project(self.root)
    }

    /// The witness this agent is bound to.
    ///
    /// Reads on the shared graph — `has`, `parents_of` on another node,
    /// `snapshot_*` — are legitimate consumer operations and there is no
    /// reason to re-export each one. Advancing still goes through
    /// [`Lca::advance`] and [`Lca::observe`], which is where the discipline
    /// is; this accessor hands out `&Arc<DistinctionEngine>` so a consumer
    /// can read, and reading changes nothing.
    #[must_use]
    pub const fn engine(&self) -> &Arc<DistinctionEngine> {
        &self.engine
    }

    /// This agent's fold, for handing to another `Lca` via
    /// [`Lca::with_fold`].
    #[must_use]
    pub const fn fold(&self) -> &Arc<ByteFold> {
        &self.fold
    }
}

impl core::fmt::Debug for Lca {
    /// Prints the root only. The engine is shared and its `Debug` is a
    /// whole-graph summary, which is not what a reader of an agent wants.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Lca").field("root", &self.root).finish_non_exhaustive()
    }
}
