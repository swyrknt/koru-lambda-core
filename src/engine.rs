use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

/// Identity hasher for keys whose bytes are already uniformly distributed
/// (i.e., SHA256-derived).
///
/// For each 16-byte `write` call, XORs in the leading 8 bytes (interpreted
/// as little-endian `u64`) at a per-write bit-rotation. The length-prefix
/// writes that the stdlib `Hash for [u8; N]` impl emits (8 bytes) are
/// ignored.
///
/// This handles both `[u8; 16]` single-array keys (one 16-byte data write
/// → state = first 8 bytes of array) and `([u8; 16], [u8; 16])` tuple
/// keys (two 16-byte data writes → state = `a8 XOR (b8 rotated)`), which
/// is critical for relationship tuples where many distinct edges share
/// `d0` or `d1` as the min element.
///
/// # Safety
///
/// Sound only when the keys are byte arrays of length 16 whose leading
/// bytes are uniformly distributed. Used here on `[u8; 16]` (Distinction
/// IDs) and `([u8; 16], [u8; 16])` tuples (canonical parent-pair
/// relationships). Both satisfy the precondition: bytes are SHA256
/// outputs (or the primordials `[0;16]` / `[1,0,…,0]`, exactly two
/// pinned values not subject to adversarial collision).
///
/// Per Exp 14 (2026): 6–13× hash speedup vs the default SipHash on
/// SHA256-distributed keys.
#[derive(Default)]
pub struct IdentityHasher {
    state: u64,
    writes: u32,
}

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.state
    }
    fn write(&mut self, bytes: &[u8]) {
        // The stdlib `Hash for [u8; N]` impl writes the length as a
        // `usize` prefix (8 bytes on 64-bit) BEFORE each array data
        // write (16 bytes). We only capture 16-byte writes (the data
        // payloads), accumulating into the state via XOR with a
        // per-write bit rotation so that the second element of a
        // tuple key contributes to the bucket.
        if bytes.len() != 16 {
            return;
        }
        let mut buf = [0u8; 8];
        buf.copy_from_slice(&bytes[..8]);
        let v = u64::from_le_bytes(buf);
        // Rotate by a multiple of 17 (coprime with 64) per write so
        // that successive elements end up in distinct bit positions.
        let rot = (self.writes.wrapping_mul(17)) & 63;
        self.state ^= v.rotate_left(rot);
        self.writes = self.writes.wrapping_add(1);
    }
}

/// Build-hasher alias for [`IdentityHasher`].
pub(crate) type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>;

/// A distinction is the fundamental unit of the system.
///
/// # Representation
///
/// A `Distinction` carries a single canonical 16-byte ID — the first 16
/// bytes of `SHA256(min_parent || max_parent)` for synthesized
/// distinctions, or `[0; 16]` / `[1, 0, ..., 0]` for the primordial Δ₀
/// and Δ₁ respectively.
///
/// `Distinction` is `Clone + PartialEq + Eq + Hash + Display + Debug`.
/// Step 7 of the foundation sub-branch makes the byte field `pub(crate)`
/// and drops `Distinction::new` entirely; from then on, every Distinction
/// in any consumer's hands either came from `engine.synthesize` (or
/// `engine.d0/d1`) or from `Distinction::from_hex` (an explicit hex
/// parse the consumer chose to do).
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Distinction {
    /// Canonical 16-byte ID. Authoritative identity.
    bytes: [u8; 16],
}

impl Distinction {
    /// Legacy constructor: parses the supplied String back into the
    /// 16-byte canonical representation. Step 7 removes this entirely.
    pub fn new(id: String) -> Self {
        let bytes = derive_bytes_from_legacy_id(&id);
        Self { bytes }
    }

    /// Returns the canonical 16-byte ID of this distinction.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    /// Internal constructor from raw bytes. The only callable Distinction
    /// constructor post step 7 (when `Distinction::new` is removed).
    pub(crate) fn from_bytes_internal(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }
}

