use crossbeam_queue::SegQueue;
use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
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
///
/// # Construction
///
/// The byte field is `pub(crate)` and there is **no public constructor**.
/// External code obtains a `Distinction` value only through:
///
/// - `engine.synthesize(a, b)` — the canonical path
/// - `engine.d0() / engine.d1()` — the primordials
/// - `engine.get_distinction_by_id(hex_str)` — lookup of a known ID
/// - `Distinction::from_hex(hex_str)` — explicit hex parse (the value
///   produced here is well-formed bytes; whether the engine recognises
///   it as a registered distinction is a separate question)
///
/// This closes the foreign-ID poisoning class structurally at compile
/// time (Exp 9 / V1 / N3 / N4) — no defensive runtime checks needed.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Distinction {
    /// Canonical 16-byte ID. Authoritative identity.
    pub(crate) bytes: [u8; 16],
}

impl Distinction {
    /// Returns the canonical 16-byte ID of this distinction.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    /// Internal constructor from raw bytes.
    pub(crate) fn from_bytes_internal(bytes: [u8; 16]) -> Self {
        Self { bytes }
    }
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

/// Internal alias: synthesis log entry storage. Each entry is a canonical
/// `(min, max)` parent tuple. `Option<...>` lets `without_log()` opt out.
type SynthesisLog = Option<SegQueue<(Distinction, Distinction)>>;

/// Internal alias: forward parent index, `child -> (parent_a, parent_b)`
/// canonical pair. IdentityHasher-hashed.
type ParentsMap = DashMap<[u8; 16], ([u8; 16], [u8; 16]), IdentityBuildHasher>;

/// Internal alias: reverse children index, `parent -> Vec<child>`.
/// IdentityHasher-hashed.
type ChildrenMap = DashMap<[u8; 16], Vec<[u8; 16]>, IdentityBuildHasher>;

/// Internal alias: degree cache, `node -> AtomicUsize`. IdentityHasher-hashed.
/// Each `synthesize()` novel-path call increments the degree of both parents
/// by 1 (each gains exactly one new relationship: parent → new child).
type DegreeMap = DashMap<[u8; 16], AtomicUsize, IdentityBuildHasher>;

#[derive(Debug)]
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    all_distinctions: AllDistinctionsMap,
    relationships: RelationshipMap,
    /// Append-only synthesis log. `Some(...)` for engines constructed via
    /// `new()`; `None` for `without_log()` engines. Each entry is a canonical
    /// `(min, max)` parent pair, pushed on every novel synthesis. Per
    /// Decision 5.7, canonical ordering enables future Merkle-over-log and
    /// cross-peer log diffing without rewriting persisted logs.
    log: SynthesisLog,
    /// Forward parent index: child bytes → canonical (parent_a, parent_b)
    /// bytes. Populated in `synthesize()` on the novel path. Section 2.2.
    parents_index: ParentsMap,
    /// Reverse children index: parent bytes → Vec of child bytes.
    /// Populated in `synthesize()` on the novel path. Section 2.2.
    children_index: ChildrenMap,
    /// Degree cache: node bytes → AtomicUsize relationship count.
    /// Populated in `synthesize()` on the novel path. Section 2.2.
    /// Primordials d0 and d1 are seeded at degree 1 to reflect the
    /// genesis (d0, d1) relationship.
    degree_cache: DegreeMap,
}

impl DistinctionEngine {
    /// Creates a new engine with the two primordial distinctions and an
    /// active synthesis log.
    pub fn new() -> Self {
        Self::new_inner(true)
    }

    /// Creates a new engine with the two primordial distinctions and no
    /// synthesis log. Use this when persistence is not needed and the log's
    /// memory cost (Exp 7: ~81 MB per 1M synths) is undesirable.
    ///
    /// Engines constructed via `without_log()` will return empty results
    /// from `synthesis_log_snapshot()` and `0` from `synthesis_log_len()`.
    /// All other engine behavior is unchanged.
    pub fn without_log() -> Self {
        Self::new_inner(false)
    }

