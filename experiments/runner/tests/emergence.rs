//! Gate for the emergence result, so it cannot drift silently.
//!
//! The probe itself is a binary that prints numbers (`src/bin/emergence_probe.rs`),
//! which is right: a measurement whose outcome we did not know does not belong
//! in a suite whose job is to go red. But its *conclusion* is structural and
//! assertable, and an ungated conclusion is the pattern this release keeps
//! finding — a check that exists and does not check.
//!
//! So the numbers stay in the probe and the claim is gated here.

use koru_experiments_runner::emergence_probe::{measure, Policy};

const SEED: u64 = 0x5EED_0F1D;

/// Under a draw policy that encodes nothing, clustering falls toward zero as
/// the graph grows; under one that encodes locality, it does not.
///
/// This is the whole emergence finding: complexity does not come from
/// synthesis alone. If the policy-free arm ever shows persistent clustering,
/// that is a constant nobody installed and the framework claim gets stronger.
/// If the locality arm ever decays, the contrast was an artifact.
#[test]
fn clustering_is_the_policy_not_the_substrate() {
    let small = measure(Policy::Uniform, 500, SEED);
    let large = measure(Policy::Uniform, 4500, SEED);

    // Falls, and faster than the graph grows slower — i.e. not merely noisy.
    assert!(
        large.mean_clustering < small.mean_clustering / 3.0,
        "policy-free clustering went {:.6} -> {:.6} as d went {} -> {}. The prediction is a \
         ~1/d decay, because two distinctions drawn from a pool of size N are adjacent with \
         probability ~4/N. Persistent clustering here would be a constant nobody installed.",
        small.mean_clustering,
        large.mean_clustering,
        small.distinctions,
        large.distinctions
    );

    let local_small = measure(Policy::LocalityBiased, 500, SEED);
    let local_large = measure(Policy::LocalityBiased, 4500, SEED);
    assert!(
        (local_large.mean_clustering - local_small.mean_clustering).abs() < 0.05,
        "locality-biased clustering moved {:.6} -> {:.6}; it is supposed to be a policy \
         constant, and if it decays too then the contrast this finding rests on is an artifact",
        local_small.mean_clustering,
        local_large.mean_clustering
    );
    assert!(
        local_large.mean_clustering > 10.0 * large.mean_clustering,
        "the two arms have converged, so the policy no longer makes a difference and the \
         finding is void"
    );
}

/// Neither policy produces a hub. Independent corroboration of the Law 11
/// demotion, and of the previous release's scale-free result having been its
/// own test harness.
#[test]
fn no_policy_here_produces_a_power_law_tail() {
    for policy in [Policy::Uniform, Policy::LocalityBiased] {
        let s = measure(policy, 4500, SEED);
        // Mean degree is pinned near 4 by Law 6. A power-law tail at this
        // scale would put the maximum orders of magnitude above it.
        assert!(
            (s.max_degree as f64) < 20.0 * s.mean_degree,
            "{:?}: max degree {} against mean {:.2} — that is a tail this substrate is not \
             supposed to produce without a policy that installs it",
            policy,
            s.max_degree,
            s.mean_degree
        );
        assert_eq!(s.relationships, 2 * s.distinctions - 3, "Law 6 control");
        assert_eq!(s.unexplained_triangles, 0, "triangle census control");
    }
}
