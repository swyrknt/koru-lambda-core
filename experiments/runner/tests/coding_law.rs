//! Coding Law ρ gate test — see `docs/BENCHMARKS.md § Throughput` for
//! scale-context; this file is the canonical source of truth for the
//! exp18 corpus SHA-256 pin (`EXP18_LOG_SHA256` and
//! `EXP18_FREQ_SHA256` below).
//!
//! Pins the exp18 corpus at `tests/corpora/exp18.{log,freq.bin}` via
//! SHA-256 digest constants, then runs the canonical Coding Law
//! workload (alpha=1.0, seed=0xC0DE, N=4096, M=8N) and asserts
//! Spearman ρ between observed frequency and engine-reported degree
//! delta clears 0.985.
//!
//! Why pin: the corpus is generated from a `rand`-based Zipf draw
//! sequence. A `rand` minor version change could shift the sequence
//! and silently regenerate the corpus with different content. The
//! SHA-256 gate catches this — if the digest doesn't match, the test
//! fails BEFORE running ρ, telling the contributor "your corpus
//! diverged; either accept the new digest (revising this gate) or
//! pin a different `rand` version".
//!
//! The `rand` version this depends on is pinned at the workspace level —
//! `rand = "=0.9.5"` in `[workspace.dependencies]` of the root `Cargo.toml`,
//! which `experiments/runner/Cargo.toml` inherits. See `CORPUS_PROVENANCE.md`
//! for why the pin is exact and for the record of the `=0.8.5` → `=0.9.5`
//! move, which left both corpus digests byte-identical.

use koru_experiments_runner::coding_law_control::{
    mechanism_correlation, saturation_sweep, shuffled_null,
};
use koru_experiments_runner::coding_law_workload::CodingLawWorkload;
use koru_lambda_core::DistinctionEngine;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

// Pinned digests — see `cargo run -p koru-experiments-runner --bin
// gen_exp18_corpus --release` output for the generator side. These
// constants change ONLY when the corpus is intentionally regenerated
// AND a corresponding `docs/BENCHMARKS.md` update is filed.
const EXP18_LOG_SHA256: &str = "9fd8a22b4b89e0937d09af13646af5b797e8369e46c11c3e1f94b3e7169878d9";
const EXP18_FREQ_SHA256: &str = "93d08b2fb4e721b679c90a56b70d69de8511a4749a4ee2ff715722efc123ca46";

const ALPHA: f64 = 1.0;
const SEED: u64 = 0xC0DE;
const N: usize = 4096;
const M: usize = 8 * N;

const RHO_GATE: f64 = 0.985;

/// Pre-flight integrity gate: the pinned corpus files must hash to
/// the pinned digests before we trust them for the ρ measurement.
///
/// Falsifier: if a future `rand` version (or any change to the
/// workload generator) silently regenerates a different corpus,
/// this test fails before reporting ρ — the contributor knows the
/// corpus drifted.
#[test]
fn exp18_corpus_integrity() {
    let log_path = corpora_path("exp18.log");
    let freq_path = corpora_path("exp18.freq.bin");

    let log_sha = sha256_hex(&log_path);
    assert_eq!(
        log_sha, EXP18_LOG_SHA256,
        "exp18.log corpus drift: regenerated content differs from pinned digest. \
         Either regenerate with `cargo run -p koru-experiments-runner --bin gen_exp18_corpus --release` \
         and revise the pinned constant, OR pin a different rand version."
    );

    let freq_sha = sha256_hex(&freq_path);
    assert_eq!(
        freq_sha, EXP18_FREQ_SHA256,
        "exp18.freq.bin corpus drift: regenerated content differs from pinned digest."
    );

    // Size sanity (cheap, falsifies grossly-wrong files independent
    // of SHA collision).
    assert_eq!(fs::metadata(&log_path).expect("log metadata").len(), (M * 32) as u64);
    assert_eq!(fs::metadata(&freq_path).expect("freq metadata").len(), (N * 4) as u64);
}

/// Gate: Spearman ρ ≥ 0.985 between Zipf-drawn freq and engine-
/// observed degree delta on the canonical exp18 workload.
///
/// The workload is regenerated in-memory (cheap — ~50ms), checked
/// against the pinned corpus byte-for-byte, then replayed against a
/// fresh engine. ρ is computed on the resulting degree deltas.
///
/// This test runs the full 32K-pair workload, so use `--release` for
/// reasonable wall-clock (~1s release, ~10s debug).
#[test]
fn exp18_rho_clears_gate() {
    // Generate the workload in-memory.
    let workload_engine = DistinctionEngine::new();
    let workload = CodingLawWorkload::generate(&workload_engine, ALPHA, SEED, N, M);

    // Cross-check: in-memory generation must produce the same bytes
    // as the pinned corpus. (Same RNG seed + version + Zipf weights
    // = identical sequence.) This is the redundancy that closes the
    // "test passed but workload diverged from corpus" gap.
    {
        let mut log_bytes = Vec::with_capacity(M * 32);
        workload.write_log(&mut log_bytes).expect("write log to vec");
        assert_eq!(
            sha256_hex_of_bytes(&log_bytes),
            EXP18_LOG_SHA256,
            "in-memory workload log differs from pinned digest — rand drift?"
        );

        let mut freq_bytes = Vec::with_capacity(N * 4);
        workload.write_freq(&mut freq_bytes).expect("write freq to vec");
        assert_eq!(
            sha256_hex_of_bytes(&freq_bytes),
            EXP18_FREQ_SHA256,
            "in-memory workload freq differs from pinned digest — rand drift?"
        );
    }

    // Build a fresh engine with the same pool, then replay pairs.
    // The pool is deterministic from the chain extension; we rebuild
    // here rather than carry workload_engine state forward so the
    // degree-delta math starts from a known-zero baseline.
    let rerun_engine = DistinctionEngine::new();
    for k in 2..N {
        let _ = rerun_engine.synthesize(workload.pool[k - 1], workload.pool[k - 2]);
    }

    let rho = workload.compute_rho(&rerun_engine);
    println!("exp18 Spearman ρ = {rho:.6} (gate ≥ {RHO_GATE})");
    assert!(
        rho >= RHO_GATE,
        "Coding Law ρ gate FAILED: got {rho:.6}, need ≥ {RHO_GATE}. \
         Substrate may be losing degree signal — investigate before relaxing."
    );
}

