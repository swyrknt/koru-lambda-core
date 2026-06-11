//! Exp 14: Hash rehashing waste.
//!
//! DashMap default hasher is AHash. When the KEY is ALREADY a SHA256 hex string
//! (or its 32-byte binary form), rehashing it with AHash does full input-dependent
//! work -- we're computing a fast hash of a slow hash.
//!
//! Compare:
//!   1. DashMap<String, ()> + AHash (default)                  -- current engine
//!   2. DashMap<[u8; 32], ()> + AHash                          -- migrated to bytes
//!   3. DashMap<[u8; 32], (), IdentityHasher>                  -- skip rehashing for bytes
//!
//! We do NOT test IdentityHasher on String: hex strings have only 4 bits entropy
//! per byte, and AHash's wider mixing is actually necessary for good distribution.
//! On [u8; 32] keys (SHA256 binary output), the bits are uniformly random, so a
//! simple identity hash from the first 8 bytes is ~optimal.

use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::hash::{BuildHasherDefault, Hasher};

#[derive(Default)]
struct IdentityHasher {
    state: u64,
}

impl Hasher for IdentityHasher {
    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        // For [u8; 32] keys, take the first 8 bytes directly.
        // These are already uniformly random (output of SHA256).
        if bytes.len() >= 8 {
            self.state = u64::from_le_bytes(bytes[..8].try_into().unwrap());
        } else {
            let mut buf = [0u8; 8];
            buf[..bytes.len()].copy_from_slice(bytes);
            self.state = u64::from_le_bytes(buf);
        }
    }
    #[inline]
    fn finish(&self) -> u64 {
        self.state
    }
}

type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>;

const N: usize = 100_000;

fn make_hex_keys() -> Vec<String> {
    (0..N)
        .map(|i| format!("{:x}", Sha256::digest(format!("k-{i}").as_bytes())))
        .collect()
}

fn make_bytes32_keys() -> Vec<[u8; 32]> {
    (0..N)
        .map(|i| {
            let mut k = [0u8; 32];
            k.copy_from_slice(&Sha256::digest(format!("k-{i}").as_bytes()));
            k
        })
        .collect()
}

fn bench_insert_string_ahash(c: &mut Criterion) {
    let keys = make_hex_keys();
    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(N as u64));
    group.bench_function("string_ahash_100k", |b| {
        b.iter(|| {
            let m: DashMap<String, ()> = DashMap::with_capacity(N);
            for k in &keys {
                m.insert(k.clone(), ());
            }
            black_box(m.len())
        })
    });
    group.finish();
}

fn bench_insert_bytes32_ahash(c: &mut Criterion) {
    let keys = make_bytes32_keys();
    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(N as u64));
    group.bench_function("bytes32_ahash_100k", |b| {
        b.iter(|| {
            let m: DashMap<[u8; 32], ()> = DashMap::with_capacity(N);
            for k in &keys {
                m.insert(*k, ());
            }
            black_box(m.len())
        })
    });
    group.finish();
}

fn bench_insert_bytes32_identity(c: &mut Criterion) {
    let keys = make_bytes32_keys();
    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(N as u64));
    group.bench_function("bytes32_identity_100k", |b| {
        b.iter(|| {
            let m: DashMap<[u8; 32], (), IdentityBuildHasher> =
                DashMap::with_capacity_and_hasher(N, IdentityBuildHasher::default());
            for k in &keys {
                m.insert(*k, ());
            }
            black_box(m.len())
        })
    });
    group.finish();
}

fn bench_insert_bytes16_ahash(c: &mut Criterion) {
    let keys: Vec<[u8; 16]> = (0..N)
        .map(|i| {
            let mut k = [0u8; 16];
            k.copy_from_slice(&Sha256::digest(format!("k-{i}").as_bytes())[..16]);
            k
        })
        .collect();
    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(N as u64));
    group.bench_function("bytes16_ahash_100k", |b| {
        b.iter(|| {
            let m: DashMap<[u8; 16], ()> = DashMap::with_capacity(N);
            for k in &keys {
                m.insert(*k, ());
            }
            black_box(m.len())
        })
    });
    group.finish();
}

