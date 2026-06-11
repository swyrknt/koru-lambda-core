# Audit — `src/subsystems/validator.rs` (qa-sentinel)

**Scope:** 350 LOC. `ConsensusValidator`, blockchain batch validator.
**Method:** Static read-only audit. `cargo clippy`/`cargo test` not run (plan mode). Repro probe code drafted; not executed.
**Verdict:** **HIGH-SEVERITY FINDINGS.** Atomic-failure docstring is wrong, foreign-ID acceptance via `from_root`/`update_local_root`, unbounded DoS via `data` field.

---

## Verdicts

| # | Finding | Severity | Lines |
|---|---|---|---|
| V1 | `ConsensusValidator::from_root` accepts externally-constructed `Distinction` | **HIGH** (v2.0 blocker) | validator.rs:90-92 |
| V2 | `LocalCausalAgent::update_local_root` accepts externally-constructed `Distinction` | **HIGH** (v2.0 blocker) | validator.rs:201-203 |
| V3 | `TransactionAction.data: Vec<u8>` per-byte synthesize fold is unbounded | **HIGH** (DoS) | validator.rs:34-37 |
| V4 | `TransactionBatch.previous_root: String` unbounded; full string copied into rejection `format!` | **MEDIUM** (mem amp) | validator.rs:114-119 |
| V5 | **Atomic-failure semantic is rolled back ONLY in validator state; mid-batch `synthesize` calls leak to engine** | **MEDIUM** (theory drift / doc lie) | validator.rs:128-148 |
| V6 | `set_expected_nonce` desyncs from `local_root` (no joint setter) | MEDIUM | validator.rs:161-163 |
| V7 | Validator inherits ByteMapping phantom-parent bug (Exp 5) | MEDIUM | validator.rs:25-42 |
| V8 | `previous_root` compared by `String !=` but never validated as a real engine ID | LOW | validator.rs:114 |
| V9 | No length cap on transactions-per-batch | LOW (DoS amp) | validator.rs:131 |
| V10 | `BatchValidationResult` Clone with full rejection string | LOW | validator.rs:57-63 |
| OK1 | No `unsafe`, `unwrap`, `panic!`, `expect` in non-test code | clean | full file |
| OK2 | No direct `Distinction::new` in validator.rs | clean | grep |
| OK3 | Canonical-pair ordering preserved (always via `engine.synthesize`) | clean | validator.rs:40, 143 |
| OK4 | Only `engine.synthesize` used to mutate engine | clean | validator.rs:30, 36, 40, 84, 143 |

---

## V1, V2 — Foreign-Distinction acceptance

Two public API paths accept arbitrary `Distinction` values with **zero provenance check**:

- `ConsensusValidator::from_root(root: Distinction, expected_nonce: u64)` (validator.rs:90-92): persistence-load surface. A malicious snapshot can set `root = Distinction::new("ATTACKER_PICKED")`. Subsequent `validate_batch` synthesizes children whose parent is the forged ghost. Exp 9 confirmed this attack class.
- `LocalCausalAgent::update_local_root(&mut self, new_root: Distinction)` (validator.rs:201-203): trait surface. Any code with `&mut self` can swap the root.

**v2.0 closes this structurally** (`pub(crate) [u8;16]` private constructor). Until then, no in-code mitigation without theory drift.

## V3 — Unbounded `data` field DoS

`TransactionAction.data: Vec<u8>` (validator.rs:21) is folded byte-by-byte via `Canonicalizable::to_canonical_structure` (lines 25-42), calling `engine.synthesize` per byte.

- 1 MB tx → ~1M synthesize calls → ~2 seconds CPU at 500K ops/sec.
- Engine grows by ~N novel distinctions, **permanent** (append-only).
- Several large batches saturate engine memory.

No length cap on `data`.

## V4 — Rejection message amplifies attacker input

```rust
// validator.rs:115-119
BatchValidationResult::Rejected(format!(
    "previous_root mismatch: expected {}, got {}",
    self.local_root.id(), batch.previous_root
))
```

`batch.previous_root` is untrusted. A 1 MB string amplifies to ~1 MB allocation per rejection. If logged/gossiped, multiplies further.

## V5 — Atomic-failure semantics are LIES (theory-drift dimension)

