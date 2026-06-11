//! Exp 15 (rigorous): Full synthesize hot-path cost breakdown via criterion.
//!
//! Measure each step and the combined hot-path for three ID representations:
//!   - String (current engine)
//!   - [u8; 32]
//!   - [u8; 16]

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use dashmap::DashMap;
use once_cell::sync::Lazy;
use sha2::{Digest, Sha256};

static PARENTS_STR: Lazy<Vec<String>> = Lazy::new(|| {
    (0..1024)
        .map(|i| format!("{:x}", Sha256::digest(format!("parent-{i}").as_bytes())))
        .collect()
});

static PARENTS_32: Lazy<Vec<[u8; 32]>> = Lazy::new(|| {
    (0..1024)
        .map(|i| {
            let mut k = [0u8; 32];
            k.copy_from_slice(&Sha256::digest(format!("parent-{i}").as_bytes()));
            k
        })
        .collect()
});

static PARENTS_16: Lazy<Vec<[u8; 16]>> = Lazy::new(|| {
    (0..1024)
        .map(|i| {
            let mut k = [0u8; 16];
            k.copy_from_slice(&Sha256::digest(format!("parent-{i}").as_bytes())[..16]);
            k
        })
        .collect()
});

fn bench_sha256_digest(c: &mut Criterion) {
    let a = &PARENTS_STR[10];
    let b = &PARENTS_STR[20];
    let input = format!("{a}:{b}");
    let input_bytes = input.as_bytes();
    c.bench_function("sha256_digest_129bytes", |bn| {
        bn.iter(|| {
            let d = Sha256::digest(black_box(input_bytes));
            black_box(d)
        })
    });

    let input2 = [&PARENTS_32[10][..], b":", &PARENTS_32[20][..]].concat();
    c.bench_function("sha256_digest_65bytes", |bn| {
        bn.iter(|| {
            let d = Sha256::digest(black_box(&input2[..]));
            black_box(d)
        })
    });
}

fn bench_hex_encode(c: &mut Criterion) {
    let d = Sha256::digest(b"test");
    let bytes: [u8; 32] = d.into();
    c.bench_function("hex_encode_32bytes", |bn| {
        bn.iter(|| {
            let s = hex::encode(black_box(bytes));
            black_box(s)
        })
    });
    c.bench_function("format_x_digest", |bn| {
        bn.iter(|| {
            let d = Sha256::digest(black_box(b"test"));
            let s = format!("{d:x}");
            black_box(s)
        })
    });
}

fn bench_full_synth_string(c: &mut Criterion) {
    let engine: DashMap<String, String> = DashMap::new();
    for p in PARENTS_STR.iter() {
        engine.insert(p.clone(), p.clone());
    }

    let a = PARENTS_STR[10].clone();
    let b = PARENTS_STR[20].clone();

    // Precompute and insert the result so we hit the "hit" branch.
    let (lo, hi) = if a < b { (&a, &b) } else { (&b, &a) };
    let result_id = format!("{:x}", Sha256::digest(format!("{lo}:{hi}").as_bytes()));
    engine.insert(result_id.clone(), result_id.clone());

    c.bench_function("synth_string_hit_path", |bn| {
        bn.iter(|| {
            let a_id = black_box(&a);
            let b_id = black_box(&b);
            if a_id == b_id {
                return String::new();
            }
            let (first, second) = if a_id < b_id { (a_id, b_id) } else { (b_id, a_id) };
            let new_id_str = format!("{first}:{second}");
            let new_id = format!("{:x}", Sha256::digest(new_id_str.as_bytes()));
            engine.get(&new_id).map(|e| e.value().clone()).unwrap_or_default()
        })
    });
}

fn bench_full_synth_bytes32(c: &mut Criterion) {
    let engine: DashMap<[u8; 32], [u8; 32]> = DashMap::new();
    for p in PARENTS_32.iter() {
        engine.insert(*p, *p);
    }
    let a = PARENTS_32[10];
    let b = PARENTS_32[20];
    let (lo, hi) = if a < b { (&a, &b) } else { (&b, &a) };
    let mut result = [0u8; 32];
    let mut h = Sha256::new();
    h.update(lo);
    h.update(b":");
    h.update(hi);
    result.copy_from_slice(&h.finalize());
    engine.insert(result, result);

    c.bench_function("synth_bytes32_hit_path", |bn| {
        bn.iter(|| {
            let a_id = black_box(&a);
            let b_id = black_box(&b);
            if a_id == b_id {
                return [0u8; 32];
            }
            let (lo, hi) = if a_id < b_id { (a_id, b_id) } else { (b_id, a_id) };
            let mut h = Sha256::new();
            h.update(lo);
            h.update(b":");
            h.update(hi);
            let mut out = [0u8; 32];
            out.copy_from_slice(&h.finalize());
            engine.get(&out).map(|e| *e.value()).unwrap_or([0u8; 32])
        })
    });
}

fn bench_full_synth_bytes16(c: &mut Criterion) {
    let engine: DashMap<[u8; 16], [u8; 16]> = DashMap::new();
    for p in PARENTS_16.iter() {
        engine.insert(*p, *p);
    }
    let a = PARENTS_16[10];
    let b = PARENTS_16[20];
    let (lo, hi) = if a < b { (&a, &b) } else { (&b, &a) };
    let mut result = [0u8; 16];
    let mut h = Sha256::new();
    h.update(lo);
    h.update(b":");
    h.update(hi);
    result.copy_from_slice(&h.finalize()[..16]);
    engine.insert(result, result);

    c.bench_function("synth_bytes16_hit_path", |bn| {
        bn.iter(|| {
            let a_id = black_box(&a);
            let b_id = black_box(&b);
            if a_id == b_id {
                return [0u8; 16];
            }
            let (lo, hi) = if a_id < b_id { (a_id, b_id) } else { (b_id, a_id) };
            let mut h = Sha256::new();
            h.update(lo);
            h.update(b":");
            h.update(hi);
            let mut out = [0u8; 16];
            out.copy_from_slice(&h.finalize()[..16]);
            engine.get(&out).map(|e| *e.value()).unwrap_or([0u8; 16])
        })
    });
}

criterion_group!(
    benches,
    bench_sha256_digest,
    bench_hex_encode,
    bench_full_synth_string,
    bench_full_synth_bytes32,
    bench_full_synth_bytes16,
);
criterion_main!(benches);
