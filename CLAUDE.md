# CLAUDE.md — koru-lambda-core

You are working on the synthesis substrate that powers both ALIS (cognitive architecture) and koru (economic protocol). This is the engine — 194 lines of theory-pure code at the core, surrounded by subsystems built for koru's blockchain use case.

---

## What This Project Is

koru-lambda-core is a minimal axiomatic system for distributed computation. It implements distinction theory: one operator (`synthesize`), four axioms (determinism, commutativity, irreflexivity, content addressing), two primordials (d0, d1). From these, all structure emerges.

The engine is used by:
- **ALIS** (`/Users/sawyerkent/Projects/alis-ai/`) — cognitive architecture, language, self-reference
- **koru-protocol** (`/Users/sawyerkent/Projects/koru/`) — economic consensus, currency, trust
- Both depend on `koru-lambda-core = "1.2"` from crates.io

The engine doesn't know what domain it serves. It knows about parents.

---

## The Theory — What Five Rounds of ALIS Research Proved

The ALIS warroom (5 rounds, 50+ experiments, 11 research documents) produced deep understanding of how this engine behaves. This knowledge is load-bearing for any changes.

### The Five Axioms (all enforced in engine.rs)
1. **Determinism** — `synthesize(a, b)` always produces the same result. Proven: two cold engines on identical inputs produce byte-identical state.
2. **Commutativity** — `synthesize(a, b) = synthesize(b, a)`. Enforced by canonical `(min, max)` ordering before hashing (engine.rs:108-112).
3. **Irreflexivity** — `synthesize(a, a) = a`. Enforced by early return (engine.rs:103-105). No new structure created.
4. **Content addressing** — ID = `SHA256(min(a.id, b.id) : max(a.id, b.id))`. Identity IS the process.

### The Coding Law (experimentally precise)
**Degree = total synthesis participations.** Correlates 0.99 (Spearman rho) with raw frequency of use. Each time a distinction participates as input to a NEW synthesis (not a repeat of an existing pair), its degree increments by 1. Idempotent repeats add nothing.

Two mechanisms produce hubs:
- **Fold Law (seed layer, depth ≤ 8):** d0/d1 become mega-hubs by topological necessity — they are on every derivation path because ByteMapping routes every byte through them 8 times.
- **Coding Law proper (depth > 8):** degree tracks usage frequency. Content hubs form where usage concentrates.

### Structural Invariants (proven at scale, zero exceptions)
- **r = 2d − 3** (exactly). Each novel synthesis adds 1 node + 2 relationships.
- **Average degree → 4.0** (not 2.0 — that was density, not degree).
- **Binary parentage.** Every non-primordial has exactly 2 distinct parents.
- **Content addressing is engine-state-independent.** Same chain on different engines with different histories → identical IDs.
- **Nesting is the UNIQUE order encoding** in a commutative substrate (derivable from axioms).
- **Saturation.** Repeating the same synthesis adds zero nodes, zero relationships.
- **Self-reference through mediated observation → infinite novelty.** Direct self⊗self is irreflexive (returns self). Mediated self-observation (synth(synth(self, obs), self)) produces unique distinctions at every depth.

### Key Behavioral Facts
- **Performance:** 425–540K synths/sec single-threaded depending on chain depth. 2.6M on 8 threads (4.8× scaling via DashMap shards on M3 Pro). *(Exp 10, 2026)*
- **Memory:** ~629 bytes per distinction measured at 1M live heap via dhat (String hex IDs — the main scalability bottleneck). v2.0's `Distinction([u8; 16])` reduces this 8× to ~80 bytes per distinction.
- **Ceiling:** ~10M distinctions on 16GB laptop today; post-v2.0, the same hardware fits the equivalent of ~80M distinctions.
- **Log replay:** 450K ops/sec ordered / 367K shuffled with perfect fidelity (M3 Pro). Append-only log of (parent_a_id, parent_b_id) pairs round-trips exactly; shuffled order produces byte-identical state via content addressing. *(Exp 7)*
- **Snapshots tear:** `get_state_snapshot_unsynchronized()` under concurrent writes has rare (≤0.1%) but avalanche-sized torn rate — two independent DashMap iterations, not atomic. Not safe for persistence under load. *(Exp 6, 2026; the older "16.5%" figure was an early estimate that did not survive empirical measurement.)*
- **Compactor mutations are theory-clean:** all compactor writes go through `engine.synthesize` (append-only, axioms preserved). The compaction event itself becomes a first-class distinction in the engine; the engine record is the canonical compaction history. *("non-destructive" was an earlier framing; mutations exist but they are append-only and therefore axiom-aligned.)*
- **ByteMapping phantom nodes:** `map_byte_to_distinction()` returns Distinction objects NOT registered in the calling engine's `all_distinctions`. The cache was built against a throwaway engine. *(Closed by Section 1.1 #1 in CHECKLIST: register the 8-step chain into the calling engine on first byte use.)*

