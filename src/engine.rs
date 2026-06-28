//! Substrate engine — [`Distinction`] type, [`IdentityHasher`], and
//! [`DistinctionEngine`] with the entry-gated synthesize hot path.
//!
//! This is the heart of the crate. See `THEORY.md` for the axioms it
//! enforces and `ARCHITECTURE.md` for the engine state layout (a single
//! `nodes` map of `<id, EngineNode { parents, degree }>` exposing the
//! three canonical O(1) projections — saturation check, parent lookup,
//! degree query — as per-node fields).

use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};

/// A distinction — the unique kind of thing the substrate talks about.
///
/// A distinction is identified solely by its 16-byte content-addressed
/// identity. There is no internal structure beyond these bytes.
///
/// Constructed exclusively by [`DistinctionEngine`]: either by retrieving
/// a primordial (`engine.d0()`, `engine.d1()`), by synthesis
/// (`engine.synthesize(a, b)`), or by parsing bytes the engine has
/// already registered ([`Distinction::from_hex`] of a known ID).
///
/// The field is `pub(crate)` — there is no public constructor. Foreign-byte
/// injection is closed structurally: external code cannot mint a
/// `Distinction` that didn't originate from a `DistinctionEngine`. The
/// only exception is [`Distinction::from_hex`], which parses bytes
/// without validating engine membership; passing such a value to
/// `engine.synthesize` triggers a debug-build panic via the foreign-byte
/// guard.
///
/// `#[repr(transparent)]`: layout-compatible with `[u8; 16]`, enabling
/// zero-copy FFI/WASM transit (`*const Distinction` ↔ `*const [u8; 16]`).
///
/// # Safety justification for `bytemuck::Pod` and `bytemuck::Zeroable`
///
/// `Pod` requires the type to have no padding bytes and all bit patterns
/// to be valid. `[u8; 16]` has no padding (it's just 16 bytes), all 2^128
/// bit patterns are valid (any byte sequence is a syntactically valid
/// Distinction identity — the engine is what decides which ones are
/// "registered"), and `#[repr(transparent)]` preserves the inner array's
/// layout guarantees. Both invariants hold.
///
/// `Zeroable` requires that the all-zero bit pattern is a valid value of
/// the type. `[0u8; 16]` is the primordial `d0`'s identity — a valid
/// distinction by definition.
///
/// [`DistinctionEngine`]: crate::DistinctionEngine
/// [`Distinction::from_hex`]: crate::Distinction::from_hex
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct Distinction(pub(crate) [u8; 16]);

impl Distinction {
    /// Construct a `Distinction` directly from bytes, without any engine
    /// registration check.
    ///
    /// **Crate-internal** — used inside the substrate by
    /// [`from_hex`](Distinction::from_hex) and by the engine itself.
    /// External crates cannot reach this constructor.
    #[must_use]
    pub(crate) const fn from_bytes_unchecked(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 16 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// IdentityHasher
// ---------------------------------------------------------------------------

/// Hasher used by the substrate's [`DashMap`]s for byte-keyed lookups.
///
/// SHA-256 prefixes are uniformly distributed. The hasher returns the
/// leading 8 bytes of the 16-byte key as a `u64`. No XOR, no rotation,
/// no diffusion math — the input is already uniform, so adding
/// "scrambling" would only cost cycles.
///
/// # Misuse detection
///
/// The substrate uses this hasher exclusively for 16-byte keys (raw
/// distinction identities). Calls to `write_u8` / `write_u16` /
/// `write_u32` / etc. trigger `unreachable!()`: those code paths exist
/// only to satisfy the [`Hasher`] trait, and the substrate has no
/// business invoking them on its own keys.
///
/// `write_usize` is the one exception: slice/array `Hash` impls call
/// `state.write_usize(self.len())` before writing the actual bytes (the
/// length prefix). For our 16-byte keys that's always 16. The hasher
/// absorbs the length prefix as a no-op and relies entirely on the
/// `debug_assert!` in `write()` to catch non-16-byte slices.
///
/// In release builds, the `debug_assert!` is compiled out; if a
/// non-16-byte slice somehow reaches `write`, only the first 8 bytes
/// are read, which still produces a valid `u64` — just one not derived
/// from the expected 16-byte key. The substrate's contract is: callers
/// must only hash 16-byte keys.
///
/// [`DashMap`]: dashmap::DashMap
#[derive(Default)]
pub struct IdentityHasher {
    state: u64,
}

impl Hasher for IdentityHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.state
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        debug_assert_eq!(bytes.len(), 16, "IdentityHasher only handles 16-byte keys (invariant)");
        // Leading 8 bytes of the 16-byte key.
        // `[..8].try_into()` is fallible only if the slice is shorter
        // than 8 bytes — which the debug_assert above rules out in
        // debug, and which the substrate contract rules out in release.
        let prefix: [u8; 8] =
            bytes[..8].try_into().expect("IdentityHasher requires ≥8 bytes (invariant)");
        self.state = u64::from_le_bytes(prefix);
    }

    fn write_u8(&mut self, _: u8) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u16(&mut self, _: u16) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u32(&mut self, _: u32) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u64(&mut self, _: u64) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u128(&mut self, _: u128) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_usize(&mut self, _len: usize) {
        // Slice/array Hash impls call `write_usize(len)` before writing
        // the actual bytes (the length prefix). For our 16-byte keys
        // that's always 16, and the hasher only consumes information
        // from `write()`. Absorb the length prefix as a no-op; the
        // misuse-detection responsibility falls entirely on the
        // `write()` debug_assert.
    }
    fn write_i8(&mut self, _: i8) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i16(&mut self, _: i16) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i32(&mut self, _: i32) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i64(&mut self, _: i64) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i128(&mut self, _: i128) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_isize(&mut self, _: isize) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    // `write_length_prefix` is unstable (issue #96762); intentionally
    // omitted on stable Rust. If a future stable promotion changes the
    // default slice/array Hash path to use it instead of `write_usize`,
    // we'll need to add a matching no-op override.
}

/// `BuildHasher` flavor of [`IdentityHasher`], used as the hash builder
/// for the engine's [`DashMap`]s.
///
/// [`DashMap`]: dashmap::DashMap
pub type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>;

// ---------------------------------------------------------------------------
// Primordials
// ---------------------------------------------------------------------------

/// The first primordial. Bytes: `[0x00; 16]`.
const PRIMORDIAL_D0: Distinction = Distinction::from_bytes_unchecked([0u8; 16]);

