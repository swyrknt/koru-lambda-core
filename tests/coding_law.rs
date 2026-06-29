//! Coding Law ρ gate test — CHECKLIST.md lines 133–134.
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
//! Also pins the rand version via `rand = "=0.8.5"` in
//! `experiments/runner/Cargo.toml`.

use koru_experiments_runner::coding_law_workload::CodingLawWorkload;
use koru_lambda_core::DistinctionEngine;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::Path;

// Pinned digests — see `cargo run -p koru-experiments-runner --bin
// gen_exp18_corpus --release` output for the generator side. These
// constants change ONLY when the corpus is intentionally regenerated
// AND a CHECKLIST line 134 / DESIGN.md amendment is filed.
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
