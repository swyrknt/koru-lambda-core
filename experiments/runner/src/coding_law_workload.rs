//! Coding Law workload — Spearman ρ between Zipf-drawn pair frequency
//! and engine-observed degree delta on a fixed chain pool.
//!
//! Implements the workload specified at `CHECKLIST.md` line 133:
//!
//! - Chain pool of `N` distinctions: `pool[0] = d0`, `pool[1] = d1`,
//!   `pool[k] = synthesize(pool[k-1], pool[k-2])` for `k >= 2`.
//! - `M` parent pairs sampled from `pool` via Zipf(alpha) on the
//!   chain index, with both indices distinct (irreflexive).
//! - For each pair `(i, j)`: synthesize `pool[i]`, `pool[j]`; record
//!   that both pool[i] and pool[j] were drawn (i.e. `freq[i] += 1`,
//!   `freq[j] += 1`).
//! - After all M syntheses: compute Spearman ρ between `freq[k]` and
//!   `degree_after[k] − degree_before[k]` across all k.
//!
//! Coding Law (Law 12): a distinction's degree is correlated with how
//! often it participates as a parent. The Zipf workload provides the
//! ground-truth participation distribution; the engine's degree counter
//! is what we measure. ρ ≈ 1 means the engine correctly accounts for
//! every participation; ρ << 1 means the substrate is losing degree
//! signal somewhere.
//!
//! The corpus pinning lives in the parent crate's `tests/corpora/`
//! (binaries under `src/bin/` generate it; tests under `tests/` verify
//! integrity and replay). See `src/bin/gen_exp18_corpus.rs` for the
//! generator and `tests/coding_law.rs` (in the main crate) for the
//! gate test.

use koru_lambda_core::{Distinction, DistinctionEngine};
use rand::distributions::{Distribution, WeightedIndex};
use rand::rngs::StdRng;
use rand::SeedableRng;

/// Reusable Coding Law workload + corpus payload.
///
/// Construct once via [`Self::generate`] (drives the engine to build
/// the pool and Zipf-sample the pairs), then either:
/// - Serialize to a pinned corpus via [`Self::write_log`] and
///   [`Self::write_freq`], OR
/// - Replay against a fresh engine via [`Self::compute_rho`] to verify
///   Spearman ρ.
pub struct CodingLawWorkload {
    /// Chain pool of N distinctions, in chain order.
    pub pool: Vec<Distinction>,
    /// `freq[k]` = number of times `pool[k]` was sampled as a parent
    /// (in either position) across the M pair draws. Each pair
    /// contributes +1 to both endpoints' freq.
    pub freq: Vec<u32>,
    /// The M (i, j) pair indices into `pool`. Stored as u32 because
    /// N ≤ 4096 in the canonical exp18 instantiation and corpus
    /// compactness matters for repo size.
    pub pairs: Vec<(u32, u32)>,
}

impl CodingLawWorkload {
    /// Generate the workload by driving `engine` to build the chain
    /// pool, then Zipf-sampling M pairs with `seed` and `alpha`.
    ///
    /// Postconditions:
    /// - `pool.len() == n`
    /// - `freq.len() == n`
    /// - `pairs.len() == m`
    /// - Every `pair.0 != pair.1` (irreflexive)
    /// - `sum(freq) == 2 * m` (each pair contributes +1 to two pool slots)
    ///
    /// # Panics
    ///
    /// Panics if `n < 2` (the chain pool requires both primordials)
    /// or if `WeightedIndex` rejects the weight vector (zero or NaN
    /// weights — should not happen for finite positive `alpha`).
    #[must_use]
    pub fn generate(engine: &DistinctionEngine, alpha: f64, seed: u64, n: usize, m: usize) -> Self {
        assert!(n >= 2, "workload requires N >= 2 (d0, d1)");

        let mut pool = Vec::with_capacity(n);
        pool.push(engine.d0());
        pool.push(engine.d1());
        for k in 2..n {
            let next = engine.synthesize(pool[k - 1], pool[k - 2]);
            pool.push(next);
        }

        // Zipf(alpha) weights on the chain index: weight[k] = 1/(k+1)^alpha.
        // For alpha = 1.0, this is the canonical Zipf law on rank.
        let weights: Vec<f64> = (0..n).map(|k| 1.0 / ((k + 1) as f64).powf(alpha)).collect();
        let dist = WeightedIndex::new(&weights).expect("zipf weights valid (invariant: alpha > 0)");

        // Deterministic RNG seeded from the supplied seed.
        let mut rng = StdRng::seed_from_u64(seed);

        let mut freq = vec![0u32; n];
        let mut pairs = Vec::with_capacity(m);
        for _ in 0..m {
            let i = dist.sample(&mut rng);
            // Irreflexive: redraw j until j != i. With Zipf(1) over
            // 4096 entries the expected redraws per pair is < 0.01.
            let mut j = dist.sample(&mut rng);
            while j == i {
                j = dist.sample(&mut rng);
            }
            freq[i] += 1;
            freq[j] += 1;
            pairs.push((i as u32, j as u32));
        }

        Self { pool, freq, pairs }
    }

