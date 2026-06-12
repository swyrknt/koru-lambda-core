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

1. **Phase 1 — Audit + Validate** (Sections 3 + 4 in parallel, read-only) — **complete (audits); validations drafted, not run**
2. **Phase 1.5 — Empirical follow-up** (Sections 1.11 + 1.12: persist drafted code, run experiments, capture baseline)
3. **Phase 2 — Quick wins** (Section 1.4 doc corrections, Section 1.2 hex swap, Section 1.1 snapshot doc)
4. **Phase 3 — ByteMapping fix** (Section 1.1 phantom fix, ~5 LOC additive)
5. **Phase 4 — Settle all Section 5 decisions** (9 items: 3 original + 6 from Phase 1 audits)
6. **Phase 5 — Consensus-correctness patch decision** (Section 1.5: ship as v1.2.1 security release, or bundle into v2.0?)
7. **Phase 6 — Implement v2.0** (Sections 1.6–1.10 + Section 2)
8. **Phase 7 — Version bump + CHANGELOG rename + integration PR to `dev`**
9. **Phase 8 — Consumer migration** (ALIS, koru-protocol)

---

## Section 1 — FIX (confirmed bugs and drift, evidence in hand)

### 1.1 Live bugs in engine core
- [ ] **ByteMapping phantom parents** — `primitives.rs:26–38` builds cache against a throwaway engine. Byte-derived IDs exist in relationships but not in `all_distinctions`. Violates `r = 2d − 3` semantically. *(Exp 5, qa)*
  - Fix: register the 8-step chain into the calling engine on first byte use.
  - Scope: ~5 LOC. Zero in-tree tests break (Exp 5).
- [ ] **Foreign-ID acceptance** — public `Distinction::new(String)` (`engine.rs:13`) lets external callers mint IDs. *(Exp 9, qa)*
  - 1MB ID → 7.3 ms/synth (225× DoS slowdown).
  - Cross-engine ID → silent phantom parent in receiver.
  - Empty / colon-containing strings accepted.
  - **Structural fix = TODO #6 (`pub(crate)`)**. Do not patch with runtime length checks (theory drift).
- [ ] **Snapshot tearing** — `get_state_snapshot` (`engine.rs:154–156`) does two independent DashMap iterations. *(Exp 6, qa)*
  - Real, but 0.06–0.08% rate (not the 16.5% CLAUDE.md claims).
  - When it tears, it tears avalanche-sized.
  - Fix: rename to `get_state_snapshot_unsynchronized()` and document, OR add quiesced variant. Decide before any persistence consumer ships.

### 1.2 Performance / cleanup (no theory impact)
- [ ] **`format!("{:x}", digest)` → `hex::encode(digest)`** in `engine.rs` hot path. *(Exp 15, rust)*
  - 213 ns → 113 ns. ~15% end-to-end synth speedup.
  - 2 LOC, additive, ship as 1.2.x patch.

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
- [ ] **TODO #2 memory escape hatch** — 1M synths = 81 MB resident log; worst case ~176 B/entry. At 10M = ~1.7 GB.
  - Add `DistinctionEngine::without_log()` constructor or a `log` feature flag.

### 1.4 CLAUDE.md numerical drift (documentation)
- [ ] Memory: `~656 bytes` → `629 B measured at 1M (live heap, dhat)`. *(Exp 1)*
- [ ] Throughput single-thread: `500K–900K/s` → `425–540K/s depending on chain depth`. *(Exp 10)*
- [ ] Throughput 8-thread: `1.85M, 3.5×` → `2.6M, 4.8× on M3 Pro`. *(Exp 10)*
- [ ] Replay: `714K ops/sec` → `450K ordered / 367K shuffled on M3 Pro`. *(Exp 7)*
- [ ] Snapshot tearing: `16.5%` → `rare (~0.1%) but avalanche-sized when it occurs`. *(Exp 6)*
- [ ] v2.0 framing: `clone elimination` → `8× memory density`. Clone is 1.6% of synth cost. *(Exp 11, 16)*
- [ ] Compactor: "non-destructive — classifies but never mutates the engine" is wrong; `new()` and `synthesize_action` mutate. *(audit/compactor)*

