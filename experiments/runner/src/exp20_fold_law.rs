//! Experiment 20: Validate the Fold Law mechanism.
//!
//! Claim (CLAUDE.md):
//!   "Fold Law (seed layer, depth <= 8): d0/d1 become mega-hubs by topological
//!    necessity -- they are on every derivation path because ByteMapping routes
//!    every byte through them 8 times."
//!
//! Exp 4 confirmed deg(d0) ~ 0.10 * N at large scale. Exp 20 isolates WHY:
//! is it the ByteMapping 8-step fold specifically, or general content
//! concentration?
//!
//! Method:
//! - CONTROL: a Fibonacci-style chain that touches d0/d1 only at the root.
//!   M syntheses. Measure deg(d0) and deg(d1) after.
//! - TREATMENT: replicate ByteMapping's 8-step MSB-first fold (mirrors
//!   primitives.rs:11-22) but apply it to the user's engine (NOT the
//!   throwaway engine the cache uses). Process all 256 distinct bytes.
//!   Each byte folds through d0/d1 8 times.
//! - Compare deg(d0) and deg(d1) in each. Treatment must be much larger.
//!
//! Also: pick a non-primordial node at depth 5 in the treatment graph.
//! Its degree should stay small (only seed nodes scale).
//!
//! Note: we replicate the fold by hand rather than calling
//! `ByteMapping::map_byte_to_distinction` because that uses a throwaway
//! engine for its cache (TODO #3 phantom-parent bug). Manual replication
//! mutates the engine under test.

use std::time::Instant;

use koru_lambda_core::{Distinction, DistinctionEngine};

/// Mirror of `primitives.rs:compute_byte_distinction_uncached` but parameterized
/// to record every intermediate synthesis. Each byte goes through 8 syntheses,
/// MSB-first. Starts from d0 (which is also the bit_d for bit==0).
fn fold_byte_into_engine(byte: u8, engine: &DistinctionEngine) -> Vec<Distinction> {
    let mut intermediates = Vec::with_capacity(8);
    let mut current = engine.d0().clone();
    for i in (0..8).rev() {
        let bit = (byte >> i) & 1;
        let bit_d = if bit == 1 {
            engine.d1().clone()
        } else {
            engine.d0().clone()
        };
        current = engine.synthesize(&current, &bit_d);
        intermediates.push(current.clone());
    }
    intermediates
}

fn deg_of(engine: &DistinctionEngine, d: &Distinction) -> u32 {
    let mut count = 0u32;
    for (a, b) in engine.get_relationships_snapshot() {
        if a == *d.id() || b == *d.id() {
            count += 1;
        }
    }
    count
}

struct PhaseResult {
    label: &'static str,
    syntheses_total: usize,
    distinctions_added: usize,
    relationships_added: usize,
    deg_d0: u32,
    deg_d1: u32,
    deg_random_interior: Option<u32>,
}

fn run_control_fibonacci(m: usize) -> PhaseResult {
    println!("\n## CONTROL: Fibonacci-style chain (no ByteMapping)");
    println!("        a_2 = synth(d0, d1); a_{{i+2}} = synth(a_i, a_{{i+1}}).");

    let engine = DistinctionEngine::new();
    let d_before = engine.distinction_count();
    let r_before = engine.relationship_count();
    let deg0_before = deg_of(&engine, engine.d0());
    let deg1_before = deg_of(&engine, engine.d1());

    let t0 = Instant::now();
    let mut a = engine.synthesize(engine.d0(), engine.d1());
    let mut b = engine.synthesize(&a, engine.d1());
    let mut syntheses = 2usize;
    let mut interior_at_depth_5: Option<Distinction> = None;
    let mut depth = 2usize;
    while syntheses < m {
        let c = engine.synthesize(&a, &b);
        if depth == 5 && interior_at_depth_5.is_none() {
            interior_at_depth_5 = Some(c.clone());
        }
        a = b;
        b = c;
        syntheses += 1;
        depth += 1;
    }
    let secs = t0.elapsed().as_secs_f64();

    let d_after = engine.distinction_count();
    let r_after = engine.relationship_count();
    let deg0_after = deg_of(&engine, engine.d0());
    let deg1_after = deg_of(&engine, engine.d1());
    let random_deg = interior_at_depth_5.as_ref().map(|d| deg_of(&engine, d));

    println!(
        "  syntheses = {}  secs = {:.3}  ({:.0} ops/sec)",
        syntheses,
        secs,
        syntheses as f64 / secs
    );
    println!(
        "  engine grew: d {} -> {} ({}); r {} -> {} ({})",
        d_before,
        d_after,
        d_after - d_before,
        r_before,
        r_after,
        r_after - r_before
    );
    println!(
        "  deg(d0): {} -> {} (delta {})",
        deg0_before,
        deg0_after,
        deg0_after - deg0_before
    );
    println!(
        "  deg(d1): {} -> {} (delta {})",
        deg1_before,
        deg1_after,
        deg1_after - deg1_before
    );
    if let Some(d) = random_deg {
        println!("  deg(depth-5 interior node): {}", d);
    }
    PhaseResult {
        label: "control_fibonacci",
        syntheses_total: syntheses,
        distinctions_added: d_after - d_before,
        relationships_added: r_after - r_before,
        deg_d0: deg0_after - deg0_before,
        deg_d1: deg1_after - deg1_before,
        deg_random_interior: random_deg,
    }
}

