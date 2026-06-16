# Checklist — Path to Theory-Aligned, Audited, Validated

**Integration branch:** `research/warroom-experiments`
**Base:** koru-lambda-core 1.2.0 (`src/` unmodified at start)
**Evidence basis:** 17 experiments across `experiments/{runner,qa,rust}/`, summarized in `experiments/findings/`.
**Target release:** 2.0.0 (single cut, no intermediate 1.3)

Status legend: `[ ]` not done · `[~]` partial · `[x]` done

## Tier 0 — SHOW-STOPPERS (demonstrated, must-not-ship-to-a-network)

Two consensus-correctness bugs CONFIRMED by Phase 1.5 probes that **break consensus determinism or cryptographic attribution**. Cannot ship at any version on a network. See `PHASE_1_5_RESULTS.md` Section 5.

1. **N6** — `BatchCommitment::compute` does not hash `leader_id` → cryptographic attribution forgeable. Same batch + nonce + epoch + different leader_id → byte-identical commitment_hash. *(evidence: `run_log/audit_network_commitment_unbound.log`)*
2. **N5** — `NetworkAction::BatchProposed` truncates `previous_root` to first 8 bytes → two distinct chain heads with shared hex prefix produce identical action distinctions. **Breaks consensus determinism: two valid forks can converge.** *(evidence: `run_log/audit_network_foreign_peers.log` D)*

## Tier 1 — Critical bugs, demonstrated, fix in v2.0 cohort

V5 was initially flagged Tier 0 after the probe demonstration but on review belongs here. The engine grows on rejected batches but does so *deterministically* — all nodes leak identically, so consensus is not broken. Still a real DoS amplifier + validator-engine consistency issue that must be fixed.

1. **V5** — Validator pre-failure syntheses leak into engine despite "atomic" rejection (4 distinctions per pre-failure tx demonstrated in a 3-tx batch). *(evidence: `run_log/exp_validator_audit.log` C)*. Fix: pre-validate batches before any `synthesize` call.

## Working agreements

- **All sub-work branches off `research/warroom-experiments` and merges back into it.** Naming: `fix/*`, `audit/*`, `validate/*`, `impl/*`. The integration branch eventually merges to `dev` as one v2.0 PR.
- **`Cargo.toml` stays at `1.2.0` throughout.** The final commit on the integration branch (right before the PR to `dev`) is the single bump to `2.0.0`. No version edits in any sub-branch.
- **`CHANGELOG.md` grows during the work**, not at the end. Each sub-PR appends to an `## Unreleased` section. The final commit renames it to `## 2.0.0` with the date.
- **Phase 1 (Sections 3 + 4) runs first**, in parallel, before any `src/` edits. We don't refactor on top of unaudited subsystems.
- **No defensive runtime checks (length validation, existence checks) on engine inputs.** Foreign-ID bugs close structurally via the `pub(crate)` constructor change. Theory-pure stays theory-pure.

## Execution order

1. **Phase 1 — Audit + Validate** — **COMPLETE**
2. **Phase 1.5 — Empirical follow-up** — **COMPLETE**
3. **Phase 4 — Settle Section 5 decisions** — **COMPLETE** (all 9 locked; details in Section 5 below)
4. **Phase 2 — Quick wins** — **COMPLETE** (commits `727f7f9`, `7265bfd`, `6b247d3`, `36d4cdd` merged in `00fcdb3`)
5. **Phase 3 — ByteMapping fix** — **COMPLETE** (commit `86747db` merged in `58a7ff5`; Phase 1.5 probe re-run confirms phantoms 253 → 0)
6. **Phase 6 — Implement v2.0** — **IN PROGRESS** (1 of 11 sub-branches merged: `529ae5e` foundation)
7. **Phase 7 — `SECURITY.md` + CHANGELOG finalization + Cargo.toml bump 1.2.0 → 2.0.0 + integration PR to `dev`**
8. **Phase 8 — Consumer migration** (ALIS, koru-protocol)

---

## Section 1 — FIX (confirmed bugs and drift, evidence in hand)

### 1.1 Live bugs in engine core
- [x] **ByteMapping phantom parents** — `primitives.rs:26–38` builds cache against a throwaway engine. Byte-derived IDs exist in relationships but not in `all_distinctions`. Violates `r = 2d − 3` semantically. *(Exp 5, qa)* — DONE in commit `86747db` (Phase 3). Static cache + throwaway engine removed; `map_byte_to_distinction` now folds each byte through the calling engine, registering the full chain. Phase 1.5 probe re-run confirms phantom count 253 → 0; distinction_count = unique_parent_ids. 103 tests still pass.
- [x] **Foreign-ID acceptance** — public `Distinction::new(String)` (`engine.rs:13`) lets external callers mint IDs. *(Exp 9, qa)* — DONE in Phase 6 sub-branch #1 (merge `529ae5e`, foundation step 7). `Distinction` field is now `pub(crate)`; no public constructor exists; `Distinction::new(String)` removed entirely. Foreign-ID poisoning closed structurally at compile time. V1, N3, N4 (in addition to this engine-level item) all closed by the same change — see Section 2.1.
- [x] **Snapshot tearing** — `get_state_snapshot` (`engine.rs:154–156`) does two independent DashMap iterations. *(Exp 6, qa)* — DONE in commit `36d4cdd` (Phase 2). Renamed to `get_state_snapshot_unsynchronized`; tearing semantics documented in docstring; 8 call sites updated across src/, tests/, experiments/qa/.

