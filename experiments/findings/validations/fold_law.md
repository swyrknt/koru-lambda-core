# Validation Design — Fold Law Mechanism (research-lead)

**Claim under test (CLAUDE.md):** *"Fold Law (seed layer, depth ≤ 8): d0/d1 become mega-hubs by topological necessity — they are on every derivation path because ByteMapping routes every byte through them 8 times."*

**Status:** Experiment design only. Binary NOT drafted (agent hit sandbox limits before completing code). Exp 4 already confirmed d0/d1 are linear-in-N hubs (deg ≈ 0.10·N); this experiment isolates the *mechanism*.

---

## What's already known

Exp 4 (rust-craftsman, in SUMMARY_rust_craftsman.md):
- At N = 1M, deg(d0) ≈ 102,101, deg(d1) ≈ 81,831, random node deg ≈ 2.
- deg(d0)/N ≈ 0.10 at all sizes (10K, 100K, 1M).
- d0/d1 are strictly linear in N.

**What was NOT isolated:** whether hub formation is specifically driven by ByteMapping's 8-step alternating-bit fold, or by general content concentration in synthesis patterns.

## Method

ByteMapping (`src/primitives.rs:26-38`) routes every byte through 8 syntheses: folds 8 bits via successive `synthesize` calls, alternating d0/d1 at each step. So byte X's chain is:
```
step_0 = synth(d_{bit_0_of_X}, d_{bit_1_of_X})
step_1 = synth(step_0, d_{bit_2_of_X})
...
step_7 = synth(step_6, d_{bit_7_of_X})
```
d0/d1 each appear ~4 times per byte chain.

### Control: synthesis WITHOUT ByteMapping
Build a deep chain of N=10K novel syntheses starting from d0/d1 only. E.g., Fibonacci-style chain: `a_{i+2} = synth(a_i, a_{i+1})`. Measure deg(d0), deg(d1).

### Treatment: synthesis VIA ByteMapping
Introduce N bytes through the public byte canonicalization API. Each byte triggers ~8 syntheses internally. Measure deg(d0), deg(d1).

### Predictions

- **Control:** deg(d0), deg(d1) are O(log N) at most — used only as seeds; descendants take over.
- **Treatment:** deg(d0), deg(d1) are O(N) because each byte routes through them.
- **Quantify:** ratio of deg(d0) / total_syntheses in each case.

**Bonus check:** pick a random non-primordial node at depth 5 — does its degree scale with N? Prediction: no, only seed nodes do.

## Implementation note (TODO)

Agent did not get to drafting the binary code. Pseudocode:

```rust
// experiments/runner/src/exp20_fold_law.rs (TO BE WRITTEN)

fn control_fibonacci_chain(n: usize) -> (DistinctionEngine, /* metrics */) {
    let engine = DistinctionEngine::new();
    let mut a = engine.synthesize(engine.d0(), engine.d1());
    let mut b = engine.synthesize(&a, engine.d1());
    for _ in 0..n {
        let c = engine.synthesize(&a, &b);
        a = b;
        b = c;
    }
    let rels = engine.get_relationships_snapshot();
    let d0_deg = rels.iter().filter(|(x, y)| x == "0" || y == "0").count();
    let d1_deg = rels.iter().filter(|(x, y)| x == "1" || y == "1").count();
    (engine, d0_deg, d1_deg)
}

fn treatment_bytes(n: usize) -> (DistinctionEngine, /* metrics */) {
    let engine = DistinctionEngine::new();
    for i in 0..n {
        let _ = ByteMapping::byte_to_distinction((i % 256) as u8, &engine);
        // or whatever the public API for byte canonicalization is
    }
    let rels = engine.get_relationships_snapshot();
    let d0_deg = rels.iter().filter(|(x, y)| x == "0" || y == "0").count();
    let d1_deg = rels.iter().filter(|(x, y)| x == "1" || y == "1").count();
    (engine, d0_deg, d1_deg)
}
```

Verify public API in `src/primitives.rs` before writing — may need `ByteMapping::map_byte_to_distinction` or a thin wrapper.

**Important:** ByteMapping cache is built against a throwaway engine (TODO #3 phantom node bug). To get accurate degree measurement against the engine being measured, the byte chains must actually register into that engine — either fix the phantom bug first (Section 1 in CHECKLIST) or replicate the 8-step fold by hand inside the experiment.

## Reproduction (once binary lands)

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/runner
cargo build --release
cargo clippy --all-targets    # must be zero warnings
./target/release/exp20_fold_law
```

## Pending actions

1. **Write `experiments/runner/src/exp20_fold_law.rs`** (agent ran out of sandbox-permitted operations before drafting code).
2. Add `[[bin]]` stanza to `Cargo.toml`.
3. Run; update this report with measured values.

**Note:** This validation depends on the ByteMapping phantom bug (Section 1.1 #1 in CHECKLIST) being fixed first, OR the experiment manually replicating the 8-step fold to bypass the cache. Otherwise byte-derived distinctions won't appear in the calling engine's relationships.