fn bench_insert_bytes16_identity(c: &mut Criterion) {
    let keys: Vec<[u8; 16]> = (0..N)
        .map(|i| {
            let mut k = [0u8; 16];
            k.copy_from_slice(&Sha256::digest(format!("k-{i}").as_bytes())[..16]);
            k
        })
        .collect();
    let mut group = c.benchmark_group("insert");
    group.throughput(Throughput::Elements(N as u64));
    group.bench_function("bytes16_identity_100k", |b| {
        b.iter(|| {
            let m: DashMap<[u8; 16], (), IdentityBuildHasher> =
                DashMap::with_capacity_and_hasher(N, IdentityBuildHasher::default());
            for k in &keys {
                m.insert(*k, ());
            }
            black_box(m.len())
        })
    });
    group.finish();
}

fn bench_get_string_ahash(c: &mut Criterion) {
    let keys = make_hex_keys();
    let m: DashMap<String, ()> = DashMap::with_capacity(N);
    for k in &keys {
        m.insert(k.clone(), ());
    }
    c.bench_function("get_string_ahash", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % keys.len();
            black_box(m.contains_key(&keys[i]))
        })
    });
}

fn bench_get_bytes32_ahash(c: &mut Criterion) {
    let keys = make_bytes32_keys();
    let m: DashMap<[u8; 32], ()> = DashMap::with_capacity(N);
    for k in &keys {
        m.insert(*k, ());
    }
    c.bench_function("get_bytes32_ahash", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % keys.len();
            black_box(m.contains_key(&keys[i]))
        })
    });
}

fn bench_get_bytes32_identity(c: &mut Criterion) {
    let keys = make_bytes32_keys();
    let m: DashMap<[u8; 32], (), IdentityBuildHasher> =
        DashMap::with_capacity_and_hasher(N, IdentityBuildHasher::default());
    for k in &keys {
        m.insert(*k, ());
    }
    c.bench_function("get_bytes32_identity", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % keys.len();
            black_box(m.contains_key(&keys[i]))
        })
    });
}

fn bench_get_bytes16_ahash(c: &mut Criterion) {
    let keys: Vec<[u8; 16]> = (0..N)
        .map(|i| {
            let mut k = [0u8; 16];
            k.copy_from_slice(&Sha256::digest(format!("k-{i}").as_bytes())[..16]);
            k
        })
        .collect();
    let m: DashMap<[u8; 16], ()> = DashMap::with_capacity(N);
    for k in &keys {
        m.insert(*k, ());
    }
    c.bench_function("get_bytes16_ahash", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % keys.len();
            black_box(m.contains_key(&keys[i]))
        })
    });
}

fn bench_get_bytes16_identity(c: &mut Criterion) {
    let keys: Vec<[u8; 16]> = (0..N)
        .map(|i| {
            let mut k = [0u8; 16];
            k.copy_from_slice(&Sha256::digest(format!("k-{i}").as_bytes())[..16]);
            k
        })
        .collect();
    let m: DashMap<[u8; 16], (), IdentityBuildHasher> =
        DashMap::with_capacity_and_hasher(N, IdentityBuildHasher::default());
    for k in &keys {
        m.insert(*k, ());
    }
    c.bench_function("get_bytes16_identity", |b| {
        let mut i = 0usize;
        b.iter(|| {
            i = (i + 1) % keys.len();
            black_box(m.contains_key(&keys[i]))
        })
    });
}

criterion_group!(
    benches,
    bench_insert_string_ahash,
    bench_insert_bytes32_ahash,
    bench_insert_bytes32_identity,
    bench_insert_bytes16_ahash,
    bench_insert_bytes16_identity,
    bench_get_string_ahash,
    bench_get_bytes32_ahash,
    bench_get_bytes32_identity,
    bench_get_bytes16_ahash,
    bench_get_bytes16_identity,
);
criterion_main!(benches);
