# CLAUDE.md — koru-lambda-core

You are working on the synthesis substrate that powers both ALIS (cognitive architecture) and koru (economic protocol). The engine core is theory-pure; the surrounding subsystems are built for koru's blockchain use case.

---

## What This Project Is

koru-lambda-core is a minimal axiomatic system for distributed computation. It implements distinction theory: one operator (`synthesize`), four axioms (determinism, commutativity, irreflexivity, content addressing), two primordials (d0, d1). From these, all structure emerges.

The engine is used by:
- **ALIS** (`/Users/sawyerkent/Projects/alis-ai/`) — cognitive architecture, language, self-reference
- **koru-protocol** (`/Users/sawyerkent/Projects/koru/`) — economic consensus, currency, trust
- Both pin `koru-lambda-core = "1.2"` today; v2.0 is queued on `research/warroom-experiments` (single bundled major bump per Decision 5.1).

The engine doesn't know what domain it serves. It knows about parents.

---

## The Theory — What Five Rounds of ALIS Research Proved

The ALIS warroom (5 rounds, 50+ experiments, 11 research documents) produced deep understanding of how this engine behaves. This knowledge is load-bearing for any changes.

### The Five Axioms (all enforced in engine.rs)
1. **Determinism** — `synthesize(a, b)` always produces the same result. Proven: two cold engines on identical inputs produce byte-identical state.
2. **Commutativity** — `synthesize(a, b) = synthesize(b, a)`. Enforced by canonical `(min, max)` ordering on the 16-byte IDs before hashing.
3. **Irreflexivity** — `synthesize(a, a) = a`. Enforced by early return. No new structure created.
4. **Content addressing** — ID = first 16 bytes of `SHA256(min(a.bytes) || max(b.bytes))`. Identity IS the process.

### The Coding Law (experimentally precise)
**Degree = total synthesis participations.** Correlates 0.99 (Spearman rho) with raw frequency of use. Each time a distinction participates as input to a NEW synthesis (not a repeat of an existing pair), its degree increments by 1. Idempotent repeats add nothing.

Two mechanisms produce hubs:
- **Fold Law (seed layer, depth ≤ 8):** d0/d1 become mega-hubs by topological necessity — they are on every derivation path because ByteMapping routes every byte through them 8 times.
- **Coding Law proper (depth > 8):** degree tracks usage frequency. Content hubs form where usage concentrates.