    fn new_inner(with_log: bool) -> Self {
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

        // The seed (d0, d1) relationship is NOT pushed onto the log; the log
        // records *novel synthesis events*, and the seed is part of the
        // engine's initial state. Replay from an empty log against a fresh
        // `new()` engine reproduces the seed state automatically.
        let log = if with_log { Some(SegQueue::new()) } else { None };

        // Traversal indices (Section 2.2). Primordials d0 and d1 have no
        // parents (parents_index empty for them). They have one child each
        // post-genesis: each other (children_index empty until first
        // synthesis registers their shared child). They have degree 1
        // (the seed (d0, d1) relationship).
        let parents_index: ParentsMap = DashMap::with_hasher(IdentityBuildHasher::default());
        let children_index: ChildrenMap = DashMap::with_hasher(IdentityBuildHasher::default());
        let degree_cache: DegreeMap = DashMap::with_hasher(IdentityBuildHasher::default());
        degree_cache.insert(d0.bytes, AtomicUsize::new(1));
        degree_cache.insert(d1.bytes, AtomicUsize::new(1));

        Self {
            d0,
            d1,
            all_distinctions,
            relationships,
            log,
            parents_index,
            children_index,
            degree_cache,
        }
    }

    /// Returns a reference to the first primordial distinction (Δ₀).
    pub fn d0(&self) -> &Distinction {
        &self.d0
    }

    /// Returns a reference to the second primordial distinction (Δ₁).
    pub fn d1(&self) -> &Distinction {
        &self.d1
    }

    /// Retrieves a cloned Distinction by its unique 32-character hex ID.
    ///
    /// O(1) lookup via the internal byte-keyed DashMap. Returns `None` if
    /// the hex is malformed or the distinction isn't registered.
    ///
    /// Thread-safe: can be called concurrently from multiple threads.
    pub fn get_distinction_by_id(&self, id: &str) -> Option<Distinction> {
        let parsed = Distinction::from_hex(id).ok()?;
        self.all_distinctions.get(parsed.as_bytes()).map(|entry| entry.value().clone())
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

        // Append canonical (min, max) parent tuple to the synthesis log
        // (CHECKLIST 2.3, Decision 5.7). `first` and `second` are already
        // canonically ordered by the comparison at line 211. Skipped on
        // engines built via `without_log()`. A saturation race could let
        // two threads both pass the existence check and push duplicate
        // entries; this is benign because replay is idempotent — the
        // second synthesize call on the duplicate pair hits saturation.
        if let Some(log) = &self.log {
            log.push((
                Distinction::from_bytes_internal(*first),
                Distinction::from_bytes_internal(*second),
            ));
        }

        // Populate traversal indices (Section 2.2). Engine-first ordering
        // per Exp 8: distinction + relationships are inserted FIRST (above);
        // the new indices are populated AFTER. A concurrent reader querying
        // children_of(d) may observe slightly fewer children than
        // all_distinctions contains. This is the safe direction — no
        // orphan IDs in the indices pointing to unregistered distinctions.
        // Parents store the canonical (min, max) pair.
        self.parents_index.insert(new_bytes, (*first, *second));
        self.children_index.entry(a.bytes).or_default().push(new_bytes);
        self.children_index.entry(b.bytes).or_default().push(new_bytes);
        // Each parent gains +1 degree from the new (parent → new_distinction)
        // relationship.
        self.degree_cache
            .entry(a.bytes)
            .or_insert_with(|| AtomicUsize::new(0))
            .fetch_add(1, Ordering::Relaxed);
        self.degree_cache
            .entry(b.bytes)
            .or_insert_with(|| AtomicUsize::new(0))
            .fetch_add(1, Ordering::Relaxed);
        // New distinction starts at degree 2 (its two parent relationships).
        self.degree_cache.insert(new_bytes, AtomicUsize::new(2));

        new_distinction
    }

