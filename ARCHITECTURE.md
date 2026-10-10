# Architecture

`koru-lambda-core` is the implementation of distinction theory (see
`THEORY.md`) plus the reference consumer patterns and language bindings
needed to actually use it. This document describes the v2.0 architecture:
what's in each module, why it's there, and how the layers compose.

For *how the codebase got to this shape* — the design decisions, review
rounds, and step-by-step plan — see `DESIGN.md`.

---

## Three crates

These were three *layers inside one crate* until 2.0.0 split them, which is
why the substrate used to contain things that were never substrate. They are
now separate crates with a one-way dependency, enforced over the crate graph
by `scripts/gates/crate_boundary.sh` rather than asserted here.
`ECOSYSTEM.md` is authoritative on the layering and on which artifact ships
where; this document describes the inside of the substrate crate.

```
┌─────────────────────────────────────────────────┐
│  crates/koru-lambda-wasm   — bindings           │
│  The same engine with no Rust toolchain.        │
│  Publishes to npm as the conformance surface.   │
├─────────────────────────────────────────────────┤
│  crates/koru-lambda-agents — consumers          │
│  Worked examples: a validator and a commitment  │
│  agent, plus the agent contract, the recorder   │
│  and the children index, which left the         │
│  substrate because none of them determines a    │
│  byte. publish = false; ships no files in the   │
│  core tarball.                                  │
├─────────────────────────────────────────────────┤
│  koru-lambda-core          — the substrate      │
│  The engine and the operator, the projection    │
│  read and its wire form, the lift, the trust    │
│  boundary, replay, and the published vectors.   │
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
  engine.rs           Distinction, DistinctionEngine,
                      RawDistinctionId + verify, SynthesisOutcome
  projection.rs       projection primitive (Root, Direction, Signal,
                      Materialized) and its wire codec
  primitives.rs       Canonicalizable, ByteMapping, ByteFold,
                      DomainLabel, domain_anchor — the lift
  distinction_hex.rs  to_hex / from_hex / Display / Debug / serde
  replay.rs           snapshot_parentage / replay_topological
  vectors.rs          the published vectors, mirroring
                      conformance/vectors.json
```

Seven modules. `agent.rs` and `recorder.rs` were here until 2.0.0, and
`build_children_index` was part of `replay.rs`; all three now live in the
agents crate. What the substrate kept from the agent contract is not a
function but the fact every such function reads: parentage, via
`DistinctionEngine::parents_of` in `engine.rs`.

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
children call the agents crate's `build_children_index` (O(N) once, O(1)
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
    // Axiom 3, and the foreign-parent guard that covers it
    if a == b { self.assert_registered(a); return a; }
    // (the guard for the two-parent path runs below, after the
    //  saturation check and before anything is inserted)
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
wanting children iteration call `build_children_index`, in the agents
crate, on a snapshot.

**Hasher misuse guard.** In addition to the `debug_assert!` on 16-byte
keys, `IdentityHasher` implements `unreachable!()` on every non-`write`
`Hasher` method (`write_u8`, `write_u16`, …, `write_length_prefix`).
Misuse — hashing a slice, a non-16-byte key, or a typed integer —
triggers an immediate panic instead of silently corrupting state.
v2.0.0 has no tuple keys, so the hasher only ever sees single 16-byte
writes. `pub(crate) type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>`
is what the `DashMap` field type parameterizes on. Both are crate-private as
of 2.0.0: a hashing choice is not something to promise anyone. They went
private together because they cannot be split — a public alias over a private
struct is a hard error.

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

### `engine.rs` — the trait that left, and the read that replaced it

This document used to carry the `LocalCausalAgent` trait here, as part of the
substrate. It is no longer in this crate. The reason is not that it was
misfiled but that it was the wrong kind of thing:

**Every** non-primordial distinction is an advance of each of its parents —
set `root = a` and `action = b` in `synth(a, b)` and the monotonicity
condition holds with `a` as the root, then again with the parents swapped. So
agent-ness is a *reading of the graph*, available everywhere, and a trait
asserting that only some types have it misdescribes the structure.

What the substrate keeps is `parents_of`. Advancement is then one
comparison on top of it:

```rust
engine.parents_of(candidate) == Some((previous, _)) // or (_, previous)
```

The substrate shipped that comparison as `advances_from` for three days and
then removed it. It failed the membership test this document applies to
everything else: it determines no byte, no identifier and no wire field, and
it is derivable in one line by anyone holding `parents_of`. A substrate that
names every one-line question about parentage has stopped being a substrate.
The distinction worth keeping is the *observation* the function encoded — an
advance that does not descend from the root it replaces is not a causal step
but an unrelated assignment — and that belongs in prose, where it now is.

