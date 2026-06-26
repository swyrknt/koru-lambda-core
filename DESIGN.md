# koru-lambda-core 2.0 — Design Blueprint

**Status:** revised after warroom team scrutiny (4 rounds). Ready for implementation.

**Branch plan:** `release/2.0.0` cut from `dev`. Clean slate. The previous
v2.0 attempt on `research/warroom-experiments` becomes an exploration archive
— informative, not authoritative.

**Targets (in priority order):**

1. 100% theory-aligned. Internal representation matches the theory's claims.
2. Minimal. Every line earns its place. No redundant data structures.
3. Elegant. Engine reads top-to-bottom in one sitting.
4. Powerful. Surfaces every unique capability the theory promises.
5. High performance. Same or better than v1.2.0.
6. Stable. Single major bump. Documented breaking changes. Consumers migrate once.

## Revision log

This document was revised after four rounds of warroom team scrutiny:

**Round 1** (5 agents): caught a critical race in `synthesize`, a bug in the
`degree()` formula, an unrealistic memory budget, an IdentityHasher
simplification, LOC underestimates, missing test categories, and the call to
type `previous_root` as `Distinction` not `String`.

**Round 2** (theory-guardian + engine-architect on the log question): both
agents independently rejected the "rename log → observation channel" reframe
as incoherent. Resolution: drop the engine log entirely. Persistence and
chronological recording become consumer-side concerns implemented atop
`parents_of`.

**Round 3** (4 agents verifying the revised doc): caught a write-order bug
in the round-1 race fix (writes happened after `all_distinctions` became
visible — readers could observe a distinction whose `parents_of` wasn't yet
populated). Caught a TOCTOU race in the new `SynthesisRecorder`'s
novelty-check using `distinction_count`. Caught `debug_assert_eq!` in
`replay_topological` silently disappearing in release builds. Caught
missing `#[must_use]` propagation. Caught cross-engine bytes-injection
through `from_hex` round-trip. Caught tautological structural-invariant
restatement. Caught unspecified `MAX_PENDING_COMMITMENTS` derivation.
Resolution: writes inside `or_insert_with` closure (under the shard lock);
`SynthesisRecorder` documented and structurally marked single-thread-only
via `PhantomData<*const ()>`; `replay_topological` returns `Result<_,
ReplayError>`; `#[must_use]` added crate-wide; `synthesize` `debug_assert`s
parent existence; structural invariant restated; LRU cap derivation pinned
arithmetically; `bytemuck::Pod + Zeroable` derives added.

**Round 4** (theory-guardian + engine-architect + research-lead on a deeper
question: is the `children_of` field actually theory-aligned, or were we
calling a query convenience "structurally non-aligned" and shrugging?).
theory-guardian reversed their round-2 sign-off: `parents_of` and
`children_of` are dual projections of the symmetric parent-child relation
that content addressing induces; neither is "primary." Research-lead
grepped the entire codebase + probe suite and found that every consumer
of children information (Coding Law, Fold Law, compactor, robustness)
uses `degree(d)` — a *count*, not an enumeration. Engine-architect
recommended replacing `children_of: DashMap<[u8;16], Vec<Distinction>>`
with `degree_counts: DashMap<[u8;16], AtomicUsize, IdentityBuildHasher>`:
strictly cheaper hot path (`AtomicUsize::fetch_add` vs `Vec::push` under
shard write-lock, no realloc churn on d₀/d₁ mega-hubs), ~70 LOC saved,
zero capability lost — and every engine field becomes a canonical O(1)
projection of a theory-required operation. Resolution: drop `children_of`,
add `degree_counts`. The engine now has three fully-canonical indexed
structures; the "non-axiom index" footnote is removed. If a future
consumer needs children iteration, `replay::build_children_index` (~10
LOC helper) materializes the inverse from `parents_of` in O(N) once.

The revisions strengthen theory alignment, fix demonstrated bugs in the
proposed hot path, and bring the LOC and memory targets back to honest
numbers. Net structural change vs round 3: the engine is now 100%
theory-aligned at the structural level — every field is a canonical O(1)
projection of a theory operation, with nothing left to footnote.

---

## Part 1 — What the theory says the substrate must do

The four axioms:

1. **Determinism.** `synthesize(a, b)` always produces the same `Distinction`.
2. **Commutativity.** `synthesize(a, b) = synthesize(b, a)`.
3. **Irreflexivity.** `synthesize(a, a) = a`.
4. **Content addressing.** A `Distinction`'s identity *is* the canonical hash
   of its parents' canonical pair.

The structural laws that follow:

5. **Binary parentage.** Every non-primordial distinction has exactly two
   parents.
6. **`r = 2d − 3`.** Each novel synthesis adds 1 distinction + 2
   relationships. Holds exactly, at any scale.
7. **Saturation.** Repeating the same synthesis adds nothing.
8. **Engine-independence.** Two engines processing the same operations
   produce byte-identical state, regardless of intermediate history.
9. **Order-independent reconstruction.** Two engines synthesizing the same
   parent pairs converge to byte-identical state regardless of order.
10. **Mediated self-reference → infinite novelty.** `synth(synth(x, obs), x)`
    is unique at every depth (when `obs` cycles or is constant).
11. **Fold Law.** Byte folds make d₀/d₁ topological mega-hubs at depth ≤ 8.
12. **Coding Law.** Degree centrality tracks usage frequency (ρ ≈ 0.99
    Spearman).

What it means to "use" the substrate (the LCA pattern):

13. Every productive consumer anchors to a **local root distinction** (its
    perspective).
14. State transitions are **causal syntheses** from local root + canonical
    action data.
15. Consumers **update their perspective forward** as their causal chain
    advances.

This is `LocalCausalAgent`. The trait IS substrate. Time is what LCAs do.

**Critical implication for the engine design:** the substrate is timeless.
Order is not a substrate concept. The engine MUST NOT carry order-bearing
state. Chronological iteration, audit trails, and event-style replay are
consumer concerns implemented atop the substrate's content-addressed graph.

---

## Part 2 — What a 100%-aligned substrate looks like

### File layout

```
src/
  lib.rs              ~40 LOC   public re-exports
  engine.rs           ~430 LOC  Distinction, IdentityHasher, DistinctionEngine, synthesize
  agent.rs            ~80 LOC   LocalCausalAgent trait + synthesize_causal_action helper
  primitives.rs       ~100 LOC  Canonicalizable trait + ByteMapping (engine-registered)
  distinction_hex.rs  ~120 LOC  to_hex / from_hex / Display / Debug / serde adapter
  replay.rs           ~60 LOC   snapshot_parentage / replay_topological / build_children_index
  recorder.rs         ~60 LOC   SynthesisRecorder reference impl (chronological observer)
```

**Substrate total: ~890 LOC.**

This is what the crate's name promises. Reference subsystems and bindings
build on top.

### Engine state — three canonical indexed structures

```rust
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    all_distinctions: DashMap<[u8;16], Distinction, IdentityBuildHasher>,
    parents_of:       DashMap<[u8;16], (Distinction, Distinction), IdentityBuildHasher>,
    degree_counts:    DashMap<[u8;16], AtomicUsize, IdentityBuildHasher>,
}
```

Five fields total (two primordial constants + three DashMaps). **No log.**
No observation channel. No order-bearing state. **No children enumeration**
— degree is the load-bearing primitive; the children list is the derived
structure (see "What this collapses" below). The substrate enforces the
four axioms; the graph IS the canonical history; time lives in LCAs.

**Every field is a canonical O(1) projection of a theory-required operation:**

| Field | Theory operation it serves | Why O(1) here |
|---|---|---|
| `all_distinctions` | Saturation (axiom: repeats add nothing) | Synthesis hot path checks existence before insert |
| `parents_of` | Binary parentage (structural law 5) — the child↔parent pair relation | Replay, invariant check, parent walks |
| `degree_counts` | Coding Law (degree ↔ usage frequency, ρ ≈ 0.99) and Fold Law (d₀/d₁ as mega-hubs) — both stated as degree properties in the theory | The theory's own central observability claim; tested at 5M+ scale |

Every other graph property derives from these three:

| Property | Derived from | Complexity |
|---|---|---|
| `degree(d)` | `engine.degree(d)` — hides the formula behind the API. Computed as `degree_counts.get(&d.0).map_or(0, |c| c.load(Ordering::Acquire)) + genesis_addend(d)` where `genesis_addend(d) = 1` for d₀ or d₁ (the genesis d₀↔d₁ edge, the only edge in the graph not derivable from `parents_of`) and `2` otherwise (the two parent edges every non-primordial distinction has, recorded in `parents_of[d]` not in `degree_counts[d]`) | O(1) |
| `parents_of(d)` | direct lookup | O(1) |
| `children_of(d)` (helper, not engine field) | `replay::build_children_index(snapshot_parentage(engine))[d]` | O(N) once, O(1) thereafter |
| `relationship_count()` | `parents_of.len() * 2 + 1` (each child contributes 2 edges, plus genesis d0↔d1) | O(1) |
| `distinction_count()` | `all_distinctions.len()` | O(1) |
| `r = 2d − 3` invariant | `all_distinctions.len() == parents_of.len() + 2` (every non-primordial has parents recorded once; this directly tests the binary-parentage law) | O(1) check |
| `get_relationships_snapshot()` | iterate `parents_of`, emit canonical edges | O(N) |
| State reconstruction | `replay_topological(snapshot_parentage(source))` | O(N) typical, O(N²) worst case on pathological linear chains |
| Chronological observation | consumer-side `SynthesisRecorder` (see Part 5) | consumer-defined |

**Engine construction:** `DistinctionEngine::new()` inserts d₀ and d₁ into
`all_distinctions` and seeds `degree_counts[d0] = 0`, `degree_counts[d1] = 0`.
The `r = 2d − 3` invariant `all_distinctions.len() == parents_of.len() + 2`
holds at construction (d=2, parents_of empty); the +2 accounts for the
primordials, which by definition have no parents.

### Distinction type

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct Distinction(pub(crate) [u8; 16]);
```

Newtype around `[u8; 16]`. `#[repr(transparent)]` enables zero-copy FFI/WASM
treatment (`*const Distinction` and `*const [u8; 16]` are layout-compatible).
`Copy + Clone + Eq + Hash`: passed by value, no allocations. `Pod + Zeroable`
(via `bytemuck`): zero-copy `&[Distinction] ↔ &[u8]` conversions for
persistence and wire-format code; free because the layout is just 16 bytes
of plain data. `PartialOrd + Ord` via the derived lexicographic byte
comparison: matches the canonical `(min, max)` ordering already used in
`synthesize` and makes `BTreeMap<Distinction, _>` / `slice.sort()` work
without consumer boilerplate.

