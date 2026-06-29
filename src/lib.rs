//! # koru-lambda-core
//!
//! Minimal axiomatic substrate implementing distinction theory.
//! See `THEORY.md` for the theory, `ARCHITECTURE.md` for the code structure,
//! and `DESIGN.md` for the v2.0 plan.
//!
//! ## Substrate at a glance
//!
//! - One operator: `DistinctionEngine::synthesize`
//! - Two primordials: `engine.d0()`, `engine.d1()`
//! - Four axioms enforced inline: determinism, commutativity, irreflexivity,
//!   content addressing.
//! - Three canonical O(1) projections — saturation check, parent
//!   lookup, degree query — backed by a single `nodes` map of
//!   `<id, EngineNode { parents, degree }>`.
//!
//! ## Lint floor
//!
//! `.expect()` is permitted in production paths ONLY with a message
//! ending `"(invariant)"`; CI escalates `clippy::unwrap_used` to deny.
//! Rationale: panics in invariant-violation paths are preferable to
//! silent corruption of the append-only graph.

#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::missing_const_for_fn)]
#![warn(missing_docs)]

pub mod agent;
pub mod distinction_hex;
pub mod engine;
pub mod primitives;
pub mod recorder;
pub mod replay;

pub use agent::{synthesize_causal_action, LocalCausalAgent};
pub use distinction_hex::ParseError;
pub use engine::{
    Distinction, DistinctionEngine, IdentityBuildHasher, IdentityHasher, InvariantError, ParentPair,
};
pub use primitives::{ByteMapping, Canonicalizable};
pub use recorder::SynthesisRecorder;
pub use replay::{build_children_index, replay_topological, ReplayError};
