//! Experiment 3: Log design A/B/C.
//!
//! Compare three proposed append-only log designs wrapped around the engine:
//!   A: Arc<RwLock<Vec<(String, String)>>>    (the TODO's design)
//!   B: Arc<Mutex<Vec<(String, String)>>>
//!   C: Arc<crossbeam::queue::SegQueue<(String, String)>>
//! plus a baseline D: no log at all.
//!
//! For each design, drive N syntheses on T threads. Each thread owns a
//! chain head so the threads do not contend for the same SHA256 result.
//! Per-thread: N/T synths. Measure elapsed wall time, compute ops/sec.
//!
//! To keep every synthesis novel, each thread seeds its chain with a
//! distinct distinction (produced from a per-thread index byte). Within
//! a thread, `current = synth(current, d1)` keeps producing new ids.

use std::sync::{Arc, Mutex, RwLock};
use std::thread;
use std::time::Instant;

use koru_lambda_core::{Canonicalizable, Distinction, DistinctionEngine};

// -------------------- Log backends --------------------

trait Log: Send + Sync {
    fn append(&self, a: &str, b: &str);
    fn len(&self) -> usize;
}

struct NoLog;
impl Log for NoLog {
    fn append(&self, _a: &str, _b: &str) {}
    fn len(&self) -> usize {
        0
    }
}

struct RwLockLog(RwLock<Vec<(String, String)>>);
impl Log for RwLockLog {
    fn append(&self, a: &str, b: &str) {
        self.0.write().unwrap().push((a.to_string(), b.to_string()));
    }
    fn len(&self) -> usize {
        self.0.read().unwrap().len()
    }
}

struct MutexLog(Mutex<Vec<(String, String)>>);
impl Log for MutexLog {
    fn append(&self, a: &str, b: &str) {
        self.0.lock().unwrap().push((a.to_string(), b.to_string()));
    }
    fn len(&self) -> usize {
        self.0.lock().unwrap().len()
    }
}

struct SegQueueLog(crossbeam::queue::SegQueue<(String, String)>);
impl Log for SegQueueLog {
    fn append(&self, a: &str, b: &str) {
        self.0.push((a.to_string(), b.to_string()));
    }
    fn len(&self) -> usize {
        self.0.len()
    }
}

// -------------------- Driver --------------------

fn run_design(
    label: &str,
    log: Arc<dyn Log>,
    total_synths: usize,
    threads: usize,
) -> (f64, usize) {
    let engine = Arc::new(DistinctionEngine::new());

    // Each thread seeds its chain with a distinct byte-distinction so the
    // threads do not collide in the SHA256 space. Seeds are created
    // sequentially before the timed region.
    let mut seeds: Vec<Distinction> = Vec::with_capacity(threads);
    for i in 0..threads {
        // Use the byte canonicalization then compose with d1 once to get a
        // per-thread unique head.
        let byte = (i as u8).to_canonical_structure(&engine);
        let seed = engine.synthesize(&byte, engine.d1());
        seeds.push(seed);
    }

    // Per-thread synth count (ceil so total is at least `total_synths`).
    let per = (total_synths + threads - 1) / threads;

    let start = Instant::now();
    let handles: Vec<_> = seeds
        .into_iter()
        .map(|seed| {
            let engine = Arc::clone(&engine);
            let log = Arc::clone(&log);
            thread::spawn(move || {
                let d1 = engine.d1().clone();
                let mut current = seed;
                for _ in 0..per {
                    log.append(current.id(), d1.id());
                    current = engine.synthesize(&current, &d1);
                }
                current
            })
        })
        .collect();

    let mut tails: Vec<Distinction> = Vec::with_capacity(threads);
    for h in handles {
        tails.push(h.join().unwrap());
    }
    let elapsed = start.elapsed();

    let ops = per * threads;
    let sec = elapsed.as_secs_f64();
    println!(
        "[{label}] threads={threads:<3} ops={ops:<8} elapsed={sec:.4}s \
         throughput={:>10.0} ops/sec log_len={}",
        ops as f64 / sec,
        log.len()
    );
    (ops as f64 / sec, log.len())
}

fn main() {
    // 1M synths total (per sweep), tested at 1/2/4/8 threads.
    let total: usize = std::env::var("SYNTHS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(1_000_000);

    let thread_counts = [1usize, 2, 4, 8];

    println!("# exp03_log_ab total_synths={}", total);

    // Warm up: run a full pass that we throw away, so caches/allocator settle.
    println!("## warmup (discarded)");
    let _ = run_design(
        "warmup",
        Arc::new(NoLog) as Arc<dyn Log>,
        total.min(200_000),
        4,
    );

    // Per thread count, median of 3 runs.
    fn median(v: &mut [f64]) -> f64 {
        v.sort_by(|a, b| a.partial_cmp(b).unwrap());
        v[v.len() / 2]
    }

    for &t in &thread_counts {
        println!("\n## threads = {}", t);

        let mut b_tps = Vec::new();
        let mut a_tps = Vec::new();
        let mut b2_tps = Vec::new();
        let mut c_tps = Vec::new();
        for _ in 0..3 {
            let baseline = Arc::new(NoLog) as Arc<dyn Log>;
            let (tp, _) = run_design("baseline(no-log)", baseline, total, t);
            b_tps.push(tp);

            let a = Arc::new(RwLockLog(RwLock::new(Vec::with_capacity(total)))) as Arc<dyn Log>;
            let (tp, _) = run_design("A RwLock<Vec>", a, total, t);
            a_tps.push(tp);

            let b = Arc::new(MutexLog(Mutex::new(Vec::with_capacity(total)))) as Arc<dyn Log>;
            let (tp, _) = run_design("B Mutex<Vec>", b, total, t);
            b2_tps.push(tp);

            let c = Arc::new(SegQueueLog(crossbeam::queue::SegQueue::new())) as Arc<dyn Log>;
            let (tp, _) = run_design("C SegQueue", c, total, t);
            c_tps.push(tp);
        }

        let b_tp = median(&mut b_tps);
        let a_tp = median(&mut a_tps);
        let b2_tp = median(&mut b2_tps);
        let c_tp = median(&mut c_tps);
        println!(
            "  MEDIAN: baseline={:.0}  A={:.0}  B={:.0}  C={:.0}",
            b_tp, a_tp, b2_tp, c_tp
        );
        println!(
            "  overhead A = {:+.2}%  B = {:+.2}%  C = {:+.2}%  (vs baseline)",
            (a_tp / b_tp - 1.0) * 100.0,
            (b2_tp / b_tp - 1.0) * 100.0,
            (c_tp / b_tp - 1.0) * 100.0,
        );
    }
}