/// Derive the canonical 16-byte representation from a legacy String id.
///
/// Only invoked by the soon-to-be-removed `Distinction::new(String)` (step 7).
///
/// Handles:
/// - Primordials `"0"` → `[0; 16]`, `"1"` → `[1, 0, ..., 0]`
/// - 32-char hex → decode directly (post-step-3 ids)
/// - 64-char SHA256 hex → first 16 bytes (pre-step-3 ids)
/// - Anything else → SHA256 the string and take first 16 bytes (covers
///   foreign IDs minted via `Distinction::new` until step 7 closes that
///   surface structurally)
fn derive_bytes_from_legacy_id(id: &str) -> [u8; 16] {
    match id {
        "0" => return [0u8; 16],
        "1" => {
            let mut b = [0u8; 16];
            b[0] = 1;
            return b;
        },
        _ => {},
    }
    if id.len() == 32 {
        if let Ok(bytes) = hex::decode(id) {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes);
            return arr;
        }
    }
    if id.len() == 64 {
        if let Ok(bytes) = hex::decode(id) {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes[..16]);
            return arr;
        }
    }
    // Fallback: hash whatever was supplied. Covers foreign-ID inputs
    // (Exp 9) that step 7 closes structurally.
    let digest = Sha256::digest(id.as_bytes());
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&digest[..16]);
    arr
}

/// Type alias for a canonical relationship between two distinctions.
///
/// Stored internally as `([u8; 16], [u8; 16])`; the public alias remains
/// `(String, String)` during steps 4–5 of the foundation sub-branch so
/// existing callers continue to compile. Step 5 mechanically rewrites
/// callers to the byte tuple; step 6 retypes this alias to
/// `([u8; 16], [u8; 16])`.
pub type Relationship = (String, String);

/// Type alias for a complete state snapshot.
pub type StateSnapshot = (Vec<Distinction>, Vec<Relationship>);

/// The core engine implementing the five axioms of distinction calculus:
/// 1. Identity: A distinction is defined solely by its unique identifier
/// 2. Nontriviality: The system initializes with two primordial distinctions (Δ₀, Δ₁)
/// 3. Synthesis: Two distinctions combine deterministically to create a third
/// 4. Symmetry: Relationships are bidirectional and order-independent
/// 5. Irreflexivity: A distinction synthesized with itself yields itself
///
/// Concurrency Model: Uses interior mutability via DashMap to allow concurrent
/// synthesis operations from multiple threads. The engine can be safely shared via
/// Arc<DistinctionEngine> across threads without requiring mutable access.
/// Internal alias: byte-keyed Distinction store, IdentityHasher-hashed.
type AllDistinctionsMap = DashMap<[u8; 16], Distinction, IdentityBuildHasher>;

/// Internal alias: canonical-pair relationship set, IdentityHasher-hashed.
type RelationshipMap = DashMap<([u8; 16], [u8; 16]), (), IdentityBuildHasher>;

#[derive(Debug)]
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    all_distinctions: AllDistinctionsMap,
    relationships: RelationshipMap,
}

impl DistinctionEngine {
    /// Creates a new engine with the two primordial distinctions.
    pub fn new() -> Self {
        let mut d1_bytes = [0u8; 16];
        d1_bytes[0] = 1;
        let d0 = Distinction::from_bytes_internal([0u8; 16]);
        let d1 = Distinction::from_bytes_internal(d1_bytes);

        let all_distinctions: AllDistinctionsMap =
            DashMap::with_hasher(IdentityBuildHasher::default());
        all_distinctions.insert(d0.bytes, d0.clone());
        all_distinctions.insert(d1.bytes, d1.clone());

        let relationships: RelationshipMap = DashMap::with_hasher(IdentityBuildHasher::default());
        // d0.bytes < d1.bytes is true ([0; 16] < [1, 0, ..., 0]).
        relationships.insert((d0.bytes, d1.bytes), ());

        Self { d0, d1, all_distinctions, relationships }
    }

    /// Returns a reference to the first primordial distinction (Δ₀).
    pub fn d0(&self) -> &Distinction {
        &self.d0
    }

    /// Returns a reference to the second primordial distinction (Δ₁).
    pub fn d1(&self) -> &Distinction {
        &self.d1
    }

    /// Retrieves a cloned Distinction by its unique ID (32-char hex).
    ///
    /// O(1) lookup via the internal byte-keyed DashMap. Parses the supplied
    /// hex string to bytes; returns `None` if the hex is malformed or the
    /// distinction isn't registered.
    ///
    /// Thread-safe: can be called concurrently from multiple threads.
    pub fn get_distinction_by_id(&self, id: &str) -> Option<Distinction> {
        let bytes = derive_bytes_from_legacy_id(id);
        self.all_distinctions.get(&bytes).map(|entry| entry.value().clone())
    }

