//! dhat memory probe at 1M distinctions — DESIGN.md gate 13.
//!
//! Measures live-heap memory per distinction in the steady state after
//! allocating 1M distinctions via chain extension on a single engine.
//! Writes a dhat profile to `dhat-heap.json` (in the working directory)
//! and prints the headline number to stdout.
//!
//! # Running
//!
//! ```text
//! cargo run --example dhat_1m
//! ```
//!
//! **Run in debug mode, not release.** dhat 0.3.3 has a known
//! backtrace-frame bug on aarch64-apple-darwin in release builds
//! (panics in `Backtrace::get_frames_to_trim` because the release
//! `panic = "abort"` profile strips backtrace frames). Debug builds
//! preserve enough frames for dhat's symbol logic. The dhat
//! measurement itself is allocator-level and identical between
//! debug/release — only the wall-clock time differs.
//!
//! Why an example, not a test:
//! - `#[global_allocator]` is a per-binary-crate setting. Applying
//!   `dhat::Alloc` would slow EVERY test in the suite. An example is
//!   a separate binary crate.
//! - The gate is a one-off measurement, not a per-commit assertion.
//!   CI invokes this example explicitly when a Gate 13 measurement
//!   is needed.
//!
//! # What the output means
//!
//! `max_blocks_size` is the peak live-heap byte total during the
//! profile. Per-distinction is `max_blocks_size / 1_000_000`. Compare
//! against:
//! - DESIGN.md Gate 13 target: ≤ 180 B
//! - DESIGN.md Gate 13 floor:  ≤ 220 B
//! - Merged-map prediction:    ~80 B (Step 1e refactor estimate;
//!   amendment expected at Step 4)

use koru_lambda_core::DistinctionEngine;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    // `testing()` mode keeps stats queryable via `HeapStats::get()` while
    // still writing the dhat-heap.json profile on drop. Without `testing()`,
    // `HeapStats::get()` panics because the in-memory snapshot table isn't
    // populated.
    //
    // `trim_backtraces(None)` disables backtrace frame trimming. The
    // default trim logic indexes `frames[0]` unconditionally, which
    // panics on aarch64-apple-darwin where backtrace::Backtrace
    // sometimes captures zero frames during allocator-callback recursion.
    // Disabling trim costs us some symbol clutter at the top of each
    // dh_view backtrace but unblocks the run.
    let _profiler = dhat::Profiler::builder().testing().trim_backtraces(None).build();

    const N: usize = 1_000_000;

    let engine = DistinctionEngine::new();
    let mut prev = engine.d0();
    let mut cur = engine.d1();
    for _ in 0..N {
        let next = engine.synthesize(cur, prev);
        prev = cur;
        cur = next;
    }

    // Sanity: structural invariant holds.
    engine.check_structural_invariant().expect("r = 2d − 3 holds at 1M (invariant)");

    let stats = dhat::HeapStats::get();
    let bytes_per_distinction = stats.max_bytes as f64 / N as f64;

    println!();
    println!("dhat 1M memory probe — DESIGN.md gate 13");
    println!("=========================================");
    println!("Distinctions allocated:    {N}");
    println!("Peak live-heap (max_bytes): {}", stats.max_bytes);
    println!("Peak live-blocks:           {}", stats.max_blocks);
    println!("Per-distinction:            {bytes_per_distinction:.1} B");
    println!();
    println!(
        "Gate 13 target: ≤ 180 B   {}",
        if bytes_per_distinction <= 180.0 { "PASS" } else { "MISS" }
    );
    println!(
        "Gate 13 floor:  ≤ 220 B   {}",
        if bytes_per_distinction <= 220.0 { "PASS" } else { "FAIL — design event" }
    );
    println!();
    println!("Profile written to dhat-heap.json — open with dh_view.html for shape detail.");
}
