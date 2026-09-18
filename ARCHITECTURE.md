# Architecture

`koru-lambda-core` is the implementation of distinction theory (see
`THEORY.md`) plus the reference consumer patterns and language bindings
needed to actually use it. This document describes the v2.0 architecture:
what's in each module, why it's there, and how the layers compose.

For *how the codebase got to this shape* — the design decisions, review
rounds, and step-by-step plan — see `DESIGN.md`.

---

## Three layers

The crate divides into three layers, each independent of the next:

```
┌─────────────────────────────────────────────────┐
│  Bindings (FFI + WASM)                          │
│  Exposes the substrate to C and JavaScript.     │
├─────────────────────────────────────────────────┤
│  Reference subsystems (LCA implementations)     │
│  Worked examples of the LCA pattern: a          │
│  validator, network, commitment, compactor.     │
│  These are not the substrate — they USE it.     │
├─────────────────────────────────────────────────┤
│  Substrate                                      │
│  The engine, the synthesize operator, the LCA   │
│  trait, and consumer helpers (replay, recorder, │
│  hex display). This is what the crate's name    │
│  promises.                                      │
└─────────────────────────────────────────────────┘
```

The substrate is sacrosanct. It enforces the four axioms and exposes the
minimum API consumers need. Subsystems are demonstrations; bindings are
boundary layers. Neither can introduce theory violations into the
substrate.

---

## Substrate

```
src/
  lib.rs              public re-exports
  engine.rs           Distinction, IdentityHasher, DistinctionEngine,
                      RawDistinctionId + verify, SynthesisOutcome
  projection.rs       projection primitive (Root, Direction, Signal,
                      Materialized) and its wire codec
  agent.rs            LocalCausalAgent trait + helper
  primitives.rs       Canonicalizable trait + ByteMapping
  distinction_hex.rs  to_hex / from_hex / Display / Debug / serde
  replay.rs           snapshot_parentage / replay_topological /
                      build_children_index
  recorder.rs         SynthesisRecorder reference observer
```

This is the heart of the crate.

### `engine.rs` — the engine itself

The `Distinction` newtype:

```rust
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash,
         bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct Distinction(pub(crate) [u8; 16]);
```

Sixteen bytes. `Copy`. `repr(transparent)` for zero-copy FFI/WASM. The
field is `pub(crate)` — there is no public constructor. The only way to
obtain a `Distinction` is via the engine itself (`synthesize`,
`get_distinction_by_id`) or by parsing one with `from_hex` (whose output
is debug-asserted at the next `synthesize` to belong to the engine
receiving it). Foreign-byte injection is closed structurally.

`DistinctionEngine` carries one indexed `nodes` map. Each value is an
`EngineNode` carrying both parents and degree per distinction. The
three canonical O(1) projections (saturation check, parent lookup,
degree query) live as per-node fields:

```rust
struct EngineNode {
    parents: Option<(Distinction, Distinction)>,  // None for primordials
    degree:  AtomicUsize,                          // novel-synth participations
}

pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    nodes: DashMap<[u8;16], EngineNode, IdentityBuildHasher>,
}
```

| Projection | How it's served | What it serves |
|---|---|---|
| Saturation check | `nodes.contains_key(id)` | Axiom-equivalent: repeats add nothing. O(1) existence check in the synthesize hot path. |
| Parent lookup | `nodes.get(id).parents` | Binary parentage (Law 5). Canonical child→parents projection used by replay, invariant checks, and parent walks. |
| Degree query | `nodes.get(id).degree.load(Acquire)` + addend | Coding Law (Law 12) and Fold Law (Law 11) — both stated as degree properties. The theory's central observability claim. |

Identity-IS-process: each distinction is one `EngineNode`. The
`children_of` enumeration is *not* in the engine — `node.degree`
carries the count the theory names; consumers that need to iterate
children call `replay::build_children_index` (O(N) once, O(1)
thereafter).

`IdentityHasher` is the engine's internal `DashMap` hasher. SHA-256
prefixes are uniformly distributed; the hasher returns the leading 8
bytes as a `u64` with no XOR, rotation, or diffusion. Misuse is caught
by `debug_assert! + unreachable!()` on every method that isn't a 16-byte
`write`.

The synthesize hot path enforces all four axioms inline (≈ 50 LOC of
body, mostly happens-before contract bookkeeping):