    /// Adds a canonical relationship between two distinctions, keyed on bytes.
    fn add_relationship(&self, a: &[u8; 16], b: &[u8; 16]) {
        let (min, max) = if a <= b { (*a, *b) } else { (*b, *a) };
        self.relationships.insert((min, max), ());
    }

    /// Synthesizes two distinctions to create a third.
    /// Returns the resulting distinction.
    ///
    /// Implements:
    /// - Irreflexivity - synthesize(a, a) = a
    /// - Symmetry - synthesize(a, b) = synthesize(b, a)
    /// - Timeless consistency - repeated synthesis yields the same result
    ///
    /// Concurrency: This method is thread-safe and can be called concurrently
    /// from multiple threads without requiring mutable access to the engine.
    /// Uses DashMap for lock-free concurrent access.
    pub fn synthesize(&self, a: &Distinction, b: &Distinction) -> Distinction {
        // Irreflexivity - a distinction synthesized with itself yields itself
        if a.bytes == b.bytes {
            return a.clone();
        }

        // Symmetry - canonical ordering on the 16-byte ID ensures order
        // independence. Byte comparison; no String allocation in the hot path.
        let (first, second) =
            if a.bytes <= b.bytes { (&a.bytes, &b.bytes) } else { (&b.bytes, &a.bytes) };

        // Content addressing: SHA256(min || max), truncated to first 16 bytes.
        // Per Exp 13 (2026): 0 collisions in 268M synths at 16-byte truncation.
        let mut hasher = Sha256::new();
        hasher.update(first);
        hasher.update(second);
        let digest = hasher.finalize();
        let mut new_bytes = [0u8; 16];
        new_bytes.copy_from_slice(&digest[..16]);

        // Return existing if already synthesized (timeless consistency).
        if let Some(existing) = self.all_distinctions.get(&new_bytes) {
            return existing.clone();
        }

        // Create new distinction and establish relationships.
        // DashMap handles concurrent insertion safely.
        let new_distinction = Distinction::from_bytes_internal(new_bytes);
        self.all_distinctions.insert(new_bytes, new_distinction.clone());
        self.add_relationship(&new_bytes, &a.bytes);
        self.add_relationship(&new_bytes, &b.bytes);

        new_distinction
    }

    /// Returns a snapshot of all distinctions as a Vec.
    ///
    /// Note: In production, prefer iterating over distinctions directly
    /// rather than creating snapshots, to avoid collecting all values.
    pub fn get_distinctions_snapshot(&self) -> Vec<Distinction> {
        self.all_distinctions.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Returns a snapshot of all relationships as a Vec of canonical
    /// `(String, String)` hex tuples.
    ///
    /// Translates the internal byte-keyed storage to the public
    /// `Relationship = (String, String)` shape on the way out. Step 6 retypes
    /// the alias and removes the per-entry allocation.
    pub fn get_relationships_snapshot(&self) -> Vec<Relationship> {
        self.relationships
            .iter()
            .map(|entry| {
                let (a, b) = entry.key();
                (hex::encode(a), hex::encode(b))
            })
            .collect()
    }

    /// Returns a complete state snapshot.
    ///
    /// # Tearing semantics
    ///
    /// This method performs **two independent DashMap iterations** (one for
    /// distinctions, one for relationships) with no synchronization barrier
    /// between them. Under concurrent writes, the two halves are NOT consistent
    /// with each other: a distinction may appear in the first half while its
    /// relationships have not yet been written, or relationships may reference
    /// a distinction not present in the first half.
    ///
    /// Measured tearing rate: rare (~0.1% under 8 concurrent writers) but
    /// avalanche-sized when it occurs — when a tear happens, thousands of
    /// novel syntheses can land between the two iterations (Exp 6, 2026).
    ///
    /// # When to use
    ///
    /// - Quiescent reads (no concurrent writers): consistent.
    /// - Diagnostic / observability use during writes: tolerable.
    /// - **Persistence under load: NOT SAFE.** A future `get_state_snapshot_quiesced`
    ///   API (gated on a write barrier) will be added if and when a concrete
    ///   persistence consumer needs it.
    ///
    /// The suffix `_unsynchronized` is intentional: every caller acknowledges
    /// at the call site that the snapshot is not atomic across the two halves.
    pub fn get_state_snapshot_unsynchronized(&self) -> StateSnapshot {
        (self.get_distinctions_snapshot(), self.get_relationships_snapshot())
    }

    /// Returns the total number of distinctions in the engine.
    pub fn distinction_count(&self) -> usize {
        self.all_distinctions.len()
    }

    /// Returns the total number of relationships tracked.
    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }
}

