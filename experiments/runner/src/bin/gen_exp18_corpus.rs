//! Generate the pinned exp18 corpus from the canonical workload constants.
//!
//! Canonical exp18 workload constants:
//!   alpha = 1.0, seed = 0xC0DE, N = 4096, M = 8N = 32_768.
//!
//! Writes:
//!   tests/corpora/exp18.log       (M × 32 bytes = 1,048,576 bytes)
//!   tests/corpora/exp18.freq.bin  (N × 4 bytes  =    16,384 bytes)
//!
//! Prints SHA-256 of each file to stdout so the gate test in
//! `tests/coding_law.rs` (main crate) can pin the digests.
//!
//! # Running
//!
//! ```text
//! cargo run -p koru-experiments-runner --bin gen_exp18_corpus --release
//! ```
//!
//! Run-once: the corpus files are checked in; CI does not regenerate.
//! Re-run only when the workload constants change (which itself
//! triggers a separate gate-amendment discussion).

use koru_experiments_runner::coding_law_workload::CodingLawWorkload;
use koru_lambda_core::DistinctionEngine;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufWriter, Read, Write};
use std::path::Path;

const ALPHA: f64 = 1.0;
const SEED: u64 = 0xC0DE;
const N: usize = 4096;
const M: usize = 8 * N;

fn main() -> std::io::Result<()> {
    // Resolve corpus paths relative to the workspace root.
    // CARGO_MANIFEST_DIR for this binary is .../experiments/runner;
    // climb two levels to the workspace root.
    let workspace_root =
        Path::new(env!("CARGO_MANIFEST_DIR")).parent().and_then(Path::parent).ok_or_else(|| {
            std::io::Error::other("could not resolve workspace root from CARGO_MANIFEST_DIR")
        })?;
    let corpora_dir = workspace_root.join("tests").join("corpora");
    std::fs::create_dir_all(&corpora_dir)?;
    let log_path = corpora_dir.join("exp18.log");
    let freq_path = corpora_dir.join("exp18.freq.bin");

    println!("Generating exp18 corpus");
    println!("  alpha = {ALPHA}");
    println!("  seed  = 0x{SEED:X}");
    println!("  N     = {N}");
    println!("  M     = {M}");
    println!();

    let engine = DistinctionEngine::new();
    let workload = CodingLawWorkload::generate(&engine, ALPHA, SEED, N, M);

    // Sanity checks before writing.
    assert_eq!(workload.pool.len(), N);
    assert_eq!(workload.freq.len(), N);
    assert_eq!(workload.pairs.len(), M);
    let freq_sum: u32 = workload.freq.iter().sum();
    assert_eq!(freq_sum, 2 * M as u32, "freq sum invariant");

    // Write log.
    {
        let f = File::create(&log_path)?;
        let mut w = BufWriter::new(f);
        workload.write_log(&mut w)?;
        w.flush()?;
    }
    let log_sha = sha256_of_file(&log_path)?;
    let log_size = std::fs::metadata(&log_path)?.len();
    println!("Wrote {} ({} bytes)", log_path.display(), log_size);
    println!("  SHA-256: {log_sha}");
    println!();

    // Write freq.
    {
        let f = File::create(&freq_path)?;
        let mut w = BufWriter::new(f);
        workload.write_freq(&mut w)?;
        w.flush()?;
    }
    let freq_sha = sha256_of_file(&freq_path)?;
    let freq_size = std::fs::metadata(&freq_path)?.len();
    println!("Wrote {} ({} bytes)", freq_path.display(), freq_size);
    println!("  SHA-256: {freq_sha}");
    println!();

    println!("Pin these constants in tests/coding_law.rs:");
    println!("    const EXP18_LOG_SHA256:  &str = \"{log_sha}\";");
    println!("    const EXP18_FREQ_SHA256: &str = \"{freq_sha}\";");

    Ok(())
}

fn sha256_of_file(path: &Path) -> std::io::Result<String> {
    let mut f = File::open(path)?;
    let mut buf = Vec::new();
    f.read_to_end(&mut buf)?;
    Ok(hex::encode(Sha256::digest(&buf)))
}