The contract that *chooses* to advance, and its `synthesize_causal_action`
helper, ship in `koru-lambda-agents` with the consumers that use them. See
[`THEORY.md § Using the substrate`](THEORY.md#using-the-substrate).

### `primitives.rs` — the lift

The substrate's only free parameter, and one rule at three levels. All
three call the same private fold; the levels differ only in which
alphabet supplies the element tips.

| Level | Alphabet | Elements | Entry point |
|---|---|---|---|
| bits | `{d₀, d₁}` | a byte's 8 bits | `ByteMapping::map_byte_to_distinction` |
| bytes | the 256 bit-tips | a sequence's bytes | `ByteFold::fold` |
| composites | the parts' own tips | a value's parts, in order | `ByteFold::fold_distinctions` |

There is no fourth level for nesting: a composite's fold is a
distinction, so it is an element one level up. That is why the composite
rule folds distinctions rather than specifying a byte layout — a layout
would have had to say how a nested composite is framed *within* bytes,
which is a serialisation format growing inside `docs/SPEC.md`.

`Canonicalizable` is the contract a liftable type satisfies, taking
`&self` and a prebuilt `ByteFold`. Both parameters were learned from its
implementors bypassing it: a batch commit folds `for tx in
&transactions` and cannot consume, and rebuilding the 256-tip table per
call dominates a thousand-transaction batch. The substrate ships one
impl, `u8`, because every other candidate type has no domain of its own
and a substrate-chosen one would collide — see ENC-4 in `CHECKLIST.md`
for why that clause was rejected rather than implemented.

`DomainLabel` is a newtype over the label byte. It makes the choice
typed; it does not prevent two crates choosing `0x01`, and says so
rather than implying a registry.

No static cache anywhere: every byte folded gets every intermediate
distinction registered in the engine that folded it, so the phantom-node
problem is closed by construction.

### `distinction_hex.rs` — the hex display boundary

`to_hex`, `from_hex`, `Display`, `Debug`, and a `serde` adapter live
here. Hex is not theory. It is the human/wire-format rendering of bytes.
The substrate is byte-canonical; hex is a boundary concern.

### `replay.rs` — the parentage-sufficiency theorem

Replay stays in the substrate while the recorder and the children index left,
and the distinction is worth stating because it is the membership test doing
real work. Replay is not a convenience: it is the claim that recorded
parentage is *sufficient* to reconstruct an engine, which is what makes Law 9
checkable. The other two determine no byte and reconstruct nothing.

`snapshot_parentage` dumps the parent map; `replay_topological` rebuilds a
fresh engine by repeatedly synthesizing entries whose parents are already
registered, until quiescent. It returns `Result<_, ReplayError>` with three
variants — `Unreachable`, `Mismatch`, `MissingPrimordial` — so a caller
handles corrupted parentage in release builds rather than discovering it as a
silent divergence.

The engine still has no observation infrastructure and no order-bearing state.
A consumer wanting chronology uses `SynthesisRecorder`, and one wanting
children iteration uses `build_children_index`; both now live in
`koru-lambda-agents`. `SynthesisRecorder` remains `!Send + !Sync` via
`PhantomData<*const ()>`, enforcing single-thread use at compile time, and its
compile-time assertion travelled with it.

---

## Reference subsystems

Reference subsystems live in their own crate, `koru-lambda-agents`,
and are **not published with 2.0.0**:

```
crates/koru-lambda-agents/src/
  lib.rs              declarations + re-exports
  validator.rs        ConsensusValidator
  commitment.rs       CommitmentAgent + BatchCommitment
```

Each subsystem is a worked example of the LCA pattern with real
consensus-protocol semantics. They demonstrate how a productive consumer
uses the substrate. They are **not** the substrate; if a consumer
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
crates/koru-lambda-wasm/
  src/lib.rs          WasmEngine and the JS-facing surface
  tests/              wasm_smoke.rs, cond_d_prime_wasm.rs
```

The bindings are their own crate because `wasm-pack` needs `cdylib`, which
would otherwise force a dynamic library on every pure-Rust consumer. Only that
crate declares it (`crate-type = ["cdylib", "rlib"]`); the substrate is
`["rlib"]`.

### FFI

Post-v2.0.0 work; **not shipped in the current release.** Nor are the
crate-types reserved for it: this document claimed the `cdylib` / `staticlib`
types "remain declared in `Cargo.toml`" and they do not — the substrate
declares `["rlib"]` alone. There is no `src/ffi.rs`, no opaque handle set, and
no C header. Whether a C ABI ships at all is an open owner decision; see
`ECOSYSTEM.md`, which argues the polyglot case is served by the wasm binary
and the wire protocol, leaving in-process embedding as the remaining job. The design sketch below documents what a
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

The JS/TS surface is its own crate, `crates/koru-lambda-wasm`, published
to npm alongside the substrate. It exposes `DistinctionEngine`, the
projection primitive and `verify`; the current ship-set of signals is
`Adjacency`, `Degree` and `HopDistance`. It is a separate crate rather
than a feature because `wasm-pack` requires `crate-type = ["cdylib"]`,
which every pure-Rust consumer would otherwise pay for.

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
            │  Consumer crate                      │
            └────────────────┬────────────────────┘
                             │ depends on
                             ▼
            ┌─────────────────────────────────────┐
            │  koru-lambda-core  (crates.io)       │
            │   Distinction, DistinctionEngine,    │
            │   synthesize, verify, parents_of,    │
            │   Projection, ByteFold, replay,      │
            │   vectors                            │
            ├─────────────────────────────────────┤
            │  koru-lambda-agents  (unpublished)   │
            │   LocalCausalAgent, ConsensusValidator│
            │   CommitmentAgent, SynthesisRecorder │
            ├─────────────────────────────────────┤
            │  koru-lambda-wasm  (npm)             │
            │   WasmEngine, idToHex, ...           │
            └─────────────────────────────────────┘
```

A consumer depends on the substrate for the operator and the reads, and on the
agents crate for a worked causal pattern. The agent layer is the intended
default path — see `ECOSYSTEM.md` — but using the substrate directly is legal
and supported, because the correctness guarantees flow from the axioms and not
from a consumer's shape. For JavaScript, the wasm crate exposes the same
engine; there is no C ABI in this release.

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
- Not a production consensus layer.
- Not a general graph library. The graph is content-addressed,
  append-only, monotone. If you want mutable edges, this isn't the
  crate.

What it *is*: the minimum implementation of distinction theory plus the
worked examples needed to demonstrate the LCA pattern, plus the
boundary layers needed to use the substrate from other languages.