/// The second primordial. Bytes: `[0x01, 0x00, ..., 0x00]`.
///
/// The byte values themselves aren't theory — the theory only requires
/// d₀ and d₁ to be distinct. The choice of `[0x01, 0x00, ...]` (a single
/// high bit at position 0) is arbitrary but conventional.
const PRIMORDIAL_D1: Distinction =
    Distinction::from_bytes_unchecked([1, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Reasons [`DistinctionEngine::check_structural_invariant`] can fail.
///
/// `#[non_exhaustive]`: future variants will not break consumer semver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum InvariantError {
    /// Structural law 5 (binary parentage) failed:
    /// `all_distinctions.len() != parents_of.len() + 2`.
    ///
    /// This means either a non-primordial distinction lacks a `parents_of`
    /// entry, or a `parents_of` entry references a child that isn't in
    /// `all_distinctions`. Either case means the engine is internally
    /// inconsistent — a theory event, not a budget event.
    #[error("binary parentage violation: all_distinctions.len()={all_distinctions}, parents_of.len()+2={parents_of_plus_two}")]
    BinaryParentageMismatch {
        /// `all_distinctions.len()`.
        all_distinctions: usize,
        /// `parents_of.len() + 2`.
        parents_of_plus_two: usize,
    },
}

// ---------------------------------------------------------------------------
// DistinctionEngine
// ---------------------------------------------------------------------------

/// Canonical `(min, max)` parent pair recorded in an `EngineNode` (via
/// [`DistinctionEngine::parents_of`] and
/// [`DistinctionEngine::snapshot_parentage`]). The ordering is
/// guaranteed: `pair.0.as_bytes() <= pair.1.as_bytes()`.
pub type ParentPair = (Distinction, Distinction);

/// The per-distinction record stored in the engine's single `nodes` map.
///
/// `parents` is `None` for the two primordials and `Some(min, max)` for
/// every other distinction. `degree` counts the number of NOVEL syntheses
/// in which this distinction has been used as a parent (saturated repeats
/// contribute nothing). The primordial genesis edge (d₀↔d₁) is accounted
/// for in [`DistinctionEngine::degree`] as a +1 addend, not in the field.
struct EngineNode {
    parents: Option<ParentPair>,
    degree: AtomicUsize,
}

/// Crate-internal alias for the engine's `nodes` map type.
type NodeMap = DashMap<[u8; 16], EngineNode, IdentityBuildHasher>;

/// The substrate engine — implements the four axioms in
/// [`synthesize`](DistinctionEngine::synthesize) and exposes the
/// canonical O(1) projections through a single `nodes` map.
///
/// # State
///
/// Three fields total: two primordial constants + one indexed `nodes`
/// map whose value is an [`EngineNode`] carrying both parents and degree
/// per distinction. The three conceptual O(1) projections live as
/// per-node fields:
///
/// - **Saturation check** (Law 7): `nodes.contains_key(id)`.
/// - **Parent lookup** (Law 5, binary parentage): `nodes[id].parents`.
/// - **Degree query** (Law 12 Coding Law + Law 11 Fold Law):
///   `nodes[id].degree` + the appropriate genesis/parent-edges addend.
///
/// Merging into one map (vs. three side-by-side maps) preserves the
/// O(1) semantics of every projection and removes the two extra shard
/// lookups per novel synthesis. Identity-IS-process: each distinction
/// is one node.
///
/// No log. No observation channel. No order-bearing state. The substrate
/// is timeless; time is what consumers (`LocalCausalAgent`) do.
///
/// # Concurrency
///
/// All public methods take `&self`. Interior mutability via `DashMap` +
/// `AtomicUsize`. Multiple threads can synthesize concurrently against a
/// shared engine reference (typically `Arc<DistinctionEngine>`).
///
/// The entry-gated insert in [`synthesize`](DistinctionEngine::synthesize)
/// ensures byte-equivalent post-join state regardless of thread
/// interleaving. See `synthesize`'s "happens-before contract" for the
/// in-flight ordering details.
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    nodes: NodeMap,
}

impl DistinctionEngine {
    /// Construct a fresh engine containing only the two primordials.
    ///
    /// Post-conditions (verified by the primordial smoke test):
    /// - `distinction_count() == 2`
    /// - `parents_of(d0).is_none()` and `parents_of(d1).is_none()`
    /// - `degree(d0) == 1` and `degree(d1) == 1` (the genesis d₀↔d₁ edge)
    /// - `check_structural_invariant().is_ok()` (r = 2d − 3 with d = 2, r = 1)
    /// - `synthesize(d0, fresh_x)` immediately after construction does not
    ///   panic (primordial `EngineNode`s pre-seeded for both d₀ and d₁).
    #[must_use]
    pub fn new() -> Self {
        let d0 = PRIMORDIAL_D0;
        let d1 = PRIMORDIAL_D1;

        let nodes = DashMap::with_hasher(IdentityBuildHasher::default());
        // Pre-seed primordial nodes so the synthesize hot path's
        // `nodes.get(...).expect("(invariant)")` never trips on a fresh
        // engine where d0 or d1 is the first parent in a synthesis.
        // Primordials have `parents: None` (they have no parents) and
        // `degree: 0` (no novel-synthesis participations yet — the
        // genesis d₀↔d₁ edge is accounted for in `degree()` as a +1
        // addend).
        nodes.insert(d0.0, EngineNode { parents: None, degree: AtomicUsize::new(0) });
        nodes.insert(d1.0, EngineNode { parents: None, degree: AtomicUsize::new(0) });

        Self { d0, d1, nodes }
    }

    /// Borrow the first primordial.
    #[must_use]
    pub const fn d0(&self) -> Distinction {
        self.d0
    }

    /// Borrow the second primordial.
    #[must_use]
    pub const fn d1(&self) -> Distinction {
        self.d1
    }