Docstring claims (validator.rs:96-97):

> "Atomic Failure Semantics: If any transaction fails, entire batch is rejected. This preserves causal ordering integrity."

**Reality:** the validator's `local_root` and `expected_nonce` correctly do NOT update on `Rejected`. But the engine is mutated **mid-loop** via `engine.synthesize(&current_state, &tx_distinction)` (validator.rs:143). When tx N+1 fails validation:
- Validator state: rolled back ✓
- Engine state: contains all syntheses from tx 0..N ✗

The engine is append-only by design (CLAUDE.md principle 3). Rejected-batch prefix distinctions become unreferenced orphans, classified COLD by the compactor. This is "consistent with theory" — the engine is allowed to grow monotonically — but the validator's docstring overpromises.

**Fix options:**
1. Rephrase the contract — acknowledge "atomic" applies to validator-visible state, not engine state.
2. Pre-validate the entire batch before any `synthesize` call. Adds one pass; eliminates leakage.

## V6 — Two independent state fields

`local_root: Distinction` (validator.rs:71) and `expected_nonce: u64` (validator.rs:73) have no invariant binding. Setters:
- `from_root(root, expected_nonce)` — sets both, caller can pass inconsistent values
- `set_expected_nonce(nonce)` — sets only nonce
- `update_local_root(new_root)` — sets only root

No atomic `restore_state(root, nonce)`. Partial restore via persistence loader is legal and silent.

Worse: process kill mid-batch leaves engine ahead of validator's persisted root.

## V7 — Phantom parent inheritance

Per-byte canonicalization at lines 29, 35 routes through `ByteMapping::map_byte_to_distinction` (primitives.rs:64-71) which returns IDs from a cache built against a throwaway engine. Subsequent `engine.synthesize(&acc, &byte_d)` synthesizes a real result distinction over phantom-id parents. **Inherited from TODO #3.**

## v2.0 migration list (compile-time blockers when `[u8;16]` + `pub(crate)` lands)

1. `ConsensusValidator::from_root(root: Distinction, ...)` — must take an engine reference and verify `engine.get_distinction_by_id(root.id())` returns `Some`. Add `RestoreError::UnknownRoot`.
2. `LocalCausalAgent::update_local_root` — either remove from public trait or require engine reference.
3. `from_root + set_expected_nonce` → single `restore_state(engine, root_id, nonce)` with joint validation.

## Recommended actions

1. **Fix docstring** (v1.2.x or v2.0): rephrase atomic-failure semantics to acknowledge engine-side leakage.
2. **Pre-validate batches** (v2.0): walk `batch.transactions` once to check nonce sequence BEFORE any `synthesize`. Eliminates V5.
3. **Bound rejection-message string** (v2.0): clip `previous_root` to first 64 chars in error message.
4. **Add missing tests:**
   - Foreign-root via `from_root`
   - Foreign-root via `update_local_root`
   - Mid-batch engine-leakage on rejection (assert engine growth on rejected batch with valid prefix)
   - Empty-data degenerate tx (two txs, same nonce, both empty → same id)
   - Oversized-data resource-budget assertion
   - Oversized-previous_root rejection format size
5. **Hard cap on `TransactionAction.data` length** (v2.0): refuse before fold. Doc the limit.

## Repro probe (drafted, not executed)

Full source for `experiments/qa/src/exp_validator_audit.rs` provided in agent transcript. Exercises all seven findings (oversized previous_root, oversized data, out-of-order nonces with engine leakage observation, empty distinctions, phantom-parent count, foreign Distinction via `from_root`, `set_expected_nonce` desync). Compiles against current public API; uses only re-exported types.

To execute (after plan mode lifted):
```bash
cd /Users/sawyerkent/Projects/koru-lambda-core/experiments/qa
cargo build --release
./target/release/exp_validator_audit
```

## Bottom line

`validator.rs` is the **second-largest theory-drift surface** in the codebase (after compactor). The append-only invariant is preserved structurally, but the public API's atomic-failure docstring overstates rollback guarantees, and three foreign-ID acceptance paths await v2.0's `pub(crate)` lockdown. The repro probe should be run as part of the Section 1 (FIX) work before v2.0 lands.
