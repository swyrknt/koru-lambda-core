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
//! ## Novelty bit
//!
//! Every synthesis is either new to this engine or a repeat. Access
//! the bit at the operator return via `synthesize_novel`:
//!
//! ```
//! use koru_lambda_core::{DistinctionEngine, SynthesisOutcome};
//! let engine = DistinctionEngine::new();
//! match engine.synthesize_novel(engine.d0(), engine.d1()) {
//!     SynthesisOutcome::Novel(d) => { /* first observation */ let _ = d; },
//!     SynthesisOutcome::Existing(d) => { /* repeat */ let _ = d; },
//!     _ => { /* `#[non_exhaustive]` — future variants land here */ },
//! }
//! ```
//!
//! The helper methods `is_novel()` and `distinction()` provide a
//! future-safe alternative — consumers who use them won't need to
//! update their code when new `SynthesisOutcome` variants land:
//!
//! ```
//! use koru_lambda_core::DistinctionEngine;
//! let engine = DistinctionEngine::new();
//! // Or use the helper methods — safe across future
//! // `#[non_exhaustive]` additions:
//! let outcome = engine.synthesize_novel(engine.d0(), engine.d1());
//! if outcome.is_novel() {
//!     // first observation
//!     let _d = outcome.distinction();
//! }
//! ```
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
    Distinction, DistinctionEngine, IdentityBuildHasher, IdentityHasher, InvariantError,
    ParentPair, SynthesisOutcome,
};
pub use primitives::{ByteMapping, Canonicalizable};
// Curated re-exports at crate root (Option B — approved). The remaining
// 12 projection items live under `koru_lambda_core::projection::*`.
pub use projection::{
    Adjacency, CoreSignal, Degree, HopDistance, Projection, ProjectionId, ProjectionSpec, Signal,
};
pub use recorder::SynthesisRecorder;
pub use replay::{build_children_index, replay_topological, ReplayError};