```rust
#[must_use]
pub fn synthesize(&self, a: Distinction, b: Distinction) -> Distinction {
    // foreign-byte guard (debug_assert on nodes.contains_key)
    if a == b { return a; }                              // Axiom 3
    let (first, second) = if a.0 <= b.0 { (a, b) } else { (b, a) };  // Axiom 2
    let mut h = Sha256::new(); h.update(first.0); h.update(second.0);
    let mut new_bytes = [0u8; 16];
    new_bytes.copy_from_slice(&h.finalize()[..16]);     // Axioms 1, 4
    if self.nodes.contains_key(&new_bytes) {
        return Distinction(new_bytes);                   // Saturation
    }
    let new_d = Distinction(new_bytes);
    // Vacant/Occupied match captures whether THIS thread won the
    // novel insert. The Entry's shard write-lock drops at end of
    // block, BEFORE the parent fetch_adds — B1 deadlock mitigation
    // for cases where a parent shares a shard with new_bytes.
    let inserted_new = {
        match self.nodes.entry(new_bytes) {
            dashmap::Entry::Vacant(slot) => {
                slot.insert(EngineNode {
                    parents: Some((first, second)),
                    degree: AtomicUsize::new(0),
                });
                true
            }
            dashmap::Entry::Occupied(_) => false,
        }
    };
    if inserted_new {
        self.nodes.get(&first.0)
            .expect("first parent registered at its insertion (invariant)")
            .degree.fetch_add(1, Ordering::Release);
        self.nodes.get(&second.0)
            .expect("second parent registered at its insertion (invariant)")
            .degree.fetch_add(1, Ordering::Release);
    }
    new_d
}
```

The fast-path `nodes.contains_key(&new_bytes)` return is **saturation**
(Law 7). Repeats short-circuit before any shard write-lock is taken —
degree is never bumped for an already-existing synthesis.

**Happens-before contract.** Documented in full on `synthesize`'s
rustdoc — two guarantees: (1) new-child observation synchronizes-with
parents (entry shard lock); (2) parent `degree` bumps are eventually
consistent (Release/Acquire pair; B1 mitigation drops the lock before
the bumps). Consumers driving sequential LCAs never see the in-flight
window; quiescent reads are always consistent.

**Memory ordering contract.** `fetch_add(1, Release)` on parent degree
pairs with `Ordering::Acquire` loads in `pub fn degree` so probes
reading `node.degree` directly (without first observing the new child)
still get a happens-before edge to the writing synthesis. The
Release/Acquire kernel has a loom test at `tests/loom_kernel.rs` —
loom verifies the abstract memory-model interleavings; TSan on the
concurrent-write byte-equivalence test verifies the DashMap-shard
side. Neither alone covers both; both are required.

