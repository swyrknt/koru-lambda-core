# Audit — `src/subsystems/parallel.rs` (rust-craftsman)

**Scope:** 379 LOC. `ParallelBatchProcessor`, `ParallelSynthesizer`, `ParallelAction`.
**Method:** Static read-only audit. `cargo clippy`/`cargo test` not run (plan mode).
**Verdict:** `ParallelBatchProcessor` should be **DELETED**. `ParallelSynthesizer` is genuinely parallel; keep with cleanup.

---

## Verdicts

| Q | Concern | Verdict | Severity |
|---|---|---|---|
| 1 | `ParallelBatchProcessor` is parallel in name only | **CONFIRMED** | High (false advertising) |
| 2 | `ParallelSynthesizer` is actually parallel | CONFIRMED with caveats | Medium |
| 2b | DashMap shard contention on hot keys defeats rayon | PARTIAL — measurable, not catastrophic | Low-Medium |
| 3 | Per-batch nonce loop is race-free | CONFIRMED (`&mut self`, no shared mutation) | — |
| 3b | Multi-processor coordination on same engine | SAFE at engine layer; divergence-prone across batches | Medium |
| 4 | Clones become free under v2.0 `Copy + [u8;16]` | CONFIRMED — 8 sites; 7 collapse | — |
| 5 | Idiomatic concerns | One reachable `.expect()` via dependency; minor style | Low |
| 6 | clippy/test sweep | UNVERIFIED (plan mode) | — |
| 7 | Should `ParallelBatchProcessor` be deleted in v2.0? | **YES** | High value |

### Code-quality grades

| Component | Grade | Rationale |
|---|---|---|
| `ParallelBatchProcessor` (58-211) | D | Misleading API, sequential body, dead `worker_count` field, single-variant enum |
| `ParallelAction` + `Canonicalizable` (17-51) | B− | Correct; `par_iter` over 8 bytes is theatre but harmless once cache hits |
| `ParallelSynthesizer` (217-263) | C+ | Actually parallel, but allocates Strings on hot path; lossy errors |
| Tests (266-372) | C | Happy-paths only. No reject-path coverage. No multi-thread test. |
| Internal `num_cpus` shim (375-379) | B | `available_parallelism` returns logical, not physical — function is named `physical_count` |

---

## Q1 — `ParallelBatchProcessor` is Sequential-only

**CONFIRMED.** Evidence:
1. `ProcessingStrategy` enum has one variant: `Sequential` (29-32). Comment: "Process sequentially (maintains strict causal order)."
2. `process_batches` (101-114) is a plain `for batch in action.batches` loop. No `par_iter`, no rayon, no threads.
3. `validate_batch_internal` (118) takes `&mut self`. Cannot be invoked concurrently on one processor.
4. `worker_count` (66) is stored, exposed via getter, never read. Dead field. `num_cpus::physical_count()` is called once in `new` (78) purely to populate it.
5. Doc claims "Parallelizes batch validation across multiple CPU cores using rayon" (3) and "100k+ tx/s throughput on multi-core systems" (4). Neither describes the implementation.
6. The only rayon use in `ParallelBatchProcessor` paths is `Canonicalizable for ParallelAction` (41), which `par_iter`s over 8 bytes from `u64::to_le_bytes()`. Rayon dispatch overhead dominates.

**Verdict:** misnamed. It's `SequentialBatchProcessor` with a `worker_count` field for cosmetics. The TODO.md "Not Planned to fix" line is correct.

## Q2 — `ParallelSynthesizer` is genuinely parallel

`synthesize_parallel` (230-247) uses `into_par_iter` on the pair list, calling `engine.get_distinction_by_id` and `engine.synthesize` inside the closure. Both are `&self` on `DistinctionEngine`, thread-safe via DashMap. Real data-parallelism over distinct work units.

**Three caveats:**

1. **Hot-key contention** bounded by the byte cache, not DashMap shards. `byte.to_canonical_structure(&self.engine)` (258) hits the static `BYTE_DISTINCTION_CACHE` (primitives.rs:26). Returns IDs that exist in *some* engine — not necessarily this one (Exp 5 phantom nodes). Bottleneck is `String::clone` (21 ns per Exp 16), not contention.

2. **`synthesize_parallel` returns `String`, takes `(String, String)`.** Each pair: 2 input allocations + 1 output allocation + `format!` inside `engine.synthesize`. Small batches slower than serial. No batch-size threshold guard.

