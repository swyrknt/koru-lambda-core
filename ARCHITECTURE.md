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
  lib.rs              ~40 LOC   public re-exports
  engine.rs           ~430 LOC  Distinction, IdentityHasher, DistinctionEngine
  agent.rs            ~80 LOC   LocalCausalAgent trait + helper
  primitives.rs       ~100 LOC  Canonicalizable trait + ByteMapping
  distinction_hex.rs  ~120 LOC  to_hex / from_hex / Display / Debug / serde
  replay.rs           ~60 LOC   snapshot_parentage / replay_topological / build_children_index
  recorder.rs         ~60 LOC   SynthesisRecorder reference observer
```

**Total: ~890 LOC.** This is the heart of the crate.

LOC sizes are targets. The `engine.rs ≤ 480 LOC` ceiling in `CHECKLIST.md`
(and gate 23 of `DESIGN.md` Part 10) is the upper bound that triggers
the hard gate; ~430 LOC is what we're trying to hit. Same idea applies
to every other file size in this layout.

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

`DistinctionEngine` carries three indexed maps, each a unique O(1)
projection of a theory operation:

```rust
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    all_distinctions: DashMap<[u8;16], Distinction, IdentityBuildHasher>,
    parents_of:       DashMap<[u8;16], (Distinction, Distinction), IdentityBuildHasher>,
    degree_counts:    DashMap<[u8;16], AtomicUsize, IdentityBuildHasher>,
}
```

| Field | What it serves |
|---|---|
| `all_distinctions` | Saturation (Axiom-equivalent: repeats add nothing). O(1) existence check in the synthesize hot path. |
| `parents_of` | Binary parentage (Law 5). The canonical child→parents projection used by replay, invariant checks, and parent walks. |
| `degree_counts` | Coding Law (Law 12) and Fold Law (Law 11) — both stated as degree properties. The theory's central observability claim, tested at 5M+ scale. |

Each field is needed at O(1). None is "convenience": dropping any one of
them would either break a theory probe at scale or stall the synthesize
hot path. The `children_of` enumeration is *not* in the engine —
`degree_counts` carries the count the theory names, and consumers that
need to iterate children call `replay::build_children_index` to
materialize the dual O(N) once.

`IdentityHasher` is the engine's internal `DashMap` hasher. SHA-256
prefixes are uniformly distributed; the hasher returns the leading 8
bytes as a `u64` with no XOR, rotation, or diffusion. Misuse is caught
by `debug_assert! + unreachable!()` on every method that isn't a 16-byte
`write`.

The synthesize hot path enforces all four axioms in under 40 LOC of body:

```rust
#[must_use]
pub fn synthesize(&self, a: Distinction, b: Distinction) -> Distinction {
    // foreign-byte guard (debug_assert)
    if a == b { return a; }                              // Axiom 3
    let (first, second) = if a.0 <= b.0 { (a, b) } else { (b, a) };  // Axiom 2
    let mut h = Sha256::new(); h.update(first.0); h.update(second.0);
    let mut new_bytes = [0u8; 16];
    new_bytes.copy_from_slice(&h.finalize()[..16]);     // Axioms 1, 4
    if let Some(existing) = self.all_distinctions.get(&new_bytes) {
        return *existing;                                // Saturation
    }
    let new_d = Distinction(new_bytes);
    self.all_distinctions.entry(new_bytes).or_insert_with(|| {
        self.parents_of.insert(new_bytes, (first, second));
        self.degree_counts.insert(new_bytes, AtomicUsize::new(0));
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

The fast-path `if let Some(existing) = self.all_distinctions.get(&new_bytes)`
return is **saturation** (Law 7). Repeats short-circuit before the closure
runs — degree is never bumped for an already-existing synthesis. The
closure runs only on novel synthesis.

The `or_insert_with` closure runs under the `all_distinctions` shard
write-lock for `new_bytes` and completes before that lock releases. Any
reader that later observes `new_d` in `all_distinctions` has a
happens-before edge to the populates inside the closure. Release/Acquire
ordering on `degree_counts` makes the contract uniform for probes that
read `degree_counts` directly without touching another DashMap first.

The `expect("…invariant")` panic messages encode the proof obligation
in source — a contributor who breaks the pre-seed invariant gets a
breadcrumb to the right line instead of a bare `unwrapped None`.

### `agent.rs` — the LCA trait

```rust
pub trait LocalCausalAgent {
    type ActionData: Canonicalizable;
    fn get_current_root(&self) -> &Distinction;
    fn synthesize_action(&mut self, action: Self::ActionData,
                         engine: &Arc<DistinctionEngine>) -> Distinction;
    fn update_local_root(&mut self, new_root: Distinction);
}

pub fn synthesize_causal_action<A: Canonicalizable>(
    local_root: Distinction, action: A, engine: &Arc<DistinctionEngine>,
) -> Distinction { /* ... */ }
```

The trait IS substrate. Subsystems are implementers. The trait formalizes
the LCA pattern: a consumer carries a local root, performs a causal
synthesis from `(root, action)`, advances its root forward.

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
  network.rs          ~550 LOC  NetworkAgent + PeerIdentity + NetworkAction
  compactor.rs        ~250 LOC  StructuralCompactor + CompactionAction
  parallel.rs         ~80  LOC  BatchSynthesizer
```

**Total: ~1,450 LOC.**

Each subsystem is a worked example of the LCA pattern with real
consensus-protocol semantics. They demonstrate how a productive consumer
uses the substrate. They are **not** the substrate; if `koru-protocol`
wants different semantics it forks them.

Bug-correct from the start:

- **Validator** pre-validates batches before any `synthesize` call —
  engine state is invariant on rejection (closes V5 by construction).
- **Commitment** `compute` hashes `leader_id` (closes N6 by construction).
- **Network** `TransactionBatch::previous_root: Distinction`, not
  `String` (closes N5 by construction; no truncation possible).
- **PeerIdentity::new** returns typed `Result<Self, PeerIdentityError>`
  with bounded id length (closes N1/N2 by construction).
- **Compactor** takes explicit `(hot, warm)` thresholds at construction.
  No magic defaults. All mutations go through `engine.synthesize`
  (append-only).

---

## Bindings

```
src/
  ffi.rs              ~650 LOC  C ABI
  wasm.rs             ~450 LOC  WASM bindings (feature-gated)
```

**Total: ~1,100 LOC.**

### FFI

- Opaque types: `#[repr(C)] pub struct KoruEngine { _private: [u8; 0] }`
  for `KoruEngine`, `KoruAgent`, `KoruValidator`. The C header gets
  distinct typedefs; callers can't pass one through as another.
- Handle wrapping: `Box<parking_lot::Mutex<NetworkAgent>>`,
  `Box<parking_lot::Mutex<ConsensusValidator>>`. `parking_lot::Mutex`
  is ~5× faster than `std::sync::Mutex` uncontended and lacks
  poisoning semantics that don't apply to FFI.
- Engine borrows: `ManuallyDrop<Arc<DistinctionEngine>>` via one
  `borrow_engine` helper. No `Arc::from_raw` + `into_raw` re-leak dance.
- `panic = "abort"` on the release profile. No unwinding across `extern "C"`.
- Length guards on slice inputs (`batch_len > isize::MAX` rejected).

### WASM

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