    /// Synthesize two distinctions into a child distinction.
    ///
    /// Enforces all four axioms:
    /// 1. **Determinism** — pure function of `(a, b)`; same inputs always
    ///    produce the same child bytes (via SHA-256).
    /// 2. **Commutativity** — argument order doesn't matter; canonical
    ///    `(min, max)` ordering on the 16-byte ids before hashing.
    /// 3. **Irreflexivity** — `synthesize(a, a) == a`; early return.
    /// 4. **Content addressing** — child identity IS the SHA-256 prefix
    ///    of the canonical parent-pair bytes.
    ///
    /// Plus Law 7 (saturation) via the `all_distinctions.get(...)`
    /// fast-path return BEFORE the entry-gated insert closure runs —
    /// repeated syntheses bump nothing, allocate nothing.
    ///
    /// # Contract
    ///
    /// Both `a` and `b` must be distinctions registered in **this**
    /// engine — either obtained from [`d0`](DistinctionEngine::d0),
    /// [`d1`](DistinctionEngine::d1), or a prior `synthesize` call on
    /// this engine, or returned by [`replay_topological`] driving this
    /// engine. The foreign-byte `debug_assert!` enforces this in debug
    /// builds; in release builds, the entry closure's
    /// `degree_counts.get(...).expect("(invariant)")` panics if a foreign
    /// byte slips through. Either way: foreign-byte injection is closed.
    ///
    /// # Concurrency
    ///
    /// Safe to call concurrently from any number of threads against a
    /// shared engine reference. The entry-gated insert into `nodes`
    /// serializes novel insertions for a given `new_bytes` on a single
    /// shard write-lock; concurrent reads through the saturation
    /// fast-path don't block writes for *different* `new_bytes`. See
    /// the implementation comments below for the full happens-before
    /// contract.
    ///
    /// # Happens-before contract
    ///
    /// Two distinct guarantees with different scopes:
    ///
    /// 1. **New-child observation → parents.** A reader that observes
    ///    `new_d` via [`nodes`](Self::has) or [`parents_of`](Self::parents_of)
    ///    is guaranteed (via the entry shard lock) to observe the new
    ///    node's `parents` field — the parent identities are committed
    ///    atomically with the new node's insertion.
    /// 2. **Parent degree bumps — eventually consistent.** Parent degree
    ///    `fetch_add`s occur AFTER the entry shard lock releases (so that
    ///    a parent hashing to the same shard as `new_bytes` cannot
    ///    self-deadlock — the merged-map's B1 mitigation). A racing
    ///    reader that observes `new_d` and immediately queries
    ///    `degree(parent)` may transiently see the pre-bump value. Post-
    ///    join state is consistent: the `Release`/`Acquire` pair on
    ///    `degree` ensures the bump becomes visible to subsequent loads,
    ///    and the per-parent sum invariant
    ///    `sum_of_degrees == 2 * non_primordial_count` holds at every
    ///    quiescent point.
    ///
    /// Consumers (`LocalCausalAgent`) drive synthesis sequentially within
    /// a single LCA, so the relaxed in-flight ordering between distinct
    /// LCAs is invisible to any contract built on the LCA pattern. Probes
    /// reading degree directly (`Coding Law`, `Fold Law` traversals)
    /// should call them at a quiescent point (after a join barrier or
    /// after consumer-driven epoch boundary), not mid-flight, to get the
    /// post-bump values they expect.
    ///
    /// [`replay_topological`]: crate::replay::replay_topological
    #[must_use]
    pub fn synthesize(&self, a: Distinction, b: Distinction) -> Distinction {
        // Foreign-byte guard (Axiom 4 enforcement at the engine boundary).
        // The `pub(crate)` Distinction field closes mint-from-thin-air at
        // compile time, but `Distinction::from_hex` can produce a value
        // whose bytes aren't registered in any engine. The debug_assert
        // catches this in debug; the post-entry `expect` (below) catches
        // it in release on the parent lookup.
        debug_assert!(
            self.nodes.contains_key(&a.0),
            "synthesize: parent `a` ({:?}) not registered in this engine — foreign-byte injection",
            a
        );
        debug_assert!(
            self.nodes.contains_key(&b.0),
            "synthesize: parent `b` ({:?}) not registered in this engine — foreign-byte injection",
            b
        );

        // Axiom 3 — irreflexivity.
        if a == b {
            return a;
        }

        // Axiom 2 — commutativity via canonical (min, max) byte ordering.
        let (first, second) = if a.0 <= b.0 { (a, b) } else { (b, a) };

        // Axioms 1 + 4 — content-addressed identity via SHA-256 leading-16.
        let mut h = Sha256::new();
        h.update(first.0);
        h.update(second.0);
        let digest = h.finalize();
        let mut new_bytes = [0u8; 16];
        new_bytes.copy_from_slice(&digest[..16]);

        // Law 7 — saturation. Fast path: if the child already exists,
        // return it without taking any shard write-lock. This is load-
        // bearing: repeated syntheses must contribute NOTHING to degree.
        // The Some/None result is purely an optimization; correctness
        // comes from the entry's exclusive Vacant/Occupied dispatch
        // below.
        if self.nodes.contains_key(&new_bytes) {
            return Distinction(new_bytes);
        }

        let new_d = Distinction(new_bytes);

        // Entry-gated insert (race-free) with a per-thread "did I win"
        // flag.
        //
        // Without the flag, every thread that races past the saturation
        // check would unconditionally fetch_add on the parents in the
        // post-entry block — double-counting whenever two threads race
        // the same novel `new_bytes`. The Vacant/Occupied match makes
        // the closure-equivalent (parent bumps) run exactly once per
        // novel insertion.
        //
        // Lock-holding discipline: the `Entry` value holds the shard
        // write-lock for `new_bytes` in `nodes`. The block scope ensures
        // the lock is dropped BEFORE the parent fetch_adds below. This
        // is the merged-map's B1 deadlock mitigation: a parent
        // (`first` / `second`) may hash to the same shard as
        // `new_bytes`, and acquiring its shard read-lock while still
        // holding the same shard's write-lock would self-deadlock.
        let inserted_new = {
            match self.nodes.entry(new_bytes) {
                dashmap::Entry::Vacant(slot) => {
                    slot.insert(EngineNode {
                        parents: Some((first, second)),
                        degree: AtomicUsize::new(0),
                    });
                    true
                },
                dashmap::Entry::Occupied(_) => false,
            }
            // Entry (and any RefMut from slot.insert) drops here,
            // releasing the shard write-lock before parent fetch_adds.
        };

        if inserted_new {
            // `fetch_add(1, Ordering::Release)` pairs with
            // `Ordering::Acquire` loads in `degree()`. Probes reading
            // node.degree directly (without first observing `new_d`)
            // still get a happens-before edge to the writing synthesis.
            // Loom verifies this kernel; TSan on the concurrent-write
            // byte-equivalence test verifies the DashMap-shard side.
            self.nodes
                .get(&first.0)
                .expect("first parent registered at its insertion (invariant)")
                .degree
                .fetch_add(1, Ordering::Release);
            self.nodes
                .get(&second.0)
                .expect("second parent registered at its insertion (invariant)")
                .degree
                .fetch_add(1, Ordering::Release);
        }

        new_d
    }

    /// Look up the canonical `(min, max)` parent pair of a non-primordial
    /// distinction.
    ///
    /// Returns `None` for d₀, d₁, or any distinction not registered in
    /// this engine.
    #[must_use]
    pub fn parents_of(&self, d: Distinction) -> Option<(Distinction, Distinction)> {
        self.nodes.get(&d.0).and_then(|entry| entry.value().parents)
    }