    /// Run all pairs through `engine.synthesize` and return the
    /// degree delta `degree_after[k] − degree_before[k]` for each k.
    ///
    /// The returned vector has the same length as `self.pool`.
    /// Caller compares it against `self.freq` via Spearman ρ (see
    /// [`spearman_rho`]).
    #[must_use]
    pub fn run_against(&self, engine: &DistinctionEngine) -> Vec<i64> {
        let degree_before: Vec<usize> = self.pool.iter().map(|d| engine.degree(*d)).collect();

        for (i, j) in &self.pairs {
            // generate() guarantees i, j ∈ 0..pool.len() — direct index is safe.
            let _ = engine.synthesize(self.pool[*i as usize], self.pool[*j as usize]);
        }

        self.pool
            .iter()
            .enumerate()
            .map(|(k, d)| engine.degree(*d) as i64 - degree_before[k] as i64)
            .collect()
    }

    /// Convenience wrapper: run the workload and return Spearman ρ.
    #[must_use]
    pub fn compute_rho(&self, engine: &DistinctionEngine) -> f64 {
        let delta = self.run_against(engine);
        spearman_rho(&self.freq, &delta)
    }

    /// Write the pair log as binary: each entry is 32 bytes, the
    /// canonical `(min, max)` byte pair (`min || max`). Length =
    /// `pairs.len() * 32`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the writer fails.
    pub fn write_log<W: std::io::Write>(&self, mut w: W) -> std::io::Result<()> {
        for (i, j) in &self.pairs {
            let a = self.pool[*i as usize].as_bytes();
            let b = self.pool[*j as usize].as_bytes();
            let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
            w.write_all(lo)?;
            w.write_all(hi)?;
        }
        Ok(())
    }

    /// Write the freq array as binary: `N` u32 little-endian values.
    /// Length = `freq.len() * 4`.
    ///
    /// # Errors
    ///
    /// Returns an `io::Error` if the writer fails.
    pub fn write_freq<W: std::io::Write>(&self, mut w: W) -> std::io::Result<()> {
        for f in &self.freq {
            w.write_all(&f.to_le_bytes())?;
        }
        Ok(())
    }
}

/// Spearman rank correlation between two arrays of equal length.
///
/// Returns ρ in `[-1.0, 1.0]`. Computes by ranking each input (average
/// ranks for ties), then Pearson correlation on the ranks.
///
/// # Panics
///
/// Panics if `xs.len() != ys.len()` or if either is empty.
#[must_use]
pub fn spearman_rho<X: PartialOrd + Copy, Y: PartialOrd + Copy>(xs: &[X], ys: &[Y]) -> f64 {
    assert_eq!(xs.len(), ys.len(), "spearman_rho: input length mismatch");
    let n = xs.len();
    assert!(n > 1, "spearman_rho: needs n > 1");

    let xr = rank(xs);
    let yr = rank(ys);

    // Pearson on ranks. With average-rank ties, this is the standard
    // Spearman ρ formula.
    let mean_x: f64 = xr.iter().sum::<f64>() / n as f64;
    let mean_y: f64 = yr.iter().sum::<f64>() / n as f64;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for k in 0..n {
        let dx = xr[k] - mean_x;
        let dy = yr[k] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }

    if var_x == 0.0 || var_y == 0.0 {
        // Degenerate input: all values equal in one array. ρ is
        // undefined; convention: return 0.0 (no correlation).
        return 0.0;
    }

    cov / (var_x.sqrt() * var_y.sqrt())
}