### 1.5 Consensus-correctness bugs (Phase 1 audit) — MUST NOT SHIP TO A NETWORK
- [ ] **`BatchCommitment::compute` does NOT hash `leader_id`** — leader attribution is forgeable. *(audit/network N6, audit/ffi F7, audit/wasm W10)* — EVIDENCE: `run_log/audit_network_commitment_unbound.log` (honest/forged `commitment_hash` identical despite `leader_id` swap "alice"↔"EVE")
  - Fix: include `leader_id.as_bytes()` in the SHA256 hasher (commitment.rs:46-58). Trivial.
- [ ] **`NetworkAction::BatchProposed` truncates `previous_root` to first 8 bytes** — two different chain heads sharing 8-byte hex prefix produce identical action distinctions (causal-chain collision). *(audit/network N5)* — EVIDENCE: `run_log/audit_network_foreign_peers.log` D (`deadbeefAAAA…` and `deadbeefBBBB…` both produced action_id `11e9fd90…`)
  - Fix: drop `.take(8)` (network.rs:73-78); hash full string or validate as 64-hex-char SHA256.
- [ ] **Validator atomic-failure semantics are misstated** — engine retains all syntheses from the rejected batch's valid prefix; only validator state rolls back. *(audit/validator V5; severity reverted MEDIUM→HIGH→HIGH-but-not-Tier-0 after correction — engine leakage is deterministic across nodes so consensus is preserved; still a DoS amplifier)* — EVIDENCE: `run_log/exp_validator_audit.log` C (3-tx out-of-order batch rejected; 4 distinctions leaked into engine)
  - Fix: pre-validate batches (walk `transactions` once before any `synthesize` call). Doc-only fix is insufficient given demonstrated leakage.

### 1.6 Consensus hardening (Phase 1 audit)
- [ ] Empty peer-id ("") collapses to `engine.d0()` — primordial impersonation. *(audit/network N2)* — EVIDENCE: `run_log/audit_network_foreign_peers.log` B (`peer("").distinction_id() == "0" == d0.id()`)
  - Fix: reject `id.is_empty()` in `PeerIdentity::new` (network.rs:30-39).
- [ ] Unbounded peer-id length — 1MB peer-id = ~1.85s CPU + 1M permanent distinctions per join. *(audit/network N1)* — EVIDENCE: `run_log/audit_network_foreign_peers.log` A (1MB peer-id → 1,000,002 distinctions in 1.85s)
  - Fix: cap peer-id length (suggest 64 bytes) in `PeerIdentity::new`.
- [ ] Unbounded `TransactionAction.data` — 100K bytes = 149 ms + 100K permanent distinctions per tx. *(audit/validator V3)* — EVIDENCE: `run_log/exp_validator_audit.log` B (100K bytes → 100,002 distinctions in 148.9ms)
  - Fix: length cap on `data` field before fold.
- [ ] Rejection message amplifies attacker input — full untrusted `previous_root` copied into `format!`. *(audit/validator V4)* — EVIDENCE: `run_log/exp_validator_audit.log` A (1MB previous_root → 1,000,102-byte rejection reason)
  - Fix: clip to first 64 chars in error message (validator.rs:115-119).
- [ ] Validator-set dedupe mismatch — `join_peer` dedupes on `peer.id` (String), leader election hashes `peer.distinction_id()`. *(audit/network N7)* — EVIDENCE: source-only (not exercised by Phase 1.5 probes; requires multi-peer election scenario)
  - Fix: dedupe on joint `(id, distinction_id)` key (network.rs:175).
- [ ] `pending_commitments` unbounded growth on never-finalized proposals. *(audit/network N11)* — EVIDENCE: source-only (requires multi-round commit flow)
  - Fix: size cap + TTL eviction.

### 1.7 FFI hardening (Phase 1 audit)
- [ ] **Panic safety** — no `catch_unwind`, no `panic = "abort"`, unwinding across `extern "C"` is UB or hard abort. *(audit/ffi F1, F3)* — EVIDENCE: NOT-PROBEABLE this round (requires synthetic panic injection); structural fix is single-line
  - Fix: add `panic = "abort"` to release and dev profiles in `Cargo.toml`. One line, eliminates F1+F3.