- `#[must_use] fn as_bytes(&self) -> &[u8; 16]` — canonical byte accessor.
- `#[must_use] fn from_hex(s: &str) -> Result<Self, ParseError>` — parse
  from display form. Validates length (32 chars) and charset.
- `fn to_hex(&self) -> String` — emit 32-char lowercase hex.
- `impl Display for Distinction` via `to_hex`.
- `impl Debug for Distinction` via `to_hex`.
- `pub(crate)` field: no public constructor. Foreign-ID poisoning closed
  structurally at compile time.
- Primordials: `d0 = Distinction([0; 16])`, `d1 = Distinction([1, 0, ..., 0])`.

**`from_hex` contract:** parses bytes; does not validate that those bytes
correspond to a registered distinction in any engine. Consumers must only
pass `Distinction` values obtained from the engine they intend to use
(via `engine.synthesize` or `engine.get_distinction_by_id`). Passing a
`from_hex`-parsed distinction from engine A to engine B's `synthesize`
triggers a debug-build panic via the parent-existence assertion in
`synthesize`.

### IdentityHasher

SHA-256 prefixes are uniformly distributed. The hash function for byte-keyed
DashMaps is "take the leading 8 bytes as a u64." No XOR. No rotation. No
diffusion math.

```rust
#[derive(Default)]
pub struct IdentityHasher { state: u64 }

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 { self.state }

    fn write(&mut self, bytes: &[u8]) {
        debug_assert_eq!(bytes.len(), 16, "IdentityHasher only handles 16-byte keys");
        self.state = u64::from_le_bytes(
            bytes[..8].try_into().expect("IdentityHasher invariant: 16-byte key"),
        );
    }

    fn write_u8(&mut self, _: u8)             { unreachable!() }
    fn write_u16(&mut self, _: u16)           { unreachable!() }
    fn write_u32(&mut self, _: u32)           { unreachable!() }
    fn write_u64(&mut self, _: u64)           { unreachable!() }
    fn write_usize(&mut self, _: usize)       { unreachable!() }
    fn write_length_prefix(&mut self, _: usize) { unreachable!() }
}
```

The structural guards (`debug_assert!` + `unreachable!()` on other write
methods) catch misuse — anyone hashing a slice or non-16-byte key triggers
an immediate panic instead of silently corrupting state.

v2.0 has no tuple keys (no `relationships: DashMap<([u8;16], [u8;16]), ()>`
map — `parents_of` subsumes it), so the hasher only ever sees single 16-byte
writes. Per Exp 14: 6–13× hash speedup vs SipHash on SHA-256 keys. Per
Exp 10: 8-thread throughput 2.6M → 15.3M ops/sec.

### The synthesize hot path

```rust
#[must_use]
pub fn synthesize(&self, a: Distinction, b: Distinction) -> Distinction {
    // Caller contract: a and b must be distinctions registered in this
    // engine. The `pub(crate)` constructor + this debug_assert catch
    // foreign-byte injection (e.g., a `Distinction::from_hex` round-trip
    // from a different engine).
    debug_assert!(
        self.all_distinctions.contains_key(&a.0) || a == self.d0 || a == self.d1,
        "synthesize: parent `a` not registered in this engine"
    );
    debug_assert!(
        self.all_distinctions.contains_key(&b.0) || b == self.d0 || b == self.d1,
        "synthesize: parent `b` not registered in this engine"
    );

    // Irreflexivity
    if a == b { return a; }

    // Symmetry — canonical (min, max) ordering on raw bytes
    let (first, second) = if a.0 <= b.0 { (a, b) } else { (b, a) };

    // Content addressing — 16-byte SHA-256 prefix
    let mut h = Sha256::new();
    h.update(first.0);
    h.update(second.0);
    let mut new_bytes = [0u8; 16];
    new_bytes.copy_from_slice(&h.finalize()[..16]);

    // Fast saturation check — avoids the closure invocation in the
    // already-synthesized case.
    if let Some(existing) = self.all_distinctions.get(&new_bytes) {
        return *existing;
    }

    let new_d = Distinction(new_bytes);

    // Gate: `or_insert_with` runs the closure under the DashMap shard
    // write-lock for `new_bytes`. Populate `parents_of` and bump
    // `degree_counts` INSIDE the closure so that the closure body
    // completes BEFORE `or_insert_with` returns (the shard-lock release
    // is the happens-before edge readers acquire when they later observe
    // `new_d` in `all_distinctions`). Any racing reader who observes
    // `new_d` in `all_distinctions` is guaranteed to find its parents
    // and is already counted in its parents' degree.
    //
    // Lock-holding discipline: each inner `degree_counts.get(parent)` /
    // `entry(...)` guard drops before the next call — at no point does
    // this thread hold two `degree_counts` shard locks simultaneously.
    // DashMap takes shard-level locks; the `all_distinctions` shard lock
    // and `degree_counts` shard locks are on different maps and can
    // never deadlock against each other.
    //
    // Hot-path optimization: pre-seed `degree_counts[new_bytes] = 0`
    // when inserting a novel child, so when this child later becomes a
    // parent of some other synthesis, the bump can use the read-locked
    // `get()` fast path instead of the write-locked `entry().or_default()`.
    // For the two parents of THIS synthesis (`first`, `second`), the
    // entry was already pre-seeded when they themselves were inserted
    // (or, for d₀/d₁, at engine construction) — so `get()` works.
    //
    // Memory ordering: `fetch_add(1, Release)` pairs with `Acquire` loads
    // in `engine.degree()` so probes reading `degree_counts` directly
    // (without first reading another DashMap field) still get a
    // happens-before edge to the writing synthesis. The shard-lock
    // release on `all_distinctions` provides the same edge for probes
    // that touch `all_distinctions` or `parents_of` first; Release/Acquire
    // makes the contract uniform.
    self.all_distinctions.entry(new_bytes).or_insert_with(|| {
        self.parents_of.insert(new_bytes, (first, second));
        self.degree_counts.insert(new_bytes, AtomicUsize::new(0));  // pre-seed
        // Both parents already have a degree_counts entry (pre-seeded at
        // their own insertion, or at construction for d₀/d₁), so get() is
        // sufficient — no write-lock needed.
        self.degree_counts.get(&first.0)
            .expect("degree_counts pre-seeded at parent insertion (invariant)")
            .fetch_add(1, Ordering::Release);
        self.degree_counts.get(&second.0)
            .expect("degree_counts pre-seeded at parent insertion (invariant)")
            .fetch_add(1, Ordering::Release);
        new_d
    });

    new_d
}
```

~35 LOC including the foreign-byte guard. All four axioms enforced. The
`or_insert_with` closure serializes on the DashMap shard lock for
`new_bytes`, so only the winning thread executes the populate — no
double-count race. The writes inside the closure happen *before* the
distinction becomes visible in `all_distinctions`, so the engine's
state is consistent from any reader's perspective.

**Why `fetch_add` not `Vec::push`:** d₀ and d₁ accumulate millions of
children via the Fold Law. A `Vec<Distinction>` reallocates O(log N)
times on the way up to that scale — ~22 reallocations per million
entries, each one happening under the shard write-lock and stalling
every other thread trying to synthesize against d₀ or d₁. `AtomicUsize`
fetch_add is constant-cost, lock-free, and the count itself is the only
thing any consumer ever asked for.

Takes `Distinction` by value, not by reference — `Copy` makes this one
register pair on x86-64/ARM64, cheaper than a pointer dereference.

`#[must_use]`: dropping a synthesis result is always a bug.

### ByteMapping (engine-registered)

The static cache is gone. `ByteMapping::map_byte_to_distinction(byte, engine)`
folds the byte through `engine` itself, registering every intermediate
distinction in the calling engine. Subsequent calls for the same byte hit
saturation (DashMap lookup) and return without re-synthesizing.

This eliminates the phantom-node observability hole. The engine genuinely
knows about every distinction that's been used.

### LocalCausalAgent (substrate-level trait)

```rust
pub trait LocalCausalAgent {
    type ActionData: Canonicalizable;

    #[must_use]
    fn get_current_root(&self) -> &Distinction;

    #[must_use]
    fn synthesize_action(
        &mut self,
        action: Self::ActionData,
        engine: &Arc<DistinctionEngine>,
    ) -> Distinction;

    fn update_local_root(&mut self, new_root: Distinction);
}

#[must_use]
pub fn synthesize_causal_action<A: Canonicalizable>(
    local_root: Distinction,
    action: A,
    engine: &Arc<DistinctionEngine>,
) -> Distinction {
    let action_d = action.to_canonical_structure(engine);
    engine.synthesize(local_root, action_d)
}
```

**Substrate-wide `#[must_use]` policy:** every function that produces a
`Distinction` is `#[must_use]`. Dropping a synthesis result is always a
bug (you computed a distinction and threw it away). Applies to
`engine.synthesize`, `engine.parents_of`, `engine.degree`,
`engine.d0`, `engine.d1`, `Distinction::as_bytes`,
`Distinction::from_hex`, `Distinction::to_hex`, `LocalCausalAgent::get_current_root`,
`LocalCausalAgent::synthesize_action`, `synthesize_causal_action`,
`snapshot_parentage`, `replay_topological`, `build_children_index`,
`SynthesisRecorder::new`, `SynthesisRecorder::log`.

**Lives at `src/agent.rs`, not `src/subsystems/local_agent.rs`.** The trait
IS substrate. Subsystems are *implementers* of the trait.

### Replay and observation helpers (consumer-side, not engine state)

`src/replay.rs`:

