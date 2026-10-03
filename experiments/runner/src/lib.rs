//! Reusable workload modules and probe utilities for koru-lambda-core
//! Measurement gates and scale validation.
//!
//! Coding Law workload lives here as a reusable module so scale
//! reruns the exact same code at scale instead of reimplementing it.
//! Same constants, same chain builder, same Zipf, same seed.

#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![warn(missing_docs)]

pub mod anchor_hub_probe;
pub mod coding_law_workload;