**Law 8 quiescence qualifier.** Two engines processing the same
operations produce byte-identical state
[at quiescence](THEORY.md#law-8-engine-independence); mid-flight
transient divergence is permitted while writes are in progress. The
claim is post-processing convergence, not instantaneous equality.
LCAs drive synthesis sequentially per LCA, so the relaxation is
invisible to the documented consumer contract; probes reading
`node.degree` mid-flight are responsible for their own quiescence
boundary (post-join barrier, epoch boundary, etc.).

**Why `AtomicUsize::fetch_add` not `Vec::push`.** Earlier design rounds
kept a `children_of: DashMap<[u8;16], Vec<Distinction>>` for
enumerating a distinction's children. d₀ and d₁ accumulate millions of
children via the Fold Law; a `Vec<Distinction>` reallocates O(log N)
times under the shard write-lock and stalls every other thread trying
to synthesize against d₀ or d₁. `AtomicUsize::fetch_add` is
constant-cost, lock-free, and the count itself is what
[Coding Law](THEORY.md#law-12-coding-law) actually names. Consumers
wanting children iteration call `build_children_index` on a snapshot.

**Hasher misuse guard.** In addition to the `debug_assert!` on 16-byte
keys, `IdentityHasher` implements `unreachable!()` on every non-`write`
`Hasher` method (`write_u8`, `write_u16`, …, `write_length_prefix`).
Misuse — hashing a slice, a non-16-byte key, or a typed integer —
triggers an immediate panic instead of silently corrupting state.
v2.0.0 has no tuple keys, so the hasher only ever sees single 16-byte
writes. `pub type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>`
is what the `DashMap` field type parameterizes on.

The `expect("…invariant")` panic messages encode the proof obligation
in source — a contributor who breaks the pre-seed invariant gets a
breadcrumb to the right line instead of a bare `unwrapped None`.

#### Public engine API

| Method | Purpose | Complexity |
|---|---|---|
| `new()` | Construct engine with d₀, d₁ pre-seeded as `EngineNode { parents: None, degree: 0 }` in `nodes` | O(1) |
| `d0()`, `d1()` | Borrow primordials (by-value Copy) | O(1) |
| `synthesize(a, b)` | The hot path — enforces all four axioms inline | O(1) amortized |
| `parents_of(d)` | Lookup canonical `(min, max)` parent pair via `nodes.get(d).parents` | O(1) |
| `degree(d)` | Per-distinction degree via `nodes.get(d).degree.load(Acquire)` + genesis/parent-edges addend (returns 0 for unregistered) | O(1) |
| `has(d)` | Test whether `d` is registered (`nodes.contains_key`) | O(1) |
| `distinction_count()` | `nodes.len()` | O(1) |
| `relationship_count()` | `(nodes.len() - 2) * 2 + 1` (primordials always present) | O(1) |
| `snapshot_parentage()` | Materialize all non-primordial `(child, parent_pair)` tuples for replay | O(N) |
| `snapshot_distinctions()` | Materialize all distinctions as a `Vec<Distinction>` for traversal probes (Coding Law, Fold Law dominance, diagnostic dumps) | O(N) snapshot, then iter is local |
| `check_structural_invariant()` | Quiescent-mode assertion that `nodes.len() == (count of nodes with parents) + 2` (equivalent to `r = 2d − 3`); returns `Result<(), InvariantError>` | O(N) (iterates nodes to count parented entries; not on the hot path) |

`InvariantError` is a public `#[non_exhaustive]` error enum with one
variant `BinaryParentageMismatch { all_distinctions, parents_of_plus_two }`
carrying both counts for programmatic inspection and display. The
field names are preserved from the pre-merge three-map design for
public-API stability; their semantic mapping in the merged-map layout
is documented on the variant itself.

### `agent.rs` — the LCA trait

```rust
pub trait LocalCausalAgent {
    type ActionData: Canonicalizable;
    fn get_current_root(&self) -> Distinction;  // by value — Distinction is Copy
    fn synthesize_action(&mut self, action: Self::ActionData,
                         engine: &Arc<DistinctionEngine>) -> Distinction { /* default impl */ }
    fn update_local_root(&mut self, new_root: Distinction);
}

pub fn synthesize_causal_action<A: Canonicalizable>(
    local_root: Distinction, action: A, engine: &Arc<DistinctionEngine>,
) -> Distinction { /* ... */ }
```

The trait IS the substrate's reference consumer contract — not the only
legal pattern. The four axioms constrain `synthesize`, not consumer
shape; multi-perspective agents and non-root-anchored consumers can use
the substrate directly. The LCA pattern is canonical because (a) it's
how every consumer we've built (ALIS, koru-protocol, the reference
subsystems) uses the substrate; (b) it captures "time is what consumers
do" cleanly. The default `synthesize_action` implementation calls
`synthesize_causal_action` then `update_local_root` — implementers
override only when they need finer control (e.g., batching actions
before advancing root).

### `primitives.rs` — Canonicalizable and ByteMapping

The `Canonicalizable` trait — "how do you turn this type into the
canonical chain of distinctions that represents it?" — is the contract
LCAs use to lift their action data into the substrate.

`ByteMapping` folds a single byte through the calling engine. There is
no static cache: every byte folded gets every intermediate distinction
registered in the engine that folded it. The phantom-node problem is
closed by construction.

### `distinction_hex.rs` — the hex display boundary

`to_hex`, `from_hex`, `Display`, `Debug`, and a `serde` adapter live
here. Hex is not theory. It is the human/wire-format rendering of bytes.
The substrate is byte-canonical; hex is a boundary concern.

### `replay.rs` and `recorder.rs` — consumer-side observers

These are reference patterns, not substrate. The engine has no
observation infrastructure and no order-bearing state. Consumers wanting
chronology use `SynthesisRecorder`. Consumers wanting persistence use
`snapshot_parentage` + `replay_topological`. Consumers wanting children
iteration use `build_children_index`.

`SynthesisRecorder` is `!Send + !Sync` via `PhantomData<*const ()>` —
the marker enforces single-thread use at compile time. `replay_topological`
returns `Result<_, ReplayError>` with three variants (`Unreachable`,
`Mismatch`, `MissingPrimordial`) so callers handle corrupted-parentage
cases in release builds.

---

## Reference subsystems

```
src/subsystems/
  mod.rs              ~20 LOC   declarations + re-exports
  validator.rs        ~300 LOC  ConsensusValidator
  commitment.rs       ~250 LOC  CommitmentAgent + BatchCommitment
```

Each subsystem is a worked example of the LCA pattern with real
consensus-protocol semantics. They demonstrate how a productive consumer
uses the substrate. They are **not** the substrate; if `koru-protocol`
wants different semantics it forks them.

Bug-correct from the start:

- **Validator** pre-validates batches before any `synthesize` call —
  engine state is invariant on rejection (closes V5 by construction).
- **Commitment** `compute` hashes `leader_id` (closes N6 by construction).
  `TransactionBatch::previous_root: Distinction`, not `String` (closes
  N5 by construction; no truncation possible).

Earlier design rounds enumerated additional subsystems (`network.rs`,
`compactor.rs`, `parallel.rs`) as shipping in this release. Those
files were removed before v2.0.0 tagged; if they return, they will
land as separate reference LCAs alongside `validator` and `commitment`.

---

## Bindings

```
src/
  wasm.rs             ~680 LOC  WASM bindings (feature-gated)
```

### FFI

Post-v2.0.0 work; **not shipped in the current release.** The
`cdylib` / `staticlib` crate-types remain declared in `Cargo.toml`
so a future C-ABI binding release can drop into place without another
manifest change, but there is no `src/ffi.rs`, no opaque handle set,
and no C header in v2.0.0. The design sketch below documents what a
future FFI binding is expected to look like when the surface is
built; nothing here is a shipping commitment.

- Opaque types: `#[repr(C)] pub struct KoruEngine { _private: [u8; 0] }`
  for `KoruEngine`, `KoruAgent`, `KoruValidator`. The C header gets
  distinct typedefs; callers can't pass one through as another.
- Handle wrapping: `Box<parking_lot::Mutex<...>>`. `parking_lot::Mutex`
  is ~5× faster than `std::sync::Mutex` uncontended and lacks
  poisoning semantics that don't apply to FFI.
- Engine borrows: `ManuallyDrop<Arc<DistinctionEngine>>` via one
  `borrow_engine` helper. No `Arc::from_raw` + `into_raw` re-leak dance.
- `panic = "abort"` on the release profile. No unwinding across `extern "C"`.
- Length guards on slice inputs (`batch_len > isize::MAX` rejected).

### WASM

`src/wasm.rs` is gated behind `#[cfg(feature = "wasm")]` and exposes
`DistinctionEngine`, the projection primitive, and `verify` to JS/TS.
The current ship-set of signals is `Adjacency`, `Degree`, and
`HopDistance`.

- Bytes-canonical end to end. Every distinction ID crossing the JS
  boundary is `Uint8Array` of length 16.
- `WasmEngine::synthesize(&[u8], &[u8]) -> Result<Vec<u8>, JsValue>`,
  length-validated.
- `idToHex(arr) -> string`, `idFromHex(s) -> Uint8Array` as freestanding
  helpers for display boundaries.
- `#[wasm_bindgen(start)] fn _wasm_start()` wires up
  `console_error_panic_hook` under the `wasm` feature.
- Tests are `#[wasm_bindgen_test]` and run via `wasm-pack test --node --features wasm`.

---

## How the layers compose

```
            ┌─────────────────────────────────────┐
            │  Consumer crate (ALIS, koru-protocol)│
            └────────────────┬────────────────────┘
                             │ depends on
                             ▼
            ┌─────────────────────────────────────┐
            │  koru-lambda-core public API         │
            │                                     │
            │   Distinction, DistinctionEngine,   │
            │   LocalCausalAgent, synthesize_*,   │
            │   ConsensusValidator, NetworkAgent, │
            │   CommitmentAgent, StructuralCompactor│
            │   (Reference impls — fork or replace │
            │    if your consensus differs.)      │
            │                                     │
            │   FFI (opaque KoruEngine, etc.)     │
            │   WASM (WasmEngine, idToHex, ...)   │
            └─────────────────────────────────────┘
```

A consumer crate depends on `koru-lambda-core`. It uses the substrate
directly (creating a `DistinctionEngine`, implementing
`LocalCausalAgent` for its own state), or it uses the reference
subsystems (extending `ConsensusValidator` or replacing it). For C or
JavaScript consumers, the FFI and WASM layers expose the same substrate
behind opaque or `Uint8Array`-flavored APIs.

The substrate doesn't know what domain it serves. It knows about
parents.

---

## Build and test

```bash
cargo test --release                       # full test suite
cargo clippy --all-targets --release       # bare lint
cargo clippy --all-targets --features wasm --release  # wasm lint
cargo build --release                      # rlib + cdylib + staticlib
cargo +nightly miri test --lib             # UB check on substrate
wasm-pack test --node --features wasm      # WASM tests
```

CI gates run all of the above plus `cargo fmt --check` and (stretch)
TSan on the 8-thread FFI concurrent test.

---

## What this architecture is not

- Not a configuration framework. Subsystems are opinionated; parameters
  that vary between deployments are the consumer's choice, not a
  feature flag inside the crate.
- Not a production consensus layer. Use `koru-protocol` for that.
- Not a general graph library. The graph is content-addressed,
  append-only, monotone. If you want mutable edges, this isn't the
  crate.

What it *is*: the minimum implementation of distinction theory plus the
worked examples needed to demonstrate the LCA pattern, plus the
boundary layers needed to use the substrate from other languages.
