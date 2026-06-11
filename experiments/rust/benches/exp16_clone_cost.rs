//! Exp 16: Copy vs Clone for Distinction representations.
//!
//! Measure:
//!   - String-backed Distinction (current)
//!   - [u8; 16] Distinction (proposed 2.0)
//!   - [u8; 32] Distinction (untruncated)
//!
//! In a synthesize hot loop, how much of total time is clone cost?

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use sha2::{Digest, Sha256};

#[derive(Clone, PartialEq, Eq, Hash)]
struct DistinctionString {
    id: String,
}

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct Distinction16([u8; 16]);

#[derive(Copy, Clone, PartialEq, Eq, Hash)]
struct Distinction32([u8; 32]);

fn bench_clone(c: &mut Criterion) {
    let s_id = format!("{:x}", Sha256::digest(b"test"));
    let d_string = DistinctionString { id: s_id };
    let mut b16 = [0u8; 16];
    b16.copy_from_slice(&Sha256::digest(b"test")[..16]);
    let d16 = Distinction16(b16);
    let mut b32 = [0u8; 32];
    b32.copy_from_slice(&Sha256::digest(b"test"));
    let d32 = Distinction32(b32);

    c.bench_function("clone_string_distinction", |b| {
        b.iter(|| {
            let x = black_box(&d_string).clone();
            black_box(x)
        })
    });

    c.bench_function("copy_distinction_16", |b| {
        b.iter(|| {
            let x = *black_box(&d16);
            black_box(x)
        })
    });

    c.bench_function("copy_distinction_32", |b| {
        b.iter(|| {
            let x = *black_box(&d32);
            black_box(x)
        })
    });
}

fn bench_clone_in_loop(c: &mut Criterion) {
    let s_id = format!("{:x}", Sha256::digest(b"test"));
    let d_string = DistinctionString { id: s_id };
    let mut b16 = [0u8; 16];
    b16.copy_from_slice(&Sha256::digest(b"test")[..16]);
    let d16 = Distinction16(b16);
    let mut b32 = [0u8; 32];
    b32.copy_from_slice(&Sha256::digest(b"test"));
    let d32 = Distinction32(b32);

    c.bench_function("clone_string_100x", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(100);
            for _ in 0..100 {
                v.push(black_box(&d_string).clone());
            }
            black_box(v)
        })
    });

    c.bench_function("copy_16_100x", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(100);
            for _ in 0..100 {
                v.push(*black_box(&d16));
            }
            black_box(v)
        })
    });

    c.bench_function("copy_32_100x", |b| {
        b.iter(|| {
            let mut v = Vec::with_capacity(100);
            for _ in 0..100 {
                v.push(*black_box(&d32));
            }
            black_box(v)
        })
    });
}

fn bench_equality(c: &mut Criterion) {
    let s_id = format!("{:x}", Sha256::digest(b"test"));
    let a = DistinctionString { id: s_id.clone() };
    let b = DistinctionString { id: s_id };

    let mut b16a = [0u8; 16];
    b16a.copy_from_slice(&Sha256::digest(b"test")[..16]);
    let d16a = Distinction16(b16a);
    let d16b = Distinction16(b16a);

    let mut b32a = [0u8; 32];
    b32a.copy_from_slice(&Sha256::digest(b"test"));
    let d32a = Distinction32(b32a);
    let d32b = Distinction32(b32a);

    c.bench_function("eq_string", |b_| {
        b_.iter(|| black_box(&a) == black_box(&b))
    });
    c.bench_function("eq_16", |b_| b_.iter(|| black_box(&d16a) == black_box(&d16b)));
    c.bench_function("eq_32", |b_| b_.iter(|| black_box(&d32a) == black_box(&d32b)));
}

criterion_group!(benches, bench_clone, bench_clone_in_loop, bench_equality);
criterion_main!(benches);