3. **Lossy error path** (238-244):
```rust
match (d_a, d_b) {
    (Some(a), Some(b)) => { ... result.id().to_string() },
    _ => String::new(),
}
```
When either id is unknown, output is empty string — indistinguishable from a successful synthesis. Empty strings collide between misses. Plus `Distinction::new("")` is constructable (Exp 9 class). Fix: `Vec<Option<String>>` or `Result<...>`.

## Q4 — Clone-site enumeration vs v2.0

| # | Line | Site | Op | After v2.0 |
|---|---|---|---|---|
| 1 | 49 | `engine.d0().clone()` (fold init) | Distinction clone | `*engine.d0()` — free |
| 2 | 73 | `engine.d0().clone()` (new) | Distinction clone | free |
| 3 | 74 | `engine.d1().clone()` (new) | Distinction clone | free |
| 4 | 145 | `self.local_root.clone()` (validate init) | Distinction clone | free |
| 5 | 153 | `current_state.clone()` (update root) | Distinction clone | free |
| 6 | 194 | `action_data.clone()` (synthesize_action) | **NOT** a Distinction clone — clones whole `ParallelAction` incl. `Vec<TransactionBatch>` | **STILL CLONES** — pre-existing waste |
| 7 | 203 | `new_root.clone()` (update root) | Distinction clone | free |
| 8 | 241 | `result.id().to_string()` | String alloc | `result.0` — free |
| 9 | 259 | `distinction.id().to_string()` | String alloc | free |

**7 of 8 distinction clones become free.** Site #6 is real waste that v2.0 will not fix. Post-v2.0, refactor `synthesize_action` to take `&Self::ActionData` or canonicalize before processing.

## Q5 — Other idiomatic concerns

- **Reachable `.expect()` on hot path** at `primitives.rs:67`, touched by every byte canonicalization including parallel.rs:258. Cache is built `for byte in 0u8..=255` so the expect cannot panic, but a hot-path expect is a smell.
- **No `Arc<Mutex>` / `RwLock` anywhere** — good.
- **`Vec::with_capacity + push` loop** (106-111) could be `collect`. Stylistic.
- **`num_cpus` shim** (375-379) returns logical cores but is named `physical_count`. Cosmetic.
- **`into_par_iter` over 8 elements** (41) costs more than serial.
- **Test dead code** (296): `previous_root: String::new()` with "// Will be updated" comment; never updated. Reject paths (123-129, 135-139) untested.

## Q6 — clippy / tests not executed (plan mode)

To run:
```bash
cd /Users/sawyerkent/Projects/koru-lambda-core
cargo clippy --all-targets 2>&1 | grep -i parallel
cargo test --lib parallel 2>&1 | tail -30
```

Expected: clean at default level. 5 tests should pass (all happy paths).

## Q7 — Recommendation: DELETE `ParallelBatchProcessor` in v2.0

Rationale:
1. **Misnamed.** "Parallel" everywhere, absent from body. Reputational debt.
2. **TODO.md line 151** already states fix is excluded; consumers don't rely on parallelism.
3. **Blockchain-specific.** Coupled to `TransactionBatch`/`BatchValidationResult`. Belongs in koru-protocol per CLAUDE.md.
4. **`ConsensusValidator` already does exactly this**, minus the unread `worker_count` field. No behavioral delta.
5. **Single-variant enum** (YAGNI).
6. **Tests duplicate validator coverage.**

### Suggested deletion plan

Remove:
- `ParallelAction` (21-26)
- `ProcessingStrategy` (28-32)
- `Canonicalizable for ParallelAction` (34-51)
- `ParallelBatchProcessor` (58-179)
- `LocalCausalAgent for ParallelBatchProcessor` (181-211)
- Tests 1, 2, 3, 5 (creation, sequential batch ×2, from_root)
- Internal `num_cpus` mod (375-379)

Keep:
- `ParallelSynthesizer` (217-263), rename to `BatchSynthesizer`
- `test_parallel_synthesizer` (337-354)

**Net:** removes ~250 LOC, leaves ~80 LOC, all genuinely parallel.

Minimum-acceptable alternative if deletion is too aggressive: rename to `SequentialBatchProcessor` and remove `worker_count` / parallelism doc claims.

## Bottom line

Two improvements at v2.0:
1. Delete `ParallelBatchProcessor` (~250 LOC removed).
2. Refactor `ParallelSynthesizer` to return `Vec<Option<Distinction>>`, rename to `BatchSynthesizer`, drop empty-string fallback.

Combined with `Copy + [u8;16]`, the synthesizer benefits ~4× from Exp 15 measurements with no further work.
