//! Experiment 19: Mediated self-reference produces infinite novelty.
//!
//! Validates the ALIS-imported claim from CLAUDE.md:
//!   "Direct self ⊗ self is irreflexive (returns self). Mediated
//!    self-observation (synth(synth(self, obs), self)) produces unique
//!    distinctions at every depth."
//!
//! T1: s_{n+1} = synth(s_n, s_n)                                 -> expect fixed point.
//! T2: inner = synth(s_n, obs_n); s_{n+1} = synth(inner, s_n)    obs_n varying -> all distinct.
//! T3: inner = synth(s_n, d1);    s_{n+1} = synth(inner, s_n)    obs constant  -> still all distinct (predicted).
//!
//! Also verifies r = 2d - 3 at every checkpoint.
//!
//! Env:
//!   DEPTHS=1,10,100,1000,10000  (comma list)
//!   DIRECT_ITERS=1000

use std::collections::HashSet;
use std::env;
use std::time::Instant;

use koru_lambda_core::{Distinction, DistinctionEngine};

fn counter_distinction(engine: &DistinctionEngine, n: usize) -> Distinction {
    let d1 = engine.d1().clone();
    let mut cur = engine.synthesize(engine.d0(), &d1);
    for _ in 0..n {
        cur = engine.synthesize(&cur, &d1);
    }
    cur
}

fn snapshot_and_check(engine: &DistinctionEngine, label: &str, depth: usize) -> (usize, usize) {
    let d = engine.distinction_count();
    let r = engine.relationship_count();
    let expected_r = if d >= 2 { 2 * d - 3 } else { 0 };
    let delta = r as i64 - expected_r as i64;
    if delta != 0 {
        eprintln!(
            "AXIOM VIOLATION {} depth={} : d={} r={} expected_r={} delta={}",
            label, depth, d, r, expected_r, delta
        );
    }
    (d, r)
}

fn run_t1_direct(iterations: usize) -> bool {
    println!("\n## T1: direct self-reference (control)");
    println!("       s_{{n+1}} = synth(s_n, s_n)");

    let engine = DistinctionEngine::new();
    let seed = engine.d0().clone();
    let mut s = seed.clone();
    let d_before = engine.distinction_count();
    let r_before = engine.relationship_count();

    let t0 = Instant::now();
    let mut deviations = 0usize;
    for i in 0..iterations {
        let next = engine.synthesize(&s, &s);
        if next.id() != seed.id() {
            deviations += 1;
            if deviations <= 3 {
                eprintln!("  T1 deviation at i={} : id={}", i, next.id());
            }
        }
        s = next;
    }
    let secs = t0.elapsed().as_secs_f64();

    let new_d = engine.distinction_count() - d_before;
    let new_r = engine.relationship_count() - r_before;

    println!(
        "  iterations = {}  secs = {:.3}  ({:.0} ops/sec)",
        iterations,
        secs,
        iterations as f64 / secs
    );
    println!(
        "  s.id stable     : {}",
        if deviations == 0 { "YES" } else { "NO" }
    );
    println!(
        "  new distinctions: {}   new relationships: {}",
        new_d, new_r
    );

    let pass = deviations == 0 && new_d == 0 && new_r == 0;
    println!("  T1 verdict: {}", if pass { "PASS" } else { "FAIL" });
    pass
}

