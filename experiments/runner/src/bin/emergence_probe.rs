//! Does anything scale-invariant appear under a policy that encodes nothing?
//!
//! Run: `cargo run -p koru-experiments-runner --release --bin emergence_probe`

use koru_experiments_runner::emergence_probe::{measure, Policy};

const SEED: u64 = 0x5EED_0F1D;

fn main() {
    // 3x steps, matching the scale range over which the ~0.156 figure was
    // reported stable to three decimals.
    let scales = [500usize, 1500, 4500, 13500];

    println!("policy-free arm vs locality-biased arm");
    println!(
        "\n{:<16} {:>7} {:>7} {:>8} {:>6} {:>12} {:>10} {:>6}",
        "policy", "d", "r", "mean deg", "maxdeg", "clustering", "triangles", "unexp"
    );
    for &steps in &scales {
        for policy in [Policy::Uniform, Policy::LocalityBiased] {
            let s = measure(policy, steps, SEED);
            println!(
                "{:<16} {:>7} {:>7} {:>8.4} {:>6} {:>12.6} {:>10} {:>6}",
                format!("{:?}", s.policy),
                s.distinctions,
                s.relationships,
                s.mean_degree,
                s.max_degree,
                s.mean_clustering,
                s.triangles,
                s.unexplained_triangles
            );
            assert_eq!(
                s.relationships,
                2 * s.distinctions - 3,
                "Law 6 broken under {:?} — the harness is wrong, not the substrate",
                s.policy
            );
            assert_eq!(s.unexplained_triangles, 0, "a triangle with no self-reference witness");
        }
        println!();
    }

    // The prediction: clustering under the policy-free arm falls toward zero
    // roughly as 1/N, so the product (clustering x d) should stay bounded
    // while clustering itself does not.
    println!("policy-free arm, scaling of the clustering coefficient:");
    println!("{:>8} {:>14} {:>14}", "d", "clustering", "clustering*d");
    for &steps in &scales {
        let s = measure(Policy::Uniform, steps, SEED);
        println!(
            "{:>8} {:>14.6} {:>14.3}",
            s.distinctions,
            s.mean_clustering,
            s.mean_clustering * s.distinctions as f64
        );
    }
}
