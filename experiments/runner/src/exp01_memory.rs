//! Experiment 1: Memory baseline.
//!
//! Builds a DistinctionEngine with N distinctions and reports peak heap
//! allocation via dhat-rs. We measure one size per process invocation so the
//! heap profile captures only the relevant engine.
//!
//! Usage:
//!   exp01_memory <size>
//! where <size> is 1000, 10000, 100000, or 1000000. Defaults to 10000.
//!
//! Per-distinction amortized = (peak_bytes / N).

use std::env;

mod common;

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

fn main() {
    let args: Vec<String> = env::args().collect();
    let target: usize = args
        .get(1)
        .and_then(|s| s.parse().ok())
        .unwrap_or(10_000);

    // Fresh dhat profiler for this run.
    let _profiler = dhat::Profiler::builder().testing().build();

    let t0 = std::time::Instant::now();
    let (engine, _tail) = common::build_chain_engine(target);
    let elapsed = t0.elapsed();

    let d_count = engine.distinction_count();
    let r_count = engine.relationship_count();

    let stats = dhat::HeapStats::get();

    println!("# exp01_memory target={}", target);
    println!("distinctions = {}", d_count);
    println!("relationships = {}", r_count);
    println!("elapsed_secs = {:.3}", elapsed.as_secs_f64());
    println!("synth_per_sec = {:.0}", d_count as f64 / elapsed.as_secs_f64());
    println!("---- dhat heap stats ----");
    println!("total_blocks = {}", stats.total_blocks);
    println!("total_bytes = {}", stats.total_bytes);
    println!("max_blocks = {}", stats.max_blocks);
    println!("max_bytes = {}", stats.max_bytes);
    println!("curr_blocks = {}", stats.curr_blocks);
    println!("curr_bytes = {}", stats.curr_bytes);
    println!("---- amortized ----");
    println!(
        "bytes_per_distinction_max = {:.2}",
        stats.max_bytes as f64 / d_count as f64
    );
    println!(
        "bytes_per_distinction_curr = {:.2}",
        stats.curr_bytes as f64 / d_count as f64
    );

    // Hold the engine alive across the stats read above, then release it
    // so the final profiler drop reports correctly.
    drop(engine);
}
