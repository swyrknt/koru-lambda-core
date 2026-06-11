# Warroom Findings — TODO.md Empirical Assessment

**Date:** 2026-04-21
**Hardware:** Apple M3 Pro, 18 GB RAM, rustc 1.91.1 (release, lto=thin)
**Base:** koru-lambda-core 1.2.0, `src/` unmodified throughout all experiments
**Team:** research-lead, qa-sentinel, rust-craftsman (three specialists, 17 experiments)

---

## Executive Summary

TODO.md is **directionally correct but technically wrong in three of its load-bearing items**. Experiments settle the empirical claims as follows:

- **Item #5 (`[u8; 16]` Distinction)** — GO. Every measured axis improves 4–26×. Collision risk is cryptographically negligible; truncation to 128 bits is safe. FFI does not break (0 public C functions change). The TODO should reframe this as a *memory-density* win (8×), not a clone-throughput win (only 5% CPU).
- **Item #2 (synthesis log)** — NEEDS REDESIGN. `RwLock<Vec<_>>` loses 26% of 8-thread throughput. Replace with `crossbeam::queue::SegQueue` (7% overhead, scales with cores). Order-independence confirmed — canonical ordering not required for replay correctness.
- **Item #1 (traversal API)** — NEEDS RESHAPE. `children_of(d0)` is NOT O(1) at scale; d0/d1 are linear-in-N hubs (deg(d0) ≈ 0.10·N). API must split into O(1) `degree()` + streaming `children_of() -> impl Iterator`. Must be implemented inside the engine (external reverse-index races).
- **Item #3 (phantom fix)** — GO. Bug confirmed live; no in-tree tests break under the fix. But the broader issue is `Distinction::new(String)` being public — any external caller can mint phantom IDs.

Unplanned finding: **foreign-ID poisoning is a standalone bug** that item #6 (`pub(crate)`) fixes but doesn't fully close — synthesize accepts 1MB IDs (225× DoS slowdown). Consider this a hardening gap independent of the v2.0 roadmap.

---

## Claim-by-Claim Settlement

| TODO claim | Verdict | Evidence |
|---|---|---|
| "~656 B per distinction" | **Supported** | 629 B measured at 1M via dhat |
| "10M in 800 MB" (post-v2.0) | **Plausible** | Today: 6 GB at 10M. Post `[u8;16]`: ~80 B/entry projected. |
| "5–8× memory reduction" | **Supported** | String→bytes: 88→16 (5.5×). DashMap overhead caps total gain at ~8×. |
| "714K ops/sec replay, perfect fidelity" | **Fidelity yes, throughput partial** | 450K ordered / 367K shuffled on M3 Pro. Perfect state match. |
| "500K–900K synths/sec single-threaded" | **Range too wide** | 425–540K depending on graph depth |
| "1.85M on 8 threads, 3.5× scaling" | **Conservative** | 2.59M measured (4.8×) |
| "r = 2d − 3 exactly" | **Confirmed at scale** | Zero deviation at 100, 10K, 1M, 5M |
| "Degree rho=0.99 vs frequency" | **External (ALIS)** | Not measured in this repo |
| "Snapshot tears 16.5%" | **Refuted — rate** | 0.06–0.08% observed; mechanism real; avalanche-sized when it happens |
| "ByteMapping creates phantoms" | **Confirmed** | 128 phantom IDs per 256 bytes exercised |
| "Truncation 1-in-2^64 effectively zero" | **Confirmed** | 0 collisions in 268M pairs; preimage attack infeasible |
| "Synthesis log append <5% overhead" | **Design-dependent** | RwLock: 26% fail. SegQueue: 7% pass. |
| "Clone elimination is the big v2.0 win" | **Refuted — framing** | Clone is 1.6% of synth cost. Real win is memory density. |

---

## Top-Priority Changes to the TODO

### MUST FIX before implementation

1. **Item #2 log backend:** change from `RwLock<Vec<(String, String)>>` to `crossbeam::queue::SegQueue<(Distinction, Distinction)>`. Exp 3 measured 26% throughput regression from the TODO's design vs 7% from SegQueue at 8 threads. Exp 7 and Exp 12 confirmed order-independence, so SegQueue's weaker cross-thread ordering is safe for replay.

2. **Item #1 API shape:** TODO signatures are wrong for hubs.
   - `degree(d) -> usize` — stored in `DashMap<Distinction, AtomicUsize>`, O(1).
   - `children_of(d) -> impl Iterator<Item = Distinction>` — never a Vec. d0 at 1M has 102K children; a Vec clone is 13.3 ms.
   - Implement **inside** the engine (Exp 8: external reverse-index with index-first ordering produces orphans).
   - Underlying storage: `DashMap<Distinction, Vec<Distinction>>`. DashSet is 5–60× slower and unnecessary (append-only prevents duplicates).
   - TODO's proposed `children_of` algorithm ("check `synthesize(d, other) == some_known_child`") is broken — synthesize has side effects. Correct design uses a forward index `child -> (parent_a, parent_b)` captured at synthesis time.