fn run_t2_mediated_varying(checkpoints: &[usize]) -> bool {
    println!("\n## T2: mediated, varying observation");
    let max_depth = *checkpoints.iter().max().expect("non-empty");
    let engine = DistinctionEngine::new();
    let mut s = engine.d0().clone();

    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(s.id().to_string());
    let mut trajectory: Vec<String> = vec![s.id().to_string()];

    let (d_before, _) = snapshot_and_check(&engine, "T2/pre", 0);

    let mut next_cp = 0usize;
    let t0 = Instant::now();
    let mut collisions = 0usize;
    for n in 0..max_depth {
        let obs = {
            let c = counter_distinction(&engine, n + 1);
            engine.synthesize(engine.d1(), &c)
        };
        let inner = engine.synthesize(&s, &obs);
        let s_next = engine.synthesize(&inner, &s);

        if !seen.insert(s_next.id().to_string()) {
            collisions += 1;
            if collisions <= 3 {
                eprintln!("  T2 COLLISION at depth {} : id={}", n + 1, s_next.id());
            }
        }
        trajectory.push(s_next.id().to_string());
        s = s_next;

        if next_cp < checkpoints.len() && (n + 1) == checkpoints[next_cp] {
            let depth = n + 1;
            let (d, r) = snapshot_and_check(&engine, "T2", depth);
            println!(
                "  depth = {:>5}  unique s_i = {:>5} / {:>5}   d = {:>7}  r = {:>7}  r-(2d-3) = {}",
                depth,
                seen.len(),
                depth + 1,
                d,
                r,
                r as i64 - (2 * d as i64 - 3)
            );
            next_cp += 1;
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let new_d_total = engine.distinction_count() - d_before;

    println!(
        "  total steps = {}  secs = {:.3}  ({:.0} ops/sec)",
        max_depth,
        secs,
        max_depth as f64 / secs
    );
    println!(
        "  unique s_i overall : {} / {}  (collisions: {})",
        seen.len(),
        max_depth + 1,
        collisions
    );
    println!(
        "  new distinctions in engine over the trial: {}",
        new_d_total
    );
    println!(
        "  first 5 s_i ids: {:?}",
        trajectory.iter().take(5).collect::<Vec<_>>()
    );
    println!(
        "  last  5 s_i ids: {:?}",
        trajectory.iter().rev().take(5).rev().collect::<Vec<_>>()
    );

    let pass = collisions == 0 && seen.len() == max_depth + 1;
    println!("  T2 verdict: {}", if pass { "PASS" } else { "FAIL" });
    pass
}

fn run_t3_mediated_constant(checkpoints: &[usize]) -> bool {
    println!("\n## T3: mediated, CONSTANT observation (= d1)");
    let max_depth = *checkpoints.iter().max().expect("non-empty");
    let engine = DistinctionEngine::new();
    let d1 = engine.d1().clone();
    let mut s = engine.d0().clone();

    let mut seen: HashSet<String> = HashSet::new();
    seen.insert(s.id().to_string());
    let mut trajectory: Vec<String> = vec![s.id().to_string()];

    let (d_before, _) = snapshot_and_check(&engine, "T3/pre", 0);

    let mut next_cp = 0usize;
    let t0 = Instant::now();
    let mut collisions = 0usize;
    let mut fixed_point_step: Option<usize> = None;
    for n in 0..max_depth {
        let inner = engine.synthesize(&s, &d1);
        let s_next = engine.synthesize(&inner, &s);

        if s_next.id() == s.id() && fixed_point_step.is_none() {
            fixed_point_step = Some(n + 1);
        }
        if !seen.insert(s_next.id().to_string()) {
            collisions += 1;
            if collisions <= 3 {
                eprintln!("  T3 COLLISION at depth {} : id={}", n + 1, s_next.id());
            }
        }
        trajectory.push(s_next.id().to_string());
        s = s_next;

        if next_cp < checkpoints.len() && (n + 1) == checkpoints[next_cp] {
            let depth = n + 1;
            let (d, r) = snapshot_and_check(&engine, "T3", depth);
            println!(
                "  depth = {:>5}  unique s_i = {:>5} / {:>5}   d = {:>7}  r = {:>7}  r-(2d-3) = {}",
                depth,
                seen.len(),
                depth + 1,
                d,
                r,
                r as i64 - (2 * d as i64 - 3)
            );
            next_cp += 1;
        }
    }
    let secs = t0.elapsed().as_secs_f64();
    let new_d_total = engine.distinction_count() - d_before;

    println!(
        "  total steps = {}  secs = {:.3}  ({:.0} ops/sec)",
        max_depth,
        secs,
        max_depth as f64 / secs
    );
    println!(
        "  unique s_i overall : {} / {}  (collisions: {})",
        seen.len(),
        max_depth + 1,
        collisions
    );
    println!(
        "  new distinctions in engine over the trial: {}",
        new_d_total
    );
    if let Some(step) = fixed_point_step {
        println!(
            "  fixed-point step (s_{{n+1}} == s_n) FIRST observed at: {}",
            step
        );
    } else {
        println!("  fixed-point step: never reached");
    }
    println!(
        "  first 5 s_i ids: {:?}",
        trajectory.iter().take(5).collect::<Vec<_>>()
    );
    println!(
        "  last  5 s_i ids: {:?}",
        trajectory.iter().rev().take(5).rev().collect::<Vec<_>>()
    );

    let pass = collisions == 0 && seen.len() == max_depth + 1;
    println!(
        "  T3 verdict: {}",
        if pass {
            "PASS (constant obs still produces novelty)"
        } else {
            "FAIL"
        }
    );
    pass
}

fn parse_depths(arg: Option<String>) -> Vec<usize> {
    match arg {
        None => vec![1, 10, 100, 1_000, 10_000],
        Some(s) => s
            .split(',')
            .map(|s| s.trim().parse::<usize>().expect("usize"))
            .collect(),
    }
}

fn main() {
    let depths = parse_depths(env::var("DEPTHS").ok());
    let direct_iters: usize = env::var("DIRECT_ITERS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000);

    println!("# exp19_mediated_self_reference");
    println!("# depths        = {:?}", depths);
    println!("# direct_iters  = {}", direct_iters);

    let p1 = run_t1_direct(direct_iters);
    let p2 = run_t2_mediated_varying(&depths);
    let p3 = run_t3_mediated_constant(&depths);

    println!("\n## Summary");
    println!(
        "  T1 direct (irreflexive)                : {}",
        if p1 { "PASS" } else { "FAIL" }
    );
    println!(
        "  T2 mediated, varying obs               : {}",
        if p2 { "PASS" } else { "FAIL" }
    );
    println!(
        "  T3 mediated, constant obs              : {}",
        if p3 { "PASS" } else { "FAIL" }
    );

    if !(p1 && p2 && p3) {
        std::process::exit(1);
    }
}
