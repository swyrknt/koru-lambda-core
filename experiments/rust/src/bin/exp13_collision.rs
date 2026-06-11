//! Exp 13: Adversarial 16-byte collision test.
//!
//! Truncating SHA256 to 16 bytes means birthday collisions at ~2^64. At 2^28
//! random pairs, the expected number of 16-byte prefix collisions is
//!   n^2 / 2^129 ~= 2^-73  (vanishing)
//!
//! We still verify empirically up to feasible N.
//!
//! Memory strategy: a HashMap<[u8;16], (u64,u64)> with 2^28 = 268M entries
//! would cost ~10GB. Instead, store only the 8-byte high half of the hash
//! + the 64-bit seed. That's 16 bytes/entry -> 4GB at 2^28. Still too much.
//!
//! Better: sort-based collision detection.
//!   - Generate N hashes into a Vec<u128> (16 bytes each, interpreted as u128).
//!   - Sort.
//!   - Scan for adjacent duplicates.
//! Memory = 16 * N bytes = 4 GB at 2^28. Still too much on typical laptop.
//!
//! Chunked approach: partition hashes by top K bits. Process partitions
//! sequentially, keeping only one partition in memory at a time.
//!   - Generate all N hashes; write those with top byte == p to a per-p Vec.
//!   - After generating, for each p, sort and scan.
//! This requires regenerating 256x, OR writing to 256 buckets once.
//! Writing to 256 bucket-Vecs: memory = total N * 16 bytes still. Hmm.
//!
//! Much better: use a radix partition. Do two passes:
//!   Pass 1: just count hashes per bucket (256 u64 counters).
//!   Pass 2: for each bucket, rebuild hashes from scratch, only keeping ones
//!           matching the bucket.
//! Cost: 2x hash work but bounded memory = max(bucket_size) * 16 bytes.
//!
//! At N=2^28 with 256 buckets, max bucket ~2^20 * 16 = 16 MB. Fine.

use rayon::prelude::*;
use sha2::{Digest, Sha256};
use std::time::Instant;

#[inline]
fn sha16_u128(a: u64, b: u64) -> u128 {
    let (lo, hi) = if a < b { (a, b) } else { (b, a) };
    let mut hasher = Sha256::new();
    hasher.update(format!("{lo:x}:{hi:x}").as_bytes());
    let digest = hasher.finalize();
    u128::from_be_bytes(digest[..16].try_into().unwrap())
}

#[inline]
fn seeds_from_idx(i: u64) -> (u64, u64) {
    let a = i.wrapping_mul(0x9E3779B97F4A7C15) ^ 0xDEADBEEF;
    let b = i.wrapping_mul(0xBF58476D1CE4E5B9) ^ 0xCAFEBABE;
    (a, b)
}

fn run_collision_search(n_bits: u32) {
    let n: u64 = 1u64 << n_bits;
    println!("\n--- Random collision search: N = 2^{n_bits} = {n} pairs ---");
    let start = Instant::now();

    // Single-pass: 16 bytes per hash. At N=2^28 that's 4 GB.
    let mut hashes: Vec<u128> = (0..n)
        .into_par_iter()
        .map(|i| {
            let (a, b) = seeds_from_idx(i);
            sha16_u128(a, b)
        })
        .collect();
    println!(
        "  generated {} hashes in {:.1}s  (mem ~{:.1} GB)",
        hashes.len(),
        start.elapsed().as_secs_f64(),
        (hashes.len() * 16) as f64 / 1_073_741_824.0
    );

    let t_sort = Instant::now();
    hashes.par_sort_unstable();
    println!("  sorted in {:.1}s", t_sort.elapsed().as_secs_f64());

    let mut collisions = 0u64;
    let mut first: Option<u128> = None;
    for w in hashes.windows(2) {
        if w[0] == w[1] {
            collisions += 1;
            if first.is_none() {
                first = Some(w[0]);
            }
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("  done: N=2^{n_bits}  elapsed={elapsed:.1}s  collisions found: {collisions}");
    if let Some(h) = first {
        println!("  first collision hash: {h:032x}");
    }

    let expected = (n as f64).powi(2) / 2f64.powi(129);
    println!("  expected by birthday bound: {expected:.3e}");
}

fn run_adversarial_search() {
    println!("\n--- Adversarial: 2^24 pairs with structured parent IDs ---");
    let n: u64 = 1u64 << 24;
    let start = Instant::now();

    // Parents share common prefix structure to test against structural weakness.
    let hashes: Vec<u128> = (0..n)
        .into_par_iter()
        .map(|i| {
            let a = i << 32; // sparse high bits
            let b = i ^ 0xAAAA_AAAA_AAAA_AAAAu64; // low-bit correlation
            sha16_u128(a, b)
        })
        .collect();

    let mut h_sorted = hashes.clone();
    h_sorted.par_sort_unstable();
    let mut c = 0u64;
    for w in h_sorted.windows(2) {
        if w[0] == w[1] {
            c += 1;
        }
    }
    let elapsed = start.elapsed().as_secs_f64();
    println!("  done: elapsed={elapsed:.1}s  collisions: {c}  (expected ~{:.3e})",
        (n as f64).powi(2) / 2f64.powi(129));
}

fn run_birthday_extrapolation() {
    println!("\n--- Birthday bound extrapolation (16-byte / 128-bit IDs) ---");
    for exp in [20u32, 24, 28, 32, 48, 60, 61, 62, 63, 64, 65, 68] {
        let n = 2f64.powi(exp as i32);
        let expected = n * n / 2.0 / 2f64.powi(128);
        let prob_at_least_one = 1.0 - (-expected).exp();
        println!(
            "    N = 2^{exp:<3}  expected = {expected:>12.3e}  P(>=1 collision) = {prob_at_least_one:.3e}"
        );
    }
    println!("  50% threshold: N ~= 2^64.3  (2.16e19 pairs)");
    println!("  1% threshold:  N ~= 2^61.7  (3.72e18 pairs)");
    println!();
    println!("--- Comparison to 32-byte IDs (full SHA256) ---");
    for exp in [64u32, 96, 120, 128] {
        let n = 2f64.powi(exp as i32);
        let expected = n * n / 2.0 / 2f64.powi(256);
        println!(
            "    N = 2^{exp:<3}  expected = {expected:>12.3e}  (256-bit keyspace)"
        );
    }
    println!("  32-byte 50% threshold: N ~= 2^128.3 (unattainable)");
}

fn main() {
    println!("=== Exp 13: Adversarial 16-byte collision ===");
    run_birthday_extrapolation();

    let args: Vec<String> = std::env::args().collect();
    let max_bits: u32 = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(24);

    for bits in [20u32, 22, 24, 26, 28] {
        if bits > max_bits {
            println!("\n(skipping N=2^{bits}; pass {bits} as arg to run)");
            continue;
        }
        run_collision_search(bits);
    }

    run_adversarial_search();
}