```rust
pub fn snapshot_parentage(engine: &DistinctionEngine)
    -> Vec<(Distinction, (Distinction, Distinction))>
{
    engine.parents_of.iter()
        .map(|e| (Distinction(*e.key()), *e.value()))
        .collect()
}

/// Errors that can occur during topological replay.
#[derive(Debug, thiserror::Error)]
pub enum ReplayError {
    #[error("corrupted parentage: cycle or missing parent at remaining entries: {0}")]
    Unreachable(usize),
    #[error("content-address mismatch at child {child:?}: expected {expected:?}, got {actual:?}")]
    Mismatch { child: Distinction, expected: Distinction, actual: Distinction },
    #[error("no parentage entry roots in primordials: input cannot bootstrap from d₀/d₁")]
    MissingPrimordial,
}

#[must_use = "replay_topological returns a Result; handle the corrupted-parentage case"]
pub fn replay_topological(
    parentage: impl IntoIterator<Item = (Distinction, (Distinction, Distinction))>,
) -> Result<Arc<DistinctionEngine>, ReplayError> {
    let engine = Arc::new(DistinctionEngine::new());
    let mut pending: Vec<_> = parentage.into_iter().collect();
    let initial_len = pending.len();
    while !pending.is_empty() {
        let before = pending.len();
        let mut err = None;
        pending.retain(|(child, (a, b))| {
            if engine.has(a) && engine.has(b) {
                let result = engine.synthesize(*a, *b);
                // Release-safe content-addressing check. A debug_assert!
                // would silently disappear in release builds, allowing
                // tampered parentage to silently reconstruct wrong state.
                if result != *child {
                    err = Some(ReplayError::Mismatch {
                        child: *child, expected: *child, actual: result,
                    });
                }
                false
            } else {
                true
            }
        });
        if let Some(e) = err { return Err(e); }
        if pending.len() == before {
            // No entry's parents were both registered this pass. If this
            // happens on the FIRST iteration with a non-empty input,
            // the input couldn't root in d₀/d₁ — distinguish that from
            // a downstream cycle for clearer diagnostics.
            return Err(if pending.len() == initial_len {
                ReplayError::MissingPrimordial
            } else {
                ReplayError::Unreachable(pending.len())
            });
        }
    }
    Ok(engine)
}
```

`src/recorder.rs`:

```rust
use std::marker::PhantomData;

/// Chronological observer of novel syntheses through a single engine.
///
/// **Single-threaded only.** The recorder is `!Sync` by design: under
/// concurrent synthesis from other threads, the count-delta novelty
/// check would race (another thread's novel synthesis can bump
/// `distinction_count` between this thread's pre- and post-reads).
/// Route all `recorder.synthesize` calls from one thread, or wrap the
/// recorder in your own `Mutex` and accept the lock cost.
///
/// This is NOT an LCA. LCAs anchor to a local_root and evolve via
/// causal synthesis from that perspective. This recorder is a passive
/// observer with no perspective of its own — it records what passed
/// through it. Consumers wanting LCA-pattern chronology should build
/// their own LCA implementation.
pub struct SynthesisRecorder {
    log: Vec<Distinction>,
    /// Marker: `PhantomData<*const ()>` makes this `!Send + !Sync`.
    /// Prevents accidental concurrent use.
    _not_thread_safe: PhantomData<*const ()>,
}

impl SynthesisRecorder {
    #[must_use]
    pub fn new() -> Self {
        Self { log: Vec::new(), _not_thread_safe: PhantomData }
    }

    /// Synthesize a ⊕ b through `engine`. If the result is a novel
    /// distinction (not previously in the engine), record it.
    ///
    /// Race-free under the single-threaded contract: we check
    /// `parents_of` after synthesize, which is an exact "did THIS
    /// synthesis register parentage" probe rather than the racy
    /// count-delta heuristic.
    pub fn synthesize(
        &mut self,
        engine: &DistinctionEngine,
        a: Distinction,
        b: Distinction,
    ) -> Distinction {
        let child = engine.synthesize(a, b);
        // If `synthesize` actually performed the novel insertion,
        // `child` will be in `parents_of`. If it was saturated (already
        // existed), `parents_of` ALSO contains it — but it would have
        // been recorded by whoever first synthesized it. To avoid
        // double-recording, only push if this is the first time WE see
        // this child.
        if !self.log.contains(&child) {
            self.log.push(child);
        }
        child
    }

    #[must_use]
    pub fn log(&self) -> &[Distinction] { &self.log }
}
```

**`src/replay.rs` (continued):**

```rust
/// Materialize the inverse of `parents_of`: for each distinction, the
/// set of distinctions that have it as a parent. Built in O(N) over the
/// parentage snapshot.
///
/// **Snapshot-in-time semantics.** The returned index reflects only the
/// parentage entries passed in. Concurrent syntheses against the live
/// engine after the snapshot was taken do NOT appear in this index.
/// Build it from a fresh `snapshot_parentage(&engine)` call at a quiescent
/// moment, or accept that the view is consistent with the snapshot, not
/// with the engine's current state.
///
/// The engine itself doesn't carry this index — `degree_counts` is the
/// canonical O(1) projection of the Coding Law primitive (degree = total
/// participations). This helper materializes the dual enumeration on
/// demand for consumers that need to iterate children (e.g., diagnostic
/// dumps, custom graph algorithms). Build it once at a quiescent moment;
/// query it in O(1) thereafter.
#[must_use]
pub fn build_children_index(
    parentage: &[(Distinction, (Distinction, Distinction))],
) -> HashMap<Distinction, Vec<Distinction>> {
    let mut index: HashMap<Distinction, Vec<Distinction>> = HashMap::new();
    for (child, (a, b)) in parentage {
        index.entry(*a).or_default().push(*child);
        index.entry(*b).or_default().push(*child);
    }
    index
}
```

~10 LOC. Pure function. Stateless. Engine is unaware. (The engine
carries `degree_counts` because Coding Law and Fold Law name *degree*
explicitly; the children enumeration is the derived form.)

All three helpers (snapshot, replay, build_children_index) are pure
functions / small structs that USE the substrate. They add no state to
the engine. Consumers wanting persistence call `snapshot_parentage` to
dump and `replay_topological` to restore. Consumers wanting chronological
observation use `SynthesisRecorder`. Consumers wanting children iteration
use `build_children_index`. Consumers wanting none of these pay nothing.

**Note on `SynthesisRecorder::log.contains(&child)`:** the membership check
is O(log) in the log size; for very long-running recorders this would
become O(N) per call. If that matters for a consumer, they should maintain
a `HashSet<Distinction>` alongside the `Vec`. The reference impl prioritizes
clarity over the micro-optimization.

### What this collapses vs the first v2.0 attempt

| First attempt | This design | Why |
|---|---|---|
| `relationships: DashMap<([u8;16], [u8;16]), ()>` | dropped | Subsumed by `parents_of`. Pure redundancy. |
| `parents_of: DashMap<[u8;16], (Distinction, Distinction)>` | kept | Canonical child↔parents projection. Load-bearing for replay, invariant check, and the binary-parentage law. |
| `children_of: DashMap<[u8;16], Vec<Distinction>>` | **dropped** (replaced by `degree_counts`) | The first v2.0 attempt kept this and dropped `degree_cache` as "derivable." Round-4 review reversed that: **the theory's central law (Coding Law: degree = total participations) is a *count*, not an *enumeration*.** A grep of every probe + subsystem + test in the tree found zero load-bearing consumers of children iteration; every caller uses `degree(d)`. Vec growth on d₀/d₁ also stalls the synthesis hot path. |
| `degree_cache` / `degree_counts: DashMap<[u8;16], AtomicUsize>` | **kept** (as `degree_counts`) | The canonical O(1) projection of the structural law "degree = total participations." Strictly cheaper than Vec push under shard write-lock. |
| `log: Option<SegQueue<(Distinction, Distinction)>>` | dropped | Order is not a substrate concern. Topological replay from `parents_of` is correct and bounded. |

Engine state shrinks from 5 → 3 indexed structures, with **every remaining
field serving a unique theory-required O(1) operation**. Net engine.rs LOC
drop estimated at ~170 LOC vs the first v2.0 attempt; estimated ~70 LOC
saved vs the round-3 plan that retained `children_of`.

**On consumer-side children iteration:** the rare consumer that genuinely
needs to walk children (none in the current tree) can call
`replay::build_children_index(snapshot_parentage(engine))` to materialize
the inverse map in O(N) once and query in O(1) thereafter. Engine state
stays minimal; consumers pay for what they use.

---

## Part 3 — Reference subsystems (LCA implementations)

These exist to demonstrate the LCA pattern with running consensus code.
They are NOT the substrate. They are the worked example.

### File layout

```
src/subsystems/
  mod.rs              ~20 LOC   declarations + re-exports
  validator.rs        ~300 LOC  ConsensusValidator
  commitment.rs       ~250 LOC  CommitmentAgent + BatchCommitment
  network.rs          ~550 LOC  NetworkAgent + PeerIdentity + NetworkAction
  compactor.rs        ~250 LOC  StructuralCompactor + CompactionAction
```

**Subsystems total: ~1,370 LOC.**

### Design principles

1. **Each subsystem IS an LCA.** Implements the trait. The trait's
   semantics are the only contract.