/// Average-rank tied ranking of `values`. Returns a `Vec<f64>` of
/// length `values.len()` where `ranks[k]` is the average rank of
/// `values[k]` in the sorted order.
fn rank<T: PartialOrd + Copy>(values: &[T]) -> Vec<f64> {
    let n = values.len();
    // Index permutation sorted by value (stable for tie handling).
    let mut idx: Vec<usize> = (0..n).collect();
    idx.sort_by(|&a, &b| {
        values[a]
            .partial_cmp(&values[b])
            .unwrap_or_else(|| panic!("rank() received NaN; ρ undefined"))
    });

    let mut ranks = vec![0.0; n];
    let mut k = 0;
    while k < n {
        // Find run of ties starting at k.
        let mut j = k + 1;
        while j < n
            && values[idx[j]].partial_cmp(&values[idx[k]]) == Some(std::cmp::Ordering::Equal)
        {
            j += 1;
        }
        // Average rank for the tie group is (k+1 + j) / 2 (1-indexed
        // average; we use 1-indexed ranks throughout).
        let avg_rank = (k + j + 1) as f64 / 2.0;
        for t in k..j {
            ranks[idx[t]] = avg_rank;
        }
        k = j;
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spearman_perfect_positive_is_one() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        let rho = spearman_rho(&xs, &ys);
        assert!((rho - 1.0).abs() < 1e-9, "expected 1.0, got {rho}");
    }

    #[test]
    fn spearman_perfect_negative_is_minus_one() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![50.0, 40.0, 30.0, 20.0, 10.0];
        let rho = spearman_rho(&xs, &ys);
        assert!((rho - -1.0).abs() < 1e-9, "expected -1.0, got {rho}");
    }

    #[test]
    fn spearman_uncorrelated_near_zero() {
        // Independent permutation; should hover near 0.
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let ys = vec![3.0, 1.0, 4.0, 6.0, 2.0, 5.0];
        let rho = spearman_rho(&xs, &ys);
        assert!(rho.abs() < 0.5, "expected near 0, got {rho}");
    }

    #[test]
    fn spearman_ties_average_rank() {
        let xs = vec![1.0, 2.0, 2.0, 3.0];
        let ys = vec![10.0, 20.0, 20.0, 30.0];
        let rho = spearman_rho(&xs, &ys);
        assert!((rho - 1.0).abs() < 1e-9, "expected 1.0 with ties, got {rho}");
    }

    #[test]
    fn workload_generates_expected_shape() {
        let engine = DistinctionEngine::new();
        let n = 64;
        let m = 256;
        let w = CodingLawWorkload::generate(&engine, 1.0, 0xC0DE, n, m);
        assert_eq!(w.pool.len(), n);
        assert_eq!(w.freq.len(), n);
        assert_eq!(w.pairs.len(), m);
        let freq_sum: u32 = w.freq.iter().sum();
        assert_eq!(freq_sum, 2 * m as u32, "each pair contributes +2 to freq sum");
        // Every pair is irreflexive.
        for (i, j) in &w.pairs {
            assert_ne!(i, j, "pair indices must differ");
        }
    }

    #[test]
    fn workload_rho_is_high_on_freshly_used_pool() {
        // A fresh engine sees no prior participation; degree delta
        // tracks freq tightly. ρ should be near 1.0.
        let workload_engine = DistinctionEngine::new();
        let n = 64;
        let m = 1024;
        let w = CodingLawWorkload::generate(&workload_engine, 1.0, 0xC0DE, n, m);
        // Build a fresh engine with the SAME pool (chain extension is
        // deterministic by Axiom 1), then run pairs against it.
        let rerun_engine = DistinctionEngine::new();
        for k in 2..n {
            let _ = rerun_engine.synthesize(w.pool[k - 1], w.pool[k - 2]);
        }
        let rho = w.compute_rho(&rerun_engine);
        assert!(rho >= 0.95, "small-scale ρ should clear 0.95, got {rho}");
    }
}
