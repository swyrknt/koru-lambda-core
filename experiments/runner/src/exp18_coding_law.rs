//! Experiment 18: Validate the Coding Law claim.
//!
//! Claim (CLAUDE.md):
//!   "Degree = total synthesis participations. Correlates 0.99 (Spearman rho)
//!    with raw frequency of use."
//!
//! Method:
//! 1. Build a pool of N candidate distinctions via a linear chain so they
//!    exist in the engine with a known, uniform construction degree.
//! 2. Snapshot per-node degree BEFORE the Zipf phase (baseline).
//! 3. For M >> N steps, sample two distinct pool indices (i, j) from a Zipf
//!    distribution with skew alpha. Call engine.synthesize(pool[i], pool[j]).
//!    Track freq[i]++ and freq[j]++ (every call, including saturated).
//! 4. Snapshot per-node degree AFTER the Zipf phase.
//! 5. delta_degree[i] = (after - before) for each pool node.
//! 6. Spearman rho between freq[] and delta_degree[].
//!
//! Sanity check: repeat the exact same pair K times. freq grows by K on each
//! side; delta_degree grows by exactly 1 each (the first novel call only).

use std::collections::HashMap;
use std::time::Instant;

use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use koru_lambda_core::{Distinction, DistinctionEngine};

struct Zipf {
    cdf: Vec<f64>,
}

impl Zipf {
    fn new(n: usize, s: f64) -> Self {
        assert!(n >= 2);
        let mut cdf = Vec::with_capacity(n);
        let mut acc = 0.0f64;
        for k in 1..=n {
            acc += 1.0 / (k as f64).powf(s);
            cdf.push(acc);
        }
        let total = cdf[n - 1];
        for v in cdf.iter_mut() {
            *v /= total;
        }
        Self { cdf }
    }

    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> usize {
        let u: f64 = rng.gen();
        self.cdf.partition_point(|&p| p < u).min(self.cdf.len() - 1)
    }
}

fn spearman(x: &[f64], y: &[f64]) -> f64 {
    assert_eq!(x.len(), y.len());
    pearson(&ranks(x), &ranks(y))
}

fn ranks(v: &[f64]) -> Vec<f64> {
    let n = v.len();
    let mut idx: Vec<(usize, f64)> = v.iter().copied().enumerate().collect();
    idx.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));
    let mut ranks = vec![0.0f64; n];
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && idx[j].1 == idx[i].1 {
            j += 1;
        }
        let avg = ((i + 1) as f64 + j as f64) / 2.0;
        for entry in idx.iter().take(j).skip(i) {
            ranks[entry.0] = avg;
        }
        i = j;
    }
    ranks
}

fn pearson(x: &[f64], y: &[f64]) -> f64 {
    let n = x.len() as f64;
    let mx: f64 = x.iter().sum::<f64>() / n;
    let my: f64 = y.iter().sum::<f64>() / n;
    let (mut num, mut dx2, mut dy2) = (0.0f64, 0.0f64, 0.0f64);
    for (xi, yi) in x.iter().zip(y.iter()) {
        let dx = xi - mx;
        let dy = yi - my;
        num += dx * dy;
        dx2 += dx * dx;
        dy2 += dy * dy;
    }
    let den = (dx2 * dy2).sqrt();
    if den == 0.0 {
        0.0
    } else {
        num / den
    }
}

fn build_pool(n: usize) -> (DistinctionEngine, Vec<Distinction>) {
    assert!(n >= 4);
    let engine = DistinctionEngine::new();
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let mut pool = Vec::with_capacity(n);
    pool.push(d0.clone());
    pool.push(d1.clone());
    let mut current = engine.synthesize(&d0, &d1);
    pool.push(current.clone());
    while pool.len() < n {
        current = engine.synthesize(&current, &d1);
        pool.push(current.clone());
    }
    (engine, pool)
}