- [ ] **TOCTOU on `*mut KoruAgent`** — concurrent calls = `&mut` aliasing = UB. Header doc lies about thread safety. *(audit/ffi F2, F8)* — EVIDENCE: NOT-PROBEABLE this round (requires C-thread harness); related N8 pattern demonstrated via `run_log/audit_network_concurrency.log`
  - Fix (minimum): document the contract — engine is shared-thread-safe, agents/validators require exclusive access. One paragraph.
  - Fix (heavier): wrap agents in `Mutex<NetworkAgent>` inside FFI. ~30 LOC.
- [ ] **Opaque types collapse to `c_void`** — type confusion silently accepted. *(audit/ffi F4)*
  - Fix: `#[repr(C)] pub struct KoruEngine { _private: [u8; 0] }` (same for agent, validator). cbindgen emits distinct typedefs; C mismatches become C compile errors.
- [ ] cbindgen `include` list omits 18/22 functions from generated header. *(audit/ffi F5)*
  - Fix: remove the `include` list in `cbindgen.toml:14` (cbindgen exports every `pub extern "C"` it finds).
- [ ] FFI Arc-bookkeeping comment is wrong; pattern works only by coincidence. *(audit/ffi F6)*
  - Fix: use `ManuallyDrop<Arc<DistinctionEngine>>` instead of `Arc::from_raw + Arc::into_raw` dance.
- [ ] FFI `check_commitment` hash parameter is decorative (never inspected). *(audit/ffi F7)*
  - Fix: add hash parameter to `BatchCommitment::verify`; FFI passes it through. Related to commitment.rs `leader_id` fix in 1.5.
- [ ] `batch_len: usize` unbounded — UB if > `isize::MAX`. *(audit/ffi F9)*
  - Fix: reject `batch_len > isize::MAX as usize` at FFI entrypoints.

### 1.8 WASM wire-format (Phase 1 audit) — depends on §5 hex-on-wire decision
- [ ] **`id_to_bytes` heuristic (`id.len() == 64`) breaks at v2.0** — every distinction falls through to UTF-8 fallback path. *(audit/wasm W1)* — EVIDENCE: implicit (structural; v2.0 changes `id().len()` from 64 to 32)
  - Fix: kill heuristic. Expose `Distinction::as_bytes() -> &[u8; 16]`; `id_to_bytes` becomes one `.to_vec()`.
- [ ] **Primordials leak as `[0x30]` / `[0x31]` UTF-8 bytes** instead of real IDs. *(audit/wasm W2)* — EVIDENCE: `experiments/findings/baseline.md` (`test_wasm_engine_primordial_consistency` FAILS in `cargo test --features wasm`)
  - Fix: v2.0 gives primordials real 16-byte IDs. Special case disappears.
- [ ] **Silent UTF-8 fallback on hex decode error** — `hex::decode(id).unwrap_or_else(|_| id.as_bytes().to_vec())`. *(audit/wasm W4)* — EVIDENCE: source-only (would need to inject non-hex 64-char string into JS bridge)
  - Fix: propagate as `JsValue::from_str(...)`. Fail-closed.
- [ ] WASM `synthesize(&str, &str)` should be `synthesize(&[u8], &[u8])` at v2.0 (symmetric with outputs). *(audit/wasm W9)*
- [ ] WASM `checkCommitment` builds Frankenstein `BatchCommitment { leader_id: "", batch_size: 0 }`. *(audit/wasm W10)*
  - Fix: add proper params, OR narrower API in `NetworkAgent`.
- [ ] No panic hook in WASM — Rust panics surface as opaque `RuntimeError`. *(audit/wasm W5)*
  - Fix: add `console_error_panic_hook` + `#[wasm_bindgen(start)]`.
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

### 1.10 Architectural cleanup
- [ ] **Delete `ParallelBatchProcessor`** (~250 LOC removed). It's misnamed (Sequential body, dead `worker_count` field, single-variant enum). Already covered by `ConsensusValidator`. *(audit/parallel)*
  - Pre-flight: grep ALIS + koru-protocol for `ParallelBatchProcessor` use before deleting.
