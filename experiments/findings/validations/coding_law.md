# Validation Design — Coding Law (research-lead)

**Claim under test (CLAUDE.md):** *"Degree = total synthesis participations. Correlates 0.99 (Spearman rho) with raw frequency of use."*

**Status:** Experiment binary `experiments/runner/src/exp18_coding_law.rs` drafted but NOT run (plan mode blocked file creation). Design report only. Pre-registered predictions below.

---

## Method

1. **Build pool** of N candidate distinctions via a linear chain so they exist in the engine with known, uniform construction degree.
2. **Snapshot per-node degree BEFORE Zipf phase** (baseline).
3. For M ≫ N steps, sample two distinct pool indices (i, j) from **Zipf distribution** (skew α). Call `engine.synthesize(pool[i], pool[j])`. Track `freq[i]++`, `freq[j]++` on every call (novel and saturated).
4. **Snapshot per-node degree AFTER Zipf phase.**
5. `delta_degree[i] = (after - before)` for each pool node.
6. **Spearman rho** between `freq[]` and `delta_degree[]`.

**Sanity check:** repeat the exact same pair K=10K times. freq grows by K on each side; delta_degree grows by exactly 1 each (only the first novel call counts).

**Scales:** N=1K (M=200K), N=10K (M=1M), N=100K (M=5M). Three rho values.

**Implementation:** Pure-Rust Zipf, Spearman, and Pearson rolled by hand. No new dependencies.

## Predictions (pre-registered)

| N | M | α | Predicted rho(freq, delta_degree) |
|---|---|---|---|
| 1K | 200K | 1.0 | 0.97 – 0.999 |
| 10K | 1M | 1.0 | 0.97 – 0.999 |
| 100K | 5M | 1.0 | 0.97 – 0.999 |

**Predicted sanity outcome:** +1 distinction, +2 relationships, +1 degree on each parent after 10K identical synth calls. **CONFIRMED expected.**

**Possible refutation paths:**
- rho materially below 0.95 → most likely cause is **truncation by ties at the freq=0 floor** (many pool nodes never sampled under Zipf α=1). The rank-averaging in Spearman handles ties correctly, so this would be a measurement artifact, not theory failure. Re-run at higher M or lower α to clear it.
- rho < 0.5 → genuine theory failure. Would refute Coding Law claim.

## Design rationale (load-bearing)

- **freq counts every call**, novel or saturated, per claim's wording "raw frequency of use".
- **degree** measured via `engine.get_relationships_snapshot()` enumeration. Each relationship contributes 1 to each endpoint.
- **delta_degree isolates the Zipf-phase contribution** from construction-time degree (each interior linear-chain node has baseline degree 2).
- **i ≠ j enforced** because `i == j` hits irreflexivity (different branch, no degree change).
- **Sanity uses non-primordial parents** (already-synthesized nodes) — not d0/d1 — to avoid the special primordial relationship.

## Reproduction (once binary lands)

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/runner
cargo build --release
cargo clippy --all-targets    # must be zero warnings
./target/release/exp18_coding_law             # sanity + sweep
./target/release/exp18_coding_law sanity      # sanity only
./target/release/exp18_coding_law 10000 1000000 1.0 0xC0DE
```

## Pending actions

1. Create `experiments/runner/src/exp18_coding_law.rs` with the full source drafted in the agent transcript.
2. Add to `experiments/runner/Cargo.toml`:
   ```toml
   [[bin]]
   name = "exp18_coding_law"
   path = "src/exp18_coding_law.rs"
   ```
3. **Important caveat from the agent:** the draft includes `mod common;` to follow the existing pattern, but the experiment does not use `common::*`. **Remove the `mod common;` line before building** to avoid `dead_code` warnings.
4. Run sanity-only first; if it passes, run the full sweep.
5. Update this report with measured rho values, replacing the prediction table.

## Source location

Full drafted source: in agent transcript for task `a920774c94b13f38e`. ~280 LOC, pure Rust, compiles against current public API. To be persisted to `experiments/runner/src/exp18_coding_law.rs` when plan mode is lifted.
