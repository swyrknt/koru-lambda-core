//! Reusable workload modules and probe utilities for koru-lambda-core
//! Step 1 measurement gates and Step 4 scale validation.
//!
//! Per `CHECKLIST.md` line 133: Step 1 produces the Coding Law workload
//! as a reusable module here so Step 4 reruns the exact same code at
//! scale instead of reimplementing it. Same constants, same chain
//! builder, same Zipf, same seed.

#![warn(clippy::unwrap_used)]
#![warn(clippy::must_use_candidate)]
#![warn(missing_docs)]

pub mod coding_law_workload;
