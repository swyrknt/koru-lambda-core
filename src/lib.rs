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
//! - Three canonical O(1) projections: `all_distinctions`, `parents_of`,
//!   `degree_counts`.
//!
//! ## Lint floor
//!
//! `clippy::unwrap_used` is denied across the substrate. `.expect()` is
//! permitted ONLY with a message ending in `"(invariant)"` identifying
//! the load-bearing precondition; the Step 5 hygiene grep verifies this.

#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![warn(clippy::missing_const_for_fn)]
#![warn(missing_docs)]

pub mod distinction_hex;
pub mod engine;

pub use distinction_hex::ParseError;
pub use engine::{Distinction, DistinctionEngine, IdentityBuildHasher, IdentityHasher};