---

## Architecture

```
src/
  engine.rs         194 LOC  The core. DashMap-backed. &self. Thread-safe.
  primitives.rs      78 LOC  Canonicalizable trait + ByteMapping (256-entry cache)
  lib.rs            138 LOC  Re-exports
  subsystems/
    local_agent.rs   76 LOC  LocalCausalAgent trait + synthesize_causal_action helper
    compactor.rs    503 LOC  StructuralCompactor (thermal classification, read-only)
    validator.rs    350 LOC  ConsensusValidator (blockchain: nonce-ordered batch validation)
    network.rs      603 LOC  NetworkAgent (blockchain: leader election, epochs, gossip)
    commitment.rs   504 LOC  CommitmentAgent (blockchain: two-stage gossip, LRU cache)
    parallel.rs     379 LOC  ParallelBatchProcessor (Sequential only) + ParallelSynthesizer (rayon)
  ffi.rs            899 LOC  C ABI surface
  wasm.rs           741 LOC  Browser bindings (feature-gated)
```

**The core that matters:** engine.rs + primitives.rs + local_agent.rs = ~350 LOC. Everything else is either blockchain-specific application logic or FFI/WASM bindings.

---

## What's Known to Need Improvement

From ALIS warroom findings (documented in `/Users/sawyerkent/Projects/alis-ai/warroom/findings/upstream-opportunities.md`):

### Additive (non-breaking, zero regression risk)
- **Parent/child traversal:** `engine.parents_of(d)`, `engine.children_of(d)`, `engine.degree(d)` — ~40 LOC. The engine stores relationships but exposes no traversal API. Forces consumers to duplicate the graph.
- **Append-only synthesis log:** record every novel synthesis as `(parent_a_id, parent_b_id)`. ~30 LOC. Enables trivial persistence (write the log, replay to reconstruct).
- **Phantom node fix:** ByteMapping builds its cache against a throwaway engine. Byte distinction IDs exist in relationships but not in `all_distinctions`. ~5 LOC fix.
- **Streaming SIS:** `calculate_sis` currently clones the full relationship snapshot. With a reverse index it becomes O(1) per node.

### Breaking (version 2.0)
- **`Distinction([u8; 16]) + Copy`:** 8× memory density (629 B → ~80 B per distinction). Every measured axis improves 4–26× (Exp 13–16, 2026). The single highest-leverage change possible. Clone-elimination is a small CPU win (~1.6% of synth cost); the dominant gain is memory density and cache effects.
- **`Distinction` field `pub(crate)`:** Prevents external construction of invalid IDs. Closes the foreign-ID poisoning class (Exp 9, 2026) structurally at compile time — no defensive runtime checks required.
- **Bytes-on-wire canonical + explicit hex serialization layer:** `Distinction::to_hex()` / `Distinction::from_hex()` / `impl Display` in a separate module. JSON wire uses hex via `#[serde(with = "distinction_hex")]`; WASM uses `Uint8Array` with `idToHex` / `idFromHex` JS helpers; FFI keeps existing `*mut c_char` (hex) surfaces and adds byte-native accessors. The substrate stays pure bytes; humans interact via the hex layer at boundaries.
- **Serde derives on log types:** Enables bincode serialization of the synthesis log.

---

## Development Principles

1. **The engine core is sacrosanct.** engine.rs is 194 lines, theory-pure, all axioms enforced. Changes here must be minimal, additive, and provably correct. No heuristics. No thresholds. No magic constants.

2. **The axioms are not negotiable.** Determinism, commutativity, irreflexivity, content addressing. Any change that violates these is wrong, regardless of what it enables.

3. **Append-only is correct.** The engine has no `remove_distinction`. This is not a limitation — it is the theory. The graph is monotone. Pruning, if ever needed, happens by building a new engine from a filtered log.

4. **The theory has been proven reliable.** When tests fail or results surprise, check the test first, then the hypothesis. The axioms have survived 50+ experiments with zero exceptions. They earn the benefit of the doubt.

5. **Two consumers exist.** Changes must not break ALIS or koru-protocol. Coordinate version bumps.

---

## Testing

```bash
cargo test              # 103 tests, zero warnings (baseline 2026-06-11)
cargo clippy            # clean at default level
cargo build --release   # FFI + rlib
```

Tests cover: axiom verification, synthesis determinism, compactor classification, consensus validation, network agent epochs/leaders, parallel processing, byte canonicalization, FFI safety.

For the `wasm` feature, the host-side `#[test]` blocks in `src/wasm.rs` cannot exercise the wasm-bindgen runtime; some tests fail/panic on host as of 1.2.0 (see `experiments/findings/baseline.md`). v2.0 migrates them to `#[wasm_bindgen_test]` and uses `wasm-pack test --node` (Section 1.8 in CHECKLIST).
