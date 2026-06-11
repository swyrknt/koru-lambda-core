//! Exp 15: Synth cost breakdown.
//!
//! Profile a single synthesize() call to determine where time goes:
//!   - SHA256 digest
//!   - hex::encode
//!   - format! string allocation
//!   - DashMap probe (hit / miss)
//!   - DashMap insert
//!   - Distinction clone (String clone)
//!
//! This informs the [u8; 32] migration: how many ns are we saving per synth,
//! and what fraction is that of the total?

use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::time::Instant;

const WARMUP: u64 = 10_000;
const ITERS: u64 = 1_000_000;

fn time_ns<F: FnMut() -> u64>(label: &str, mut f: F) {
    // warmup
    let mut sink = 0u64;
    for _ in 0..WARMUP {
        sink = sink.wrapping_add(f());
    }
    let t = Instant::now();
    for _ in 0..ITERS {
        sink = sink.wrapping_add(f());
    }
    let elapsed = t.elapsed().as_nanos();
    let per = elapsed as f64 / ITERS as f64;
    println!("  {label:<40} {per:>8.2} ns/op   (sink={})", sink & 0xFF);
}

fn main() {
    println!("=== Exp 15: Synth cost breakdown ===\n");

    // Precompute some parent id strings of realistic SHA256 hex length (64 bytes).
    let parents: Vec<String> = (0..1024)
        .map(|i| format!("{:x}", Sha256::digest(format!("parent-{i}").as_bytes())))
        .collect();

    // 1. SHA256 digest on "a:b" input
    let a = &parents[0];
    let b = &parents[1];
    let ab = format!("{a}:{b}");
    time_ns("SHA256 digest (128-char input)", || {
        let d = Sha256::digest(ab.as_bytes());
        d[0] as u64
    });

    // 2. hex::encode of 32 bytes
    let digest32 = Sha256::digest(b"hello world");
    let digest_bytes: [u8; 32] = digest32.into();
    time_ns("hex::encode([u8; 32])", || {
        let s = hex::encode(digest_bytes);
        s.len() as u64
    });

    // 3. format!("{:x}", digest) which is how engine.rs does it
    time_ns("format!(\"{:x}\", Sha256::digest)", || {
        let d = Sha256::digest(b"hello");
        let s = format!("{d:x}");
        s.len() as u64
    });

    // 4. format!("{}:{}", a, b) string alloc
    time_ns("format!(\"{}:{}\", a, b) -- 128+1 chars", || {
        let s = format!("{a}:{b}");
        s.len() as u64
    });

    // 5. DashMap<String, ()> probe (miss)
    let miss_map: DashMap<String, ()> = DashMap::new();
    for i in 0..10_000 {
        miss_map.insert(format!("prefilled-{i}"), ());
    }
    time_ns("DashMap<String,()> probe miss (N=10K)", || {
        let r = miss_map.contains_key("absent-key");
        r as u64
    });

    // 6. DashMap<String, ()> probe (hit)
    let hit_map: DashMap<String, ()> = DashMap::new();
    for p in &parents {
        hit_map.insert(p.clone(), ());
    }
    let hit_key = parents[500].clone();
    time_ns("DashMap<String,()> probe hit", || {
        hit_map.contains_key(&hit_key) as u64
    });

    // 7. DashMap<String, String> get + clone on hit (mirrors engine.rs:119-121)
    let dm: DashMap<String, String> = DashMap::new();
    for p in &parents {
        dm.insert(p.clone(), p.clone());
    }
    let hit_k = parents[500].clone();
    time_ns("DashMap<String,String> get+clone hit", || {
        dm.get(&hit_k).map(|e| e.value().clone()).unwrap().len() as u64
    });

    // 8. DashMap<String, ()> insert fresh (amortized)
    time_ns("DashMap<String,()> insert fresh", || {
        let k = format!("{:x}", Sha256::digest(b"x"));
        let mut hasher = Sha256::new();
        hasher.update(k.as_bytes());
        let h = hasher.finalize();
        h[0] as u64
    });

    // 9. String clone of a 64-char SHA256 hex
    let long = parents[0].clone();
    time_ns("String clone (64-char)", || {
        let s = long.clone();
        s.len() as u64
    });

    // 10. Simulated full synthesize path (hit/idempotent)
    let all_d: DashMap<String, String> = DashMap::new();
    for p in &parents {
        all_d.insert(p.clone(), p.clone());
    }
    let id_a = parents[10].clone();
    let id_b = parents[20].clone();
    let (first, second) = if id_a < id_b { (&id_a, &id_b) } else { (&id_b, &id_a) };
    let new_id_str = format!("{first}:{second}");
    let new_id = format!("{:x}", Sha256::digest(new_id_str.as_bytes()));
    all_d.insert(new_id.clone(), new_id.clone());

    time_ns("full synth (String-based, hit path)", || {
        // Replicate engine.rs lines 103-130 for a hit.
        let a_id = &id_a;
        let b_id = &id_b;
        if a_id == b_id {
            return 0;
        }
        let (first, second) = if a_id < b_id { (a_id, b_id) } else { (b_id, a_id) };
        let new_id_str = format!("{first}:{second}");
        let new_id = format!("{:x}", Sha256::digest(new_id_str.as_bytes()));
        let existing = all_d.get(&new_id).map(|e| e.value().clone());
        existing.map(|s| s.len()).unwrap_or(0) as u64
    });

    // 11. Simulated synth path using [u8; 32] id (no hex, no alloc)
    let all_b: DashMap<[u8; 32], [u8; 32]> = DashMap::new();
    let mut id_a_b = [0u8; 32];
    let mut id_b_b = [0u8; 32];
    id_a_b.copy_from_slice(&Sha256::digest(b"parent-10"));
    id_b_b.copy_from_slice(&Sha256::digest(b"parent-20"));
    let new_id_b: [u8; 32] = {
        let (lo, hi) = if id_a_b < id_b_b { (&id_a_b, &id_b_b) } else { (&id_b_b, &id_a_b) };
        let mut h = Sha256::new();
        h.update(lo);
        h.update(b":");
        h.update(hi);
        let mut out = [0u8; 32];
        out.copy_from_slice(&h.finalize());
        out
    };
    all_b.insert(new_id_b, new_id_b);

    time_ns("full synth ([u8;32]-based, hit path)", || {
        if id_a_b == id_b_b {
            return 0;
        }
        let (lo, hi) = if id_a_b < id_b_b { (&id_a_b, &id_b_b) } else { (&id_b_b, &id_a_b) };
        let mut h = Sha256::new();
        h.update(lo);
        h.update(b":");
        h.update(hi);
        let mut out = [0u8; 32];
        out.copy_from_slice(&h.finalize());
        let existing = all_b.get(&out).map(|e| *e.value());
        existing.map(|x| x[0]).unwrap_or(0) as u64
    });

    // 12. Same but with [u8; 16]
    let all_16: DashMap<[u8; 16], [u8; 16]> = DashMap::new();
    let mut id_a_16 = [0u8; 16];
    let mut id_b_16 = [0u8; 16];
    id_a_16.copy_from_slice(&Sha256::digest(b"parent-10")[..16]);
    id_b_16.copy_from_slice(&Sha256::digest(b"parent-20")[..16]);
    let new_id_16: [u8; 16] = {
        let (lo, hi) = if id_a_16 < id_b_16 { (&id_a_16, &id_b_16) } else { (&id_b_16, &id_a_16) };
        let mut h = Sha256::new();
        h.update(lo);
        h.update(b":");
        h.update(hi);
        let mut out = [0u8; 16];
        out.copy_from_slice(&h.finalize()[..16]);
        out
    };
    all_16.insert(new_id_16, new_id_16);

    time_ns("full synth ([u8;16]-based, hit path)", || {
        if id_a_16 == id_b_16 {
            return 0;
        }
        let (lo, hi) = if id_a_16 < id_b_16 { (&id_a_16, &id_b_16) } else { (&id_b_16, &id_a_16) };
        let mut h = Sha256::new();
        h.update(lo);
        h.update(b":");
        h.update(hi);
        let mut out = [0u8; 16];
        out.copy_from_slice(&h.finalize()[..16]);
        let existing = all_16.get(&out).map(|e| *e.value());
        existing.map(|x| x[0]).unwrap_or(0) as u64
    });
}