    /// Returns `true` if the engine satisfies the structural invariant
    /// `r = 2d - 3` (for `d >= 2`), or `true` trivially when `d < 2`.
    ///
    /// # Concurrency
    ///
    /// This method is only meaningful when **no concurrent writers** are
    /// active. Mid-synthesize, the engine transiently observes
    /// `d_new + 1, r_new` between the `all_distinctions.insert` and the first
    /// `add_relationship` call, and `d_new + 1, r_new + 1` between the two
    /// `add_relationship` calls. A concurrent reader can therefore see
    /// `r != 2d - 3` even though the long-term invariant holds.
    ///
    /// Use this from tests and quiescent diagnostics; do NOT use it as a
    /// hot-path assertion. The `r = 2d - 3` invariant is verified at scale
    /// by `tests/falsification/structural_coherence.rs` and Exp 2 (2026,
    /// 5M synths, zero deviations).
    ///
    /// Section 2.4 / Phase 6 sub-branch #3.
    pub fn check_structural_invariant(&self) -> bool {
        let d = self.distinction_count();
        let r = self.relationship_count();
        d < 2 || r == 2 * d - 3
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

    /// Returns the number of relationships involving this distinction.
    /// `O(1)` via cached `AtomicUsize`. Returns `0` if the distinction is
    /// not registered in this engine.
    ///
    /// Genesis: `degree(d0) == 1` and `degree(d1) == 1` (the seed
    /// relationship). Each subsequent novel synthesis increments the degree
    /// of both parents by 1, and the new child is initialized at degree 2.
    ///
    /// Section 2.2 / Phase 6 sub-branch #5.
    pub fn degree(&self, d: &Distinction) -> usize {
        self.degree_cache.get(&d.bytes).map(|e| e.value().load(Ordering::Relaxed)).unwrap_or(0)
    }

    /// Returns the canonical `(min, max)` parents whose synthesis produced
    /// this distinction. `O(1)` via the forward parent index. Returns `None`
    /// for primordials (`d0`, `d1`) and for any distinction not registered
    /// in this engine.
    ///
    /// The returned pair is in canonical byte order (the smaller-bytes
    /// parent first), regardless of the call order at synthesis time.
    /// `engine.synthesize(d1, d0)` and `engine.synthesize(d0, d1)` produce
    /// the same child and `parents_of(child) == Some((d0, d1))` in both
    /// cases.
    ///
    /// Section 2.2 / Phase 6 sub-branch #5.
    pub fn parents_of(&self, d: &Distinction) -> Option<(Distinction, Distinction)> {
        self.parents_index.get(&d.bytes).map(|e| {
            let (a, b) = *e.value();
            (Distinction::from_bytes_internal(a), Distinction::from_bytes_internal(b))
        })
    }

    /// Returns an iterator over the children that were produced with `d`
    /// as one of their parents, in insertion order.
    ///
    /// Implementation: snapshots the children Vec under a DashMap read
    /// guard, then returns an owned iterator. The intermediate Vec is
    /// `Vec<[u8; 16]>` (16 bytes per element, `Copy`); conversion to
    /// `Distinction` is free. Holding a `RefMulti` guard across the
    /// iterator return would be self-referential w.r.t. the shard lock,
    /// risking deadlock if a caller iterates while another writer hits
    /// the same shard. The snapshot trades a one-time allocation per
    /// `children_of()` call for guard-lifetime simplicity.
    ///
    /// For unregistered distinctions returns an empty iterator (not
    /// `None`) — iterator semantics let callers write
    /// `for child in engine.children_of(&d)` regardless.
    ///
    /// Section 2.2 / Phase 6 sub-branch #5.
    pub fn children_of(&self, d: &Distinction) -> impl Iterator<Item = Distinction> + '_ {
        let snapshot: Vec<[u8; 16]> =
            self.children_index.get(&d.bytes).map(|e| e.value().clone()).unwrap_or_default();
        snapshot.into_iter().map(Distinction::from_bytes_internal)
    }

    /// Returns the number of entries in the synthesis log, or `0` if the
    /// engine was constructed via [`DistinctionEngine::without_log`].
    ///
    /// Each entry represents one novel `synthesize()` call. Idempotent
    /// repeats of the same pair do not increment the count. Section 2.3.
    pub fn synthesis_log_len(&self) -> usize {
        self.log.as_ref().map_or(0, |q| q.len())
    }