    /// Compute the degree of a distinction (Law 12, Coding Law).
    ///
    /// Formula:
    /// - For d₀ or d₁: `nodes[d].degree.load(Acquire) + 1` — the +1
    ///   accounts for the genesis d₀↔d₁ edge, the only edge in the
    ///   graph not derivable from `parents_of`.
    /// - For any other distinction REGISTERED in this engine:
    ///   `nodes[d].degree.load(Acquire) + 2` — the +2 accounts for the
    ///   two parent edges every non-primordial has, recorded in
    ///   `nodes[d].parents` rather than in `nodes[d].degree`.
    /// - For a distinction NOT registered in this engine: `0`. A foreign
    ///   distinction has no edges in this engine's graph; the genesis
    ///   addend doesn't apply to it.
    ///
    /// `nodes[d].degree` counts the number of NOVEL syntheses in which
    /// `d` participated as a parent (saturated repeats contribute nothing).
    /// `Acquire` ordering pairs with `Release` ordering on the
    /// `fetch_add` inside `synthesize` so probes reading this field
    /// directly get a happens-before edge to writes.
    #[must_use]
    pub fn degree(&self, d: Distinction) -> usize {
        // Primordials: registered at construction with `degree` pre-seeded
        // to 0. The `map_or(0, ...)` is defensive — the node should always
        // exist for d0/d1 in a valid engine.
        if d == self.d0 || d == self.d1 {
            return self
                .nodes
                .get(&d.0)
                .map_or(0, |entry| entry.value().degree.load(Ordering::Acquire))
                + 1;
        }
        // Non-primordial: registration is signaled by the presence of a
        // node entry (pre-seeded at insertion in the synthesize hot
        // path). Foreign distinctions get neither the addend nor any
        // participation count.
        match self.nodes.get(&d.0) {
            Some(entry) => entry.value().degree.load(Ordering::Acquire) + 2,
            None => 0,
        }
    }

    /// Number of distinctions in the engine (including primordials).
    ///
    /// At construction: 2 (just d₀ and d₁).
    #[must_use]
    pub fn distinction_count(&self) -> usize {
        self.nodes.len()
    }

    /// Number of relationships in the engine.
    ///
    /// Each non-primordial child contributes 2 edges (one to each parent).
    /// Plus 1 for the genesis d₀↔d₁ edge. The primordials are always
    /// present in `nodes`, so the non-primordial count is
    /// `nodes.len() - 2`, giving:
    /// `relationship_count() == (nodes.len() - 2) * 2 + 1`.
    ///
    /// At construction: 1 (just the genesis edge).
    #[must_use]
    pub fn relationship_count(&self) -> usize {
        (self.nodes.len() - 2) * 2 + 1
    }

    /// Test whether a distinction is registered in this engine.
    ///
    /// Returns `true` for d₀, d₁, and any distinction produced by
    /// `synthesize` against this engine. Returns `false` for any
    /// distinction whose bytes weren't registered (e.g., a value parsed
    /// via `Distinction::from_hex` for an ID that has never been
    /// synthesized in this engine).
    ///
    /// Used by [`replay_topological`](crate::replay::replay_topological)
    /// to determine which pending parentage entries are ready to apply.
    ///
    /// O(1) — single DashMap lookup.
    #[must_use]
    pub fn has(&self, d: Distinction) -> bool {
        self.nodes.contains_key(&d.0)
    }

    /// Snapshot every parent-child relationship as an owned `Vec`.
    ///
    /// Returns the contents of `parents` (across all nodes that have
    /// one) as a list of `(child, (parent_min, parent_max))` tuples.
    /// Order is unspecified (DashMap iteration is shard-dependent and
    /// not stable across runs). For deterministic ordering, sort the
    /// returned `Vec` by child bytes.
    ///
    /// Primordials are NOT included — their `parents` is `None`.
    ///
    /// This is the canonical persistence dump: combined with
    /// [`replay_topological`](crate::replay::replay_topological), it
    /// reconstructs a byte-identical engine on any machine
    /// (engine-independence, Law 8) in any input order (Law 9).
    ///
    /// O(N) where N is `distinction_count()` — iterates all nodes and
    /// filters to those with parents.
    #[must_use]
    pub fn snapshot_parentage(&self) -> Vec<(Distinction, ParentPair)> {
        self.nodes
            .iter()
            .filter_map(|entry| entry.value().parents.map(|p| (Distinction(*entry.key()), p)))
            .collect()
    }

    /// Snapshot of every distinction registered in this engine.
    ///
    /// Returns a `Vec<Distinction>` — a snapshot, not a live iterator.
    /// Iterating a `DashMap` requires holding shard locks; materializing
    /// to a `Vec` lets callers process the result without blocking other
    /// threads' synthesis calls.
    ///
    /// Order is unspecified (DashMap iteration order is shard-dependent
    /// and not stable across runs). For deterministic ordering, sort
    /// the returned `Vec` by `Distinction::as_bytes()`.
    ///
    /// Cost: O(N) where N is `distinction_count()`.
    ///
    /// Intended for traversal probes (Coding Law degree-vs-frequency
    /// in Step 4, Fold Law dominance tests, diagnostic dumps). Not
    /// intended for the synthesis hot path.
    #[must_use]
    pub fn snapshot_distinctions(&self) -> Vec<Distinction> {
        self.nodes.iter().map(|entry| Distinction(*entry.key())).collect()
    }

    /// Verify structural law `r = 2d − 3` holds.
    ///
    /// In the merged-map representation, this verifies that
    /// `nodes.len() == (count_of_nodes_with_parents) + 2` — every
    /// non-primordial distinction has its `parents` field populated
    /// (binary parentage, Law 5), and every primordial has `parents:
    /// None`. The merged storage makes the *count* relationship
    /// trivially true (a single `EngineNode` carries both id and
    /// parents, so they cannot get out of sync), so this check primarily
    /// catches a shape corruption (e.g., a primordial accidentally
    /// having parents, or a non-primordial accidentally missing them).
    ///
    /// Failure is a *theory event*, not a budget event: it means the
    /// implementation no longer satisfies the structural laws. The
    /// response is rewrite, not amendment.
    ///
    /// Cost: O(N) where N is `distinction_count()` — one iteration over
    /// all nodes counting parented ones. Not on the hot path.
    ///
    /// # Errors
    ///
    /// Returns [`InvariantError::BinaryParentageMismatch`] if the
    /// counts don't match.
    pub fn check_structural_invariant(&self) -> Result<(), InvariantError> {
        let d_count = self.nodes.len();
        let with_parents =
            self.nodes.iter().filter(|entry| entry.value().parents.is_some()).count();
        let p_plus_two = with_parents + 2;
        if d_count != p_plus_two {
            return Err(InvariantError::BinaryParentageMismatch {
                all_distinctions: d_count,
                parents_of_plus_two: p_plus_two,
            });
        }
        Ok(())
    }
}