### 1.2 Performance / cleanup (no theory impact)
- [x] **`format!("{:x}", digest)` → `hex::encode(digest)`** in `engine.rs` hot path. *(Exp 15, rust)* — DONE in commit `6b247d3` (Phase 2). 1 LOC swap in `synthesize()`; `hex = "0.4"` added as direct dep; output byte-identical; all 103 tests pass.

### 1.3 TODO.md design errors (must be corrected before implementation)
- [ ] **TODO #1 API shape** — current proposal returns `Vec`, probes via `synthesize()`. *(Exp 4 + Exp 8)*
  - `children_of` must return `impl Iterator<Item = Distinction>`. d0 at 1M has 102K children; Vec clone costs 13.3 ms.
  - Reverse index must live **inside** the engine. External index races produce orphans (Exp 8: 37 orphans / 20K sweeps).
  - `parents_of` cannot use the "check `synthesize(d, other) == known_child`" trick — synthesize has side effects. Use a forward index `DashMap<Distinction, (Distinction, Distinction)>` populated at synth time.
  - Underlying child storage: `Vec<Distinction>` (DashSet is 5–60× slower; append-only already prevents duplicates).
  - Cache degree separately in `DashMap<Distinction, AtomicUsize>` for O(1) reads.
- [ ] **TODO #2 log backend** — current proposal is `RwLock<Vec<(String, String)>>`. *(Exp 3)*
  - 26% throughput regression at 8 threads. Mutex is worse (52.7%).
  - Replace with `crossbeam::queue::SegQueue<(Distinction, Distinction)>` (7% at 8 threads, scales with cores).
  - Order-independence of replay confirmed (Exp 7, Exp 12), so SegQueue's weaker ordering is safe.
  - DECISION 5.7: push canonical `(min, max)` tuples (not raw `(parent_a, parent_b)`). Enables future Merkle-over-log and cross-peer log diffing without rewriting persisted logs. Reuse `engine.rs::synthesize`'s existing min/max canonicalization at push site.
- [ ] **TODO #2 memory escape hatch** — 1M synths = 81 MB resident log; worst case ~176 B/entry. At 10M = ~1.7 GB.
  - Add `DistinctionEngine::without_log()` constructor or a `log` feature flag.

### 1.4 CLAUDE.md numerical drift (documentation) — DONE in commit `7265bfd` (Phase 2)
- [x] Memory: `~656 bytes` → `629 B measured at 1M (live heap, dhat)`. *(Exp 1)*
- [x] Throughput single-thread: `500K–900K/s` → `425–540K/s depending on chain depth`. *(Exp 10)*
- [x] Throughput 8-thread: `1.85M, 3.5×` → `2.6M, 4.8× on M3 Pro`. *(Exp 10)*
- [x] Replay: `714K ops/sec` → `450K ordered / 367K shuffled on M3 Pro`. *(Exp 7)*
- [x] Snapshot tearing: `16.5%` → `rare (~0.1%) but avalanche-sized when it occurs`. *(Exp 6)*
- [x] v2.0 framing: `clone elimination` → `8× memory density`. Clone is 1.6% of synth cost. *(Exp 11, 16)*
- [x] Compactor: "non-destructive" reworded to "all compactor mutations are append-only `synthesize` calls" per DECISION 5.2. *(audit/compactor)*
- [x] Test count: `114 tests` → `103 tests` (baseline 2026-06-11). Plus note that `--features wasm` host tests fail/panic per W13.

### 1.5 Consensus-correctness bugs (Phase 1 audit) — MUST NOT SHIP TO A NETWORK
- [ ] **`BatchCommitment::compute` does NOT hash `leader_id`** — leader attribution is forgeable. *(audit/network N6, audit/ffi F7, audit/wasm W10)* — EVIDENCE: `run_log/audit_network_commitment_unbound.log` (honest/forged `commitment_hash` identical despite `leader_id` swap "alice"↔"EVE")
  - Fix: include `leader_id.as_bytes()` in the SHA256 hasher (commitment.rs:46-58). Trivial.
- [ ] **`NetworkAction::BatchProposed` truncates `previous_root` to first 8 bytes** — two different chain heads sharing 8-byte hex prefix produce identical action distinctions (causal-chain collision). *(audit/network N5)* — EVIDENCE: `run_log/audit_network_foreign_peers.log` D (`deadbeefAAAA…` and `deadbeefBBBB…` both produced action_id `11e9fd90…`)
  - Fix: drop `.take(8)` (network.rs:73-78); hash full string or validate as 64-hex-char SHA256.
- [ ] **Validator atomic-failure semantics are misstated** — engine retains all syntheses from the rejected batch's valid prefix; only validator state rolls back. *(audit/validator V5; severity reverted MEDIUM→HIGH→HIGH-but-not-Tier-0 after correction — engine leakage is deterministic across nodes so consensus is preserved; still a DoS amplifier)* — EVIDENCE: `run_log/exp_validator_audit.log` C (3-tx out-of-order batch rejected; 4 distinctions leaked into engine)
  - Fix (DECISION 5.3): pre-validate batches. Walk `batch.transactions` once to verify nonce sequence + `previous_root` BEFORE any `engine.synthesize` call. ~30 LOC. Eliminates leakage; makes the "atomic" docstring true.