    /// Returns a snapshot of the synthesis log as a `Vec` of canonical
    /// `(min, max)` parent tuples in insertion order.
    ///
    /// Each tuple `(lo, hi)` satisfies `lo.as_bytes() <= hi.as_bytes()`
    /// (Decision 5.7). Replaying the log against a fresh engine via
    /// `for (a, b) in log { new_engine.synthesize(&a, &b); }` produces a
    /// byte-identical engine state (Exp 7, Exp 12 — confirmed
    /// order-independent).
    ///
    /// Implementation: drain-and-refill on the underlying `SegQueue`
    /// (`SegQueue` has no non-destructive iterator). Concurrent writes
    /// during the snapshot are visible in the returned `Vec`; writes
    /// during refill end up at the tail. Both are benign because replay
    /// is order-independent. For engines constructed via `without_log()`,
    /// returns an empty `Vec`.
    pub fn synthesis_log_snapshot(&self) -> Vec<(Distinction, Distinction)> {
        let Some(log) = self.log.as_ref() else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(log.len());
        while let Some(entry) = log.pop() {
            out.push(entry);
        }
        for entry in &out {
            log.push(entry.clone());
        }
        out
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

#[cfg(test)]
mod structural_invariant_tests {
    use super::*;

    /// Genesis: 2 distinctions (d0, d1) and 1 relationship satisfies
    /// `r = 2d - 3`: 2 * 2 - 3 = 1.
    #[test]
    fn genesis_satisfies_invariant() {
        let engine = DistinctionEngine::new();
        assert!(engine.check_structural_invariant());
    }

    /// Every novel synthesis adds 1 distinction and 2 relationships,
    /// preserving the invariant. Confirms tripwire detects single-thread
    /// monotone growth without false positives.
    #[test]
    fn invariant_holds_across_100_step_chain() {
        let engine = DistinctionEngine::new();
        let d1 = engine.d1().clone();
        let mut current = engine.synthesize(engine.d0(), &d1);
        assert!(engine.check_structural_invariant());
        for _ in 0..100 {
            current = engine.synthesize(&current, &d1);
            assert!(
                engine.check_structural_invariant(),
                "invariant violated after synth: d={} r={}",
                engine.distinction_count(),
                engine.relationship_count()
            );
        }
    }

    /// Saturated synthesis adds nothing and preserves the invariant.
    #[test]
    fn invariant_holds_under_saturation() {
        let engine = DistinctionEngine::new();
        let a = engine.synthesize(engine.d0(), engine.d1());
        let b = engine.synthesize(&a, engine.d1());
        assert!(engine.check_structural_invariant());
        for _ in 0..1000 {
            let _ = engine.synthesize(&a, &b);
        }
        assert!(engine.check_structural_invariant());
    }
}

#[cfg(test)]
mod synthesis_log_tests {
    use super::*;

    #[test]
    fn new_engine_has_no_log_entries() {
        let engine = DistinctionEngine::new();
        assert_eq!(engine.synthesis_log_len(), 0);
        assert!(engine.synthesis_log_snapshot().is_empty());
    }

    #[test]
    fn single_synth_logs_one_canonical_entry() {
        let engine = DistinctionEngine::new();
        let _ = engine.synthesize(engine.d0(), engine.d1());

        let log = engine.synthesis_log_snapshot();
        assert_eq!(log.len(), 1);
        let (lo, hi) = &log[0];
        // d0.bytes ([0; 16]) < d1.bytes ([1, 0, ..., 0]).
        assert_eq!(lo.as_bytes(), engine.d0().as_bytes());
        assert_eq!(hi.as_bytes(), engine.d1().as_bytes());
    }

    #[test]
    fn idempotent_synth_logs_once() {
        let engine = DistinctionEngine::new();
        for _ in 0..100 {
            let _ = engine.synthesize(engine.d0(), engine.d1());
        }
        assert_eq!(engine.synthesis_log_len(), 1);
    }

    #[test]
    fn irreflexive_synth_does_not_log() {
        let engine = DistinctionEngine::new();
        let _ = engine.synthesize(engine.d0(), engine.d0());
        let _ = engine.synthesize(engine.d1(), engine.d1());
        assert_eq!(engine.synthesis_log_len(), 0);
    }

    #[test]
    fn without_log_engine_logs_nothing() {
        let engine = DistinctionEngine::without_log();
        for i in 0..100u8 {
            let _ = engine.synthesize(engine.d0(), engine.d1());
            // Build a small chain so we exercise novel synthesis.
            let d = engine.synthesize(engine.d0(), engine.d1());
            let mut byte_d0 = [0u8; 16];
            byte_d0[15] = i;
            let _ = engine.synthesize(&d, &Distinction::from_bytes_internal(byte_d0));
        }
        assert_eq!(engine.synthesis_log_len(), 0);
        assert!(engine.synthesis_log_snapshot().is_empty());
    }

    #[test]
    fn log_canonical_ordering_min_first() {
        let engine = DistinctionEngine::new();
        // synthesize with reversed order (d1, d0); canonical entry must
        // still be (d0, d1) — Decision 5.7.
        let _ = engine.synthesize(engine.d1(), engine.d0());

        let log = engine.synthesis_log_snapshot();
        assert_eq!(log.len(), 1);
        let (lo, hi) = &log[0];
        assert!(lo.as_bytes() <= hi.as_bytes(), "log entry must be canonical (min, max)");
        assert_eq!(lo.as_bytes(), engine.d0().as_bytes());
        assert_eq!(hi.as_bytes(), engine.d1().as_bytes());
    }

    #[test]
    fn log_replay_round_trip_byte_identical() {
        // Build engine A with a varied chain.
        let engine_a = DistinctionEngine::new();
        let d1 = engine_a.d1().clone();
        let mut current = engine_a.synthesize(engine_a.d0(), &d1);
        for _ in 0..50 {
            current = engine_a.synthesize(&current, &d1);
        }
        let snapshot_a = engine_a.synthesis_log_snapshot();
        let dists_a = engine_a.distinction_count();
        let rels_a = engine_a.relationship_count();

        // Replay against fresh engine B.
        let engine_b = DistinctionEngine::new();
        for (a, b) in &snapshot_a {
            let _ = engine_b.synthesize(a, b);
        }
        let dists_b = engine_b.distinction_count();
        let rels_b = engine_b.relationship_count();

        assert_eq!(dists_a, dists_b, "distinction count must match after replay");
        assert_eq!(rels_a, rels_b, "relationship count must match after replay");

        // Confirm the actual distinction sets match (byte-identical).
        let mut bytes_a: Vec<[u8; 16]> =
            engine_a.get_distinctions_snapshot().iter().map(|d| *d.as_bytes()).collect();
        let mut bytes_b: Vec<[u8; 16]> =
            engine_b.get_distinctions_snapshot().iter().map(|d| *d.as_bytes()).collect();
        bytes_a.sort();
        bytes_b.sort();
        assert_eq!(bytes_a, bytes_b, "distinction byte sets must match");
    }

    #[test]
    fn log_replay_shuffled_still_byte_identical() {
        // Exp 12: replay is order-independent via content addressing.
        let engine_a = DistinctionEngine::new();
        let d1 = engine_a.d1().clone();
        let mut current = engine_a.synthesize(engine_a.d0(), &d1);
        for _ in 0..50 {
            current = engine_a.synthesize(&current, &d1);
        }
        let mut snapshot_a = engine_a.synthesis_log_snapshot();

        // Deterministic shuffle: reverse the log.
        snapshot_a.reverse();

        let engine_b = DistinctionEngine::new();
        for (a, b) in &snapshot_a {
            let _ = engine_b.synthesize(a, b);
        }

        let mut bytes_a: Vec<[u8; 16]> =
            engine_a.get_distinctions_snapshot().iter().map(|d| *d.as_bytes()).collect();
        let mut bytes_b: Vec<[u8; 16]> =
            engine_b.get_distinctions_snapshot().iter().map(|d| *d.as_bytes()).collect();
        bytes_a.sort();
        bytes_b.sort();
        assert_eq!(bytes_a, bytes_b, "shuffled replay must produce identical state");
    }

    #[test]
    fn serde_round_trip_via_json() {
        use serde::{Deserialize, Serialize};

        // Wrapper that uses the existing `distinction_hex` serde adapter.
        // Documents the canonical way for consumers to persist log entries
        // when they want hex strings in JSON; bincode could equivalently
        // serialize the raw bytes.
        #[derive(Serialize, Deserialize, PartialEq, Eq, Debug)]
        struct LogEntry {
            #[serde(with = "crate::distinction_hex")]
            a: Distinction,
            #[serde(with = "crate::distinction_hex")]
            b: Distinction,
        }

        let engine = DistinctionEngine::new();
        let _ = engine.synthesize(engine.d0(), engine.d1());
        let d1 = engine.d1().clone();
        let _ = engine.synthesize(engine.d0(), &d1);
        let c = engine.synthesize(engine.d0(), &d1);
        let _ = engine.synthesize(&c, &d1);

        let entries: Vec<LogEntry> =
            engine.synthesis_log_snapshot().into_iter().map(|(a, b)| LogEntry { a, b }).collect();
        let json = serde_json::to_string(&entries).expect("serialize");
        let restored: Vec<LogEntry> = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entries, restored);
    }

    #[test]
    fn without_log_snapshot_returns_empty_vec() {
        // Defensive: no panic, empty Vec, regardless of how much we synthesize.
        let engine = DistinctionEngine::without_log();
        for _ in 0..10 {
            let _ = engine.synthesize(engine.d0(), engine.d1());
        }
        let snap = engine.synthesis_log_snapshot();
        assert!(snap.is_empty());
    }
}

#[cfg(test)]
mod traversal_api_tests {
    use super::*;