- [ ] Keep `ParallelSynthesizer`; rename to `BatchSynthesizer`; return `Vec<Option<Distinction>>` instead of empty-string fallback. *(audit/parallel)*

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
- [ ] CLAUDE.md says "114 tests" but actual is 103. Update doc (add to 1.4).
- [ ] `tests/falsification/wasm_consistency.rs` is broken under `--features wasm` — pre-existing test code that expects a wire format the wasm wrapper doesn't deliver. Fix as part of Section 1.8 wasm work.
- [ ] Pre-existing experiments (`exp01-12`) have ~20 clippy warnings (`type_complexity`, `manual_div_ceil`, `dead_code`). Either fix as cleanup or scope the "no warnings" promise to `src/` + `tests/`. Recommended: fix. Low priority but easy.

---

## Section 2 — IMPLEMENT (all v2.0, no intermediate release)

All items ship together in 2.0.0. The structural type changes and the additive APIs land in the same release so consumers migrate once.

### 2.1 Type-level changes (structural, theory-strengthening)
- [ ] `Distinction([u8; 16])` — truncated SHA256. Content addressing preserved. *(Exp 13: 0 collisions in 268M; Exp 15: 4.2× faster end-to-end)*
- [ ] `pub(crate)` on the field + private constructor — closes foreign-ID poisoning structurally. No external code can mint IDs.
- [ ] `BuildHasherDefault<IdentityHasher>` on internal DashMaps — 6–13× hash speedup. Safe ONLY with byte keys. *(Exp 14)*
- [ ] FFI: keep public C signatures, swap internal `format!`/parsing for `hex::encode`/`hex::decode`. ~25 LOC. *(Exp 17: 0 signature changes)*

### 2.2 Additive APIs (new public surface)
- [ ] `degree(d: &Distinction) -> usize` — O(1) via `DashMap<Distinction, AtomicUsize>`.
- [ ] `parents_of(d: &Distinction) -> Option<(Distinction, Distinction)>` — forward index populated in synthesize.
- [ ] `children_of(d: &Distinction) -> impl Iterator<Item = Distinction>` — reverse index, in-engine.
- [ ] Streaming `calculate_sis` using the reverse index (removes the full-snapshot clone in compactor).

### 2.3 Synthesis log
- [ ] Append-only log on `SegQueue<(Distinction, Distinction)>` (bytes from day one, not retyped from String).
- [ ] `DistinctionEngine::without_log()` constructor or `log` feature flag for memory-sensitive consumers.
- [ ] Serde derives on log entry type (`bincode` round-trip 25 ms / 14 ms at 1M). *(Exp 7)*

### 2.4 Invariant tripwire
- [ ] `debug_assert!(self.relationship_count() == 2 * self.distinction_count() - 3)` inside `synthesize()` on the novel path. Zero release cost, future-regression tripwire.

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

## Section 5 — STILL OPEN (decisions, not work)

### Original (pre-Phase 1)
- [ ] **Merkle-over-log / log-diffing semantics** — Exp 12 showed canonical ordering is NOT needed for replay. It IS needed if koru-protocol ever hashes the log for consensus or peer-diffs logs. Decide before the log API is finalized.
- [ ] **Snapshot API contract** — pick: rename to `_unsynchronized`, add `_quiesced(barrier)` variant, or document the 0.06–0.08% tear rate at the call site. Required before any persistence consumer adopts it.
- [ ] **Hex on the wire after v2.0** — WASM consumers comparing `id === "abc..."` strings will break if we switch JS-visible IDs to bytes. Decision: keep hex on the wire / serialize bytes / both? Affects Section 1.8 (WASM wire-format fixes).

### Surfaced by Phase 1 audits
- [ ] **Consensus-correctness bugs (1.5): patch in v1.2.x now, or wait for v2.0?**
  - `leader_id` not in hash, `previous_root` 8-byte truncation, validator atomic-failure leakage.
  - All three are network-correctness critical. If any code runs on a testnet, these are exploitable.
  - Fixing `leader_id` hash changes the `commitment_hash` output — wire-format break even in a "patch" release. Coordinate with consumers either way.
  - Recommended: v1.2.1 security patch alongside v2.0 development. Single v2.0 cut still holds for everything else.