3. **Item #5 sizing: stay at `[u8; 16]`.** Exp 13 proves collision infeasibility. Exp 15 shows [u8; 16] is 2× faster than [u8; 32] because 16+1+16 = 33 B fits in one SHA256 block while 32+1+32 = 65 B needs two. Exp 14 shows `BuildHasherDefault<IdentityHasher>` adds 6–13× on top; safe ONLY with bytes (hex strings have 4 bits/byte entropy and lock up the hasher).

4. **Item #3 phantom fix audit:** Exp 5 found zero in-tree tests break under the fix, but confirm before merging — the fix changes `distinction_count()` return values whenever ByteMapping is exercised.

### SHOULD ADD to TODO

5. **Replace `format!("{:x}", digest)` with `hex::encode(digest)` in current engine.** Free ~15% synth speedup (~100 ns/op). Non-breaking, additive. Ship in a 1.2.x patch, not v1.3.

6. **Harden `synthesize()` against foreign IDs** independent of item #6. Exp 9 found:
   - 1MB IDs: 7.3 ms/synth (225× DoS)
   - Cross-engine IDs: silently produces phantom parents in receiver
   - Empty strings: accepted
   Add ID-length validation (max 64 hex chars) and optionally a `verify_parents_exist` mode.

7. **`r = 2d − 3` invariant as a debug-build tripwire.** Zero deviations across all tests at 5M. Cheap `debug_assert!` in `synthesize()` would catch any future regression.

### Lower-priority reframings

8. **Rewrite the v2.0 case in memory-density terms.** 629 B → 80 B per distinction is 8×. Clone CPU saving is only 1.6%. The TODO's "clone elimination" framing undersells what actually matters.

9. **Update CLAUDE.md throughput numbers.** "500K–900K single-threaded" should be "425–540K depending on graph depth". "1.85M on 8 threads" should be "2.6M on 8 threads, 3.4M on 16".

---

## Unresolved Questions

- **Merkle-over-log / cross-peer log diffing:** Exp 12 confirmed replay doesn't need canonical ordering, but if koru-protocol ever needs to hash or diff logs, canonical `(min, max)` tuples are still required. Decide the semantic before shipping item #2.
- **Log memory cost:** 1M synths = 81 MB log (this experiment's chain). Worst-case random = ~176 B/entry = 176 MB/M. At 10M synths, log is 1.7 GB resident. Need an opt-out constructor (`DistinctionEngine::without_log()`) or a feature flag.
- **Compactor interaction with the reverse index:** item #4 (streaming SIS) depends on reverse-index degree. If `degree` is stored as `AtomicUsize`, compactor reads are always lock-free — confirm this is the chosen shape before implementing #4.
- **WASM/FFI wire format post-v2.0:** Exp 17 found 0 C signature changes, but the WASM side was not audited. Any JS code doing `id === "abc..."` string comparison will break.

---

## Recommended v1.3 / v2.0 Split (revised)

**v1.2.x patch (no API change):**
- Swap `format!("{:x}", ...)` for `hex::encode(...)`. ~15% speedup, 2 LOC.

**v1.3 (additive):**
- Item #1: `degree(d)` + `children_of(d) -> impl Iterator` + `parents_of(d)`, implemented inside the engine with a forward-index.
- Item #3: ByteMapping phantom fix.
- Item #2: synthesis log, but using `SegQueue<(String, String)>` (TODO's design is unshippable).
- Item #4: streaming SIS on top of #1.

**v2.0 (breaking):**
- Item #5: `Distinction([u8; 16]) + Copy`. Pair with `BuildHasherDefault<IdentityHasher>` on internal DashMaps.
- Item #6: `pub(crate)` field.
- Item #7: serde on log types.
- Re-type the synthesis log from v1.3's `SegQueue<(String, String)>` to `SegQueue<(Distinction, Distinction)>`.
- **Add:** ID-length validation and foreign-ID hardening in `synthesize()`.

Rust-craftsman originally recommended collapsing v1.3 to just #1 + #3 and deferring #2 to v2.0 for byte-native log format. Counter-argument: shipping #2 in v1.3 using `SegQueue<(String, String)>` gives consumers (ALIS) persistence a version sooner, and the format change in v2.0 is a straightforward re-type, not a rewrite.

---

## Artifacts

**Specialist summaries:**
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/findings/SUMMARY_research_lead.md`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/findings/SUMMARY_qa_sentinel.md`
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/findings/SUMMARY_rust_craftsman.md`

**Experiment code (all compiles and runs reproducibly):**
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/runner/` — Exp 1, 2, 3, 7, 10, 11
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/qa/` — Exp 5, 6, 8, 9, 12
- `/Users/sawyerkent/Projects/koru-lambda-core/experiments/rust/` — Exp 4, 13, 14, 15, 16, 17

**Source `src/` was not modified during any experiment.** All prototypes built as sibling crates depending on `koru-lambda-core` by path.

---

## Bottom Line

The five-round ALIS warroom's theoretical claims survive empirical testing. The TODO's specific **implementation designs** do not — three of the highest-leverage items (#1, #2, #5) need concrete changes before they ship. The `[u8; 16]` migration is the right call, defensible, and unblocks everything downstream. It should lead v2.0. The other items either fit around it or are independent hardening opportunities.