### 1.6 Consensus hardening — DONE in Phase 6 sub-branch #7
- [x] Empty peer-id ("") collapses to `engine.d0()` — primordial impersonation. *(audit/network N2)* — EVIDENCE: `run_log/audit_network_foreign_peers.log` B → re-run post-fix shows N2 CLOSED (both empty-id calls return `Err`).
  - DONE: `PeerIdentity::new` now returns `Result<Self, String>` and rejects `id.is_empty()`.
- [x] Unbounded peer-id length — 1MB peer-id = ~1.85s CPU + 1M permanent distinctions per join. *(audit/network N1)* — EVIDENCE: re-run shows N1 CLOSED — 1 MB id rejected in 5.9 µs with engine distinction count unchanged.
  - DONE: `MAX_PEER_ID_LEN = 64`; ids beyond cap return `Err` before any synth.
- [x] Unbounded `TransactionAction.data` — 100K bytes = 149 ms + 100K permanent distinctions per tx. *(audit/validator V3)*
  - DONE: `MAX_TX_DATA_BYTES = 4096` checked in pre-validation pass alongside nonce sequence; oversized txs reject without any engine mutation. New test: `validate_batch_rejects_oversized_data_without_leaking` (+ boundary `validate_batch_accepts_data_at_cap_boundary`).
- [x] Rejection message amplifies attacker input — full untrusted `previous_root` copied into `format!`. *(audit/validator V4)*
  - DONE: `previous_root` clipped to `PREVIOUS_ROOT_DISPLAY_PREFIX = 64` chars + `…(truncated)` suffix. New test: `validate_batch_clips_oversized_previous_root_in_error` (rejection reason < 1 KiB on a 1 MB attacker input).
- [x] Validator-set dedupe mismatch — `join_peer` dedupes on `peer.id` (String), leader election hashes `peer.distinction_id()`. *(audit/network N7)*
  - DONE: `join_peer` now dedupes on the joint `(id, distinction)` key — same basis as the leader hash. New test: `join_peer_dedupes_on_joint_id_and_distinction`.
- [x] `pending_commitments` unbounded growth on never-finalized proposals. *(audit/network N11)*
  - DONE: `pending_commitments` is a bounded `LruCache<[u8; 32], BatchCommitment>` (`MAX_PENDING_COMMITMENTS = 256`). `advance_epoch` clears the set (commitments bind `epoch` in their hash). New tests: `pending_commitments_bounded_by_lru_cap`, `advance_epoch_clears_pending_commitments`.
- [x] **`ConsensusValidator` has no joint invariant between `local_root` and `expected_nonce`** — `set_expected_nonce(100)` then submitting a nonce-100 batch against the genesis root is ACCEPTED. *(audit/validator V6)*
  - DONE: removed `from_root` + `set_expected_nonce` from the public API. Added atomic `ConsensusValidator::restore_state(engine, root_id, expected_nonce) -> Result<Self, String>` that refuses fabricated roots (`engine.degree(&root_id) == 0`). Network wrapper renamed to `restore_consensus_validator_state(engine, root_id, nonce) -> Result<(), String>`; FFI surface renamed to `koru_agent_restore_state(agent, engine, root_hex, nonce)`. New tests: `restore_state_round_trips_root_and_nonce`, `restore_state_rejects_fabricated_root`, `restore_consensus_validator_state_rejects_fabricated_root`, `test_ffi_agent_restore_state_at_genesis`, `test_ffi_agent_restore_state_rejects_fabricated_root`.