fn degree_map(engine: &DistinctionEngine) -> HashMap<String, u32> {
    let rels = engine.get_relationships_snapshot();
    let mut out: HashMap<String, u32> = HashMap::with_capacity(rels.len() * 2);
    for (a, b) in rels {
        *out.entry(a).or_insert(0) += 1;
        *out.entry(b).or_insert(0) += 1;
    }
    out
}

#[derive(Debug, Clone, Copy)]
struct RunResult {
    n: usize,
    m: usize,
    alpha: f64,
    saturated_calls: usize,
    novel_calls: usize,
    rho_freq_vs_delta_degree: f64,
    rho_freq_vs_total_degree: f64,
    elapsed_secs: f64,
}

fn run_single(n: usize, m: usize, alpha: f64, seed: u64) -> RunResult {
    println!("\n--- run N={} M={} alpha={} seed={:#x} ---", n, m, alpha, seed);
    let t0 = Instant::now();
    let (engine, pool) = build_pool(n);
    let build_secs = t0.elapsed().as_secs_f64();
    println!(
        "pool built: {} distinctions, engine d_count={}, build_secs={:.3}",
        pool.len(),
        engine.distinction_count(),
        build_secs
    );

    let degree_before = degree_map(&engine);
    let d_count_before = engine.distinction_count();
    let zipf = Zipf::new(n, alpha);
    let mut rng = StdRng::seed_from_u64(seed);

    let mut freq: Vec<u64> = vec![0; n];
    let mut novel_calls = 0usize;
    let mut saturated_calls = 0usize;

    let t1 = Instant::now();
    for _ in 0..m {
        let i = zipf.sample(&mut rng);
        let mut j = zipf.sample(&mut rng);
        while j == i {
            j = zipf.sample(&mut rng);
        }
        freq[i] += 1;
        freq[j] += 1;
        let before = engine.distinction_count();
        let _ = engine.synthesize(&pool[i], &pool[j]);
        let after = engine.distinction_count();
        if after > before {
            novel_calls += 1;
        } else {
            saturated_calls += 1;
        }
    }
    let zipf_secs = t1.elapsed().as_secs_f64();
    println!(
        "zipf phase: {} calls in {:.3}s ({:.0} calls/sec) | novel={} saturated={}",
        m,
        zipf_secs,
        m as f64 / zipf_secs,
        novel_calls,
        saturated_calls
    );

    let degree_after = degree_map(&engine);
    let d_count_after = engine.distinction_count();
    println!(
        "engine d_count: before={} after={} delta={} (expect delta == novel_calls)",
        d_count_before,
        d_count_after,
        d_count_after - d_count_before
    );

    let mut delta_degree = Vec::with_capacity(n);
    let mut total_degree = Vec::with_capacity(n);
    let mut freq_f64 = Vec::with_capacity(n);
    for (i, d) in pool.iter().enumerate() {
        let before = *degree_before.get(d.id()).unwrap_or(&0) as f64;
        let after = *degree_after.get(d.id()).unwrap_or(&0) as f64;
        delta_degree.push(after - before);
        total_degree.push(after);
        freq_f64.push(freq[i] as f64);
    }

    let mut sorted_freq = freq.clone();
    sorted_freq.sort_unstable_by(|a, b| b.cmp(a));
    let top5: Vec<u64> = sorted_freq.iter().take(5).copied().collect();
    let zeros = sorted_freq.iter().filter(|&&f| f == 0).count();
    println!(
        "freq distribution: top5={:?} (sum across pool = {}), zeros={}",
        top5,
        freq.iter().sum::<u64>(),
        zeros
    );

    let rho_delta = spearman(&freq_f64, &delta_degree);
    let rho_total = spearman(&freq_f64, &total_degree);
    println!("Spearman rho(freq, delta_degree) = {:.6}", rho_delta);
    println!("Spearman rho(freq, total_degree) = {:.6}", rho_total);

    RunResult {
        n,
        m,
        alpha,
        saturated_calls,
        novel_calls,
        rho_freq_vs_delta_degree: rho_delta,
        rho_freq_vs_total_degree: rho_total,
        elapsed_secs: build_secs + zipf_secs,
    }
}