- [ ] **Compactor mutation: accept the design and fix docs, or refactor to truly read-only?**
  - The mutations (in `new()` and `synthesize_action`) all route through `engine.synthesize` — theory-clean (append-only, axioms preserved).
  - "Non-destructive" claim in CLAUDE.md/TODO.md is the actual error.
  - Two paths: (a) cheapest — rephrase docs to say "all mutations are append-only `synthesize` calls"; (b) refactor — `synthesize_action` becomes a `pure_analysis()` returning a `CompactionReport` instead of writing to the engine.
  - Recommended: (a). The append-only mutations are correct theory; the docs are wrong, not the code.
- [ ] **Validator atomic-failure: doc fix only, or pre-validate batches?**
  - Doc fix is one paragraph; honest about engine-side append-only growth.
  - Pre-validation eliminates the engine-side leakage but adds a pass per batch.
  - Recommended: pre-validation. Cheap, removes the leakage entirely, aligns implementation with the docstring's promise.
- [ ] **FFI thread-safety: doc paragraph only, or wrap agents in internal `Mutex`?**
  - Doc-only is cheap and zero perf cost; relies on embedder discipline.
  - Internal `Mutex<NetworkAgent>` is ~30 LOC; single-digit ns hot path; eliminates the footgun at compile time.
  - Recommended: internal Mutex. The "Go/Kotlin/Swift runtime serializes" assumption is unverifiable from Rust; defense in depth matches the rest of the engine's "no footguns" stance.
- [ ] **WASM panic hook: feature-gate or always-on?**
  - Add `console_error_panic_hook` as a `wasm` feature dep; call from `#[wasm_bindgen(start)]`.
  - Cost is ~5 KB of WASM bundle. Acceptable for diagnostic value.
  - Recommended: always-on under `wasm` feature.
- [ ] **Network audit: must-fix items are also useful as a security advisory.**
  - Should we publish a `SECURITY.md` describing the three consensus-correctness bugs once fixed, or quietly fix and ship?
  - Recommended: SECURITY.md once patched. ALIS/koru-protocol consumers need to know which versions are safe.

**Decided** (record only):
- ~~v1.3 vs v2.0 boundary~~ → single v2.0 cut. One migration for consumers.
- ~~Defensive runtime input validation on engine~~ → no (theory drift; closed by v2.0 `pub(crate)` structurally).

---

## Quick stats

| Category | Items | Status |
|---|---|---|
| Section 1.1–1.3 — Engine FIX (original 14 items) | 14 | not started |
| Section 1.4 — CLAUDE.md doc drift | 7 | not started |
| Section 1.5 — Consensus-correctness bugs (MUST NOT SHIP) | 3 | not started |
| Section 1.6 — Consensus hardening | 6 | not started |
| Section 1.7 — FFI hardening (incl. 2 HIGH) | 7 | not started |
| Section 1.8 — WASM wire-format (incl. 1 CRITICAL) | 7 | not started |
| Section 1.9 — Compactor cleanup | 4 | not started |
| Section 1.10 — Architectural cleanup | 2 | not started |
| Section 1.11 — Phase 1 follow-up (persist + run drafted code) | 10 | not started |
| Section 1.12 — Baseline measurement | 4 | not started |
| Section 2 — IMPLEMENT (all v2.0) | 13 | not started |
| Section 3 — AUDIT (Phase 1) | 7 subsystems | **complete** |
| Section 4 — VALIDATE | 4 claims | 3 drafted / 0 run |
| Section 5 — DECIDE | 9 | 3 original + 6 from Phase 1 |

**To call the project "completely theory-aligned, clean, high-quality, bug-free":**
- All of Section 1 (1.1–1.10) must be closed.
- Section 4 should be closed for the engine to be self-validating.
- Section 2 is the new value being shipped; Section 5 are gating decisions.

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
