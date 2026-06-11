# QA Sentinel — Experiments 5, 6, 8, 9, 12

Five falsification experiments run. All build clean, all run to completion. No modifications to `src/`.

## Verdicts

| Exp | Hypothesis | Verdict |
|---|---|---|
| 5 | ByteMapping creates phantom parent IDs | **CONFIRMED** |
| 6 | `get_state_snapshot` tears at 16.5% under 8 writers | **PARTIAL** (real, but 0.06–0.08%, not 16.5%) |
| 8 | External reverse index races with engine.synthesize | **CONFIRMED** (engine-first safe, index-first unsafe) |
| 9 | Foreign `Distinction::new` accepted silently | **CONFIRMED at HIGH severity** |
| 12 | Commutativity holds under concurrent reorderings | **CONFIRMED** |

## Top findings

### 1. Phantom parents are a live bug, not a theoretical concern (Exp 5)
With 256 bytes used via `ByteMapping`, 128 byte-IDs become phantoms (referenced but unregistered). The `r = 2d − 3` invariant is violated numerically whenever ByteMapping is used. **No in-tree test catches this.** Proposed fix (building the 8-step chain in the real engine) adds 257 distinctions / 511 relationships per full byte-universe-exercise and restores `r = 2d − 3`. **Zero in-tree tests break under the fix.**

### 2. Foreign-ID poisoning is broader than just ByteMapping (Exp 9)
`Distinction::new` + public constructor accepts:
- Empty strings
- 1MB IDs (7.3 ms per synth — **225× DoS slowdown**)
- Cross-engine IDs
- Colon-containing IDs

All succeed silently and corrupt the relationship graph with phantom parents. **Upstream item #6 (`pub(crate)` field) is necessary but not sufficient** — synthesize must also validate parent existence and ID length.

### 3. Snapshot tearing is real but rarer than CLAUDE.md claims (Exp 6)
Observed rate: **0.06–0.08% on M-series hardware** vs the documented 16.5%. Max observed |Δ| = 19,520 (meaning ~9,760 novel synths landed mid-snapshot). All tears are r-leading — never caught mid-synthesize. Post-quiescence invariant held across all 200K+ writer ops. CLAUDE.md rate claim should be revised to "rare (≤0.1%) but catastrophic when it occurs."

### 4. Log replay does NOT require canonicalization (Exp 12)
Engine state is identical across (a) forward vs shuffled serial orders and (b) 8-writer concurrent runs with different shuffles. Raw-order replay round-trips perfectly (d=212, r=421 in all cases). Canonicalization is only needed for byte-identical cross-process log equality, not for replay correctness. **The theory-guardian's commutativity-leak concern about item #2 is refuted** for the reconstruction use case. (The concern remains valid for Merkle-over-log or log-diffing use cases — those do need canonical ordering.)

### 5. External reverse index pattern requires documented ordering (Exp 8)
Engine-first (commit-then-publish) yields 0 orphans in 2000 sweeps — safe but always lagging. Index-first (publish-then-commit) with a 5000-cycle spin-loop stall produces 37 orphans in 20,000 sweeps. Upstream item #1 (`children_of`) should be implemented INSIDE the engine with a reverse-index DashMap updated in the same critical section as the existing two DashMaps.

## New failure modes worth escalating

- `r = 2d − 3` holds numerically but is **violated semantically** during ByteMapping use and foreign-ID use — the invariant is a misleading health-check in those flows.
- Cross-engine `Distinction` reuse "works" by content addressing but silently produces ghost parents in the receiving engine. Same id, two engines, legitimate in A, phantom in B.
- 1MB-id DoS: any FFI/WASM caller can pass a giant string and slow synthesize by 225×.
- Snapshot tearing, when it happens, is "avalanche-sized," not off-by-one — consumers can't use any kind of small-tolerance retry strategy.

## Experiment code

- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/qa/src/exp05_phantoms.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/qa/src/exp06_tearing.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/qa/src/exp08_concurrent_read.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/qa/src/exp09_foreign_id.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/qa/src/exp12_log_commutativity.rs`

## Reproduction

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/qa
cargo build --release
./target/release/exp05_phantoms
./target/release/exp06_tearing
./target/release/exp08_concurrent_read
./target/release/exp09_foreign_id
./target/release/exp12_log_commutativity
```

## Source sites referenced (read, not modified)

- `/Users/sawyerkent/Projects/koru-lambda-core/src/engine.rs:13` — public `Distinction::new`
- `/Users/sawyerkent/Projects/koru-lambda-core/src/engine.rs:103-105` — irreflexive short-circuit
- `/Users/sawyerkent/Projects/koru-lambda-core/src/engine.rs:108-112` — canonical ordering
- `/Users/sawyerkent/Projects/koru-lambda-core/src/engine.rs:119-128` — insert + two add_relationship calls
- `/Users/sawyerkent/Projects/koru-lambda-core/src/engine.rs:154-156` — two-iteration snapshot
- `/Users/sawyerkent/Projects/koru-lambda-core/src/primitives.rs:26-38` — throwaway-engine cache build
- `/Users/sawyerkent/Projects/koru-lambda-core/src/primitives.rs:64-70` — byte lookup
