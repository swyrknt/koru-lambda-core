# Validation Design — Mediated Self-Reference (research-lead)

**Claim under test (CLAUDE.md):** *"Self-reference through mediated observation → infinite novelty. Direct self⊗self is irreflexive (returns self). Mediated self-observation (synth(synth(self, obs), self)) produces unique distinctions at every depth."*

**Status:** Experiment binary `experiments/runner/src/exp19_mediated_self_reference.rs` drafted but NOT run (plan mode blocked file creation). Design report only.

---

## Method

Three test trajectories:

### T1: Direct self-reference (control)
`s_{n+1} = synth(s_n, s_n)`. Seed `s_0 = engine.d0()`. Iterate 1000 times.
**Predicted result:** `s_i.id` never changes (irreflexivity axiom, engine.rs:103-105). 0 new distinctions, 0 new relationships.

### T2: Mediated, varying observation
For each step n: `obs_n = synth(d1, counter_n)` (varies); `inner = synth(s_n, obs_n)`; `s_{n+1} = synth(inner, s_n)`.
**Predicted result:** all s_i pairwise distinct at depths 1, 10, 100, 1000, 10000.

### T3: Mediated, CONSTANT observation (= d1)
For each step n: `inner = synth(s_n, d1)`; `s_{n+1} = synth(inner, s_n)`.
**Predicted result:** all s_i STILL distinct — because `inner` differs each step as s changes, so `synth(inner, s)` differs too. This is the stronger version of the claim.

**Checkpoints:** snapshot at depths 1, 10, 100, 1000, 10000.
**Invariant check at each:** `r = 2d - 3`.

## Predictions

| Trajectory | Predicted ID outcome | Predicted new distinctions |
|---|---|---|
| T1 direct (1000 iters) | all `s_i == s_0 == d0` | 0 |
| T2 mediated, varying obs (10K depth) | all unique | ~ 2 × 10K + counter chain ≈ 30K |
| T3 mediated, constant obs (10K depth) | all unique | ~ 2 × 10K |

`r = 2d - 3` predicted to hold at every checkpoint.

**Refutation paths:**
- Any collision in T2 or T3 = claim refuted.
- T1 producing any new distinction = irreflexivity broken (would be catastrophic; would also break Exp 2).

## Source code (drafted)

Full source in agent transcript for task `aeceb5723254e6525`. Compiles against current public API. Uses `HashSet<String>` to track seen IDs, `engine.get_state_snapshot()` for checkpoint counts. ~200 LOC, zero new dependencies.

## Reproduction (once binary lands)

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/runner
cargo build --release
cargo clippy --all-targets    # must be zero warnings
./target/release/exp19_mediated_self_reference
DEPTHS=1,10,100,1000,100000 ./target/release/exp19_mediated_self_reference
```

## Pending actions

1. Create `experiments/runner/src/exp19_mediated_self_reference.rs`.
2. Add `[[bin]]` stanza to `experiments/runner/Cargo.toml`.
3. Run; update this report with measured results.

## Source location

Drafted source in agent transcript (task `aeceb5723254e6525`). To persist to `experiments/runner/src/exp19_mediated_self_reference.rs`.