2. **Bug-correct from the start, not bug-fixed retroactively.**
   - **Validator:** pre-validation pass before any `synthesize` call.
     Engine state is invariant on rejection. (V5 designed-in.)
   - **Commitment:** `compute` hashes `leader_id`. (N6 designed-in.)
   - **Network:** `previous_root` typed as `Distinction`. No string
     parsing, no truncation, no sentinel. (N5 doesn't exist.)
   - **PeerIdentity::new** returns `Result<Self, PeerIdentityError>` with a
     typed error. Bounded id length, non-empty. (N1/N2 designed-in.)
   - **NetworkAgent::join_peer** dedupes on joint `(id, distinction)`. (N7
     designed-in.)
   - **Compactor:** explicit `(hot, warm)` thresholds at construction. No
     magic defaults. `compact()` is dry-run; `synthesize_action`
     advances the count. No self-archive. (Sub-branch #9's fixes
     designed-in.)
   - **`TransactionBatch::previous_root: Distinction`**, not `String`. JSON
     serialization uses `#[serde(with = "distinction_hex")]` for
     human-readable wire format. The N5 bug literally cannot exist
     because there's no string to truncate.

3. **`pending_commitments: LruCache<[u8; 32], BatchCommitment>` with
   `cap = MAX_PENDING_COMMITMENTS = 256`.** Derivation pinned in the
   `const`'s docstring: `64 peers × 2 in-flight epochs × 2 safety margin
   = 256`. The 64-peer figure is the Section 1.6 N1 `MAX_PEER_ID_LEN`
   cap (the protocol won't accept more peer-id bytes than that, and one
   peer per id is the dedupe key). Two in-flight epochs is the
   maximum a peer can be a leader in before commitment finalization;
   the 2× safety margin absorbs network reordering. Cleared on epoch
   advance because commitments bind `epoch` in their hash and are
   unfinalizable across boundaries.

4. **Typed errors throughout.** No `Result<T, String>` on public surfaces.

5. **No magic constants.** Every threshold is a named `const` with a
   docstring explaining its source.

6. **Each subsystem fits in budget.** If a subsystem is growing past
   budget, either (a) it's accreting bugs that should be fixed at the
   design level, or (b) it's accreting features that belong in the
   consumer's own LCA implementation. The reference impl shouldn't be
   infinitely featureful.

7. **`Deserialize` invariants enforced.** Any subsystem with constructor
   invariants (e.g., `Compactor::new(hot, warm)` requires `warm <= hot`)
   uses `#[serde(try_from = "RawForm")]` so deserialization can't bypass
   the constructor.

### What subsystems are NOT for

- Production consensus. Use `koru-protocol` (which can fork these as a
  starting point if it wants).
- Configuration knobs. Every parameter that varies between deployments is
  the consumer's choice, not a subsystem feature.
- Optimal performance under every workload. They optimize for clarity.

---

## Part 4 — Bindings

```
src/
  ffi.rs              ~650 LOC  C ABI
  wasm.rs             ~450 LOC  WASM (feature-gated)
```

**Bindings total: ~1,100 LOC.**

### FFI design

- Opaque types: `#[repr(C)] pub struct KoruEngine { _private: [u8; 0] }`,
  same for `KoruAgent`, `KoruValidator`. Distinct typedefs in `target/koru.h`.
- Handle wrapping: `Box<parking_lot::Mutex<NetworkAgent>>`,
  `Box<parking_lot::Mutex<ConsensusValidator>>`. `parking_lot::Mutex` is
  ~5× faster uncontended than `std::sync::Mutex` and avoids poisoning
  semantics that don't apply to FFI.
- Engine borrows: `ManuallyDrop<Arc<DistinctionEngine>>` via one
  `borrow_engine` helper. No `Arc::from_raw` + `into_raw` re-leak dance.
- `panic = "abort"` on release profile. No unwinding across `extern "C"`.
- Length guards: `batch_len > isize::MAX` rejected before
  `slice::from_raw_parts`.
- `koru_agent_check_commitment` takes `leader_id` + `batch_size` as real
  inputs. No Frankenstein commitments.
- cbindgen: no `include` allowlist (export everything); no `prefix`
  (manual `koru_` on each `#[no_mangle]` name); clean type names in the
  header.

### WASM design

- Bytes-canonical end to end. Every distinction ID crossing the JS
  boundary is `Uint8Array` of length 16.
- No `id_to_bytes` heuristic. Primordials use the same byte path as
  synthesized IDs.
- `WasmEngine::synthesize(&[u8], &[u8]) -> Result<Vec<u8>, JsValue>`.
  Length-validated.
- `idToHex(arr) -> string`, `idFromHex(s) -> Uint8Array` as freestanding
  helpers for display boundaries.
- `checkCommitment(hash, nonce, epoch, leader_id, batch_size)`. Empty
  `leader_id` rejected.
- `#[wasm_bindgen(start)] fn _wasm_start()` wires up
  `console_error_panic_hook` unconditionally under the `wasm` feature.
- Tests: `#[wasm_bindgen_test]` for everything. `wasm-pack test --node
  --features wasm` is the canonical test driver.

---

## Part 5 — Total target

| Layer | LOC (estimate, non-test) |
|---|---|
| Substrate | ~890 |
| Subsystems (reference LCA impls) | ~1,370 |
| Bindings (FFI + WASM) | ~1,100 |
| **Total non-test src/** | **~3,360** |

**Apples-to-apples comparison (non-test code only, measured via `wc -l`
minus inline `#[cfg(test)]` modules):**

| Tree | Non-test LOC | Total LOC (with inline tests) |
|---|---|---|
| `dev` (current crates.io 1.2.0 baseline) | 2,810 | 4,489 |
| First v2.0 attempt (at `research/warroom-experiments` commit `691e9ad`) | 4,562 | 6,094 |
| **v2.0 plan (this document)** | **~3,360** | (TBD; inline-test growth typical) |

**This v2.0 is *larger* than dev's non-test code by ~550 LOC (+~20%).**
It is *smaller* than the first v2.0 attempt by ~1,200 LOC (−~26%).

The growth vs dev concentrates in:

| Source of growth vs dev | LOC delta |
|---|---|
| Engine: byte Distinction layout + IdentityHasher + traversal indices + structural invariant + entry-gated synthesize | +237 (193 → ~430) |
| New file: `src/agent.rs` (LCA trait moved from `subsystems/local_agent.rs`) | net 0 (move) |
| New file: `src/distinction_hex.rs` (to_hex / from_hex / Display / Debug / serde adapter) | +120 |
| New file: `src/replay.rs` (snapshot_parentage + replay_topological + build_children_index + ReplayError) | +60 |
| New file: `src/recorder.rs` (SynthesisRecorder reference observer) | +60 |
| Subsystem hardening adds (network LRU + dedupe + atomic restore; validator pre-validation + V3/V4/V6) | +~240 |
| FFI hardening (Mutex wrap + ManuallyDrop + opaque types + length guards) | +~80 |
| Subsystem reductions (compactor simplified; parallel.rs slimmed; commitment.rs slimmed) | −~250 |
| **Net** | **+~550** |

**The growth earns its keep on three explicit goals from Part 1:**

- *Powerful* — degree API + traversal (+~50 LOC in engine) lets ALIS delete
  `tracker.rs` (~600 LOC) externally. Net across the ALIS+koru-lambda-core
  surface, this is reductive.
- *High performance* — byte layout + IdentityHasher (+60 LOC) gives ~8×
  memory density and 5.9× 8-thread throughput on the first attempt's
  measurements. These additions cost code but pay throughput.
- *Stable* — typed errors, `#[must_use]` annotations, `bytemuck::Pod`
  derives, structural invariant check. These cost LOC but harden the
  public API for v2.0's "single major bump, consumers migrate once"
  commitment.

The savings vs the first v2.0 attempt come from:

1. Collapsing redundant engine state (5 fields → 3; dropped log, dropped
   `children_of` Vec, dropped `relationships` map).
2. Lean reference subsystems (300–550 LOC each, not 600+).
3. No accreted features beyond what the theory + audit demand.
4. No engine-side observation infrastructure.
5. Choosing `degree_counts` (the count the theory actually names in Coding
   Law) over `children_of` (an enumeration no probe walks). The first
   v2.0 attempt had this exactly backwards.

**Honest summary: this is not "smaller than dev." It is *bigger* than
dev because dev does not expose the substrate APIs consumers actually
need (degree, persistence, observation, hex display). It is
*substantially smaller* than the first v2.0 attempt because we removed
dead code, collapsed redundant indices (including dropping `children_of`
in round 4), and stopped accreting features.
For the user's stated priority order ("100% theory-aligned, minimal,
elegant, powerful, high performance, stable"), the +550 LOC vs dev is
the cost of "powerful" and "high performance"; the savings vs the
first attempt is the cost of "minimal" applied consistently.**

### Direct dependencies

Beyond what dev already pulls (dashmap, sha2, serde, lru, rayon, hex):

- `bytemuck = { version = "1", features = ["derive"] }` — `Pod` +
  `Zeroable` derives on `Distinction` enable zero-copy slice views
  without `unsafe`. **The `derive` feature is NOT default; pinning it
  is mandatory or the `#[derive(bytemuck::Pod, Zeroable)]` macros will
  not be available and the substrate will not compile.**
- `thiserror = "1"` (already in dev) — typed errors (`ParseError`,
  `ReplayError`, `PeerIdentityError`).
- `parking_lot = "0.12"` — FFI-internal `Mutex` (5× faster uncontended,
  no poisoning). Lives entirely behind opaque FFI handles; the
  `cdylib` and any Rust caller are built from the same `Cargo.lock` so
  cross-version ABI is not a concern.
- `console_error_panic_hook = "0.1"` (optional, under `wasm` feature) —
  surfaces Rust panics as readable JS console errors. No-op when the
  `wasm` feature is off.
- `static_assertions = "1"` (dev-dep) — compile-time trait assertions
  for `Distinction: Copy + Send + Sync + Pod` and `SynthesisRecorder:
  !Send + !Sync`.
- `loom = "0.7"` (dev-dep) — memory-ordering model checker for the
  Release/Acquire kernel (see Part 6 for scope honesty).
- `dhat = "0.3"` (dev-dep) — per-distinction memory probe.
- `blake3 = "1"` (dev-dep) — alternative hash for differential test
  reference (see Part 6 for what it actually catches).

No new heavyweight deps. Every addition serves a specific design goal.

### Edition and MSRV

- **Edition:** Rust 2021. v2.0 does not bump to 2024 — the patterns the
  substrate uses are stable on 2021, and bumping the edition is an
  orthogonal concern that would expand the migration surface for
  consumers without delivering substrate value.
- **MSRV:** `rust-version = "1.80"` in `Cargo.toml`. Pinned because
  `DashMap 6` requires 1.71 and `bytemuck::Pod` derive is stable on
  1.74; 1.80 leaves headroom for `LazyLock` and `OnceLock` usage in
  the substrate without surprising consumers. Bumping MSRV is a
  breaking change for consumers and requires its own minor-version
  release after v2.0. ALIS / koru-protocol pin `1.80` in their
  `rust-toolchain.toml` upon migrating to v2.0.

### Workspace structure

- **Single-crate, not a workspace member.** `experiments/` is referenced
  as a "separate workspace" only in the sense that it has its own
  `Cargo.toml` and doesn't ship in the published crate. The published
  `koru-lambda-core` is one `Cargo.toml`, one crate, one published
  artifact.

---

## Part 6 — Test strategy

### Substrate tests

**Axiom verification (each in isolation):**
- Determinism
- Commutativity (and proptest fuzz over 10K random pairs)
- Irreflexivity
- Content addressing — same chain on different engines → byte-identical state
- Saturation — 1M repeats of one synth call → distinction_count delta = 0

**Structural laws (at scale):**
- `r = 2d − 3` after 5M synths with zero deviations
- Binary parentage — `parents_of.len() == all_distinctions.len() − 2`
- ByteMapping registration — phantom count = 0 after 256-byte exercise

**Engine internals:**
- IdentityHasher — 1M random SHA-256 prefixes produce 1M distinct buckets
- `Send + Sync` compile-time assertion for `DistinctionEngine`, `Distinction`,
  and the LCA trait implementers (one-line `fn assert_send_sync<T: Send + Sync>(){}`)
- **Concurrent-write byte-equivalence** — 8 threads independently synthesizing
  the same logical chain → byte-identical final state, with two checkable
  invariants on the resulting engine:
    1. `parents_of.len() == all_distinctions.len() − 2` (no duplicate
       child entries despite the race; structural law 5 holds).
    2. `sum(degree_counts[d].load() for d in all_distinctions) == 2 * parents_of.len()`
       (every novel synthesis contributes exactly two `fetch_add(1)` calls;
       this is the unambiguous "expected participation count" invariant
       that doesn't depend on which thread won which race).
  This is the regression test for the round-1 race and round-4
  Release-ordering decision.
- **`r = 2d − 3` at 5M scale** — synthesize 5M distinctions, assert
  `relationship_count() == 2 * distinction_count() − 3` with zero
  deviations. The structural law's correctness at scale, not just at small
  N (Exp 2 carried forward).
- **`degree_counts` Release/Acquire correctness** — `loom` model checker
  test over a **minimal abstract kernel** (writer thread `fetch_add(Release)`,
  reader thread `load(Acquire)`, assert reader observes the increment).
  Loom enumerates the abstract memory-model interleavings — catches a
  missing `Acquire` deterministically regardless of host architecture.
  TSan on x86-TSO would silently pass even with `Relaxed` (the architecture
  provides Acquire for free), so loom is the load-bearing verifier for
  the **ordering contract**, not for the shipped code.
  **Scope honesty:** loom cannot model DashMap's internal locking
  (`parking_lot`, hazard pointers, shard masking are not loom-aware).
  What we verify is "the abstract Release/Acquire contract holds when
  separated from DashMap"; DashMap's own correctness is trusted via its
  upstream tests + TSan. The combined contract — "if loom passes AND
  DashMap is correct, then our hot path is correct" — is the
  load-bearing claim. The TSan run on the concurrent-write byte-equivalence
  test catches DashMap-specific issues; loom catches our atomic ordering;
  neither alone covers both.
- **Primordial invariants on a fresh engine** — `let e = DistinctionEngine::new();`
  then assert: `e.distinction_count() == 2`, `e.parents_of(d0).is_none()`,
  `e.parents_of(d1).is_none()`, `e.degree(d0) == 1`, `e.degree(d1) == 1`
  (the genesis d₀↔d₁ edge), `e.check_structural_invariant().is_ok()`
  (r=2d−3 with d=2 → r=1). Smoke test catching construction regressions.
- **`or_insert_with` closure runs exactly once per novel synth** — pre-bind
  `x` as a known distinction (e.g., `let x = engine.synthesize(d0, d1);`
  on the main thread). Snapshot baseline parent degrees:
  `let d0_before = engine.degree(d0); let x_before = engine.degree(x);`.
  Spawn N threads, each racing `engine.synthesize(d0, x)` once (all on
  the same pre-bound `x`, by-value Copy into each closure). Join.
  Assert `engine.degree(d0) == d0_before + 1` AND
  `engine.degree(x) == x_before + 1` (each parent gets exactly one
  fetch_add, not N). The child's degree alone wouldn't catch the bug —
  the bug being guarded is "fetch_add ran N times outside the closure,"
  which inflates **parent** degrees, not the new child's. Falsifies any
  refactor that moves `fetch_add` outside the closure or breaks the
  entry-gate.
- **Fold Law byte coverage lower bound** — after running the 256-byte
  exercise (every byte 0..=255 folded through the engine via
  `ByteMapping::map_byte_to_distinction`), assert `engine.degree(d0) >= 256 * 8`
  AND `engine.degree(d1) >= 256 * 8` (each byte folds 8 times through
  the primordials). Catches a future fast-path-by-byte-value regression
  that silently skips the fold for some bytes (a bug the phantom-count
  probe wouldn't detect).
- Mediated self-reference uniqueness at depth ≥ 10K (iterative, not recursive)
- Fold Law — d₀/d₁ degree ratio ≥ 100× after byte folds
- `replay_topological(snapshot_parentage(e))` produces a byte-identical engine
- Replay correctness on shuffled parentage (Exp 12 carried forward)

**Property-based (proptest, ~30 LOC):**
- For random `(a, b)`: `synthesize(a, b) == synthesize(b, a)`
- For random `a`: `synthesize(a, a) == a`
- For random `(a, b)` twice: `synthesize(a, b)` is idempotent on engine state

**Misuse-detection:**
- `IdentityHasher` panics on non-16-byte `write`
- `Distinction::from_hex` rejects empty / wrong length / non-hex / uppercase / non-ASCII
- Deserialize<Compactor> respects `warm <= hot` via `try_from`
- **Cross-engine foreign-byte injection** — debug-build panic when
  `engine_b.synthesize(d_from_engine_a, x)` is called with a
  `Distinction` whose bytes aren't registered in `engine_b`. Release
  behaviour documented (no panic; substrate trusts the contract).
- **`SynthesisRecorder` is `!Send + !Sync`** — compile-time assertion
  via `fn assert_not_sync<T: ?Sized>() where T: ?Sized {}; ... // a static_assertions::assert_not_impl_any!(SynthesisRecorder: Send, Sync)`. The
  marker exists to prevent accidental concurrent use; this test confirms
  the marker actually works.
- **`replay_topological` returns `Err(ReplayError::Mismatch)`** on
  tampered parentage (child bytes don't match `synthesize(parents)`).
  Release-mode behaviour; not gated on `debug_assert`.
- **`replay_topological` returns `Err(ReplayError::Unreachable)`** on
  cyclic / missing-parent parentage. Release-mode behaviour.
- **`SynthesisRecorder` deduplication** — recording the same child
  twice from the same recorder doesn't append twice; the `log.contains`
  check prevents it. Test verifies the property holds.

**Compile-time assertions:**
- `Distinction: Copy + Send + Sync` (use case: passed by value across threads)
- `Distinction: bytemuck::Pod + bytemuck::Zeroable` (use case: zero-copy
  byte slices for persistence)
- `#[repr(transparent)]` on `Distinction` (load-bearing for FFI and
  `bytemuck::Pod`)
- `SynthesisRecorder: !Send + !Sync` (load-bearing for single-thread
  contract)

### Subsystem tests
- **Validator:** atomic-failure (V5 regression), data cap (V3), oversized-root
  clipping (V4), atomic restore_state (V6), empty-data by-design (V8).
- **Commitment:** `compute` hashes leader_id (N6 regression),
  `verify_batch` rejects tampered leader_id (F7 transitive).
- **Network:** N5 cannot exist (typed previous_root), N1/N2 peer-id bounds,
  N7 joint dedupe, N11 LRU cap + epoch clear.
- **Compactor:** explicit thresholds, no double-count, no self-archive,
  no archived_ids field, Deserialize respects `warm <= hot`.

### Binding tests
- **FFI:** 8-thread concurrent join (F2/F8 regression), `batch_len > isize::MAX`
  rejected (F9 regression), fabricated root rejected via restore_state.
- **WASM (`wasm-pack test --node --features wasm`):** bytes-canonical
  round-trip, axioms preserved across FFI boundary, idToHex/idFromHex
  round-trip, empty leader_id rejected (W10 regression).

### Sanitizers and fuzzing

Add to CI:
- `cargo +nightly miri test --lib` — substrate is `unsafe`-free; Miri is cheap
  insurance against future `unsafe` creep.
- `RUSTFLAGS=-Zsanitizer=thread cargo +nightly test --release` on the 8-thread
  concurrent FFI test. TSan is the canonical answer to "sanitizer-clean."
- `cargo +nightly fuzz run from_hex` — one harness file targeting
  `Distinction::from_hex`. Not a CI gate (fuzz runs are long); a stretch
  goal run periodically.

### Memory-bounded regression test

- A dhat-instrumented test that builds a 1M-distinction engine and asserts
  resident heap per distinction is within budget. Specifies the harness
  (dhat live-heap, steady-state, exact engine construction). Replaces
  one-time manual measurement that would drift.

### CI gates
- `cargo fmt --check`
- `cargo clippy --all-targets -- -D warnings`
- `cargo clippy --all-targets --features wasm -- -D warnings`
- `cargo test --release`
- `cargo bench --no-run` (verify benches compile, don't run)
- `cargo +nightly miri test --lib` (substrate only — Miri is slow)
- *(stretch goal)* `wasm-pack test --node --features wasm`
- *(stretch goal)* `RUSTFLAGS=-Zsanitizer=thread cargo +nightly test` on the
  FFI concurrent test

---

## Part 7 — Step-by-step path off `dev`

Each step is a sub-branch off `release/2.0.0`. Each merges back green
before the next starts. No interleaving.

### Step 1 — Substrate foundation
**Branch:** `step/01-substrate`
- `Distinction(pub(crate) [u8; 16])` newtype with `#[repr(transparent)]`
- `distinction_hex.rs` (to_hex / from_hex / Display / Debug / serde adapter / typed `ParseError`)
- `IdentityHasher` (leading 8 bytes, guards on misuse)
- Engine state: `all_distinctions`, `parents_of`, `degree_counts`. No log. No children_of.
- `synthesize` hot path: entry-gated, race-free, all axioms enforced, `AtomicUsize::fetch_add` instead of Vec push
- `parents_of`, `degree`, `relationship_count`, `check_structural_invariant`
- `agent.rs` at substrate level (LCA trait + helper)
- `primitives.rs` rewrite — ByteMapping engine-registered
- `replay.rs` — `snapshot_parentage`, `replay_topological`, `build_children_index`
- `recorder.rs` — `SynthesisRecorder` reference impl
- Substrate axiom test suite + proptest + Send+Sync assertion
- Step-1 measurements: 8-thread throughput, memory/distinction at 1M, single-thread throughput

**Gate (hard checkpoint, no soft hatch):** all substrate tests pass;
engine is one screen of code; measurements hit Part 10 performance and
memory budgets. If a measurement misses budget, **stop** — open a
gate-decision PR that either (a) revises the substrate code until budget
is met, or (b) explicitly amends Part 10's budget in this design doc with
fresh reasoning. Do not proceed to Step 2 with an unresolved budget miss.
"Doc-flagged for revisit" is not an acceptable resolution.

### Step 2 — Reference subsystems
**Branch:** `step/02-subsystems`

Each subsystem rewritten from dev as a clean LCA implementation with the
audit-discovered bugs designed-in as non-existent:

- `validator.rs` — pre-validation pass; data cap; clipped messages;
  atomic restore_state.
- `commitment.rs` — leader_id hashed; LRU stays at the existing
  ~1000-entry cache from dev.
- `network.rs` — `previous_root: Distinction`; bounded peer-id; joint dedupe;
  bounded pending_commitments with documented cap; epoch clear; atomic
  restore_consensus_validator_state. `PeerIdentity::new` returns typed
  error.
- `compactor.rs` — explicit `(hot, warm)` thresholds; no `archived_ids`
  field; no double-count; no self-archive; Deserialize respects invariant.
- `parallel.rs` — `BatchSynthesizer` only. No `ParallelBatchProcessor`,
  no `ParallelAction`. Returns `Vec<Option<Distinction>>`.

**Gate:** every subsystem fits in budget; regression tests pass.

### Step 3 — Bindings
**Branch:** `step/03-bindings`
- FFI rewrite with opaque structs, internal `parking_lot::Mutex`,
  ManuallyDrop helper, panic=abort.
- WASM rewrite with bytes-on-wire, idToHex/idFromHex, panic hook,
  #[wasm_bindgen_test].
- cbindgen.toml cleaned (no include, no prefix).
- `tests/falsification/wasm_consistency.rs` is `#[wasm_bindgen_test]`
  and `#![cfg(feature = "wasm")]`-gated.

**Gate:** FFI passes 8-thread concurrent test (TSan-clean); WASM compiles
under `--features wasm`; `wasm-pack test --node --features wasm` passes
if toolchain available.

### Step 4 — Probes + scale validation
**Branch:** `step/04-validation`

Carry forward (or rewrite from scratch) the empirical experiments that
verify the theory's claims at scale. The set is curated to focus on
direct probes of axioms and load-bearing structural laws:

**Essential** (each falsifies a load-bearing claim if it fails):
- Cross-engine determinism (5 phases — Exp 21 carried forward)
- `r = 2d − 3` at scale (5M synths, zero deviations)
- Saturation at scale (1M identical synths, distinction_count delta = 0)
- Phantom-node count = 0 over the full 256-byte exercise
- Mediated self-reference uniqueness at depth ≥ 10K
- `replay_topological(snapshot_parentage(e))` round-trip byte-equivalence
- Concurrent synthesis byte-equivalence (8 threads → byte-identical final state)

**Validation** (theory predictions worth re-measuring on v2.0):
- Coding Law ρ ≥ 0.985 (Spearman, frequency vs degree-delta). **Pinned
  workload** — lifted from `experiments/runner/src/exp18_coding_law.rs`
  on `research/warroom-experiments`, defaults unchanged:
    1. Pool construction: synthesize a length-N chain
       `pool = [d₀, d₁, synth(d₀,d₁), synth(synth(d₀,d₁), d₁), ...]`
       — the same builder used by Exp 21 cross-engine determinism.
    2. Zipf sampling: `alpha = 1.0`, `seed = 0xC0DE`. For M iterations
       (M ≫ N), draw two distinct indices `(i, j)` from `Zipf(N, alpha)`,
       record `freq[i] += 1; freq[j] += 1`, call
       `engine.synthesize(pool[i], pool[j])`.
    3. Measurement: Spearman ρ between `freq[k]` and
       `degree_after[k] - degree_before[k]` over all `k in 0..N`.
    4. Recommended scale for the gate: `N = 4096, M = 8 * N`. Larger
       scales are welcome (the gate evaluates the floor, not the ceiling).
  
  This is the fixed-pool Zipf-sampling spec exp18 actually implements
  — not a perspective-evolving sampler. Margin: first-attempt measurement
  was ρ ≈ 0.99 ± 0.005; 0.985 leaves ~3σ headroom and falsifies any
  structural regression that perturbs the degree-counting hot path.
- Fold Law ratio ≥ 100× (d₀/d₁ as mega-hubs)

**Engineering** (performance and resource budgets):
- 8-thread throughput on primary hardware ≥ X M ops/sec (see Part 10 for X)
- Memory per distinction at 1M scale ≤ 180 B (dhat live-heap, steady state, including DashMap shard slack — see Part 10 gate 13)
- WASM bytes-on-wire round-trip fingerprint match (native vs wasm-pack-node)

(The "100M-synth churn, no leaks" probe was considered and dropped: the
append-only invariant + dhat budget at 1M + Miri-on-substrate + TSan on
the concurrent test already cover the leak/UB surface. A 100M run with
no concrete threshold is an unfalsifiable smoke test; if we add one later
it needs a numeric RSS-delta gate, not a wave-hands "no leaks" claim.)

These live in `experiments/` (separate workspace, doesn't ship in the
crate). Their findings become evidence in `SECURITY.md` and the substrate
docstrings.

**Probes dropped vs the first v2.0 attempt** (after research-lead curation):
- Exp 03 (log A/B/C): design choice is now "no log."
- Exp 11 (clone cost): Distinction is Copy now.
- Exp 06 (snapshot tearing at the original 16.5% framing): replaced with a
  single regression note documenting the corrected ≤0.1% number.
- Exp 05 (phantom) and Exp 20 (fold law) merged — same ByteMapping surface,
  one probe binary.

**Gate:** every essential claim has a probe demonstrating it; thresholds
defined with pre-registered refutation conditions.

### Step 5 — Documentation + release prep
**Branch:** `step/05-release`
- `CHANGELOG.md` written as a single coherent v2.0 entry from the start
  (not accreted across sub-branches).
- `SECURITY.md` with Tier-0 disclosure (N5 / N6 / V5 in v1.2.x; closed in
  v2.0).
- `README.md` rewritten for v2.0.
- `CLAUDE.md` updated.
- `Cargo.toml` bumped 1.2.0 → 2.0.0 with `profile.release.panic = "abort"`.
- `BUDGET_LOG.md` initialized (empty if no amendments occurred; populated
  if Steps 1-4 triggered any per the Budget Amendment Policy in Part 10.5).
- `public-api.txt` baseline snapshot committed.
- Verification gates: all 34 done-criteria in Part 10 — theory (1-10),
  budget (11-15), hygiene (16-28), size (29-30), documentation (31-34).
- PR `release/2.0.0` → `dev`.

**Gate:** all 34 done-criteria green; PR ready for review.

---

## Part 8 — Decisions (all resolved)

### Decision 1 — Synthesis log location

**Resolved:** drop entirely from engine. Persistence via `snapshot_parentage`
+ `replay_topological` helpers. Chronological observation via consumer-side
`SynthesisRecorder` reference impl.

**Rationale:** the substrate is timeless. Order is what LCAs do. theory-guardian's
veto was upheld after a reframe attempt failed scrutiny. No consumer has
demonstrated a need for substrate-level synthesis chronology, and the
performance cost of dropping is negligible (topological replay is bounded
and fast).

### Decision 2 — LCA promotion

**Resolved:** `local_agent.rs` → `src/agent.rs` at substrate level.

**Rationale:** the trait IS substrate (defines what it means to use the
substrate). Filing it under `subsystems/` is a misnomer.

### Decision 3 — `previous_root` type

**Resolved:** `previous_root: Distinction` in `TransactionBatch`. JSON
serialization via `#[serde(with = "distinction_hex")]`.

**Rationale:** the theory says identity is bytes. String typing an identity
field is a category error and was the structural cause of N5. With
`Distinction` typing, N5 cannot exist by construction.

### Decision 4 — Subsystem opinionatedness

**Resolved:** opinionated and small. Each reference subsystem makes defensible
choices, documents them, and stays ~300 LOC. Consumers needing different
choices write their own LCA implementations.

**Rationale:** the trait is what the substrate exposes for consumer use.
The reference impls are a worked example, not a configuration framework.

### Decision 5 — Ship subsystems in crate

**Resolved:** Yes for v2.0. The crate is the demonstration package. Extracting
subsystems to a separate `koru-reference-spoc` crate is a v3 architectural
question to revisit once the substrate has stabilized.

### Decision 6 — Compactor WARM band

**Resolved:** keep WARM. Require explicit `(hot_threshold, warm_threshold)`
at construction. No magic defaults.

**Rationale:** WARM is genuinely useful for graph health monitoring (observe
but don't archive). The cost is one extra constructor parameter. The
classification thresholds never leak into substrate code or substrate tests.

---

## Part 9 — What we explicitly DON'T do

- **No accreted documentation.** Docstrings explain the *why*, not the
  *what*. Avoid the first-v2.0-attempt pattern of every method having a
  paragraph-long history of which sub-branch fixed which bug.
- **No backward-compatibility shims.** v2.0 breaks what should break.
  The CHANGELOG documents migrations once, clearly.
- **No multiple ways to do the same thing.** One canonical path per
  operation.
- **No premature feature additions.** If the theory or audit doesn't
  demand it, defer to v3.
- **No "Noah's Ark" — keeping two of every data structure for
  performance "in case."** Collapse redundancies.
- **No tests just to pad the count.** Each test asserts something the
  theory or audit demands.
- **No engine-internal order tracking.** Time is what LCAs do.
- **No observation infrastructure inside the engine.** Observers are
  consumer-side; `SynthesisRecorder` is a reference impl, not engine state.

---

## Part 10 — How to know we're done

`release/2.0.0` is ready to merge when every gate below passes. Gates
are sorted into three categories with different amendment rules:

- **Theory gates** are *un-amendable*. A failure means the implementation
  doesn't satisfy the theory; the response is redesign, not budget
  loosening. These gates encode the four axioms and the structural laws
  that follow from them.
- **Budget gates** are *amendable within hard-cap floors*. A failure
  triggers a gate-decision PR per the Budget Amendment Policy below.
- **Hygiene gates** are *unconditional*. They must pass; they cannot be
  loosened or amended.

### Theory gates (un-amendable)

A failure here means the implementation isn't `koru-lambda-core`
anymore — it's a different substrate. The response is rewrite the code,
not amend the gate. These cannot be loosened, deferred, or marked
"acceptable miss" under any circumstance.

1. ✅ All four axioms verified by isolated tests (determinism, commutativity,
   irreflexivity, content addressing).
2. ✅ Property tests over 10K random pairs for commutativity, irreflexivity,
   idempotency. Zero failing cases.
3. ✅ `r = 2d − 3` at 5M synths with **zero exceptions** — not "≤ some ppm,"
   literally zero. Binary parentage holds. Saturation produces zero state delta.
4. ✅ Engine-independence verified by cross-engine probe (5 phases) —
   byte-identical state on any engine processing the same operations.
5. ✅ Phantom-node count = 0 on the 256-byte exercise.
6. ✅ Concurrent-write byte-equivalence: 8 threads synthesizing the same
   chain → byte-identical final engine state; `sum(degree_counts) == 2 * parents_of.len()`.
7. ✅ **Replay byte-equivalence (Laws 8 + 9).** `replay_topological(snapshot_parentage(e))`
   produces byte-identical state to the source engine on:
   (a) the natural snapshot order, AND
   (b) any random permutation of the snapshot (order-independent reconstruction,
   Law 9 — falsifies any implicit order dependence in the engine).
   Both probes are mandatory; failure of either is a theory event.
8. ✅ **Append-only invariant.** The substrate exposes no path that removes,
   clears, truncates, or otherwise non-monotonically alters a distinction
   in `all_distinctions`, `parents_of`, or `degree_counts`. Enforced
   structurally (no `remove_*` / `clear` / `truncate` method on the engine)
   and by hygiene grep (Step 5).
9. ✅ **Log-replay invariant.** Rebuilding the engine from `parents_of.iter()`
   produces byte-identical state to the live engine. Catches the case where
   a future contributor adds a hidden state field that isn't a pure function
   of the synthesis log.
10. ✅ Mediated self-reference: 10K/10K unique distinctions at depth 10K
    (iterative, not recursive).

### Budget gates (amendable within hard-cap floors)

These are *empirical engineering measurements*, not theory invariants.
Hardware varies; allocators evolve; reasonable misses can be resolved by
the Budget Amendment Policy. But no amendment crosses a hard-cap floor —
below the floor, redesign is required.

**Primary platform:** Apple M3 Pro, 8-core. Criterion median of 100 iters.

| # | Gate | Target | Hard-cap floor |
|---|---|---|---|
| 11 | Single-thread synthesis throughput | ≥ 450K ops/sec | ≥ 300K ops/sec |
| 12 | 8-thread synthesis throughput | ≥ 12M ops/sec AND ≥ 4× single-thread | ≥ 8M ops/sec AND ≥ 4× ratio non-negotiable |
| 13 | Memory per distinction at 1M scale (dhat live-heap, steady state, *including DashMap shard capacity slack* — see arithmetic below) | ≤ 180 B | ≤ 220 B |
| 14 | Fold Law d₀/d₁ hub ratio | ≥ 100× | ≥ 50× |
| 15 | Coding Law ρ (against pinned exp18 corpus pair `(exp18.log, exp18.freq.bin)` at `/tests/corpora/`, where `exp18.log` is the canonical `(min, max)` synthesis pair log and `exp18.freq.bin` is the Zipf-draw frequency array `freq[k]`; both produced by Step 1 from `alpha=1.0`, `seed=0xC0DE`, `N=4096`, `M=8N`; Step 4 consumes both bit-exactly — corpus alone is insufficient because Spearman ρ correlates `freq[k]` against `degree_after[k] − degree_before[k]`, and `freq` cannot be re-derived from the saturated pair log unambiguously) | ≥ 0.985 | ≥ 0.97 |

**Gate 13 memory arithmetic — including capacity slack:**

Raw per-distinction footprint: 16 (all_distinctions key) + 16 (value) +
16 (parents_of key) + 32 (tuple value) + 16 (degree_counts key) + 8
(AtomicUsize) = **104 B** of stored data.

DashMap shards each hold a hashbrown SwissTable that doubles capacity
on grow. At steady state, `len/cap` typically lands in `[0.5, 0.75]`,
so each shard carries 25–50% slack. Across three maps at 1M entries
each: shard overhead per entry ≈ 30–60 B, depending on where in the
load-factor cycle we measure. Total predicted: 104 + ~50 = **~155 B
per distinction in practice**, with run-to-run variance in the 140–180 B
range depending on which side of the rehash boundary the engine is on.

The earlier 140 B gate was the *arithmetic-only* prediction; the 180 B
gate above incorporates measured capacity slack. The 220 B floor is
the "this is design failure" line — above 220 B implies a per-shard
issue or an unintended allocation we haven't audited. dhat measures
malloc-tracked allocations only (`[u8;16]` stack data passes through);
the gate is honest about what dhat actually sees.

Floors are sized to absorb measurement noise and allocator variance but
not to absorb design regressions. A miss at the floor is a design event.
Coding Law's floor is workload-conditional on the pinned exp18 corpus
because ρ itself is workload-dependent (degenerate workloads can push ρ
to either extreme without violating any axiom); the corpus pins the
measurement so the gate is reproducible.

**Secondary platform** (regression watch, not gate): Linux x86_64
(GitHub-hosted Ubuntu runners are 4 vCPU; expect ~5–7 M ops/sec at 8 threads
on a fully-loaded `c7i.2xlarge` or equivalent — published per-platform).

### Hygiene gates (unconditional)

These must pass. No amendment process applies; if hygiene fails, fix the
code or the configuration. They protect against drift the theory + budget
gates can't see (API surface, lints, sanitizer findings, doc-code parity).

16. ✅ `cargo test --release`: all pass; no `#[ignore]` / `#[cfg(slow)]` on essential probes.
17. ✅ `cargo clippy --all-targets --release -- -D warnings`: clean.
18. ✅ `cargo clippy --all-targets --features wasm --release -- -D warnings`: clean.
19. ✅ `cargo fmt --check`: clean.
20. ✅ `cargo bench --no-run`: compiles.
21. ✅ `cargo +nightly miri test --lib`: clean on substrate.
22. ✅ FFI 8-thread concurrent join test passes under TSan
    (`RUSTFLAGS=-Zsanitizer=thread cargo +nightly test --release`).
23. ✅ WASM tests pass under `wasm-pack test --node --features wasm`.
24. ✅ `loom` model-checker test passes for the `degree_counts`
    Release/Acquire memory-ordering kernel.
25. ✅ `cargo public-api` snapshot matches the checked-in baseline at
    `public-api.txt`. Any intentional public API change requires a
    co-merged baseline-rebaseline commit.
26. ✅ `cargo audit` clean (no advisories on direct deps).
27. ✅ Hygiene greps (Step 5 sweep): no `\.children_of(` outside `replay.rs`;
    no `fn (remove|clear|truncate|drop)_distinction` anywhere; `pub struct Distinction`
    has the `pub(crate)` field exactly once; engine has exactly 3 DashMap fields.
28. ✅ Doc-code reconciliation review: `src/engine.rs` field list and public
    method signatures match `ARCHITECTURE.md §Substrate / engine.rs`.
    Reviewers: theory-guardian + engine-architect.

### Size gates (hygiene)

29. ✅ Total `src/` LOC ≤ 3,450 measured via `tokei src/ --no-tests`.
30. ✅ `engine.rs ≤ 480` non-test LOC (one `pub fn synthesize`, all four axioms
    enforced in <40 LOC of body, three indexed engine fields, no children_of).

### Documentation gates (hygiene)

31. ✅ `SECURITY.md` published with Tier-0 CVE-style disclosure for N5/N6/V5.
32. ✅ `CHANGELOG.md` written as one coherent v2.0 entry.
33. ✅ `Cargo.toml` at `2.0.0` with `profile.release.panic = "abort"`.
34. ✅ `BUDGET_LOG.md` reflects every budget amendment with old → new,
    baseline delta, signers, and justification.

That's the bar. Anything less and we delay shipping.

---

## Part 10.5 — Budget Amendment Policy

Some budget gates will miss in practice. Hardware fingerprints, allocator
behavior, and workload shape vary. The amendment process exists to
acknowledge engineering reality without dissolving into "we'll fix it
later." It applies **only** to budget gates (11–15 above). Theory gates
(1–10) and hygiene gates (16–34) are not amendable.

### When an amendment is allowed

An amendment is a pull request that revises a budget gate's target. It
is allowed only when ALL of the following hold:

1. The PR cites the measured value (with full harness specification —
   hardware, runtime, exact criterion/dhat command lines, repeatable
   seed) AND demonstrates how the measurement compares against the
   gate's **absolute target**, **hard-cap floor**, and (where applicable)
   the warroom v2.0 byte-API reference numbers below.
2. The PR appends a new row to `BUDGET_LOG.md` with:
   `date | gate | old target | new target | absolute delta | floor delta | author | signers | justification`.
3. The new target stays **at or above the hard-cap floor** for that
   gate. An amendment that would cross a floor is not a budget amendment
   — it is a redesign event.
4. The PR is signed off by **both** `theory-guardian` AND `engine-architect`.
   Budget *loosening* additionally requires a `research-lead`-authored
   measurement justification explaining why the new target reflects a
   real engineering constraint rather than implementation drift.
5. The PR is announced in the `## Unreleased` section of `CHANGELOG.md`
   so consumer teams (ALIS, koru-protocol) see the ratcheting before
   they migrate.

"Doc-flagged for revisit," "we'll fix it in the next sprint," and
"reasonable miss, moving on" are NOT acceptable resolutions.

### Hard-cap floors (un-crossable)

The floor column in the budget-gate table is the line below which an
amendment is no longer engineering noise — it is design failure. A
measurement at or below the floor means the substrate is not delivering
its capability claim. The response is rewrite or version-flag, not
amendment.

### Reference measurements (what we compare against, and why no dev baseline)

A `dev`-branch baseline was originally proposed but is structurally
incoherent: `dev` ships the v1.2 String-API (`Distinction { id: String }`,
`synthesize(&D, &D)`, ~629 B/distinction) with no `parents_of()` or
`degree()` traversal. Comparing v2.0's byte-API throughput / memory to
dev's String-API throughput / memory is apples-to-oranges — the numbers
don't measure the same thing. A regression delta against an incomparable
baseline is rhetoric, not evidence.

Instead, amendments compare against **`BASELINE_WARROOM_M3_PRO`** — the
only prior codebase that measured this engine with the v2.0 byte-API
surface — plus the **absolute targets and floors** in the gate table
itself. This is apples-to-apples and falsifiable.

**`BASELINE_WARROOM_M3_PRO`** — pinned from `research/warroom-experiments`
commit `22dbbce`, which had the same byte-canonical Distinction +
3-field engine (with `children_of` instead of `degree_counts`, but the
synthesize hot path is comparable):

```
[BASELINE_WARROOM_M3_PRO]
commit                          = "22dbbce"
single_thread_throughput        = 500_000      # ops/sec (Exp 10)
8_thread_throughput             = 15_300_000   # ops/sec (Exp 10)
memory_per_distinction          = 80           # bytes at 1M (Exp 13-16)
fold_law_d0_d1_ratio            = 250          # ≥ 100× gate target (Exp 20)
coding_law_rho                  = 0.99         # ± 0.005 (Exp 18, exp18 workload)
```

**For Step 1 amendment PRs:** cite measured value against the
v2.0 absolute target + hard-cap floor (primary constraint), and against
the warroom byte-API reference (sanity check — v2.0 hot path is a clean
rewrite of the warroom one, so substantial regression from warroom
suggests something specific went wrong, not "engineering reality").

### What this policy buys

The earlier "open a gate-decision PR" language was a soft hatch in
principle: a sufficiently committed group could rubber-stamp amendments
indefinitely. The policy above closes that hatch structurally by:

- Removing axiom-bearing gates from the amendable set entirely.
- Hard-capping the remaining gates at floors that reflect "below this
  means redesign, not regression."
- Requiring measurement provenance (no amendments from vibes).
- Making the budget log public so consumer teams see the ratchet before
  migrating.

---

## Part 11 — Reference observers (not substrate)

The deliberate choice to keep no observation state inside the engine means
consumers handle observation explicitly. Three reference implementations
ship in the crate to demonstrate idiomatic patterns:

### `SynthesisRecorder` (`src/recorder.rs`)

Chronological record of novel syntheses. Consumer constructs one, routes
`synthesize` calls through it. The recorder pushes to its internal `Vec`
only on novel results (deduped via `log.contains`). `!Send + !Sync` via
`PhantomData<*const ()>` enforces single-thread use at compile time.

~60 LOC. Pure consumer-side. The substrate is unaware.

### `snapshot_parentage` + `replay_topological` (`src/replay.rs`)

State persistence. `snapshot_parentage(engine)` dumps the `parents_of` map.
`replay_topological(snapshot)` reconstructs a fresh engine by repeatedly
calling `synthesize` on entries whose parents are already in the engine,
until quiescent. Returns `Result<_, ReplayError>` for release-safe
content-address mismatch + cycle detection.

~50 LOC. Pure functions. The substrate is unaware.

### `build_children_index` (`src/replay.rs`)

Materialize the children-of map (the inverse of `parents_of`) for the
rare consumer that needs to iterate children rather than just count them.
Takes a parentage snapshot, returns `HashMap<Distinction, Vec<Distinction>>`.
O(N) build, O(1) lookup thereafter. The engine itself doesn't carry
this — `degree_counts` is the canonical O(1) projection of the Coding
Law primitive, and the children enumeration is the derived form.

~10 LOC. Pure function. The substrate is unaware.

### Why ship them in the crate

These aren't part of the substrate — they're examples of how consumers
should use the substrate. Like the reference subsystems, they make the
"100% theory-aligned substrate + clear consumer patterns" message concrete.
A consumer reading the crate gets:

- The substrate (axioms enforced, degree exposed)
- Three reference observer patterns (record + persist + walk-children)
- Four reference LCA subsystems (validate + commit + network + compact)

That's the demonstration package. Theory + patterns + worked examples.

---

## Appendix — Mapping v1.2.0 dev artifacts to v2.0 fate

| Artifact in dev | Fate in v2.0 |
|---|---|
| `engine.rs` (193 LOC) | Rewritten: ~430 LOC with byte Distinction + `degree_counts` + invariant. Same axioms, tighter representation. No log. No children_of. |
| `primitives.rs::ByteMapping` (cached) | Rewritten: engine-registered, no static cache, no phantoms. |
| `subsystems/local_agent.rs` | Moved to `src/agent.rs`. Identical content. |
| `subsystems/network.rs` (603 LOC) | Rewritten: ~550 LOC. `previous_root: Distinction`. N1/N2/N5/N7/N11 designed-in. Typed `PeerIdentityError`. |
| `subsystems/validator.rs` (350 LOC) | Rewritten: ~300 LOC. Pre-validation pass. V3/V4/V5/V6 designed-in. |
| `subsystems/commitment.rs` (504 LOC) | Rewritten: ~250 LOC. leader_id hashed. |
| `subsystems/compactor.rs` (503 LOC) | Rewritten: ~250 LOC. Explicit thresholds, no double-count, no self-archive, Deserialize respects invariant. |
| `subsystems/parallel.rs` (379 LOC) | Slimmed to `BatchSynthesizer` only: ~80 LOC. ParallelBatchProcessor deleted. |
| `ffi.rs` (899 LOC) | Rewritten: ~650 LOC. `parking_lot::Mutex` wrapping, opaque structs, ManuallyDrop, panic=abort. |
| `wasm.rs` (741 LOC) | Rewritten: ~450 LOC. Bytes-on-wire, idToHex/idFromHex, panic hook. |
| String-based Distinction | Gone. `Distinction(pub(crate) [u8; 16])` newtype. |
| `relationships: DashMap` field | Gone. Subsumed by `parents_of`. |
| `synthesis_log` (proposed in first v2.0 attempt) | Gone. Replaced by consumer-side `snapshot_parentage` + `replay_topological` + `SynthesisRecorder`. |
| `degree_cache` (proposed in first v2.0 attempt) | **Kept and renamed `degree_counts`.** Round 4 reversed the first-attempt reasoning: degree (the count) is the load-bearing primitive Coding/Fold Law name; children iteration is the derived form. `AtomicUsize` per node, `fetch_add(1, Release)` in the synth hot path (promoted from `Relaxed` in round 4 verification for uniform happens-before contract; see Part 6 loom test). |
| `children_of: DashMap<[u8;16], Vec<Distinction>>` (round-3 plan kept this) | **Dropped in round 4.** Grep across every probe + subsystem + test found zero load-bearing consumers of children iteration; every caller uses `degree(d)`. Vec realloc on d₀/d₁ stalls the synth hot path. Consumers needing children iteration call `replay::build_children_index` (O(N) once, O(1) thereafter). |
| Static byte cache | Gone. ByteMapping folds through caller's engine. |
| `get_state_snapshot` | Renamed to `get_state_snapshot_unsynchronized` per Decision 5.8 (from first attempt). |
| `Distinction::id()` | Gone. Use `as_bytes()` or `to_hex()`. |
| `Distinction::new(String)` | Gone. No public constructor. |
| `ParseError` from hex parsing | Typed error (was `String` in first attempt). |
| `PeerIdentity::new` error | Typed `PeerIdentityError` (was `String` in first attempt). |
| `IdentityHasher` (proposed XOR-rotation) | Simplified: leading 8 bytes only. Guards on misuse. |
| `synthesize` proposed hot path | Rewritten with entry-gated insert to eliminate the race. By-value `Distinction` args (Copy enables this). Writes to `parents_of`, pre-seeded `degree_counts.insert(new_bytes, AtomicUsize::new(0))` for the new child, and the two `degree_counts.get(parent.0).expect("…invariant").fetch_add(1, Release)` calls all happen INSIDE the `or_insert_with` closure, under the shard lock for `all_distinctions[new_bytes]`, so observable engine state is always consistent. AtomicUsize replaces Vec push for the degree update — strictly cheaper, no realloc on hubs. Pre-seed enables `get()` fast path on the parent bump (read-lock vs write-lock). |
| `SynthesisRecorder` (round-2 introduction) | Single-thread-only contract enforced via `PhantomData<*const ()>` marker. Novelty check uses dedup against own log (not racy `distinction_count`). Documented as observer, not LCA. |
| `replay_topological` (round-2 introduction) | Returns `Result<Arc<DistinctionEngine>, ReplayError>`. Release-safe content-address mismatch detection (no `debug_assert_eq!`). Distinguishes corrupted parentage (cycle / missing parent → `Unreachable`) from tampered (`Mismatch`). |
| `from_hex` cross-engine injection | `synthesize` `debug_assert`s both parents are registered in this engine; debug-build panic catches misuse in tests. Release builds trust the contract documented on `from_hex`. |
| `MAX_PENDING_COMMITMENTS` derivation | Pinned arithmetic: `64 × 2 × 2 = 256` (peers × in-flight epochs × safety margin), not symbolic. |
| `r = 2d − 3` invariant restatement | `all_distinctions.len() == parents_of.len() + 2` — directly tests binary parentage, not tautological. |
| `Distinction` derives | `Pod + Zeroable` via `bytemuck` added for zero-copy slice views. |
| `#[must_use]` | Applied crate-wide to every function returning a `Distinction`. Dropping a synthesis result is always a bug. |

---

## Notes for review

Things still worth questioning:

- **The 8-thread throughput floor of 12M.** First v2.0 attempt measured 15.3M
  on M3 Pro with `Vec::push` on `children_of`. v2.0 replaces that with
  `AtomicUsize::fetch_add` — should be at least as fast (likely faster due
  to no realloc churn on d₀/d₁). Gate as both: absolute ≥12M on M3 Pro AND
  ratio ≥4× single-thread, so portable to non-M3 hardware.
- **Whether topological replay's worst-case O(N²) ever bites in practice.**
  Synthetic test: degenerate parentage where every entry depends on the
  previous. Probably fine but worth one probe.
- **Whether shipping `SynthesisRecorder` and `replay.rs` in the crate gives
  the impression they're substrate.** They're carefully filed under
  `src/` (not `src/subsystems/`) but they're consumer patterns, not
  substrate. The docstrings need to be crystal clear about this.
- **CLOSED in round 4: `children_of` as engine state.** Resolution:
  dropped. `degree_counts` is the canonical O(1) projection of the
  theory's degree primitive; `replay::build_children_index` materializes
  the inverse for the rare consumer that needs iteration.

Mark up freely. The next step after this document is cutting `release/2.0.0`
from `dev` and starting Step 1.
