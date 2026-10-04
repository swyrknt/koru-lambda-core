//! Controls for the Coding Law ρ measurement.
//!
//! The gate asserts Spearman ρ ≥ 0.985 between a distinction's draw frequency
//! and its degree delta, and the theory presents ρ ≈ 0.99 as a *measured,
//! workload-conditional* result. But degree is incremented by the very draws
//! frequency counts, so a high ρ might be close to arithmetically forced — in
//! which case the gate asserts that a counter counts, and the theory is
//! claiming more than the number supports.
//!
//! The previous release shipped exactly this defect in a different place: a
//! correlation reported with no null model, on two series that shared a
//! monotone trend. It passed 96% of the time against the hypothesis it claimed
//! to reject. The execution here is far better, but a correlation with no
//! control is the same shape of claim, so it gets controls.
//!
//! Two of them, and the second is the one that matters:
//!
//! 1. **The null.** Hold the degree deltas fixed and permute the frequency
//!    vector. This destroys the pairing while preserving both marginal
//!    distributions, so it answers "what ρ does a non-relationship give here?"
//!    A near-zero answer means the statistic has content.
//!
//! 2. **The stated mechanism.** The theory does not merely assert ρ ≈ 0.99 —
//!    it explains why ρ < 1, and names saturation drift as the cause: "under
//!    heavy saturation ρ drops below 1 … under low saturation ρ → 1". That is
//!    a falsifiable prediction about how ρ should *move*, and it is stronger
//!    evidence than any single value. If ρ does not track the saturation rate,
//!    the theory's account of its own headline number is wrong, whatever the
//!    number happens to be.

use koru_lambda_core::DistinctionEngine;

use crate::coding_law_workload::{spearman_rho, CodingLawWorkload};

/// What ρ looks like when the pairing is destroyed but both marginals are kept.
#[derive(Debug, Clone, Copy)]
pub struct NullStats {
    /// How many permutations were drawn.
    pub trials: usize,
    /// Mean ρ across permutations. Should sit at zero.
    pub mean: f64,
    /// Standard deviation across permutations — the noise floor.
    pub sd: f64,
    /// Largest |ρ| observed. This is the number that matters: if it reaches
    /// the gate's threshold, the gate is clearable without any relationship.
    pub max_abs: f64,
}

/// Permute `freq` against a fixed `delta`, `trials` times.
///
/// A small LCG rather than `rand`: this must not acquire a dependency whose
/// stream could shift under it, for the same reason the corpus pin is exact.
#[must_use]
pub fn shuffled_null(freq: &[u32], delta: &[i64], trials: usize, seed: u64) -> NullStats {
    assert_eq!(freq.len(), delta.len(), "freq and delta must be the same length");
    let mut state = seed;
    let mut next = || {
        state =
            state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };

    let mut rhos = Vec::with_capacity(trials);
    let mut scratch: Vec<u32> = freq.to_vec();
    for _ in 0..trials {
        // Fisher-Yates over the frequency vector only.
        for i in (1..scratch.len()).rev() {
            scratch.swap(i, next() % (i + 1));
        }
        rhos.push(spearman_rho(&scratch, delta));
    }

    let mean = rhos.iter().sum::<f64>() / rhos.len() as f64;
    let var = rhos.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / rhos.len() as f64;
    let max_abs = rhos.iter().fold(0.0f64, |m, r| m.max(r.abs()));
    NullStats { trials, mean, sd: var.sqrt(), max_abs }
}

/// A workload shape: pool size, number of draws, Zipf exponent.
pub type WorkloadShape = (usize, usize, f64);

/// One point in the saturation sweep.
#[derive(Debug, Clone, Copy)]
pub struct SweepPoint {
    /// Size of the chain pool drawn from.
    pub pool: usize,
    /// Number of synthesis calls made.
    pub draws: usize,
    /// Zipf exponent of the index draw.
    pub alpha: f64,
    /// Fraction of synthesis calls that produced nothing new. Each novel
    /// synthesis credits two parents, so `sum(delta) / 2` recovers the novel
    /// count from data the harness already returns.
    pub saturation: f64,
    /// Spearman ρ between draw frequency and degree delta at this shape.
    pub rho: f64,
}

/// Measure ρ and the saturation rate across workload shapes.
///
/// The theory predicts these move in opposite directions. Nothing in the
/// construction forces that: ρ is computed from the engine's degree
/// bookkeeping, and the saturation rate from how often the engine declined to
/// create anything. A substrate that mis-credited degree would break the
/// relationship without changing the saturation rate at all.
#[must_use]
pub fn saturation_sweep(shapes: &[WorkloadShape], seed: u64) -> Vec<SweepPoint> {
    shapes
        .iter()
        .map(|&(pool, draws, alpha)| {
            let engine = DistinctionEngine::new();
            let workload = CodingLawWorkload::generate(&engine, alpha, seed, pool, draws);
            let delta = workload.run_against(&engine);
            let novel = delta.iter().sum::<i64>() as f64 / 2.0;
            let saturation = 1.0 - (novel / draws as f64);
            SweepPoint { pool, draws, alpha, saturation, rho: spearman_rho(&workload.freq, &delta) }
        })
        .collect()
}

/// Spearman ρ between the sweep's saturation rates and its ρ values.
///
/// The theory's account requires this to be negative. A value near zero would
/// mean saturation does not explain the shortfall, and the explanation in
/// `THEORY.md` would need replacing regardless of what ρ itself measures.
#[must_use]
pub fn mechanism_correlation(points: &[SweepPoint]) -> f64 {
    let sat: Vec<f64> = points.iter().map(|p| p.saturation).collect();
    let rho: Vec<f64> = points.iter().map(|p| p.rho).collect();
    spearman_rho(&sat, &rho)
}