impl Default for DistinctionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create an Arc-wrapped engine for concurrent access.
///
/// This is the recommended way to share a DistinctionEngine across threads
/// in production environments.
///
/// Example:
/// ```
/// use std::sync::Arc;
/// use koru_lambda_core::DistinctionEngine;
///
/// let engine = Arc::new(DistinctionEngine::new());
/// // Clone Arc for each thread
/// let engine_clone = Arc::clone(&engine);
/// ```
impl DistinctionEngine {
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}

#[cfg(test)]
mod identity_hasher_tests {
    use super::*;
    use std::collections::HashSet;
    use std::hash::Hash;

    fn hash_one<K: Hash>(k: &K) -> u64 {
        let mut h = IdentityHasher::default();
        k.hash(&mut h);
        h.finish()
    }

    #[test]
    fn identity_hash_of_byte_array_is_first_8_bytes_le() {
        let mut arr = [0u8; 16];
        arr[..8].copy_from_slice(&0xdead_beef_cafe_babe_u64.to_le_bytes());
        let h = hash_one(&arr);
        assert_eq!(h, 0xdead_beef_cafe_babe);
    }

    #[test]
    fn identity_hash_of_tuple_mixes_both_elements() {
        // Two tuples with equal first element but different second element
        // must produce distinct hashes, otherwise relationships rooted
        // on `d0` / `d1` would all bucket-collide.
        let mut a = [0u8; 16];
        a[0..8].copy_from_slice(&1u64.to_le_bytes());
        let mut b1 = [0u8; 16];
        b1[0] = 7;
        let mut b2 = [0u8; 16];
        b2[0] = 13;
        let h1 = hash_one(&(a, b1));
        let h2 = hash_one(&(a, b2));
        assert_ne!(h1, h2, "tuple hash must depend on the second element");
    }

    #[test]
    fn one_million_sha256_prefixes_collision_free() {
        // Drive 1M distinct inputs through SHA256 and take the first 16
        // bytes (mirroring the engine's `synthesize` truncation). The
        // first 8 bytes — what IdentityHasher reads — must be all
        // distinct, otherwise the engine's DashMap shards would degrade
        // catastrophically.
        let mut set: HashSet<u64> = HashSet::with_capacity(1_000_000);
        for i in 0u64..1_000_000 {
            let digest = Sha256::digest(i.to_le_bytes());
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&digest[..16]);
            let h = hash_one(&arr);
            assert!(set.insert(h), "u64-prefix collision at i={i}");
        }
        assert_eq!(set.len(), 1_000_000);
    }

    #[test]
    fn primordial_pinned_relationships_distinct() {
        // The two primordial-rooted relationships `(d0, d1)` exist by
        // construction; subsequent novel synths add `(d_min, parent)`
        // pairs. With `d0 = [0; 16]` and many `parent` values, the
        // hashes must spread across buckets.
        let d0 = [0u8; 16];
        let mut d1 = [0u8; 16];
        d1[0] = 1;
        let h_01 = hash_one(&(d0, d1));
        let mut set: HashSet<u64> = HashSet::with_capacity(10_000);
        set.insert(h_01);
        for i in 0u64..10_000 {
            let digest = Sha256::digest(i.to_le_bytes());
            let mut p = [0u8; 16];
            p.copy_from_slice(&digest[..16]);
            let h = hash_one(&(d0, p));
            assert!(set.insert(h), "d0-rooted relationship hash collision at i={i}");
        }
        // 10 001 distinct hashes (the `(d0, d1)` seed plus 10K novel).
        assert_eq!(set.len(), 10_001);
    }
}
