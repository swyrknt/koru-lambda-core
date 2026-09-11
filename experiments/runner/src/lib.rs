//! Reusable workload modules and probe utilities for koru-lambda-core
//! Step 1 measurement gates and Step 4 scale validation.
//!
//! Coding Law workload lives here as a reusable module so Step 4
//! reruns the exact same code at scale instead of reimplementing it.
//! Same constants, same chain builder, same Zipf, same seed.

#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![warn(missing_docs)]

pub mod coding_law_workload;