fn sanity_saturation() {
    println!("\n## Sanity: saturated repeats contribute 0 to degree");
    let engine = DistinctionEngine::new();
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let a = engine.synthesize(&d0, &d1);
    let b = engine.synthesize(&a, &d1);
    let c = engine.synthesize(&b, &d1);
    let deg_before = degree_map(&engine);
    let a_deg0 = *deg_before.get(a.id()).unwrap_or(&0);
    let c_deg0 = *deg_before.get(c.id()).unwrap_or(&0);
    let k = 10_000usize;
    let r_before = engine.relationship_count();
    let d_before = engine.distinction_count();
    for _ in 0..k {
        let _ = engine.synthesize(&a, &c);
    }
    let r_after = engine.relationship_count();
    let d_after = engine.distinction_count();
    let deg_after = degree_map(&engine);
    let a_deg1 = *deg_after.get(a.id()).unwrap_or(&0);
    let c_deg1 = *deg_after.get(c.id()).unwrap_or(&0);
    println!(
        "K={} repeats. d: {} -> {} (delta {}). r: {} -> {} (delta {}).",
        k,
        d_before,
        d_after,
        d_after - d_before,
        r_before,
        r_after,
        r_after - r_before
    );
    println!(
        "deg(a): {} -> {} (delta {}). deg(c): {} -> {} (delta {}).",
        a_deg0,
        a_deg1,
        a_deg1 - a_deg0,
        c_deg0,
        c_deg1,
        c_deg1 - c_deg0
    );
    let ok = (d_after - d_before == 1)
        && (r_after - r_before == 2)
        && (a_deg1 - a_deg0 == 1)
        && (c_deg1 - c_deg0 == 1);
    println!(
        "expectation: +1 distinction, +2 rels, +1 deg each parent : {}",
        if ok { "CONFIRMED" } else { "VIOLATED" }
    );
    if !ok {
        std::process::exit(2);
    }
}

fn parse_arg<T: std::str::FromStr>(args: &[String], i: usize, env_key: &str, default: T) -> T {
    if let Some(s) = args.get(i) {
        if let Ok(v) = s.parse::<T>() {
            return v;
        }
    }
    if let Ok(s) = std::env::var(env_key) {
        if let Ok(v) = s.parse::<T>() {
            return v;
        }
    }
    default
}

fn print_summary(results: &[RunResult]) {
    println!("\n## Summary");
    println!(
        "{:>8} {:>10} {:>6} {:>10} {:>10} {:>16} {:>16} {:>10}",
        "N", "M", "alpha", "novel", "saturated", "rho(freq,delta)", "rho(freq,total)", "secs"
    );
    for r in results {
        println!(
            "{:>8} {:>10} {:>6.2} {:>10} {:>10} {:>16.6} {:>16.6} {:>10.3}",
            r.n,
            r.m,
            r.alpha,
            r.novel_calls,
            r.saturated_calls,
            r.rho_freq_vs_delta_degree,
            r.rho_freq_vs_total_degree,
            r.elapsed_secs
        );
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().cloned().unwrap_or_default();
    if mode == "sanity" {
        sanity_saturation();
        return;
    }
    sanity_saturation();
    if !args.is_empty() {
        let n: usize = parse_arg(&args, 0, "N", 10_000);
        let m: usize = parse_arg(&args, 1, "M", 1_000_000);
        let alpha: f64 = parse_arg(&args, 2, "ALPHA", 1.0);
        let seed: u64 = parse_arg(&args, 3, "SEED", 0xC0DE);
        let res = run_single(n, m, alpha, seed);
        print_summary(&[res]);
        return;
    }
    let alpha: f64 = std::env::var("ALPHA")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1.0);
    let seed: u64 = std::env::var("SEED")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0xC0DE);
    let mut results = Vec::new();
    let plan: &[(usize, usize)] = &[(1_000, 200_000), (10_000, 1_000_000), (100_000, 5_000_000)];
    for &(n, m) in plan {
        results.push(run_single(n, m, alpha, seed));
    }
    print_summary(&results);
}