/// The ρ gate with a null model, which it did not have.
///
/// ρ ≥ 0.985 is the headline empirical number in `THEORY.md`, and degree is
/// incremented by the very draws frequency counts — so the gate might have
/// been asserting that a counter counts. The previous release shipped that
/// exact defect elsewhere: a correlation with no null, on two series sharing a
/// monotone trend, which passed 96% of the time against the hypothesis it
/// claimed to reject.
///
/// This holds the degree deltas fixed and permutes the frequency vector,
/// destroying the pairing while preserving both marginal distributions. If a
/// non-relationship could reach 0.985, the gate would be decorative.
#[test]
fn rho_gate_is_not_clearable_without_a_relationship() {
    let engine = DistinctionEngine::new();
    let workload = CodingLawWorkload::generate(&engine, ALPHA, SEED, N, M);
    let delta = workload.run_against(&engine);

    let null = shuffled_null(&workload.freq, &delta, 200, 0xA11CE);
    println!(
        "null over {} permutations: mean {:+.4}, sd {:.4}, max |ρ| {:.4}",
        null.trials, null.mean, null.sd, null.max_abs
    );

    assert!(
        null.max_abs < RHO_GATE,
        "a permuted frequency vector reached |ρ| = {:.4} against a gate of {RHO_GATE}. \
         The gate would then be clearable with no relationship between frequency and \
         degree at all, and THEORY's ρ ≈ 0.99 would be an artifact of the construction \
         rather than a measurement.",
        null.max_abs
    );
    assert!(
        null.mean.abs() < 0.05,
        "the null is centred at {:+.4} rather than zero, so permuting frequency leaves a \
         systematic correlation — the two series share something the shuffle cannot break",
        null.mean
    );
}

/// THEORY explains why ρ < 1 — saturation drift — and that explanation is a
/// prediction about how ρ *moves*, which is stronger evidence than any single
/// value. "Under heavy saturation ρ drops below 1 ... under low saturation
/// ρ → 1."
///
/// Nothing in the construction forces this. ρ comes from the engine's degree
/// bookkeeping; the saturation rate comes from how often the engine declined
/// to create anything. A substrate that mis-credited degree would break the
/// relationship while leaving the saturation rate untouched.
#[test]
fn rho_tracks_saturation_as_the_theory_predicts() {
    let shapes = [
        (8192usize, 8192usize, 0.5f64),
        (8192, 16384, 0.8),
        (4096, 32768, 1.0),
        (2048, 32768, 1.0),
        (1024, 32768, 1.2),
        (512, 32768, 1.4),
        (256, 32768, 1.6),
        (128, 32768, 1.8),
    ];
    let points = saturation_sweep(&shapes, SEED);
    for p in &points {
        println!(
            "pool {:6} draws {:7} alpha {:.1}  saturation {:.4}  ρ {:+.4}",
            p.pool, p.draws, p.alpha, p.saturation, p.rho
        );
    }

    let mech = mechanism_correlation(&points);
    println!("ρ(saturation, ρ) = {mech:+.4}");
    assert!(
        mech < -0.9,
        "ρ(saturation, ρ) = {mech:+.4}; THEORY attributes the shortfall below 1.0 to \
         saturation drift, which requires a strong negative relationship. If this is \
         near zero, saturation does not explain the shortfall and THEORY's account of \
         its own headline number needs replacing."
    );

    // The low-saturation limit THEORY states explicitly.
    let lightest = points.first().expect("sweep is non-empty");
    assert!(
        lightest.saturation < 0.01 && lightest.rho > 0.999,
        "at saturation {:.4} ρ was {:.4}; THEORY says ρ → 1 as saturation → 0",
        lightest.saturation,
        lightest.rho
    );
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn corpora_path(name: &str) -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests").join("corpora").join(name)
}

fn sha256_hex(path: &Path) -> String {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    sha256_hex_of_bytes(&bytes)
}

fn sha256_hex_of_bytes(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut s = String::with_capacity(digest.len() * 2);
    for b in &digest {
        s.push(HEX[(b >> 4) as usize] as char);
        s.push(HEX[(b & 0x0F) as usize] as char);
    }
    s
}
