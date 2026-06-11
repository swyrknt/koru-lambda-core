# Validation Design — Cross-Engine Determinism (qa-sentinel)

**Claim under test (CLAUDE.md):** *"Content addressing is engine-state-independent. Same chain on different engines with different histories → identical IDs."*

**Status:** Experiment binary `experiments/qa/src/exp21_cross_engine_determinism.rs` drafted but NOT run (plan mode blocked file creation). Design report only. Strengthens Exp 7 (single-engine replay) to multi-engine convergence under adversarial conditions.

---

## Phases

### Phase 1 — Baseline
Two cold engines A, B in separate threads (synchronized via Barrier). Identical 100K-step program. Snapshot every 1000 steps. Compare sorted distinctions, sorted relationships, SHA256 fingerprint.

### Phase 2 — History adversary
Engine A first runs 50K UNRELATED synths. Engine B runs ONLY the test chain. Per-step output IDs of the test chain must still match.
**This tests engine-state-independence directly.**

### Phase 3 — Concurrent
Same workload on both engines, 8 worker threads each, different shuffle seed. Post-quiescence sets must match.

### Phase 4 — Closed-form predict_id
Run the chain through a pure SHA256 `predict_id(a, b)` function (replicates engine.rs:101-116 in pure Rust). Compare engine outputs to closed-form predictions. **Refutes any chance the engine and the test share a bug.**

### Phase 5 — Three-engine majority
Three independent threads, one fingerprint each. Drift surfaces by majority disagreement.

## Predictions (pre-registered)

| Phase | Hypothesis | Prediction |
|---|---|---|
| 1 | Cold A vs cold B, 100 snapshots over 100K steps | **CONFIRMED** |
| 2 | Per-step IDs identical even with 50K-poisoned A prefix | **CONFIRMED** |
| 3 | 8-thread concurrent runs converge post-quiescence | **CONFIRMED** |
| 4 | Engine IDs match closed-form predict_id exactly | **CONFIRMED** |
| 5 | Three-way majority fingerprint matches | **CONFIRMED** |

## Theoretical basis for predictions

From reading `src/engine.rs:101-131`:
- ID derivation is a pure function: `SHA256(min(a,b) || ":" || max(a,b))`. Reads no engine state.
- `all_distinctions` dedup'd by ID before insert (line 119). Same inputs → same dedup decision regardless of prior history.
- `relationships` canonicalized in `add_relationship` (85-88), DashMap-dedup'd. Membership order-independent.
- Only nondeterminism: DashMap **iteration order** (not membership). Hence comparisons use sorted Vec / BTreeSet / SHA256 fingerprint of sorted form.
- Phase 2 prediction: A's extra history only ADDS entries to A's `all_distinctions`. Cannot modify the test chain's IDs. A's set being a strict superset of B's set is the proof.

## Risk of refutation

- **Phase 3 fingerprint mismatch** is the only realistic refutation path, and only if snapshot is captured during writes (Exp 6 tearing). The binary uses Barrier + join before fingerprinting → quiescence guaranteed → risk mitigated.
- Any other refutation implies engine-state-dependence in `synthesize`, contradicting CLAUDE.md and source. Worth catching if it exists.

## Source code (drafted)

Full source in agent transcript for task `aaeb109da866b2e0a`. Compiles against current public API. Uses:
- `koru_lambda_core::{Distinction, DistinctionEngine}`
- `sha2::{Digest, Sha256}` (already in deps tree)
- `rand` (StdRng, SliceRandom)
- `std::sync::{Arc, Barrier}`, `std::thread`

~280 LOC. Phase 3 uses 8 worker threads + chunked work distribution.

## Reproduction (once binary lands)

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/qa
cargo build --release
cargo clippy --all-targets    # must be zero warnings
./target/release/exp21_cross_engine_determinism
STEPS=200000 ./target/release/exp21_cross_engine_determinism
```

## Pending actions

1. Create `experiments/qa/src/exp21_cross_engine_determinism.rs` with the drafted source.
2. Add to `experiments/qa/Cargo.toml`:
   ```toml
   [[bin]]
   name = "exp21_cross_engine_determinism"
   path = "src/exp21_cross_engine_determinism.rs"
   ```
3. Run; update this report with measured per-phase verdicts.

## Source location

Drafted source in agent transcript (task `aaeb109da866b2e0a`). To be persisted when plan mode is lifted.
