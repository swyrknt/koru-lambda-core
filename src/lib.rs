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
//! - Four axioms enforced inline: determinism, commutativity, idempotence,
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
//! `docs/SPEC.md` for the authoritative spec.
//!
//! ## Trust boundaries
//!
//! [`Distinction`] values reach an engine via three paths:
//!
//! 1. [`DistinctionEngine::synthesize`] — proven by construction.
//! 2. [`DistinctionEngine::verify`] on a [`RawDistinctionId`] — proven
//!    by engine witness. **This is the recommended path** for external
//!    bytes crossing into the engine (wire deserialization, hex parse,
//!    checkpoint restore).
//!
//! There is no third way. `Distinction::from_hex`, `FromStr`, `Pod` and
//! `Zeroable` all existed and all produced one without an engine; all four
//! are gone, and `tests/trybuild/` holds each as a compile failure.
//!
//!
//! The two-type discipline: consumers write their public wire types in
//! terms of [`RawDistinctionId`] and call [`verify`](DistinctionEngine::verify)
//! at the trust boundary; internal code operates on [`Distinction`].
//! Identity is engine-witnessed, not byte-inherent — the same 16 bytes
//! are `Distinction` on engine A (which has them) and `ForeignBytes` on
//! engine B (until it replays the same synthesis history).
//!
//! The pattern applies wherever identifiers cross a trust boundary:
//! restoring a checkpoint, ingesting telemetry, accepting a handshake
//! payload, or reading an identifier typed by a human.
//!
//! ```
//! use koru_lambda_core::{DistinctionEngine, RawDistinctionId, VerifyError};
//!
//! // Downstream error type that absorbs verify + parse failures.
//! #[derive(Debug)]
//! struct MyError(&'static str);
//! impl From<VerifyError> for MyError {
//!     fn from(_: VerifyError) -> Self { MyError("foreign bytes") }
//! }
//! impl From<koru_lambda_core::ParseError> for MyError {
//!     fn from(_: koru_lambda_core::ParseError) -> Self { MyError("bad hex") }
//! }
//!
//! let engine = DistinctionEngine::new();
//! let child = engine.synthesize(engine.d0(), engine.d1());
//! let wire_hex = child.to_hex();
//!
//! // In your custom deserializer — hex parse and verify at the
//! // trust boundary, `.map_err(MyError::from)` threading each failure
//! // into your local error type:
//! let raw = RawDistinctionId::from_hex(&wire_hex).map_err(MyError::from)?;
//! let d = engine.verify(raw).map_err(MyError::from)?;
//! assert_eq!(d.as_bytes(), child.as_bytes());
//!
//! // Foreign bytes are rejected — engine-witnessed identity in action.
//! let foreign = RawDistinctionId::from_bytes([0xCC; 16]);
//! assert!(engine.verify(foreign).is_err());
//! # Ok::<(), MyError>(())
//! ```
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
//! The projection primitive replaces the ad-hoc cone iterators, hop
//! walkers, adjacency listings and seen-set trackers that consumers
//! otherwise write for themselves — each a partial, unnamed instance of
//! the same read. A consumer holding one of those should move to
//! `engine.project(...)` with the appropriate `Direction` / `Boundary` /
//! `Signal` triple, which is the whole of it rather than a case.
//!
//! ## Lint floor
//!
//! `.expect()` is permitted in production paths ONLY with a message
//! ending `"(invariant)"`; CI escalates `clippy::unwrap_used` to deny.
//! Rationale: panics in invariant-violation paths are preferable to
//! silent corruption of the append-only graph.

#![forbid(unsafe_code)]
#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::missing_const_for_fn)]
#![warn(missing_docs)]

pub mod distinction_hex;
pub mod engine;
pub mod primitives;
pub mod projection;
pub mod replay;
pub mod vectors;

pub use distinction_hex::ParseError;
pub use engine::{
    Distinction, DistinctionEngine, IdentityBuildHasher, IdentityHasher, InvariantError,
    ParentPair, RawDistinctionId, SynthesisOutcome, VerifyError,
};
pub use primitives::{domain_anchor, ByteFold, ByteMapping, Canonicalizable, DomainLabel};
// Curated re-exports at the crate root. The other 16 projection items live
// under `koru_lambda_core::projection::*`.
//
// This said "Option B — approved" and cited an approval that appears in no
// file in the repository, and "the remaining 12" when it is 16. Eleven of
// these root re-exports have no root-path consumer anywhere in the repo, four
// of them because consumers reach the module path instead; trimming them is
// tracked.
pub use projection::{
    Adjacency, CoreSignal, Degree, HopDistance, Projection, ProjectionId, ProjectionSpec, Signal,
};
pub use replay::{replay_topological, ReplayError};