    #[test]
    fn degree_of_primordial_after_genesis() {
        let engine = DistinctionEngine::new();
        // Seeded at 1 to reflect the (d0, d1) genesis relationship.
        assert_eq!(engine.degree(engine.d0()), 1);
        assert_eq!(engine.degree(engine.d1()), 1);
    }

    #[test]
    fn degree_increments_on_synthesis() {
        let engine = DistinctionEngine::new();
        let child = engine.synthesize(engine.d0(), engine.d1());
        assert_eq!(engine.degree(engine.d0()), 2);
        assert_eq!(engine.degree(engine.d1()), 2);
        assert_eq!(engine.degree(&child), 2);
    }

    #[test]
    fn degree_unchanged_under_saturation() {
        let engine = DistinctionEngine::new();
        let _ = engine.synthesize(engine.d0(), engine.d1());
        let before = engine.degree(engine.d0());
        for _ in 0..100 {
            let _ = engine.synthesize(engine.d0(), engine.d1());
        }
        assert_eq!(engine.degree(engine.d0()), before);
    }

    #[test]
    fn degree_of_unregistered_returns_zero() {
        let engine = DistinctionEngine::new();
        // 32 'f's = [0xff; 16]; vanishingly unlikely SHA256 collision.
        let unreg = Distinction::from_hex(&"f".repeat(32)).unwrap();
        assert_eq!(engine.degree(&unreg), 0);
    }