fn run_treatment_bytes() -> PhaseResult {
    println!("\n## TREATMENT: ByteMapping 8-step MSB fold for all 256 distinct bytes");
    println!("        Mirrors primitives.rs:compute_byte_distinction_uncached,");
    println!("        but applied to the engine under test (avoids phantom-parent bug).");

    let engine = DistinctionEngine::new();
    let d_before = engine.distinction_count();
    let r_before = engine.relationship_count();
    let deg0_before = deg_of(&engine, engine.d0());
    let deg1_before = deg_of(&engine, engine.d1());

    let t0 = Instant::now();
    let mut all_intermediates: Vec<Vec<Distinction>> = Vec::with_capacity(256);
    let mut total_syntheses = 0usize;
    for byte in 0u8..=255 {
        let before = engine.distinction_count();
        let interm = fold_byte_into_engine(byte, &engine);
        let after = engine.distinction_count();
        total_syntheses += 8;
        let _ = (before, after);
        all_intermediates.push(interm);
    }
    let secs = t0.elapsed().as_secs_f64();

    let d_after = engine.distinction_count();
    let r_after = engine.relationship_count();
    let deg0_after = deg_of(&engine, engine.d0());
    let deg1_after = deg_of(&engine, engine.d1());

    let interior_at_depth_5 = all_intermediates
        .get(42)
        .and_then(|chain| chain.get(4))
        .cloned();
    let random_deg = interior_at_depth_5.as_ref().map(|d| deg_of(&engine, d));

    println!(
        "  syntheses attempted = {}  (saturated = {})  secs = {:.3}  ({:.0} ops/sec)",
        total_syntheses,
        total_syntheses as i64 - (d_after - d_before) as i64,
        secs,
        total_syntheses as f64 / secs
    );
    println!(
        "  engine grew: d {} -> {} ({}); r {} -> {} ({})",
        d_before,
        d_after,
        d_after - d_before,
        r_before,
        r_after,
        r_after - r_before
    );
    println!(
        "  deg(d0): {} -> {} (delta {})",
        deg0_before,
        deg0_after,
        deg0_after - deg0_before
    );
    println!(
        "  deg(d1): {} -> {} (delta {})",
        deg1_before,
        deg1_after,
        deg1_after - deg1_before
    );
    if let Some(d) = random_deg {
        println!(
            "  deg(byte 0x2A interior at step 5): {} (expected small, ~2)",
            d
        );
    }
    PhaseResult {
        label: "treatment_bytes",
        syntheses_total: total_syntheses,
        distinctions_added: d_after - d_before,
        relationships_added: r_after - r_before,
        deg_d0: deg0_after - deg0_before,
        deg_d1: deg1_after - deg1_before,
        deg_random_interior: random_deg,
    }
}

fn run_treatment_scaling(byte_counts: &[usize]) {
    println!("\n## TREATMENT SCALING: deg(d0) / N as N varies");
    println!("        N here = bytes ingested (with possible repeats forcing saturation).");

    for &n in byte_counts {
        let engine = DistinctionEngine::new();
        let mut total = 0usize;
        for k in 0..n {
            let byte = (k % 256) as u8;
            let _ = fold_byte_into_engine(byte, &engine);
            total += 8;
        }
        let deg0 = deg_of(&engine, engine.d0());
        let deg1 = deg_of(&engine, engine.d1());
        let d = engine.distinction_count();
        let r = engine.relationship_count();
        println!(
            "  N = {:>6}  syntheses_attempted = {:>7}  d = {:>6}  r = {:>6}  deg(d0) = {:>6}  deg(d1) = {:>6}  ratio = {:.4}",
            n,
            total,
            d,
            r,
            deg0,
            deg1,
            (deg0 + deg1) as f64 / total as f64
        );
    }
    println!(
        "  After saturation (>=256 distinct bytes), deg(d0)+deg(d1) plateaus at the"
    );
    println!(
        "  total distinct-byte fold size (~510) because repeated bytes are idempotent."
    );
}

fn summary(control: &PhaseResult, treatment: &PhaseResult) {
    println!("\n## Summary");
    println!(
        "  {:>22}  {:>12}  {:>12}  {:>12}  {:>12}  {:>14}",
        "phase", "syntheses", "d_added", "r_added", "deg(d0)", "deg(random)"
    );
    for p in [control, treatment] {
        println!(
            "  {:>22}  {:>12}  {:>12}  {:>12}  {:>12}  {:>14}",
            p.label,
            p.syntheses_total,
            p.distinctions_added,
            p.relationships_added,
            p.deg_d0,
            p.deg_random_interior
                .map(|x| x.to_string())
                .unwrap_or_else(|| "n/a".to_string())
        );
    }
    let ratio = if control.deg_d0 == 0 {
        f64::INFINITY
    } else {
        treatment.deg_d0 as f64 / control.deg_d0 as f64
    };
    println!(
        "\n  deg(d0) treatment / control = {:.1}x  ({} vs {})",
        ratio, treatment.deg_d0, control.deg_d0
    );

    let pass = treatment.deg_d0 > 10 * control.deg_d0
        && treatment.deg_d1 > 10 * control.deg_d1
        && degcheck_interior(treatment);
    println!(
        "\n  VERDICT: {}",
        if pass {
            "CONFIRMED -- ByteMapping concentrates degree on d0/d1, not on interior nodes."
        } else {
            "REFUTED  -- treatment did not show the expected concentration on seed nodes."
        }
    );
    if !pass {
        std::process::exit(1);
    }
}

fn degcheck_interior(p: &PhaseResult) -> bool {
    p.deg_random_interior.map(|x| x < 10).unwrap_or(true)
}

fn main() {
    let m: usize = std::env::var("M")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    println!("# exp20_fold_law");
    println!("# control syntheses (M) = {}", m);
    println!("# treatment bytes       = 256 (full byte universe)");

    let control = run_control_fibonacci(m);
    let treatment = run_treatment_bytes();
    run_treatment_scaling(&[64, 128, 256, 512, 1024]);
    summary(&control, &treatment);
}
