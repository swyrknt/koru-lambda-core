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
//! ## Your first projection
//!
//! The projection API is the read-dual of `synthesize`. Anchor at a
//! `root`, name a `direction`, `boundary`, and `signal`, and
//! `materialize()`:
//!
//! ```
//! use koru_lambda_core::{Adjacency, DistinctionEngine};
//! use koru_lambda_core::projection::Direction;
//! let engine = DistinctionEngine::new();
//! let child = engine.synthesize(engine.d0(), engine.d1());
//! let proj = engine.project(child).direction(Direction::Upstream).hops(2)
//!     .signal(Adjacency).materialize();
//! assert!(proj.contains(&child));
//! ```
//!
//! See the [`projection`] module for the full API. See
//! `PROJECTION_SPEC.md` for the authoritative spec.
//!
//! ## Retires (six ad-hoc reinventions)
//!
//! The projection primitive replaces six domain-specific ad-hoc "field"
//! or "walk" utilities that predated E02:
//!
//! - ALIS's `Field` (custom cone iterator)
//! - koru-engine's `Field` (custom hop walker)
//! - koru-spatial's adjacency listing
//! - koru-mesh's seen-set tracking
//! - koru-delta's replay-to-point verification
//! - koru-wave's dissolve-on-read pattern
//!
//! Consumers of any of the above should migrate to `engine.project(...)`
//! with the appropriate `Direction` / `Boundary` / `Signal` triple.
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
pub mod projection;
pub mod recorder;
pub mod replay;
pub mod subsystems;

pub use agent::{synthesize_causal_action, LocalCausalAgent};
pub use distinction_hex::ParseError;
pub use engine::{
    Distinction, DistinctionEngine, IdentityBuildHasher, IdentityHasher, InvariantError, ParentPair,
};
pub use primitives::{ByteMapping, Canonicalizable};
// Curated re-exports at crate root (Option B — approved). The remaining
// 12 projection items live under `koru_lambda_core::projection::*`.
pub use projection::{
    Adjacency, CoreSignal, Degree, HopDistance, Projection, ProjectionId, ProjectionSpec, Signal,
};
pub use recorder::SynthesisRecorder;
pub use replay::{build_children_index, replay_topological, ReplayError};
