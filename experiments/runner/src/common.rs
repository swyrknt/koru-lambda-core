//! Shared helpers for experiments.

use koru_lambda_core::{Distinction, DistinctionEngine};

/// Build an engine containing at least `target` total distinctions (including
/// the two primordials) via a linear SHA256 chain. Each step produces a brand
/// new distinction because the previous step's id is itself unique.
///
/// Returns `(engine, tail_distinction)`. The tail is the last distinction
/// produced, useful for downstream experiments.
pub fn build_chain_engine(target: usize) -> (DistinctionEngine, Distinction) {
    let engine = DistinctionEngine::new();
    let d1 = engine.d1().clone();

    // Starting state: d0, d1 (2 distinctions, 1 relationship).
    let mut current = engine.synthesize(engine.d0(), &d1);
    // Now: 3 distinctions, 3 relationships.

    while engine.distinction_count() < target {
        current = engine.synthesize(&current, &d1);
    }

    (engine, current)
}

/// Build a chain that produces exactly `novel_steps` synthesis events after
/// the d0/d1 primordials. Useful for experiments that want to measure
/// per-synthesis cost independent of the primordials.
pub fn build_chain_with_log(novel_steps: usize) -> (DistinctionEngine, Vec<(String, String)>) {
    let engine = DistinctionEngine::new();
    let d1 = engine.d1().clone();
    let mut log: Vec<(String, String)> = Vec::with_capacity(novel_steps);

    let mut current = engine.d0().clone();
    for _ in 0..novel_steps {
        let a = current.to_hex();
        let b = d1.to_hex();
        log.push((a, b));
        current = engine.synthesize(&current, &d1);
    }

    (engine, log)
}