### Structural Invariants (proven at scale, zero exceptions)
- **r = 2d − 3** (exactly). Each novel synthesis adds 1 node + 2 relationships. Asserted in `engine.check_structural_invariant()` (quiescent-mode, sub-branch #3).
- **Average degree → 4.0** (not 2.0 — that was density, not degree).
- **Binary parentage.** Every non-primordial has exactly 2 distinct parents.
- **Content addressing is engine-state-independent.** Same chain on different engines with different histories → identical IDs.
- **Nesting is the UNIQUE order encoding** in a commutative substrate (derivable from axioms).
- **Saturation.** Repeating the same synthesis adds zero nodes, zero relationships.
- **Self-reference through mediated observation → infinite novelty.** Direct self⊗self is irreflexive (returns self). Mediated self-observation (synth(synth(self, obs), self)) produces unique distinctions at every depth.

### Key Behavioral Facts (post-v2.0 measurements)
- **Performance:** ~500K synths/sec single-threaded; **15.3M ops/sec on 8 threads** post-foundation (was 2.6M on v1.2.0 — 5.9× from IdentityHasher + 16-byte byte keys on `DashMap`). *(Exp 10 re-run, 2026)*
- **Memory:** `Distinction([u8; 16])` is 16 B + container overhead — roughly **80 B per distinction in practice**, down 8× from v1.2.0's ~629 B (String hex). The dominant gain is cache density, not clone elimination. *(Exp 13–16)*
- **Ceiling:** ~80M distinctions on a 16 GB laptop today (was ~10M under v1.2.0). String-ID overhead removed.
- **Log replay:** 450K ops/sec ordered / 367K shuffled with perfect fidelity (M3 Pro). The append-only log of canonical `(min, max)` parent tuples (Decision 5.7) round-trips exactly; shuffled order produces byte-identical state via content addressing. *(Exp 7, Exp 12)*
- **Snapshots tear:** `get_state_snapshot_unsynchronized()` under concurrent writes has rare (≤0.1%) but avalanche-sized torn rate — two independent DashMap iterations, not atomic. Not safe for persistence under load; the rename (Decision 5.8) forces callers to acknowledge this. *(Exp 6, 2026)*
- **Compactor mutations are theory-clean:** all compactor writes go through `engine.synthesize` (append-only, axioms preserved). The compaction event itself becomes a first-class distinction in the engine; the engine record is the canonical compaction history. v2.0 also strips magic thresholds (now required at construction), removes `compaction_count` double-counting, and stops the compactor from self-archiving its own work product (sub-branch #9).
- **ByteMapping phantom nodes:** closed in Phase 3 — `map_byte_to_distinction()` now folds each byte through the calling engine, registering the 8-step chain. Phantom count 253 → 0 on the 256-byte exercise.

---

## Architecture (v2.0, on `research/warroom-experiments`)

```
src/
  engine.rs           959 LOC  Core + traversal indices + synthesis log + invariant API. DashMap<[u8; 16], _, IdentityBuildHasher>; &self everywhere.
  primitives.rs        82 LOC  Canonicalizable trait + ByteMapping (folds through caller's engine)
  distinction_hex.rs  231 LOC  to_hex / from_hex / Display / Debug / serde adapter — the only hex-aware module in the crate
  lib.rs              139 LOC  Re-exports
  subsystems/
    local_agent.rs     76 LOC  LocalCausalAgent trait + synthesize_causal_action helper
    compactor.rs      526 LOC  StructuralCompactor (explicit thresholds, append-only via synthesize)
    validator.rs      643 LOC  ConsensusValidator (pre-validation pass + V3 data cap + V4 clip + restore_state)
    network.rs        908 LOC  NetworkAgent (peer-id cap + LRU pending_commitments + restore_state)
    commitment.rs     576 LOC  CommitmentAgent (BatchCommitment::compute hashes leader_id; LRU)
    parallel.rs       123 LOC  BatchSynthesizer (renamed from ParallelSynthesizer; Vec<Option<Distinction>>)
  ffi.rs            1,137 LOC  C ABI surface — Box<Mutex<...>> handles, opaque structs, ManuallyDrop<Arc> borrows, panic=abort
  wasm.rs             699 LOC  Browser bindings (feature-gated) — bytes-on-wire, idToHex/idFromHex, console_error_panic_hook
```

**The core that matters:** engine + primitives + distinction_hex + local_agent = ~1,400 LOC. Everything else is either blockchain-specific application logic or FFI/WASM bindings. Engine grew from 194 LOC (v1.2.0) to 959 LOC because the traversal API, synthesis log, IdentityHasher, byte-keyed maps, and structural invariant assertion landed during Phase 6; the theory it implements is unchanged.

---

## v2.0 status (Phase 6 — on `research/warroom-experiments`)

All 11 sub-branches merged. The integration branch is at 161 release tests passing (was 103 at the v1.2.0 baseline), clippy clean both bare and `--features wasm`, `Cargo.toml` still at `1.2.0` (per Decision 5.1: the bump is the final commit before the integration PR to `dev`).

What changed structurally:
- `Distinction { pub(crate) bytes: [u8; 16] }` with `Display` / `Debug` / `to_hex` / `from_hex` / serde adapter.
- DashMap keys are bytes; `IdentityHasher` per Exp 14.
- Append-only `SegQueue<(Distinction, Distinction)>` synthesis log (canonical `(min, max)`).
- `engine.parents_of` / `children_of` / `degree` (O(1) via reverse index + AtomicUsize cache).
- Quiescent-mode `engine.check_structural_invariant()`.
- N5/N6/V5 (Tier 0) consensus correctness closed; N1/N2/N7/N11/V3/V4/V6/V8 (Tier 1 hardening) closed.
- F1/F2/F3/F4/F5/F6/F7/F8/F9 (FFI hardening) closed. `panic = "abort"` on release; `Box<Mutex<NetworkAgent>>` / `Box<Mutex<ConsensusValidator>>` inside the FFI boundary; opaque `#[repr(C)] struct` handles; `ManuallyDrop<Arc<DistinctionEngine>>` borrows.
- W1/W2/W4/W5/W9/W10/W13 (WASM) closed. Bytes-canonical end to end. `idToHex` / `idFromHex` JS helpers. `console_error_panic_hook` wired to `#[wasm_bindgen(start)]`.
- Compactor cleanup (1.9): explicit thresholds at construction, no `archived_ids`, no double-count, no self-archive.

The detailed checklist with per-item evidence is in `CHECKLIST.md`. The growing changelog is in `CHANGELOG.md` (`## Unreleased` — Phase 7 renames it to `## 2.0.0 — YYYY-MM-DD`).

---

## Development Principles

1. **The engine core is sacrosanct.** Changes here must be minimal, additive, and provably correct. No heuristics. No thresholds. No magic constants. Section 2's additions (traversal API, log, invariant check) preserve this.

2. **The axioms are not negotiable.** Determinism, commutativity, irreflexivity, content addressing. Any change that violates these is wrong, regardless of what it enables.

3. **Append-only is correct.** The engine has no `remove_distinction`. This is not a limitation — it is the theory. The graph is monotone. Pruning, if ever needed, happens by building a new engine from a filtered log.

4. **The theory has been proven reliable.** When tests fail or results surprise, check the test first, then the hypothesis. The axioms have survived 50+ experiments with zero exceptions. They earn the benefit of the doubt.

5. **Two consumers exist.** Changes must not break ALIS or koru-protocol. Coordinate version bumps. Phase 8 of the v2.0 plan covers consumer migration (ALIS `tracker.rs` deletion ~600 LOC; koru-protocol wire format updates).

---

## Testing

```bash
cargo test --release                       # 161 tests, all green (Phase 6 baseline)
cargo clippy --all-targets --release       # clean
cargo clippy --all-targets --features wasm --release  # clean
cargo build --release                      # FFI + rlib + cdylib + staticlib
```

Tests cover: axiom verification, synthesis determinism, compactor classification, consensus validation (V5 + V6 regressions), network agent epochs/leaders/dedupe (N5/N6/N7/N11), parallel batch synthesis, byte canonicalization, FFI safety (panic=abort + Mutex + opaque types + F7/F9), traversal API, synthesis log replay, structural invariant.

For the `wasm` feature, all WASM tests are `#[wasm_bindgen_test]` (sub-branch #10 / W13). The recommended driver is:

```bash
wasm-pack test --node --features wasm
```

Host `cargo test --features wasm` compiles them but skips execution — the wasm-bindgen-test harness is wasm-only.