impl Default for DistinctionEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for DistinctionEngine {
    /// Summary-only Debug — prints distinction and relationship counts.
    /// Full state dump would be O(N) and not useful in panic messages
    /// where `Debug` is typically invoked.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DistinctionEngine")
            .field("distinction_count", &self.distinction_count())
            .field("relationship_count", &self.relationship_count())
            .finish_non_exhaustive()
    }
}

// ---------------------------------------------------------------------------
// Compile-time assertions on Distinction
// ---------------------------------------------------------------------------

#[cfg(test)]
mod compile_time_assertions {
    use super::*;
    use static_assertions::{assert_eq_align, assert_eq_size, assert_impl_all};

    // `Distinction` is the size and alignment of a `[u8; 16]` — the
    // whole point of `#[repr(transparent)]`.
    assert_eq_size!(Distinction, [u8; 16]);
    assert_eq_align!(Distinction, [u8; 16]);

    // Threading safety — `Distinction` is `Copy + Send + Sync`.
    assert_impl_all!(Distinction: Copy, Send, Sync);

    // bytemuck plain-old-data + zeroable — enables zero-copy slice views.
    assert_impl_all!(Distinction: bytemuck::Pod, bytemuck::Zeroable);

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn distinction_send_sync() {
        _assert_send_sync::<Distinction>();
    }

    #[test]
    fn distinction_size_align() {
        assert_eq!(std::mem::size_of::<Distinction>(), 16);
        assert_eq!(std::mem::align_of::<Distinction>(), 1);
    }
}

#[cfg(test)]
mod identity_hasher_tests {
    use super::IdentityHasher;
    use std::hash::Hasher as _;

    #[test]
    fn reads_leading_8_bytes_as_u64_le() {
        let mut h = IdentityHasher::default();
        let bytes = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff,
        ];
        h.write(&bytes);
        let expected = u64::from_le_bytes([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(h.finish(), expected);
    }

    #[test]
    fn distinct_prefixes_give_distinct_hashes() {
        let mut h1 = IdentityHasher::default();
        let mut h2 = IdentityHasher::default();
        let bytes1 = [0u8; 16];
        let mut bytes2 = [0u8; 16];
        bytes2[0] = 1;
        h1.write(&bytes1);
        h2.write(&bytes2);
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn million_distinct_inputs_million_distinct_hashes() {
        use std::collections::HashSet;
        let mut seen = HashSet::with_capacity(1_000_000);
        let mut state = 0xdead_beef_cafe_babe_u64;
        for _ in 0..1_000_000 {
            // Cheap LCG to generate "random" bytes deterministically.
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut full = [0u8; 16];
            full[..8].copy_from_slice(&state.to_le_bytes());
            full[8..].copy_from_slice(&state.swap_bytes().to_le_bytes());
            let mut h = IdentityHasher::default();
            h.write(&full);
            seen.insert(h.finish());
        }
        // Distinct LCG outputs in u64 space ⟹ distinct leading-8-byte
        // prefixes ⟹ distinct hashes. Exact equality verifies the hasher
        // drops zero information from the prefix.
        assert_eq!(seen.len(), 1_000_000);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys")]
    fn panics_on_short_slice_in_debug() {
        let mut h = IdentityHasher::default();
        h.write(&[0u8; 8]);
    }

    // The misuse-detection contract: every integer-write arm
    // (`write_u8` / `write_u16` / etc.) must be `unreachable!()` because
    // the substrate exclusively hashes 16-byte keys via `write()`. These
    // tests confirm each arm panics, so a future contributor who
    // accidentally routes some other key type through this hasher gets
    // a loud error instead of a silently-wrong hash.
    //
    // `write_usize` is intentionally NOT in this list — slice/array
    // Hash impls call it for the length prefix, and the hasher absorbs
    // it as a no-op. See the IdentityHasher doc-comment for details.

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_u8_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_u8(0);
    }

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_u16_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_u16(0);
    }

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_u32_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_u32(0);
    }

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_u64_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_u64(0);
    }

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_u128_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_u128(0);
    }

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_i8_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_i8(0);
    }

    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys via write()")]
    fn write_isize_is_unreachable() {
        let mut h = IdentityHasher::default();
        h.write_isize(0);
    }

    #[test]
    fn write_usize_is_no_op() {
        // Length prefix absorbed; state unchanged.
        let mut h = IdentityHasher::default();
        let before = h.finish();
        h.write_usize(16);
        let after = h.finish();
        assert_eq!(before, after);
    }
}

// ---------------------------------------------------------------------------
// DistinctionEngine tests
//
// These are theory-load-bearing — each axiom and structural law gets at
// least one falsifying test here. The 5M-scale invariant probe and the
// loom Release/Acquire kernel land in sub-milestone 1e.
// ---------------------------------------------------------------------------

