# Research Lead — Experiments 1, 2, 3, 7, 10, 11

**Hardware:** Apple M3 Pro, 18 GB RAM, rustc 1.91.1 release (lto=thin, codegen=1). koru-lambda-core 1.2.0, `src/` unmodified.

## Verdicts at a glance

| # | Claim under test | Verdict | Measured |
|---|---|---|---|
| 1 | "~656 bytes per distinction" (CLAUDE.md) | SUPPORTED | 629 B/distinction at 1M live (range 514–629 across scales) |
| 1 | "10M distinctions in 800 MB" (v2.0 TODO) | NOT reachable on current code; plausible after `[u8; 16]` rewrite | Linear extrapolation: 10M → ~6.0 GB today |
| 2 | r = 2d − 3 exactly (zero exceptions) | CONFIRMED at 10× prior ceiling | 5,000,000 distinctions → 9,999,997 relationships, delta = 0 |
| 3 | TODO #2's `RwLock<Vec>` log stays ≤ 5% overhead | FAILS at ≥ 4 threads | 0.9% @ T=1, 7.9% @ T=4, **26.3% @ T=8** |
| 3 | `Mutex<Vec>` alternative | WORSE | 52.7% slowdown at T=8 |
| 3 | `crossbeam::queue::SegQueue` alternative | RECOMMENDED | 7.0% overhead at T=8; scales with cores |
| 7 | Log replay produces byte-identical engine | CONFIRMED | 1M synths → PERFECT id + relationship match |
| 7 | "714K ops/sec replay" (CLAUDE.md) | NOT reproduced on M3 Pro | 450K ops/sec ordered, 367K shuffled |
| 7 | Replay order-independence (content addressing) | CONFIRMED | Shuffled log → byte-identical engine |
| 10 | "500K–900K synths/sec single-threaded" | Lower half only | 538K @ 1M depth; 425K @ 2M depth |
| 10 | "1.85M on 8 threads, 3.5× scaling" | EXCEEDED at 1M depth | 2.59M @ T=8 (4.8×) for 1M; 1.67M (3.9×) for 2M |
| 11 | "Clone elimination is the big win" (v2.0 #5) | WEAK as time argument | Clone = 22 ns = 1.6% of novel synth cost |

## Key numerical evidence

**Memory (Exp 1, dhat):** 1M-distinction engine = 600 MB live heap. 629 B/distinction. Linear extrapolation to 10M = ~6.0 GB — matches the CLAUDE.md "~6.5 GB" figure. The v2.0 "800 MB for 10M" target is only achievable after the `[u8; 16]` rewrite.

**Invariants (Exp 2):** At all four scales tested (100, 10K, 1M, 5M), `relationship_count() == 2 * distinction_count() - 3` with **delta = 0**. The invariant survives 10× extension past the prior ceiling of 500K. Build time at 5M was 13.2s (single thread, cold engine, deep chain).

**Log A/B/C (Exp 3), median of 3 runs at 1M synths:**

| threads | baseline | A (RwLock) | B (Mutex) | C (SegQueue) |
|---:|---:|---:|---:|---:|
| 1 | 493K ops/s | 498K (+0.9%) | 503K (+2.0%) | 499K (+1.2%) |
| 4 | 1.43M | 1.32M (−7.9%) | 1.01M (−29.4%) | 1.33M (−6.7%) |
| 8 | 2.28M | 1.68M (**−26.3%**) | 1.08M (**−52.7%**) | 2.12M (**−7.0%**) |

**The TODO's proposed `RwLock<Vec>` log design is a scalability trap.** Only `crossbeam::queue::SegQueue` stays near the 5% overhead bar at 8 threads.

**Replay (Exp 7, 1M synths):**
- Build: 573K ops/s
- Bincode serialize: 81 MB in 25 ms. Bincode deserialize: 14 ms.
- Replay (ordered): 450K ops/s, **PERFECT fidelity** (sorted id sets equal, sorted relationship sets equal)
- Replay (shuffled with seeded RNG): 367K ops/s, **PERFECT fidelity** — content addressing makes log order irrelevant, confirmed behaviorally.
- Log size: 81 bytes/entry (because b_id = d1 = "1" on every step of this chain; random chains would be ~144 B/entry).

**Throughput (Exp 10):** Single-thread synthesize slows with graph depth (538K @ 1M depth → 425K @ 2M). 8-thread throughput = 2.59M ops/s = 4.8× scaling. 16 threads = 3.38M ops/s. The CLAUDE.md "3.5× on 8" cite is conservative for M3 Pro.

**Clone cost (Exp 11):** `Distinction::clone()` = 22.0 ns (dominated by String allocator, not memcpy). Novel synth = 1410 ns. Clone is **1.6% of synth cost**; three clones on hot path = ~5%. The v2.0 case must be argued on **memory density** (8×) not on clone throughput (~5%).

## Action items for PR reviewers

1. **Change the proposed log backing from `RwLock<Vec<..>>` to `SegQueue<..>`** (or per-thread sharded Vecs) before landing TODO #2. The TODO's design regresses 8-thread throughput by 26%. Exp 7 proves order-independence, so SegQueue's lossy cross-thread ordering is safe for replay.
2. **Update CLAUDE.md throughput ranges** — "500K–900K single-threaded" is misleadingly narrow. Correct: 400K–540K depending on chain depth. Multi-thread: up to 2.6M on 8 threads for sub-1M-depth chains.
3. **Restate the v2.0 (`[u8; 16]`) case in memory-density terms.** 629 B → ~80 B per distinction is 8× and is the real win. Clone-elimination is ~5% CPU — nice but not headline.
4. **The r = 2d − 3 invariant is safe to promote to an axiom-level guarantee** given zero deviations at 5M. Consider a cheap `debug_assert!` inside `synthesize()` as a future-regression tripwire.
5. **Replay loop optimization:** Exp 7 replay (450K ops/s) is slower than build (573K ops/s) because each step does two DashMap string lookups. A replay-local `HashMap<String, Distinction>` cache during replay would eliminate the difference.

## Experiment binaries

- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/src/exp01_memory.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/src/exp02_invariants.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/src/exp03_log_ab.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/src/exp07_replay.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/src/exp10_throughput.rs`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/src/exp11_clone_cost.rs`

## Reproduction

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/runner
cargo build --release
./target/release/exp01_memory
./target/release/exp02_invariants
SYNTHS=1000000 ./target/release/exp03_log_ab
SYNTHS=1000000 ./target/release/exp07_replay
SYNTHS=1000000 ./target/release/exp10_throughput
ITERS=1000000 ./target/release/exp11_clone_cost
```

## Load-bearing recommendation — log design (Exp 3)

TODO's spelled-out proposal: `log: RwLock<Vec<(String, String)>>`. Empirically imposes **26% slowdown at 8 threads** vs baseline. Drop-in replacement that stays under 7% at 8 threads while preserving ordering (via drain-on-snapshot):

```rust
use crossbeam::queue::SegQueue;
log: SegQueue<(String, String)>,

// hot path in synthesize(), after novel check:
self.log.push((a.id.clone(), b.id.clone()));
```

Exp 7 verified log-order permutations produce byte-identical engines, so SegQueue's weaker ordering guarantee is safe for persistence and replay.