- [x] **Empty `TransactionAction.data` produces a fixed, degenerate tx-distinction** — every empty-data tx with the same nonce collides on id `6bab8d5b...`. *(audit/validator V8)*
  - DONE: documented as by-design on `TransactionAction` (Recommendation (a) — it's correct content-addressing; consumers should not rely on tx-id uniqueness for txs with identical `(nonce, data)`).

### 1.7 FFI hardening — DONE in Phase 6 sub-branch #8
- [x] **Panic safety** — no `catch_unwind`, no `panic = "abort"`, unwinding across `extern "C"` is UB or hard abort. *(audit/ffi F1, F3)*
  - DONE: `[profile.release] panic = "abort"` added to `Cargo.toml`. Test + bench profiles continue to unwind (Cargo ignores `panic` for those profiles by design). Crate-level FFI doc now documents the abort-on-panic contract.
- [x] **TOCTOU on `*mut KoruAgent`** — concurrent calls = `&mut` aliasing = UB. Header doc lies about thread safety. *(audit/ffi F2, F8)*
  - DONE: `koru_agent_new` allocates `Box<Mutex<NetworkAgent>>`; `koru_validator_new` allocates `Box<Mutex<ConsensusValidator>>`. Every FFI entry point that needs the inner state takes the mutex via `(*(ptr as *mut FfiAgent)).lock().expect(...)`. Crate-level header doc rewritten to state the new contract honestly. New test `test_ffi_agent_concurrent_calls_serialize` (8 threads × 500 joins through one handle) demonstrates closure.
- [x] **Opaque types collapse to `c_void`** — type confusion silently accepted. *(audit/ffi F4)*
  - DONE: `KoruEngine` / `KoruAgent` / `KoruValidator` are now `#[repr(C)] pub struct { _private: [u8; 0] }`. Generated `target/koru.h` declares each as a distinct typedef. C compilers now reject pointer-type mismatches across the FFI surface.
- [x] cbindgen `include` list omits 18/22 functions from generated header. *(audit/ffi F5)*
  - DONE: `include` list removed from `cbindgen.toml`. The redundant `prefix = "koru_"` was also dropped — types now emit as `KoruEngine`/`KoruAgent`/`KoruValidator` instead of the doubled `koru_KoruEngine`. All 22 entry points now appear in the generated header.
- [x] FFI Arc-bookkeeping comment is wrong; pattern works only by coincidence. *(audit/ffi F6)*
  - DONE: introduced `borrow_engine(*const KoruEngine) -> ManuallyDrop<Arc<DistinctionEngine>>` helper. Every engine-borrowing entry point uses this; the `Arc::from_raw + Arc::into_raw` dance is gone. Panic safety: with `panic = "abort"` the strong count cannot leak on panic; under the test profile (unwind), `ManuallyDrop` still cannot double-drop or accidentally decrement.
- [x] FFI `check_commitment` hash parameter is decorative (never inspected). *(audit/ffi F7)*
  - DONE (FFI half): `koru_agent_check_commitment` now takes `leader_id: *const c_char` + `batch_size: u64` and constructs a real `BatchCommitment` (no more Frankenstein). Empty `leader_id` is rejected with `KORU_ERROR_INVALID_DATA`. The light-node `verify(nonce, epoch)` remains metadata-only by design (full hash verification requires the batch payload); the docstring is updated to state that contract honestly.
- [x] `batch_len: usize` unbounded — UB if > `isize::MAX`. *(audit/ffi F9)*
  - DONE: `koru_agent_propose_commitment` and `koru_agent_finalize_batch` reject `batch_len > isize::MAX as usize` before any `slice::from_raw_parts`. New test `test_ffi_propose_commitment_rejects_oversized_batch_len`.

### 1.8 WASM wire-format — DECISION 5.5: bytes-on-wire canonical
- [ ] **Kill `id_to_bytes` heuristic entirely.** Expose `Distinction::as_bytes() -> &[u8; 16]`; all WASM-facing IDs are `Uint8Array` of length 16. *(audit/wasm W1)*
- [ ] **Primordials get real 16-byte IDs** (`[0u8; 16]` and `[1u8; 16]` or domain-separated hashes; small open subdecision in v2.0 implementation). Special-case dies. *(audit/wasm W2)* — EVIDENCE: `experiments/findings/baseline.md` (`test_wasm_engine_primordial_consistency` FAILS in baseline)
- [ ] **`WasmEngine::synthesize` signature becomes `synthesize(&[u8], &[u8]) -> Result<Vec<u8>, JsValue>`.** Symmetric inputs/outputs. *(audit/wasm W9)*
- [ ] **Add `Distinction::to_hex(&self) -> String` and `Distinction::from_hex(s: &str) -> Result<Self, ParseError>`** in a separate hex module. Expose via WASM as `idToHex(arr: &[u8]) -> String` and `idFromHex(s: &str) -> Result<Vec<u8>, JsValue>` for JS display/parsing.
- [ ] **`impl Display for Distinction` uses `to_hex`.** `impl Debug` too.
- [ ] Remove `id_to_bytes` silent UTF-8 fallback (the function disappears entirely). *(audit/wasm W4)*
- [ ] WASM `checkCommitment` builds Frankenstein `BatchCommitment { leader_id: "", batch_size: 0 }`. *(audit/wasm W10)*
  - Fix: add proper params, OR narrower API in `NetworkAgent`.
- [ ] Add `console_error_panic_hook` under existing `wasm` feature; call `set_once()` from `#[wasm_bindgen(start)]`. *(audit/wasm W5; DECISION 5.6: always-on)*
- [ ] WASM tests are `#[test]` not `#[wasm_bindgen_test]` — never hit a WASM runtime. *(audit/wasm W13)*
  - Fix: convert. Verify host tests pass first under `--features wasm`.

### 1.9 Compactor cleanup (Phase 1 audit)
- [ ] Compactor magic constants: `hot_threshold: 3`, `warm = hot/2`, unverified "26.6×" claim. *(audit/compactor)*
  - Fix: derive thresholds from observed degree distribution OR drop HOT/WARM/COLD trichotomy for raw SIS. Remove "26.6×" until reproduced.
- [ ] `compaction_count` double-increment (`compact()` and `synthesize_action`). *(audit/compactor)*
  - Fix: remove from one site.
- [ ] Compactor "archives its own work products" — `synthesize_action` re-runs `calculate_sis` after writing compaction-event distinctions; new low-degree nodes classified COLD. *(audit/compactor)*
  - Fix: skip recompute after own synth, OR exclude compaction events from re-classification.
- [ ] Replace compactor's `calculate_sis` body with `engine.degree(d)` once 2.2 traversal API lands.

### 1.10 Architectural cleanup — DONE in Phase 6 sub-branch #2 (merge `b45c434`)
- [x] **Delete `ParallelBatchProcessor`** (~250 LOC removed). Misnamed Sequential body, dead `worker_count` field, single-variant enum. *(audit/parallel)*
  - Consumer grep confirmed: zero `ParallelBatchProcessor` use in koru/koru-protocol or koru/koru-node; ALIS does not use it either. Safe to delete.
- [x] `ParallelSynthesizer` renamed to `BatchSynthesizer`; return type `Vec<Option<Distinction>>` replaces v1.2.0 silent empty-string fallback. *(audit/parallel)*

### 1.11 Phase 1 empirical follow-up — COMPLETE

All 8 binaries persisted clippy-clean AND run. Logs at `experiments/findings/run_log/`. Consolidated results at `experiments/findings/PHASE_1_5_RESULTS.md`.

- [x] Persisted + clippy-clean: `exp18_coding_law`, `exp19_mediated_self_reference`, `exp20_fold_law`, `exp21_cross_engine_determinism`
- [x] Persisted + clippy-clean: `exp_validator_audit`, `audit_network_foreign_peers`, `audit_network_commitment_unbound`, `audit_network_concurrency`
- [x] Registered in respective `Cargo.toml`
- [x] **All 8 binaries run** — total ~60 sec; all exit 0
- [x] **All 4 validation reports CONFIRMED with measured numbers** (coding_law rho ∈ {0.980, 0.989, 0.991}; mediated 0 collisions @ 10K depth; fold_law 128× ratio; cross-engine 0 divergences across 5 phases)
- [x] **All 13 probeable HIGH+ findings CONFIRMED** (V1, V3, V4, V5, V6, V7, V8 from validator; N1, N2, N3, N4, N5, N6 from network)
- [x] Updated validation reports + audit reports with EVIDENCE annotations (in CHECKLIST 1.5–1.10 inline)

### 1.12 Baseline measurement (COMPLETE — see `experiments/findings/baseline.md`)

- [x] `cargo test --release` — **103 tests pass, 0 fail** (CLAUDE.md claims 114 — drift)
- [x] `cargo clippy --all-targets --release` — **CLEAN** on main crate (no features)
- [x] `cargo build --features wasm --release` — **library builds OK**
- [x] `cargo test --features wasm --release --lib wasm` — **2 confirmed failures:**
  - `test_wasm_engine_primordial_consistency` **FAILS** — confirms audit W2 (id_to_bytes heuristic)
  - `test_wasm_validator_atomic_failure` **PANICS** on host — confirms audit W13 (host-side `#[test]` vs `#[wasm_bindgen_test]`)
- [x] `cargo clippy --features wasm --all-targets --release` — **FAILS TO COMPILE** `tests/falsification/wasm_consistency.rs` (18 errors: `[u8]` Display)
- [x] Baseline saved to `experiments/findings/baseline.md`

**New baseline-derived items:**
- [x] CLAUDE.md says "114 tests" but actual is 103. → DONE in commit `7265bfd` (Phase 2). Per Phase 6 #1, count is now **117** tests post-foundation.
- [ ] `tests/falsification/wasm_consistency.rs` is broken under `--features wasm` — pre-existing test code that expects a wire format the wasm wrapper doesn't deliver. Fix as part of Section 1.8 wasm work (sub-branch #10).
- [ ] Pre-existing experiments (`exp01-12`) have ~20 clippy warnings (`type_complexity`, `manual_div_ceil`, `dead_code`). Either fix as cleanup or scope the "no warnings" promise to `src/` + `tests/`. Recommended: fix. Low priority but easy.

---

## Section 2 — IMPLEMENT (all v2.0, no intermediate release)

All items ship together in 2.0.0. The structural type changes and the additive APIs land in the same release so consumers migrate once.

### 2.1 Type-level changes (structural, theory-strengthening) — DONE in Phase 6 sub-branch #1 (merge `529ae5e`)
- [x] `Distinction([u8; 16])` — truncated SHA256 (first 16 bytes, MSB). Content addressing preserved. Copy + Eq + Hash + Display + Debug. Primordials `[0u8; 16]` and `[1, 0, ...]`. *(Exp 13: 0 collisions in 268M; foundation step 3)*
- [x] `pub(crate)` on the field + no public constructor — closes foreign-ID poisoning structurally. *(foundation step 7)*
- [x] `IdentityHasher` on internal DashMaps via `BuildHasherDefault<IdentityHasher>` — 17-bit rotation XOR accumulator (deeper than plan's "leading 8 bytes" — necessary for tuple keys because d0/d1 first bytes are pinned). 1M random SHA256 prefixes → 1M distinct buckets verified. *(foundation step 4)*
- [x] Hex serialization layer in `src/distinction_hex.rs` (231 LOC). `to_hex` / `from_hex` / `impl Display` / `impl Debug` / serde adapter for `#[serde(with = "distinction_hex")]`. Module is `pub` (consumers need to name the path). *(foundation steps 1-2)*
- [x] FFI: `*mut c_char` returns use `hex::encode(distinction.as_bytes())`. Public C signatures unchanged. Hex strings now 32 chars (was 64). *(foundation step 8)*

**Performance signals (Phase 6 #1 preliminary):**
- `exp10_throughput` 8-thread: 2.6M → 15.3M ops/s (5.9×)
- `test_batch_validation_performance`: 2979 → 7207 batches/s (2.4×)
- Test count: 103 → 117 (+14 new tests in distinction_hex + identity_hasher)

### 2.2 Additive APIs (new public surface)
- [ ] `degree(d: &Distinction) -> usize` — O(1) via `DashMap<Distinction, AtomicUsize>`.
- [ ] `parents_of(d: &Distinction) -> Option<(Distinction, Distinction)>` — forward index populated in synthesize.
- [ ] `children_of(d: &Distinction) -> impl Iterator<Item = Distinction>` — reverse index, in-engine.
- [ ] Streaming `calculate_sis` using the reverse index (removes the full-snapshot clone in compactor).

### 2.3 Synthesis log
- [ ] Append-only log on `SegQueue<(Distinction, Distinction)>` (bytes from day one, not retyped from String). DECISION 5.7: push canonical `(min, max)` tuples (reuse `engine.rs::synthesize`'s existing canonicalization).
- [ ] `DistinctionEngine::without_log()` constructor or `log` feature flag for memory-sensitive consumers.
- [ ] Serde derives on log entry type (`bincode` round-trip 25 ms / 14 ms at 1M). *(Exp 7)*

### 2.4 Invariant tripwire — DONE in Phase 6 sub-branch #3 (merge `550125a`)
- [x] Plan was `debug_assert!(r == 2*d - 3)` inside `synthesize()` on the novel path. Implementation found it fundamentally racy under concurrent synthesis (8-thread `test_parallel_synthesizer` triggers it on transient mid-synthesize states between `insert distinction` and the two `add_relationship` calls). Refactored as quiescent-mode `pub fn check_structural_invariant(&self) -> bool` — callers from tests / diagnostics can invoke when no writers are active. Three new unit tests assert the invariant on genesis, 100-step chain, saturated repeats. The release-mode correctness of the invariant under concurrent synthesis is verified at scale by Exp 2 (5M synths, zero deviations) and `tests/falsification/structural_coherence.rs`.

### 2.5 Consumer coordination
- [ ] ALIS (`/Users/sawyerkent/Projects/alis-ai/`) — pins `koru-lambda-core = "1.2"`. Bump to 2.0 after engine release.
- [ ] koru-protocol (`/Users/sawyerkent/Projects/koru/`) — same pin, same bump. Check JSON wire formats for embedded distinction IDs.

---

## Section 3 — AUDIT (Phase 1 complete — findings folded into Sections 1.4–1.10)

All 7 subsystems audited. Reports in `experiments/findings/audits/`. Top-level summary in `experiments/findings/AUDIT_SUMMARY.md`.

- [x] **`src/subsystems/compactor.rs`** — see 1.4, 1.9. Largest theory drift in codebase; non-destructive claim is wrong; magic constants; archives own work products.
- [x] **`src/subsystems/validator.rs`** — see 1.5, 1.6. Atomic-failure docstring lies; foreign-Distinction acceptance; unbounded DoS.
- [x] **`src/subsystems/network.rs`** — see 1.5, 1.6. 5 foreign-ID vectors; **leader_id not hashed (forgeable)**; **previous_root 8-byte truncation (causal collision)**; primordial impersonation.
- [x] **`src/subsystems/commitment.rs`** — CLEAN. No drift, one v2.0 type break in `commitment_root_id`.
- [x] **`src/subsystems/parallel.rs`** — see 1.10. Delete `ParallelBatchProcessor`; rename `ParallelSynthesizer`.
- [x] **`src/ffi.rs`** — see 1.7. 2 HIGH (panic safety, TOCTOU), both fixable in ~5 LOC. 0 leak paths.
- [x] **`src/wasm.rs`** — see 1.8. 1 CRITICAL wire-format heuristic. 0 unsafe, theory-clean.

---

## Section 4 — VALIDATE — COMPLETE

All four CLAUDE.md theory claims now have measured numbers (replacing predictions). Full results in `experiments/findings/PHASE_1_5_RESULTS.md` Section 3.

- [x] **Coding Law: degree ↔ usage frequency rho=0.99** — CONFIRMED. Measured rho = 0.980 (N=1K), 0.989 (N=10K), **0.991 (N=100K, M=5M)**. Trend approaches 0.99 as predicted. Throughput 508K–833K/s confirms CLAUDE.md band.
- [x] **Mediated self-reference → infinite novelty** — CONFIRMED for both T2 (varying obs) AND T3 (constant obs). 10001/10001 unique distinctions at depth 10K. T1 direct produces 0 new distinctions (irreflexivity).
- [x] **Fold Law mechanism (depth ≤ 8)** — CONFIRMED. Control deg(d0) = 1 after 10K Fibonacci syntheses; treatment deg(d0) = 128 after 256 byte folds. **Ratio = 128× (predicted ≥40, exceeded).** Mechanism isolated: ByteMapping's 8-step alternating fold, not general content concentration.
- [x] **Cross-engine determinism on arbitrary chains** — CONFIRMED on all 5 phases (baseline, history adversary, concurrent ×8 threads, closed-form predict_id at depth 20K, three-engine majority). Phase 4's closed-form comparison vs pure SHA256 prediction is the strongest variant.

---

## Section 5 — DECIDED (all 9 locked)

All Section 5 decisions resolved 2026-06-11. Frame: no active users today; everything bundles into v2.0 (no intermediate release); no version bump until the entire CHECKLIST is closed; "done right in every aspect" governs each call.

### 5.1 — Release shape: bundle everything into v2.0 ✓

- ~~v1.2.x patch vs v2.0 bundle for Tier 0 bugs~~ → **bundle into v2.0**
- Rationale: no active users on a real network; no urgency for a separate security patch. ALIS + koru-protocol coordinate one breaking change instead of two.
- Implication: `Cargo.toml` stays at `1.2.0` throughout. Bump to `2.0.0` is the final commit before the integration PR. CHANGELOG grows during work in an `## Unreleased` section.

### 5.2 — Compactor: accept the design, fix everything around it ✓

- ~~Refactor compactor to truly read-only~~ → **accept the design; fix docs + magic constants + 26.6× claim + double-counted compaction_count + recursive synthesize_action + dead archived_ids field**
- Rationale: compactor mutations via `engine.synthesize` are theory-clean (append-only, axioms preserved). Compaction events become first-class distinctions. Refactoring would LOSE that property. The actual bugs are in Section 1.9, all of which get fixed regardless.
- Implication: CLAUDE.md "Compactor is non-destructive" wording → "All compactor mutations are append-only `synthesize` calls; the engine record is the canonical compaction history." Section 1.9 items execute as listed.

### 5.3 — Validator V5: pre-validate batches ✓

- ~~Doc fix only~~ → **pre-validate batches; eliminate engine-side leakage**
- Rationale: 4 distinctions per pre-failure tx (Phase 1.5 demonstrated). Doc-fix says "the bug is fine because it's theoretically consistent"; that's dishonest. "Atomic" must mean atomic. ~30 LOC.
- Implication: Section 1.5 V5 fix is a code change, not a doc change. Validator gets a new pre-validation pass before any `synthesize` call.

### 5.4 — FFI thread-safety: internal Mutex ✓

- ~~Doc paragraph only~~ → **wrap agents/validators in internal `Mutex` inside the FFI boundary**
- Rationale: trusting Go/Kotlin/Swift embedders to externally lock something they can't see is exactly the kind of "users will be careful" trap the rest of the project rejects. Engine stays DashMap-safe; agents get a Mutex wrapper. Single-digit ns hot path cost. Header doc also updated for honesty.
- Implication: Section 1.7 F2 fix is the heavier ~30 LOC option, not the doc-only option.

### 5.5 — Bytes-on-wire canonical + first-class hex serialization helpers ✓

- ~~Hex-on-wire everywhere~~ → **bytes canonical everywhere; explicit hex serialization at human-facing boundaries only**
- Rationale: the Distinction *is* bytes; hex is a display format. Forcing hex through the substrate permanently couples the engine to a particular human-readable representation that's not load-bearing on the theory. With explicit serialization helpers (`Distinction::to_hex`, `Distinction::from_hex`, `impl Display`), JSON gets hex strings via `#[serde(with = "distinction_hex")]`; WASM gets `Uint8Array` + JS-exposed `idToHex` / `idFromHex` helpers; FFI gets `[u8; 16]` for binary surfaces and `*mut c_char` (hex) for the existing logging surfaces.
- Architecture (locked):
  - **DashMap keys / synthesis log / binary persistence:** bytes
  - **FFI binary surfaces:** `[u8; 16]` directly
  - **FFI human surfaces (`koru_agent_state_root`):** `*mut c_char` (hex via `hex::encode`)
  - **WASM:** `Uint8Array` for IDs + exposed `idToHex(arr)` and `idFromHex(str)` helpers
  - **JSON wire (blockchain payloads):** hex strings via serde adapter
- Implication: Section 1.8 collapses. WASM `id_to_bytes` heuristic dies completely (not "fixed at 32"). Primordials get real `[0u8; 16]` and `[1u8; 16]` IDs. `WasmEngine::synthesize` signature becomes `synthesize(&[u8], &[u8])`. `Distinction::to_hex` / `from_hex` / `Display` land in the hex module (one file, ~30 LOC, separate from the engine).

### 5.6 — WASM panic hook: always-on under `wasm` feature ✓

- ~~Feature-gated sub-feature~~ → **always-on under existing `wasm` feature**
- Rationale: 5 KB bundle cost vs huge diagnostic value. Anyone using WASM wants readable panics in JS. Feature-gating diagnostic infrastructure is a stale habit.
- Implication: add `console_error_panic_hook` as optional dep under `wasm` feature; call `set_once()` from `#[wasm_bindgen(start)]`.

### 5.7 — Synthesis log: canonical `(min, max)` tuples ✓

- ~~Raw `(parent_a, parent_b)` tuples~~ → **canonical `(min, max)` tuples**
- Rationale: Exp 12 showed both work for replay. Canonical also enables future Merkle-over-log and cross-peer log diffing without rewriting persisted logs. Cost is one comparison per push. Decide once, decide right.
- Implication: Section 2.3 log entry type stores `(Distinction, Distinction)` already; the canonicalization happens at push site (`engine.rs::synthesize` already canonicalizes for hashing — reuse the same min/max).

### 5.8 — Snapshot API: rename to `_unsynchronized`; defer `_quiesced(barrier)` ✓

- ~~Document at call site~~ / ~~add `_quiesced(barrier)` variant~~ → **rename existing `get_state_snapshot` to `get_state_snapshot_unsynchronized`; defer the quiesced variant until a real consumer asks**
- Rationale: rename forces every caller to acknowledge the unsynchronized nature at compile time. Adding a `_quiesced` variant designs a barrier API for a consumer we don't have. YAGNI.
- Implication: Section 1.1 snapshot tearing item becomes a rename + docstring update. ~5 LOC.

### 5.9 — Publish `SECURITY.md` ✓

- ~~CHANGELOG mention only~~ → **publish `SECURITY.md`** describing N5 / N6 / V5 once v2.0 ships
- Rationale: standard practice when known security bugs ship in any version. ALIS + koru-protocol consumers need to know which version to pin without grepping CHANGELOGs.
- Implication: `SECURITY.md` lands as part of the v2.0 integration PR. Includes supported version table, vulnerability list (N5/N6/V5 affecting <=1.2.0), and reporting contact.

### Previously decided (record)

- ~~v1.3 vs v2.0 boundary~~ → single v2.0 cut. One migration for consumers.
- ~~Defensive runtime input validation on engine~~ → no (theory drift; closed by v2.0 `pub(crate)` structurally).

---

## Quick stats

| Category | Items | Status |
|---|---|---|
| Section 1.1–1.3 — Engine FIX | 14 | **3 of 14 done** (snapshot rename, ByteMapping fix, foreign-ID via pub(crate)) |
| Section 1.4 — CLAUDE.md doc drift | 8 | **complete** (Phase 2) |
| Section 1.5 — Consensus-correctness bugs (MUST NOT SHIP) | 3 | **complete** (Phase 6 sub-branch #6, merge `b0a641a`) |
| Section 1.6 — Consensus hardening | 8 | **complete** (Phase 6 sub-branch #7) |
| Section 1.7 — FFI hardening (incl. 2 HIGH) | 7 | **complete** (Phase 6 sub-branch #8) |
| Section 1.8 — WASM bytes-on-wire + helpers | 9 | not started (Phase 6 sub-branch #10) |
| Section 1.9 — Compactor cleanup | 4 | not started (Phase 6 sub-branch #9) |
| Section 1.10 — Architectural cleanup | 2 | **complete** (Phase 6 sub-branch #2, merge `b45c434`) |
| Section 1.11 — Phase 1 follow-up | 13 | **complete** |
| Section 1.12 — Baseline measurement | 4 | **complete** |
| Section 2.1 — Type-level changes | 5 | **complete** (Phase 6 sub-branch #1, merge `529ae5e`) |
| Section 2.2 — Traversal API | 4 | **complete** (Phase 6 sub-branch #5, merge `e648117`) |
| Section 2.3 — Synthesis log | 3 | **complete** (Phase 6 sub-branch #4, merge `91ee3b2`) |
| Section 2.4 — Invariant tripwire | 1 | **complete** (Phase 6 sub-branch #3, merge `550125a`) |
| Section 3 — AUDIT (Phase 1) | 7 subsystems | **complete** |
| Section 4 — VALIDATE | 4 claims | **complete** |
| Section 5 — DECIDE | 9 | **complete (all locked)** |

**Current test count:** 157 release (post Phase 6 #8; was 155 before, 103 at baseline). Clippy clean (incl. `--all-targets`). `Cargo.toml` at `1.2.0`.

**Phase 6 sub-branch tracker:**

| # | Sub-branch | Status |
|---|---|---|
| 1 | `impl/foundation-distinction-bytes` | ✅ merged `529ae5e` |
| 2 | `cleanup/parallel-batch-processor` | ✅ merged `b45c434` |
| 3 | `impl/invariant-tripwire` | ✅ merged `550125a` |
| 4 | `impl/synthesis-log` | ✅ merged `91ee3b2` |
| 5 | `impl/traversal-api` | ✅ merged `e648117` (also closed #1's 107s test regression → 0.02s) |
| 6 | `fix/tier-0-consensus-correctness` | ✅ merged `b0a641a` (N5, N6, V5 + F7 bonus all closed by re-run probes) |
| 7 | `fix/consensus-hardening` | ✅ on branch (ready to merge) — Section 1.6 closed; N1/N2 confirmed via `audit_network_foreign_peers` re-run |
| 8 | `fix/ffi-hardening` | ✅ on branch (ready to merge) — Section 1.7 closed; F7 FFI half landed |
| 9 | `cleanup/compactor` | ⏸ waits on #5 (uses traversal API) |
| 10 | `impl/wasm-bytes-on-wire` | ⬜ unblocked once WASM toolchain confirmed |
| 11 | `docs/changelog-finalize` | ⏸ last (after all others) |

**To call the project "completely theory-aligned, clean, high-quality, bug-free":**
- All of Section 1.1–1.10 must be closed.
- Section 2 must be implemented.
- `SECURITY.md` + CHANGELOG finalized.
- `Cargo.toml` bumped 1.2.0 → 2.0.0 as the last commit before integration PR.

## Evidence index

**Phase 1 (audits + validation designs):**
- `experiments/findings/AUDIT_SUMMARY.md` — top-level Phase 1 synthesis
- `experiments/findings/audits/compactor.md`
- `experiments/findings/audits/commitment.md`
- `experiments/findings/audits/validator.md`
- `experiments/findings/audits/network.md`
- `experiments/findings/audits/parallel.md`
- `experiments/findings/audits/ffi.md`
- `experiments/findings/audits/wasm.md`
- `experiments/findings/validations/coding_law.md`
- `experiments/findings/validations/mediated_self_reference.md`
- `experiments/findings/validations/fold_law.md`
- `experiments/findings/validations/cross_engine_determinism.md`

**Original warroom (17 experiments):**
- `experiments/findings/WARROOM_FINDINGS.md` — top-level synthesis
- `experiments/findings/SUMMARY_research_lead.md` — Exp 1, 2, 3, 7, 10, 11
- `experiments/findings/SUMMARY_qa_sentinel.md` — Exp 5, 6, 8, 9, 12
- `experiments/findings/SUMMARY_rust_craftsman.md` — Exp 4, 13, 14, 15, 16, 17
- Reproduction commands in each summary file.