#[cfg(test)]
mod engine_tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    // ----- Primordials at construction -----------------------------------

    #[test]
    fn fresh_engine_has_exactly_two_distinctions() {
        let e = DistinctionEngine::new();
        assert_eq!(e.distinction_count(), 2);
    }

    #[test]
    fn fresh_engine_relationship_count_is_one() {
        // Just the genesis d0↔d1 edge.
        let e = DistinctionEngine::new();
        assert_eq!(e.relationship_count(), 1);
    }

    #[test]
    fn primordials_have_no_parents() {
        let e = DistinctionEngine::new();
        assert!(e.parents_of(e.d0()).is_none());
        assert!(e.parents_of(e.d1()).is_none());
    }

    #[test]
    fn primordials_have_degree_one() {
        // genesis_addend(d0) = genesis_addend(d1) = 1
        // degree_counts[d0] = degree_counts[d1] = 0 at construction
        let e = DistinctionEngine::new();
        assert_eq!(e.degree(e.d0()), 1);
        assert_eq!(e.degree(e.d1()), 1);
    }

    #[test]
    fn structural_invariant_holds_at_construction() {
        let e = DistinctionEngine::new();
        e.check_structural_invariant()
            .expect("fresh engine satisfies r=2d−3 with d=2, r=1 (invariant)");
    }

    #[test]
    fn synthesize_on_cold_engine_does_not_panic() {
        // Smoke test for qa-sentinel round-2 concern: the synthesize hot
        // path's degree_counts.get(...).expect("(invariant)") on parents
        // MUST succeed for d0/d1 on a fresh engine. If new() forgets to
        // pre-seed primordial degree_counts entries, this panics.
        //
        // Strengthened beyond just "doesn't panic": verify the child has
        // the expected shape and parent degrees update correctly.
        let e = DistinctionEngine::new();
        let d0_before = e.degree(e.d0());
        let d1_before = e.degree(e.d1());

        let c = e.synthesize(e.d0(), e.d1());

        // Child has the +2 parent-edges addend, no participations yet.
        assert_eq!(e.degree(c), 2);
        // Parents are (d0, d1) in canonical (min, max) order.
        let (p0, p1) = e.parents_of(c).expect("non-primordial has parents (invariant)");
        assert!(p0.as_bytes() <= p1.as_bytes());
        assert!((p0 == e.d0() && p1 == e.d1()) || (p0 == e.d1() && p1 == e.d0()));
        // Both primordial parent degrees bumped by exactly one.
        assert_eq!(e.degree(e.d0()), d0_before + 1);
        assert_eq!(e.degree(e.d1()), d1_before + 1);
    }

    #[test]
    fn novel_child_used_as_parent_no_expect_panic() {
        // Regression guard for the pre-seed contract: when a novel child
        // is later used as a parent in the next synthesis, the hot-path
        // `degree_counts.get(&parent.0).expect("(invariant)")` MUST find
        // the entry (pre-seeded by the closure that created the child).
        // If the pre-seed line is ever moved/removed, this panics.
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let _ = e.synthesize(c, e.d0()); // c is the parent; pre-seed must hold
        let _ = e.synthesize(c, e.d1());
        let cc = e.synthesize(c, c); // irreflexivity short-circuits, fine
        assert_eq!(cc, c);
    }

    // ----- Axiom 1 — Determinism -----------------------------------------

    #[test]
    fn axiom1_determinism_same_inputs_same_output() {
        let e = DistinctionEngine::new();
        let a = e.synthesize(e.d0(), e.d1());
        let b = e.synthesize(e.d0(), e.d1());
        assert_eq!(a, b);
    }

    #[test]
    fn axiom1_determinism_across_engines() {
        // Two independent engines processing the same operations produce
        // byte-identical state. This is Law 8 (engine independence) in
        // its simplest form; the 5-phase Exp 21 probe lands in 1e.
        //
        // 100 syntheses across a sliding-window chain — enough to cross
        // DashMap shard boundaries multiple times and catch any
        // accumulator-style bug that only manifests at depth.
        let make_chain = |e: &DistinctionEngine| -> Vec<Distinction> {
            let mut chain = Vec::with_capacity(102);
            chain.push(e.d0());
            chain.push(e.d1());
            for i in 2..102 {
                let next = e.synthesize(chain[i - 1], chain[i - 2]);
                chain.push(next);
            }
            chain
        };
        let e1 = DistinctionEngine::new();
        let e2 = DistinctionEngine::new();
        let c1 = make_chain(&e1);
        let c2 = make_chain(&e2);

        // Every step byte-identical (content addressing).
        for (a, b) in c1.iter().zip(c2.iter()) {
            assert_eq!(a.as_bytes(), b.as_bytes());
        }
        // Same final state.
        assert_eq!(e1.distinction_count(), e2.distinction_count());
        assert_eq!(e1.relationship_count(), e2.relationship_count());
        e1.check_structural_invariant().expect("e1 invariant holds (invariant)");
        e2.check_structural_invariant().expect("e2 invariant holds (invariant)");
    }

    // ----- Axiom 2 — Commutativity --------------------------------------

    #[test]
    fn axiom2_commutativity_pair_order_irrelevant() {
        let e = DistinctionEngine::new();
        let ab = e.synthesize(e.d0(), e.d1());
        let ba = e.synthesize(e.d1(), e.d0());
        assert_eq!(ab, ba);
    }

    #[test]
    fn axiom2_commutativity_does_not_double_count() {
        // synth(a,b) followed by synth(b,a) is the SAME synthesis — should
        // not create two distinctions, should not double-bump degrees.
        let e = DistinctionEngine::new();
        let _ = e.synthesize(e.d0(), e.d1());
        let d0_deg_after_first = e.degree(e.d0());
        let count_after_first = e.distinction_count();

        let _ = e.synthesize(e.d1(), e.d0());

        assert_eq!(e.distinction_count(), count_after_first);
        assert_eq!(e.degree(e.d0()), d0_deg_after_first);
    }

    // ----- Axiom 3 — Irreflexivity --------------------------------------

    #[test]
    fn axiom3_irreflexivity_synth_with_self_is_self() {
        let e = DistinctionEngine::new();
        assert_eq!(e.synthesize(e.d0(), e.d0()), e.d0());
        assert_eq!(e.synthesize(e.d1(), e.d1()), e.d1());

        let c = e.synthesize(e.d0(), e.d1());
        assert_eq!(e.synthesize(c, c), c);
    }

    #[test]
    fn axiom3_irreflexivity_does_not_grow_engine() {
        let e = DistinctionEngine::new();
        let d_count_before = e.distinction_count();
        let _ = e.synthesize(e.d0(), e.d0());
        let _ = e.synthesize(e.d1(), e.d1());
        assert_eq!(e.distinction_count(), d_count_before);
    }

    // ----- Axiom 4 — Content addressing ---------------------------------

    #[test]
    fn axiom4_content_addressing_identical_chains_identical_ids() {
        // Already covered by axiom1_determinism_across_engines, but this
        // version goes deeper: a longer chain produces byte-identical IDs
        // across two engines.
        let make_chain = |e: &DistinctionEngine| -> Vec<Distinction> {
            let mut chain = Vec::with_capacity(10);
            chain.push(e.d0());
            chain.push(e.d1());
            chain.push(e.synthesize(e.d0(), e.d1()));
            for i in 3..10 {
                let next = e.synthesize(chain[i - 1], chain[i - 2]);
                chain.push(next);
            }
            chain
        };
        let e1 = DistinctionEngine::new();
        let e2 = DistinctionEngine::new();
        let c1 = make_chain(&e1);
        let c2 = make_chain(&e2);
        for (a, b) in c1.iter().zip(c2.iter()) {
            assert_eq!(a.as_bytes(), b.as_bytes());
        }
        // qa-sentinel round-2 hardening: assert state counts match too,
        // not just chain IDs. A bug that produced correct IDs but
        // diverged on distinction_count would slip past the per-step
        // check.
        assert_eq!(e1.distinction_count(), e2.distinction_count());
        assert_eq!(e1.relationship_count(), e2.relationship_count());
    }

    // ----- Law 7 — Saturation -------------------------------------------

    #[test]
    fn law7_saturation_repeated_synth_adds_nothing() {
        let e = DistinctionEngine::new();
        let _ = e.synthesize(e.d0(), e.d1());
        let count_after_first = e.distinction_count();
        let rel_after_first = e.relationship_count();
        let d0_deg = e.degree(e.d0());
        let d1_deg = e.degree(e.d1());

        // 1000 repeats of the same synthesis.
        for _ in 0..1000 {
            let _ = e.synthesize(e.d0(), e.d1());
        }

        assert_eq!(e.distinction_count(), count_after_first);
        assert_eq!(e.relationship_count(), rel_after_first);
        assert_eq!(e.degree(e.d0()), d0_deg);
        assert_eq!(e.degree(e.d1()), d1_deg);
    }

    // ----- Law 5 — Binary parentage -------------------------------------

    #[test]
    fn law5_every_nonprimordial_has_two_parents() {
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let (p1, p2) = e.parents_of(c).expect("non-primordial has parents (invariant)");
        assert_ne!(p1, p2);
        // Canonical (min, max) ordering — p1 ≤ p2 on raw bytes.
        assert!(p1.as_bytes() <= p2.as_bytes());
        // The actual parents are d0 and d1 in some order.
        assert!((p1 == e.d0() && p2 == e.d1()) || (p1 == e.d1() && p2 == e.d0()));
    }

    // ----- Law 6 — r = 2d − 3 -------------------------------------------

    #[test]
    fn law6_r_equals_2d_minus_3_at_small_scale() {
        // 5M-scale probe lands in 1e; this is the small-scale verification
        // that the invariant holds across many synthesis steps.
        let e = DistinctionEngine::new();
        let mut prev = e.d0();
        let mut cur = e.d1();
        // 200 syntheses; each adds 1 distinction and 2 relationships.
        for _ in 0..200 {
            let next = e.synthesize(cur, prev);
            prev = cur;
            cur = next;
        }
        e.check_structural_invariant().expect("r = 2d − 3 holds (invariant)");
        // Explicit r = 2d − 3 arithmetic: 2 primordials + 200 syntheses
        // = 202 distinctions, with r = 2(202) − 3 = 401.
        assert_eq!(e.distinction_count(), 202);
        assert_eq!(e.relationship_count(), 401);
    }

    // ----- Degree formula correctness -----------------------------------

    #[test]
    fn degree_increases_with_participations() {
        let e = DistinctionEngine::new();
        let d0_deg_initial = e.degree(e.d0());
        let _c = e.synthesize(e.d0(), e.d1());
        assert_eq!(e.degree(e.d0()), d0_deg_initial + 1);
    }

    #[test]
    fn new_child_degree_is_two() {
        // Non-primordial child has +2 parent-edges addend + 0 children
        // (yet) = 2.
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        assert_eq!(e.degree(c), 2);
    }

    #[test]
    fn degree_of_unregistered_distinction_is_zero() {
        // A foreign distinction has no edges in this engine's graph,
        // so degree() returns 0 — not the +2 parent-edge addend that
        // would apply to a registered non-primordial. (qa-sentinel
        // round-2 fix: previously the formula leaked the addend even
        // for foreign queries, returning 2 misleadingly.)
        let e = DistinctionEngine::new();
        let foreign = Distinction::from_bytes_unchecked([0xCC; 16]);
        assert_eq!(e.degree(foreign), 0);
    }

    // ----- Concurrent byte-equivalence ----------------------------------

    #[test]
    fn concurrent_synth_byte_equivalent_state() {
        // 8 threads independently synthesize the same logical chain.
        // The final state should be byte-identical to single-threaded
        // execution. AND sum(degree_counts) == 2 * parents_of.len()
        // (the round-2 unambiguous "expected participation count"
        // invariant).
        let e_concurrent = Arc::new(DistinctionEngine::new());
        let n_threads = 8;
        let mut handles = Vec::with_capacity(n_threads);
        for _ in 0..n_threads {
            let e = Arc::clone(&e_concurrent);
            handles.push(thread::spawn(move || {
                let mut prev = e.d0();
                let mut cur = e.d1();
                for _ in 0..50 {
                    let next = e.synthesize(cur, prev);
                    prev = cur;
                    cur = next;
                }
            }));
        }
        for h in handles {
            h.join().expect("thread join succeeds (invariant)");
        }

        // Single-threaded reference.
        let e_single = DistinctionEngine::new();
        let mut prev = e_single.d0();
        let mut cur = e_single.d1();
        for _ in 0..50 {
            let next = e_single.synthesize(cur, prev);
            prev = cur;
            cur = next;
        }

        assert_eq!(e_concurrent.distinction_count(), e_single.distinction_count());
        assert_eq!(e_concurrent.relationship_count(), e_single.relationship_count());

        // The round-2 invariant: sum of node.degree == 2 * non_primordial_count.
        // Every novel synthesis contributes exactly two fetch_add(1) calls
        // (one per parent). Race-winners don't matter to the sum. The
        // Acquire load pairs with the synthesize hot path's Release
        // fetch_add — at this quiescent point (post-join), all writes
        // are visible.
        let nonprim_count =
            e_concurrent.nodes.iter().filter(|entry| entry.value().parents.is_some()).count();
        let degree_sum: usize = e_concurrent
            .nodes
            .iter()
            .map(|entry| entry.value().degree.load(Ordering::Acquire))
            .sum();
        assert_eq!(degree_sum, 2 * nonprim_count);
    }

    #[test]
    fn relaxed_happens_before_post_join_consistent() {
        // qa-sentinel round-3 demand: the merged-map design releases
        // the new_bytes shard write-lock BEFORE the parent degree
        // fetch_adds (the B1 deadlock mitigation). This shifts the
        // ordering contract: a racing reader who observes new_d via
        // saturation may transiently see parent degrees not yet bumped.
        //
        // This test exercises that window with 32 threads and
        // independent chains (so the engine grows novel distinctions
        // continuously) and asserts that the POST-JOIN state is
        // consistent — the eventual consistency claim holds even
        // though the intermediate states may be loose.
        //
        // Falsification target: if the parent fetch_adds were ever
        // dropped (e.g., a future refactor that lost the `if inserted_new`
        // gate or the post-entry fetch_add block), this would catch it
        // because degree_sum would diverge from 2 * non_primordial_count.
        let e = Arc::new(DistinctionEngine::new());
        let n_threads = 32;
        let synth_per_thread = 50;

        let handles: Vec<_> = (0..n_threads)
            .map(|i| {
                let e_clone = Arc::clone(&e);
                thread::spawn(move || {
                    // Build a distinct per-thread seed so chains diverge
                    // (no cross-thread saturation hiding novel work).
                    let mut acc = e_clone.synthesize(e_clone.d0(), e_clone.d1());
                    for _ in 0..=i {
                        acc = e_clone.synthesize(acc, e_clone.d0());
                    }
                    let mut prev = e_clone.d0();
                    let mut cur = acc;
                    for _ in 0..synth_per_thread {
                        let next = e_clone.synthesize(cur, prev);
                        prev = cur;
                        cur = next;
                    }
                })
            })
            .collect();

        for h in handles {
            h.join().expect("thread join succeeds (invariant)");
        }

        // Post-join: structural invariant holds.
        e.check_structural_invariant().expect("post-join r = 2d − 3 holds (invariant)");

        // Post-join: the eventual consistency claim from synthesize's
        // happens-before contract docstring — sum of degrees equals
        // twice the non-primordial count. No fetch_add was lost despite
        // the entry lock being released before the parent bumps.
        let nonprim_count = e.nodes.iter().filter(|entry| entry.value().parents.is_some()).count();
        let degree_sum: usize =
            e.nodes.iter().map(|entry| entry.value().degree.load(Ordering::Acquire)).sum();
        assert_eq!(
            degree_sum,
            2 * nonprim_count,
            "relaxed happens-before: post-join sum invariant must hold (degree_sum={degree_sum}, expected={})",
            2 * nonprim_count
        );
    }

    #[test]
    fn or_insert_with_closure_runs_exactly_once() {
        // qa-sentinel round-2 fix: the bug being guarded is "fetch_add
        // ran N times outside the closure," which inflates PARENT
        // degrees by N, not the child's. The test asserts parent degrees
        // increment by exactly 1 (not N), since all N threads race the
        // SAME novel synthesize(d0, x); only one closure invocation
        // wins and performs the fetch_add.
        let e = Arc::new(DistinctionEngine::new());
        let x = e.synthesize(e.d0(), e.d1()); // pre-bind x

        let d0_before = e.degree(e.d0());
        let x_before = e.degree(x);

        let n_threads = 16;
        let mut handles = Vec::with_capacity(n_threads);
        for _ in 0..n_threads {
            let e_clone = Arc::clone(&e);
            let d0 = e_clone.d0();
            handles.push(thread::spawn(move || e_clone.synthesize(d0, x)));
        }
        let results: Vec<_> = handles
            .into_iter()
            .map(|h| h.join().expect("thread join succeeds (invariant)"))
            .collect();

        // All threads got the SAME child distinction (Axiom 1: determinism).
        let first = results[0];
        for r in &results {
            assert_eq!(*r, first);
        }

        // The load-bearing assertion: parent degrees increased by EXACTLY 1,
        // not by n_threads. Only one novel synthesis happened; the other
        // (n_threads - 1) hit the saturation fast-path and bumped nothing.
        assert_eq!(e.degree(e.d0()), d0_before + 1);
        assert_eq!(e.degree(x), x_before + 1);
    }

    // ----- Foreign-byte guard (debug-only) ------------------------------

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "foreign-byte injection")]
    fn foreign_byte_synthesize_panics_in_debug() {
        let e = DistinctionEngine::new();
        // A random distinction never registered in this engine.
        let foreign = Distinction::from_bytes_unchecked([0xCC; 16]);
        let _ = e.synthesize(foreign, e.d0());
    }

    // ----- Property-based tests (proptest, 10K cases) -------------------

    #[cfg(test)]
    mod proptests {
        use super::*;
        use proptest::prelude::*;

        /// Build a pool of 12 distinctions for proptest exploration:
        /// d0, d1, and 10 derived via a mix of chain and cross-fold
        /// patterns so the pool exercises both monotonically-growing
        /// IDs and more varied byte distributions across shards.
        fn build_pool() -> (DistinctionEngine, Vec<Distinction>) {
            let e = DistinctionEngine::new();
            let mut pool = vec![e.d0(), e.d1()];
            pool.push(e.synthesize(pool[0], pool[1]));
            pool.push(e.synthesize(pool[2], pool[0]));
            pool.push(e.synthesize(pool[2], pool[1]));
            pool.push(e.synthesize(pool[3], pool[4]));
            pool.push(e.synthesize(pool[5], pool[0]));
            pool.push(e.synthesize(pool[5], pool[1]));
            pool.push(e.synthesize(pool[6], pool[7]));
            pool.push(e.synthesize(pool[8], pool[2]));
            pool.push(e.synthesize(pool[9], pool[3]));
            pool.push(e.synthesize(pool[10], pool[4]));
            (e, pool)
        }

        proptest! {
            #![proptest_config(ProptestConfig::with_cases(10_000))]

            /// Commutativity (Axiom 2) holds for any pair of randomly
            /// chosen distinctions in the engine.
            #[test]
            fn prop_commutativity(i in 0usize..12, j in 0usize..12) {
                let (e, pool) = build_pool();
                let a = pool[i];
                let b = pool[j];
                prop_assert_eq!(e.synthesize(a, b), e.synthesize(b, a));
            }

            /// Irreflexivity (Axiom 3) holds for every distinction.
            #[test]
            fn prop_irreflexivity(i in 0usize..12) {
                let (e, pool) = build_pool();
                let a = pool[i];
                prop_assert_eq!(e.synthesize(a, a), a);
            }

            /// Idempotency on engine state — once a synthesis has been
            /// performed, repeating it must not grow the engine (Law 7,
            /// saturation).
            #[test]
            fn prop_idempotency_on_state(i in 0usize..12, j in 0usize..12) {
                let (e, pool) = build_pool();
                let _ = e.synthesize(pool[i], pool[j]);

                let count_after_first = e.distinction_count();
                let rel_after_first = e.relationship_count();

                for _ in 0..3 {
                    let _ = e.synthesize(pool[i], pool[j]);
                }

                prop_assert_eq!(e.distinction_count(), count_after_first);
                prop_assert_eq!(e.relationship_count(), rel_after_first);
            }
        }
    }

    // ----- Compile-time Send + Sync on DistinctionEngine ----------------

    #[test]
    fn engine_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<DistinctionEngine>();
    }

    // ----- InvariantError -----------------------------------------------

    #[test]
    fn invariant_error_message_includes_counts() {
        let err =
            InvariantError::BinaryParentageMismatch { all_distinctions: 5, parents_of_plus_two: 7 };
        let msg = format!("{err}");
        assert!(msg.contains("5"));
        assert!(msg.contains("7"));
    }
}