    #[test]
    fn parents_of_primordials_is_none() {
        let engine = DistinctionEngine::new();
        assert!(engine.parents_of(engine.d0()).is_none());
        assert!(engine.parents_of(engine.d1()).is_none());
    }

    #[test]
    fn parents_of_child_is_canonical() {
        let engine = DistinctionEngine::new();
        // synth in reversed order
        let child = engine.synthesize(engine.d1(), engine.d0());
        let (a, b) = engine.parents_of(&child).expect("child must have parents");
        // Canonical: smaller bytes first.
        assert!(a.as_bytes() <= b.as_bytes());
        assert_eq!(a.as_bytes(), engine.d0().as_bytes());
        assert_eq!(b.as_bytes(), engine.d1().as_bytes());
    }

    #[test]
    fn parents_of_unregistered_returns_none() {
        let engine = DistinctionEngine::new();
        let unreg = Distinction::from_hex(&"f".repeat(32)).unwrap();
        assert!(engine.parents_of(&unreg).is_none());
    }

    #[test]
    fn children_of_d0_lists_synth_children() {
        let engine = DistinctionEngine::new();
        let child = engine.synthesize(engine.d0(), engine.d1());

        let kids: Vec<Distinction> = engine.children_of(engine.d0()).collect();
        assert_eq!(kids.len(), 1);
        assert_eq!(kids[0].as_bytes(), child.as_bytes());
    }

    #[test]
    fn children_of_unregistered_is_empty() {
        let engine = DistinctionEngine::new();
        let unreg = Distinction::from_hex(&"f".repeat(32)).unwrap();
        assert_eq!(engine.children_of(&unreg).count(), 0);
    }

    #[test]
    fn children_iterator_is_borrow_safe() {
        // The iterator owns its data (snapshot under guard); no lifetime
        // gymnastics required to drive it after the call returns.
        let engine = DistinctionEngine::new();
        let _ = engine.synthesize(engine.d0(), engine.d1());
        assert!(engine.children_of(engine.d0()).count() > 0);
    }

    #[test]
    fn invariant_holds_after_traversal_index_population() {
        // Sanity check: adding the 3 internal indices doesn't perturb the
        // r = 2d − 3 invariant. (Index population happens AFTER the
        // distinction + relationships are inserted, so this is somewhat
        // tautological — but worth a guard.)
        let engine = DistinctionEngine::new();
        let d1 = engine.d1().clone();
        let mut current = engine.synthesize(engine.d0(), &d1);
        for _ in 0..50 {
            current = engine.synthesize(&current, &d1);
            assert!(engine.check_structural_invariant());
        }
    }
}
