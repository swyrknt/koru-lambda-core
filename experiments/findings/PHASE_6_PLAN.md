# Phase 6 Plan — v2.0 Implementation

**Status:** drafted 2026-06-12, pre-dispatch. Locked before any Phase 6 sub-branch starts.
**Branch:** `research/warroom-experiments` @ `935edf6` (post Phase 3 + consumer surface)
**Scope:** CHECKLIST Sections 1.5–1.10 + Section 2 (~40 items, ~1500 LOC across `src/`).

---

## Goal

Implement all v2.0 work in a sequence of focused sub-branches that each merge back to `research/warroom-experiments`. Each sub-branch closes a coherent set of CHECKLIST items, verifies via `cargo test` + `cargo clippy`, re-runs relevant Phase 1.5 probes where applicable, and lands a CHANGELOG line.

When Phase 6 completes, the integration branch is ready for Phase 7 (SECURITY.md + version bump + integration PR to `dev`).

## Pre-flight (before dispatching #1)

These must be verified at Phase 6 entry, before any sub-branch is created:

- [ ] `git status` clean; HEAD = head of `research/warroom-experiments`
- [ ] `cargo test --release` passes (current baseline: 103 tests)
- [ ] `cargo clippy --all-targets --release` clean
- [ ] `wasm-pack --version` succeeds (or install via `cargo install wasm-pack`)
- [ ] `Cargo.toml` deps inventory — confirm `crossbeam` (for SegQueue) is present or note it as added by sub-branch #4; confirm `hex` is present (added in Phase 2)
- [ ] `experiments/findings/run_log/` exists and contains Phase 1.5 baseline logs
- [ ] No uncommitted changes in `src/`
- [ ] Note any concurrent work on `dev` or `main` to avoid surprise merge conflicts at Phase 7

---

## The foundation problem

Section 2.1 (`Distinction([u8; 16])` + `pub(crate)` + IdentityHasher + hex serialization layer + FFI internal hex swap) is the **foundation change**. It touches every file that handles `Distinction`. Everything else in Phase 6 depends on it:

- Subsystems (validator, network, commitment, compactor, parallel) use `Distinction`
- FFI returns `Distinction` ids
- WASM bytes-on-wire rewrite depends on `Distinction::as_bytes()` and the hex layer
- Traversal API (`parents_of`, `children_of`, `degree`) stores `Distinction` in indices

**If we dispatch parallel sub-branches before the foundation lands, they all write against the String-based `Distinction` and have to redo their work post-foundation.** The foundation must land alone, first.

---

## API decisions inside the foundation sub-branch

These are sub-decisions Section 2.1 implementation makes. They're not in Section 5 because they're implementation-level, but they need to be settled inside the foundation sub-branch:

| Decision | Recommendation | Rationale |
|---|---|---|
| Replace `Distinction::id() -> &str` with `Distinction::as_bytes() -> &[u8; 16]` | YES, remove `.id()`, add `.as_bytes()` | Cannot return `&str` borrow of internal bytes without holding an owned String. Forcing `.as_bytes()` everywhere makes the byte-canonical commitment visible in every callsite. |
| Provide `Distinction::to_hex() -> String` | YES | Required for display, JSON, FFI human surfaces. |
| Provide `Distinction::from_hex(s: &str) -> Result<Self, ParseError>` | YES | Required for parsing from JSON / FFI inputs / WASM idFromHex helper. |
| `impl Display for Distinction { … }` uses `to_hex()` | YES | Standard ergonomics. |
| `impl Debug for Distinction { … }` uses `to_hex()` | YES | Avoids opaque hex dump in debug output. |
| `impl Serialize/Deserialize for Distinction` | YES, hex string in JSON (transparent serde `with = "distinction_hex"`); raw bytes in bincode | Bytes-on-wire architecture. |
| Primordials | `[0u8; 16]` for d0, `[1u8; 16]` for d1 (right-aligned in 16-byte field; or pad however cleanest). NOT domain-separated hashes — keeps the SHA256 of `(d0, d1)` deterministic across versions. | Simpler; preserves the existing d0/d1 hash chain. |
| **SHA256 truncation: first 16 bytes (MSB)** | YES | Standard convention in content-addressed systems (Git object hashes, blockchain block hashes use leading bytes). Last-16 would also work — neither has stronger collision properties — but first-16 is the unsurprising choice. |
| IdentityHasher | `BuildHasherDefault<IdentityHasher>` on internal DashMaps; IdentityHasher returns raw bytes as u64. Safe because keys are uniformly distributed (SHA256 outputs). | Exp 14: 6–13× hash speedup. |
| `Distinction::new(String)` deprecation path | REMOVE entirely (no public constructor). Internal: `Distinction(pub(crate) [u8; 16])`. | Decision 5 #5 / 2.1: closes foreign-ID poisoning structurally. |

If any of these change during implementation, document in `PHASE_6_PLAN.md` as an update before merging the foundation sub-branch.

### Code to write inside #1 (concrete deliverables)

The foundation sub-branch contains these specific code units:

1. **`Distinction([u8; 16])` type** — `src/engine.rs` (replaces current `Distinction { id: String }`)
2. **`IdentityHasher`** — `src/engine.rs` or `src/hash.rs` (~10 LOC). Hashes `[u8; 16]` by reinterpreting first 8 bytes as u64. Includes a unit test that two distinct random `[u8; 16]` values get distinct hashes (no collisions in 1M random inputs).
3. **Hex layer in `src/distinction_hex.rs`** (~50 LOC total):
   - `Distinction::to_hex(&self) -> String`
   - `Distinction::from_hex(s: &str) -> Result<Self, ParseError>` (validates length and char set)
   - `impl Display for Distinction`
   - `impl Debug for Distinction`
   - `pub mod distinction_hex_serde { fn serialize<S>(...); fn deserialize<'de, D>(...) }` — the `#[serde(with = "distinction_hex_serde")]` adapter
   - Unit tests: round-trip `from_hex(to_hex(d)) == d`; reject bad length; reject non-hex chars
4. **FFI internal hex swap** — every `*mut c_char` return that previously held `id.id()` now holds `CString::new(hex::encode(distinction.as_bytes())).unwrap().into_raw()` (or equivalent safe path). ~25 LOC per Exp 17.
5. **Mechanical `.id() → .as_bytes()` rewrites** across `src/` (~30 sites estimated):
   - Subsystems: `validator.rs`, `network.rs`, `commitment.rs`, `compactor.rs`, `parallel.rs`, `local_agent.rs`
   - Tests: `tests/falsification/*.rs`
   - Experiments: `experiments/runner/src/*.rs`, `experiments/qa/src/*.rs`

---

## Sub-branch decomposition

11 sub-branches. Foundation is mandatory-first; the rest are sequenced based on file-level interactions.

### #1 — `impl/foundation-distinction-bytes` (FOUNDATION — sequential, must land first)

**CHECKLIST items closed:** Section 2.1 (all 5 items)

**Scope:**
- `Distinction([u8; 16])` + `pub(crate)` field + private constructor
- `BuildHasherDefault<IdentityHasher>` on internal DashMaps (`all_distinctions`, `relationships`)
- New hex module: `Distinction::to_hex()`, `Distinction::from_hex()`, `impl Display`, `impl Debug`
- FFI internal: `*mut c_char` returns use `hex::encode(distinction.as_bytes())` instead of String borrow
- All internal call sites updated from `.id() -> &str` to `.as_bytes() -> &[u8; 16]`
- All Distinction construction inside the crate uses the private constructor

**Files touched:**
- `src/engine.rs` (Distinction type, `synthesize` hot path, all internal byte handling, IdentityHasher integration)
- `src/primitives.rs` (`fold_byte_into_engine` may use `.as_bytes()` for bit math; Canonicalizable trait stable)
- new `src/hex.rs` (~30 LOC: `to_hex` / `from_hex` / `ParseError`)
- `src/lib.rs` (re-exports)
- `src/subsystems/{validator,network,commitment,compactor,parallel,local_agent}.rs` (mechanical `.id() → .as_bytes()` updates; trait signatures stable)
- `src/ffi.rs` (internal hex swap; ~25 LOC per Exp 17)
- `src/wasm.rs` (just enough to compile; full WASM rewrite happens in #5)
- `tests/falsification/*.rs` (mechanical updates)
- `Cargo.toml` (no new deps; `hex` already added in Phase 2)

**Specialist:** rust-craftsman + engine-architect (single agent dispatch with both lenses; or sequential)

**Tests:** all 103 must continue to pass. Determinism preserved: byte representation of distinctions changes (truncated SHA256 to 16 bytes vs full hex String), so absolute ID values are different from 1.2.0 — but the engine's internal behavior, axioms, and `r = 2d − 3` are preserved.

**Verification gates:**
- `cargo test --release` — all tests pass
- `cargo clippy --all-targets --release` — clean
- `experiments/qa/target/release/exp21_cross_engine_determinism` — re-run confirms determinism still holds
- `experiments/runner/target/release/exp18_coding_law` — re-run confirms rho ≥ 0.97 (Coding Law unaffected by representation change)
- A simple new test: `Distinction::from_hex(d.to_hex()) == d`

**Complexity:** HIGH. Single largest sub-branch in Phase 6. **Estimated LOC delta: ~+200 / ~-50 net = +150 LOC. Estimated wall time: 2–4 hours.**

**Sub-commit strategy (mandatory, mid-flight reviewable):**

The foundation sub-branch is too large to dispatch as a single commit. Internal commits in this sub-branch land in this order, each one `cargo build`-clean (not necessarily `cargo test`-passing until the last):

1. Add new `src/distinction_hex.rs` module with hex layer + tests. Distinction type unchanged. Cargo still builds, tests still pass.
2. Add `Distinction::as_bytes()` accessor alongside existing `.id()` (interim state: both APIs available). Builds and tests pass.
3. Change `Distinction` internal field type from `String` to `[u8; 16]`; rewrite `synthesize()` to use `[u8; 16]` throughout; truncate SHA256 to first 16 bytes; existing `.id()` method now allocates via `to_hex` (slow but functional). Builds; some tests pass.
4. Add IdentityHasher; rewire DashMaps. Builds; tests pass.
5. Mechanical rewrite of all `.id()` callers to `.as_bytes()` or `.to_hex()` as appropriate. Builds; tests pass.
6. Remove `.id()` method entirely. Builds; all 103 tests still pass.
7. Make `Distinction` field `pub(crate)`; remove `Distinction::new(String)`. Builds; tests pass; foreign-ID surface closed at compile time.
8. FFI internal hex swap. Builds; tests pass.
9. Update experiment crates. Builds; experiment binaries still produce expected output.

After step 9, sub-branch is ready to merge. Each step is one commit on the sub-branch; the merge to integration is `--no-ff`.

**Notes:**
- The experiments crates depend on `koru-lambda-core` by path; they break and need their `.id()` calls updated. Done in step 9.
- Probe re-runs after this sub-branch establish a new baseline (different absolute IDs, but same structural counts).
- The `panic = "abort"` change in #8 is whole-crate — it eliminates `catch_unwind` semantics everywhere. Nothing in current `src/` relies on unwinding, but flag this when sub-branch #8 lands.

---

### #2 — `cleanup/parallel-batch-processor` (PARALLEL — completely independent of foundation)

**CHECKLIST items closed:** Section 1.10 (2 items)

**Scope:**
- Delete `ParallelBatchProcessor` (~250 LOC removed)
- Delete `ProcessingStrategy` enum, `ParallelAction`, `Canonicalizable for ParallelAction`, `LocalCausalAgent for ParallelBatchProcessor`
- Remove from `lib.rs` re-exports
- Delete the 5 corresponding tests (creation, sequential batch ×2, from_root, the empty wrapper test)
- Delete internal `num_cpus` shim (lines 375-379)
- Rename `ParallelSynthesizer` → `BatchSynthesizer` (and `test_parallel_synthesizer`)
- Change `BatchSynthesizer::synthesize_parallel` return type from `Vec<String>` (with empty-string fallback for misses) to `Vec<Option<Distinction>>`

**Files touched:**
- `src/subsystems/parallel.rs` (massive deletion + rename)
- `src/subsystems/mod.rs` (re-exports update)
- `src/lib.rs` (re-exports update)

**Specialist:** rust-craftsman

**Tests:** removes 5 tests; remaining ~98 must pass. Eventually 103 → 98.

**Complexity:** LOW. Pure deletion + small rename.

**Why this can land in parallel with foundation:** Doesn't depend on `Distinction` representation. `BatchSynthesizer::synthesize_parallel` returns `Vec<Option<Distinction>>` — the new type uses whatever `Distinction` is. So whether the foundation has merged or not, this branch builds and the merge resolves cleanly.

---

### #3 — `impl/invariant-tripwire` (PARALLEL — depends on foundation; trivial)

**CHECKLIST items closed:** Section 2.4 (1 item)

**Scope:**
- Add `debug_assert!(self.relationship_count() == 2 * self.distinction_count() - 3)` inside `synthesize()` on the novel path

**Files touched:** `src/engine.rs` (1 line)

**Specialist:** rust-craftsman

**Complexity:** TRIVIAL. Could be folded into the foundation if convenient.

**Why parallel:** 1-line addition. Zero conflict surface with anything else.

---

### #4 — `impl/synthesis-log` (depends on foundation; mostly independent)

**CHECKLIST items closed:** Section 2.3 (3 items)

**Scope:**
- Add `log: SegQueue<(Distinction, Distinction)>` field to `DistinctionEngine`. Push canonical `(min, max)` (reuse the canonicalization already in `synthesize`) on every novel synthesis.
- Add `DistinctionEngine::without_log()` constructor (or wrap log in `Option`) for memory-sensitive consumers
- `#[derive(Serialize, Deserialize)]` on the log entry tuple via serde adapters (bytes for bincode; hex tuple for JSON)
- Public API: `pub fn synthesis_log(&self) -> impl Iterator<Item = (Distinction, Distinction)>` (or whatever returns the entries without exposing SegQueue internals)
- New tests: replay round-trip; canonical ordering; without_log constructor produces no log

**Files touched:**
- `src/engine.rs` (new field, push site, accessor)
- `Cargo.toml` (`crossbeam = "0.8"` if not already a dep — check)

**Specialist:** engine-architect

**Tests:** add ~3 new tests; existing tests unaffected.

**Complexity:** MEDIUM.

---

### #5 — `impl/traversal-api` (depends on foundation; affects compactor)

**CHECKLIST items closed:** Section 2.2 (4 items)

**Scope:**
- Add `parents_index: DashMap<Distinction, (Distinction, Distinction)>` to engine — forward index populated in `synthesize`
- Add `children_index: DashMap<Distinction, Vec<Distinction>>` to engine — reverse index populated in `synthesize`
- Add `degree_cache: DashMap<Distinction, AtomicUsize>` — incremented for each parent in `synthesize`
- Public API:
  - `pub fn degree(&self, d: &Distinction) -> usize` — O(1) via AtomicUsize
  - `pub fn parents_of(&self, d: &Distinction) -> Option<(Distinction, Distinction)>` — O(1)
  - `pub fn children_of(&self, d: &Distinction) -> impl Iterator<Item = Distinction> + '_` — never returns `Vec`
- Refactor `StructuralCompactor::calculate_sis` to use `engine.degree(d)` directly — drops the full-snapshot clone
- New tests: degree of d0 grows; parents_of returns canonical pair; children_of iterates correctly; calculate_sis no longer snapshots

**Files touched:**
- `src/engine.rs` (new fields, populate in `synthesize`, three new public methods)
- `src/subsystems/compactor.rs` (`calculate_sis` rewrite)

**Specialist:** engine-architect

**Tests:** add ~4 new tests; compactor tests may need numerical updates (calculate_sis now O(1) per node instead of building HashMap from snapshot).

**Complexity:** MEDIUM-HIGH. Adds three internal indices to the engine; populates them in the hot path. Performance impact: ~3 extra DashMap inserts per novel synthesis. Should still hit > 400K/s single-threaded.

---

### #6 — `fix/tier-0-consensus-correctness` (depends on foundation; serializes with #7)

**CHECKLIST items closed:** Section 1.5 (3 items: N5, N6, V5)

**Scope:**
- **N6:** `BatchCommitment::compute` includes `leader_id.as_bytes()` in the SHA256 hasher.
- **N5:** Drop `.take(8)` from `NetworkAction::BatchProposed` canonicalization. Treat `previous_root` as a full hex string and parse via `Distinction::from_hex` (or validate length and feed raw bytes through `fold_byte_into_engine`). Decide inside the sub-branch.
- **V5:** Pre-validate `batch.transactions` in `ConsensusValidator::validate_batch`: walk once to verify nonce sequence + `previous_root` match BEFORE any `engine.synthesize` call. Engine state grows only on successful batches.
- Update relevant tests where they checked the old (broken) behavior.

**Files touched:**
- `src/subsystems/commitment.rs` (N6: ~3 LOC)
- `src/subsystems/network.rs` (N5: ~5 LOC change)
- `src/subsystems/validator.rs` (V5: ~30 LOC for pre-validation pass)

**Specialist:** qa-sentinel (who identified the bugs)

**Tests:** existing tests for validator/commitment may need update. Phase 1.5 probes (`exp_validator_audit`, `audit_network_commitment_unbound`, `audit_network_foreign_peers`) MUST be re-run after this lands:
- `audit_network_commitment_unbound`: honest/forged hash MUST now differ.
- `audit_network_foreign_peers` Section D: action_a.id MUST differ from action_b.id post-fix.
- `exp_validator_audit` Section C: engine distinction delta on rejection MUST be 0.

**Consumer impact (Phase 8 backlog, noted here for completeness):**
- koru-protocol has integration tests that hardcode `commitment_hash` values. After N6 lands, those values change. Add to Phase 8: re-run koru integration tests; update expected values; document wire format change in koru CHANGELOG.
- koru-node similar — `BatchCommitment::compute` produces different hashes.
- ALIS does not use `BatchCommitment` so is unaffected by N5/N6.

**Complexity:** MEDIUM. Wire format break is intentional (Decision 5.1: bundle into v2.0). **Estimated LOC delta: ~+50 / ~-5. Estimated wall time: 1–2 hours.**

**Why serializes with #7:** Both touch `validator.rs` and `network.rs`. Land #6 first; #7 rebases off it.

---

### #7 — `fix/consensus-hardening` (depends on #6; touches same files)

**CHECKLIST items closed:** Section 1.6 (8 items)

**Scope:**
- **N1:** Cap peer-id length (suggest 64 bytes) in `PeerIdentity::new`
- **N2:** Reject `id.is_empty()` in `PeerIdentity::new`
- **V3:** Length cap on `TransactionAction.data` (suggest 1024 bytes? — decide inside)
- **V4:** Clip `previous_root` to first 64 chars in rejection message format
- **N7:** Dedupe validator set on joint `(id, distinction_id)` key in `join_peer`
- **N11:** Size cap + TTL eviction on `pending_commitments` map
- **V6:** Replace `from_root + set_expected_nonce` with single `ConsensusValidator::restore_state(engine, root_id, nonce) -> Result<Self, RestoreError>` that validates jointly. Remove the partial setters.
- **V8:** Document the empty-data degenerate tx behavior in `TransactionAction` docstring (per Decision in CHECKLIST 1.6: by-design)

**Files touched:**
- `src/subsystems/network.rs` (N1, N2, N7, N11)
- `src/subsystems/validator.rs` (V3, V4, V6, V8)

**Specialist:** qa-sentinel

**Tests:** add new tests for each length cap / empty rejection / restore_state validation. Some existing tests may need to use the new API.

**Complexity:** MEDIUM.

**Note for consumer migration (Phase 8):** V6 (joint restore_state) changes the `LocalCausalAgent::update_local_root` trait surface in koru + ALIS. 5 sites total. Already documented in `consumer_surface.md`.

---

### #8 — `fix/ffi-hardening` (depends on foundation + #6)

**CHECKLIST items closed:** Section 1.7 (7 items)

**Scope:**
- **F1+F3:** Add `panic = "abort"` to both `[profile.release]` and `[profile.dev]` in `Cargo.toml`
- **F2:** Wrap agents/validators in internal `Mutex<NetworkAgent>` / `Mutex<ConsensusValidator>` inside FFI boundary. Update header doc.
- **F4:** Typed opaque structs: `#[repr(C)] pub struct KoruEngine { _private: [u8; 0] }` (and KoruAgent, KoruValidator)
- **F5:** Remove `include` list from `cbindgen.toml` (or expand to all 22 functions)
- **F6:** Refactor Arc bookkeeping pattern from `Arc::from_raw + Arc::into_raw` dance to `ManuallyDrop<Arc<DistinctionEngine>>`
- **F7:** Add hash parameter to `BatchCommitment::verify`; FFI passes it through. Closes the "decorative hash" bug. (Interacts with N6 — N6 already includes leader_id in compute; F7 surfaces it in verify.)
- **F9:** Reject `batch_len > isize::MAX as usize` at FFI entrypoints

**Files touched:**
- `Cargo.toml` (panic = "abort")
- `src/ffi.rs` (Mutex wrappers, F4 typed opaque, F6 ManuallyDrop, F7 hash pass-through, F9 length cap)
- `cbindgen.toml` (F5)
- `src/subsystems/commitment.rs` (F7: `BatchCommitment::verify` takes hash parameter)

**Specialist:** rust-craftsman

**Tests:** existing FFI tests; add new ones for F7 (passing wrong hash → false; passing right hash → true).

**Complexity:** MEDIUM-HIGH.

**Why depends on #6:** F7 modifies `BatchCommitment::verify` which #6 already changed via N6 (which hashes leader_id in compute). Land #6 first; #8 rebases.

---

### #9 — `cleanup/compactor` (depends on #5 traversal API)

**CHECKLIST items closed:** Section 1.9 (4 items)

**Scope:**
- Remove "26.6×" claim from module docstring
- Remove or derive `hot_threshold: 3` and `warm = hot/2` magic constants. Recommended: derive from observed degree distribution (e.g., 90th percentile) or expose only as caller-tunable parameters without defaults.
- Fix `compaction_count` double-increment (remove from one of the two sites)
- Stop `synthesize_action` from re-running `calculate_sis` on its own work products (skip or exclude compaction events from re-classification)
- Document the LocalCausalAgent mutation pattern (per Decision 5.2: append-only via synthesize is theory-clean)

**Files touched:** `src/subsystems/compactor.rs`

**Specialist:** engine-architect

**Additional cleanup:**
- Convert internal `String`/`&str` ID surfaces to `Distinction`: `archived_ids: Vec<String>` → `Vec<Distinction>`; `current_root: String` → `Distinction`; `is_archived(&self, id: &str)` → `is_archived(&self, d: &Distinction)`; `get_thermal_state(&self, id: &str)` → `get_thermal_state(&self, d: &Distinction)`; internal `HashMap<String, ThermalState>` → `HashMap<Distinction, ThermalState>`.

**Tests:** existing compactor tests may need updates (the magic-threshold-tuning tests). Add a test for "compactor does not archive its own work products."

**Complexity:** MEDIUM. **Estimated LOC delta: ~-50 / ~+30 net = -20 LOC. Estimated wall time: 1–2 hours.**

**Why depends on #5:** `calculate_sis` rewrite uses `engine.degree(d)` from #5.

---

### #10 — `impl/wasm-bytes-on-wire` (depends on foundation; large standalone)

**CHECKLIST items closed:** Section 1.8 (9 items)

**Scope:**
- Kill `id_to_bytes` heuristic entirely. Replace with `Distinction::as_bytes().to_vec()` everywhere it was called.
- Update `WasmEngine::d0Id()`, `d1Id()` to return `Uint8Array` of length 16.
- `WasmEngine::synthesize(&[u8], &[u8]) -> Result<Vec<u8>, JsValue>` (symmetric inputs/outputs)
- Add WASM helpers: `idToHex(arr: &[u8]) -> String`, `idFromHex(s: &str) -> Result<Vec<u8>, JsValue>`
- Add `console_error_panic_hook` as optional dep under `wasm` feature; call `set_once()` from `#[wasm_bindgen(start)]`
- Replace `WasmEngine::checkCommitment` Frankenstein BatchCommitment with proper params (or use a narrower API that exposes hash verification directly)
- Convert host-side `#[test]` blocks in `src/wasm.rs` to `#[wasm_bindgen_test]`. Run via `wasm-pack test --node`.
- Fix pre-existing broken `tests/falsification/wasm_consistency.rs` (`[u8]` Display errors per baseline)
- Remove `Result<_, JsValue>` returns from infallible methods (W3: `join_peer`, `join_peers`, `advance_epoch`)

**Files touched:**
- `src/wasm.rs` (major rewrite, ~75 LOC churn)
- `Cargo.toml` (`console_error_panic_hook` optional dep under `wasm` feature; `wasm-bindgen-test` dev-dep)
- `tests/falsification/wasm_consistency.rs` (fix or remove)

**Specialist:** rust-craftsman

**Tests:** WASM tests now run via `wasm-pack test --node`. CI integration: separate step.

**Complexity:** HIGH (and depends on WASM toolchain being available — see Prep Item #3 in CHECKLIST).

**Pre-flight:** Run `wasm-pack --version`. If not installed: `cargo install wasm-pack`. Confirm a trivial `#[wasm_bindgen_test]` runs end-to-end before dispatching the sub-branch.

---

### #11 — `docs/changelog-and-claude-finalize` (LAST — before Phase 7)

**CHECKLIST items closed:** none net-new; closes out Section 1.4 final wording (some lines were drafted in Phase 2 with placeholder versions)

**Scope:**
- Update `CLAUDE.md` to reflect the v2.0 architecture (now bytes-on-wire, with hex layer, traversal API public, synthesis log public, no `Distinction::new`)
- Roll up CHANGELOG `Unreleased` section — every Phase 6 sub-branch contributes one or more lines. This sub-branch consolidates and ensures coverage
- Update `Architecture` block in CLAUDE.md (`primitives.rs` is now smaller; `parallel.rs` significantly smaller after #2)

**Files touched:** `CLAUDE.md`, `CHANGELOG.md`

**Specialist:** general-purpose (mostly editorial)

**Complexity:** LOW.

---

## Dependency graph

```
                    main (research/warroom-experiments @ 935edf6)
                              |
                              ▼
              #1 impl/foundation-distinction-bytes  ◀── sequential, mandatory first
                              |
                ┌─────────────┼─────────────┬─────────────┐
                ▼             ▼             ▼             ▼
       #2 cleanup/parallel    #3 tripwire   #4 log    #5 traversal
            (independent)     (trivial)      (engine)   (engine)
                              │
                              ▼
                #6 fix/tier-0-consensus-correctness   ◀── serial in validator/network
                              |
                              ▼
                #7 fix/consensus-hardening   ◀── after #6 lands in validator/network
                              |
                              ▼
                #8 fix/ffi-hardening   ◀── after #6 lands in commitment
                              |
                              ▼
                #9 cleanup/compactor   ◀── after #5 lands traversal API
                              |
                              ▼
              #10 impl/wasm-bytes-on-wire   ◀── after #1 lands hex layer
                              |
                              ▼
                #11 docs/changelog-finalize   ◀── after all sub-branches land
```

After #1 (foundation) merges, #2, #3, #4, #5, #10 can be dispatched IN PARALLEL. #6/#7/#8 serialize within validator-network-commitment territory. #9 follows #5. #11 last.

**Realistic landing order:**

| Wave | Sub-branches | Specialists | Wall time (estimate) |
|---|---|---|---|
| 1 | #1 foundation | rust-craftsman + engine-architect coordinated | 2–4 hours |
| 2a (parallel) | #2 parallel deletion, #3 tripwire, #4 synthesis log, #5 traversal API | rust-craftsman, rust-craftsman, engine-architect, engine-architect | 1–3 hours wall (parallel) |
| 2b (parallel with 2a) | #10 WASM rewrite | rust-craftsman | 2–4 hours |
| 3 | #6 tier-0 consensus | qa-sentinel | 1–2 hours |
| 4 | #7 consensus hardening, #8 FFI hardening | qa-sentinel, rust-craftsman | 2–3 hours |
| 5 | #9 compactor cleanup | engine-architect | 1–2 hours |
| 6 | #11 docs finalize | general-purpose | 30–60 min |

Total wall time: **~10–18 hours** of agent work, parallelizable so real elapsed could be lower.

---

## Per-sub-branch tracking template

For every sub-branch, the prompt to the dispatched specialist must include:

1. **Branch name** (`fix/...` or `impl/...` or `cleanup/...`)
2. **Base commit** (head of `research/warroom-experiments` at dispatch time)
3. **CHECKLIST items closed** (cite specific item numbers / lines)
4. **Files allowed to modify** (explicit list)
5. **Files NOT to modify** (explicit list — usually the rest of `src/`)
6. **Verification gate** (`cargo test` + `cargo clippy` must pass)
7. **Phase 1.5 probe re-run** (if applicable — sub-branches #6, #7, #8, #10)
8. **CHANGELOG line** (specialist drafts a CHANGELOG entry under the appropriate `Unreleased` category: Added / Changed / Deprecated / Removed / Fixed / Security)
9. **Commit message format** (lowercase prefix, no Claude branding, refer to CHECKLIST item)
10. **No version bump** (`Cargo.toml` stays at `1.2.0`)

## Sub-branch landing protocol (every sub-branch)

1. Specialist creates branch off current head of `research/warroom-experiments`
2. Specialist implements the changes; commits incrementally; ensures `cargo test --release` + `cargo clippy --all-targets --release` pass at HEAD of sub-branch
3. Specialist drafts CHANGELOG entry under appropriate Unreleased category
4. Specialist reports back to integration agent with: commit hash, file diff stats, CHANGELOG entry, any open questions
5. **Integration agent (me) reviews** the diff against the sub-branch's stated scope. Verify: only allowed files modified; no scope creep; matches plan
6. If sub-branch passes review: `git merge --no-ff <branch>` into `research/warroom-experiments` with a thematic merge commit
7. If sub-branch involves probe-able changes (#6, #7, #8, #10): re-run the relevant Phase 1.5 probe and confirm the predicted change happens
8. Delete the merged sub-branch (`git branch -d <branch>`)
9. Update CHECKLIST: mark items closed with EVIDENCE pointer (commit hash); update Phase 6 progress
10. Push integration branch to `origin`

If review identifies scope creep or correctness issue, the sub-branch goes back to specialist for fix. Do not merge half-done work.

---

## Acceptance criteria (Phase 6 done = ?)

Phase 6 is complete when **all** of:

### Build + tests

- [ ] Sub-branches #1–#11 merged into `research/warroom-experiments`
- [ ] `cargo test --release` — all tests pass. **Expected count: 120–145** (baseline 103, minus 5 from #2 ParallelBatchProcessor deletion, plus ~25–45 new tests across #3–#10). A deviation > ±20 from this range is suspicious; investigate.
- [ ] `cargo clippy --all-targets --release` — clean
- [ ] `cargo build --features wasm --release` — succeeds
- [ ] `wasm-pack test --node` — passes (post #10)
- [ ] `cargo doc --no-deps` — succeeds (sanity check for docstring syntax)

### Probe re-runs (closures confirmed empirically)

- [ ] `audit_network_commitment_unbound`: honest ≠ forged hash ← N6 closed
- [ ] `audit_network_foreign_peers` D: action_a.id ≠ action_b.id ← N5 closed
- [ ] `audit_network_foreign_peers` A: peer-id length capped ← N1 closed
- [ ] `audit_network_foreign_peers` B: empty peer-id rejected ← N2 closed
- [ ] `audit_network_foreign_peers` C: `from_state` foreign Distinction rejected at type level ← N3 closed
- [ ] `audit_network_foreign_peers` E: `update_local_root` foreign Distinction rejected at type level ← N4 closed (NOTE: the probe code itself won't compile after `Distinction::new` becomes `pub(crate)` — that's the structural closure)
- [ ] `exp_validator_audit` A: rejection message clipped to 64 chars ← V4 closed
- [ ] `exp_validator_audit` B: data length cap enforced ← V3 closed
- [ ] `exp_validator_audit` C: distinction leak on rejection == 0 ← V5 closed
- [ ] `exp_validator_audit` E: phantom count == 0 ← Phase 3 confirmation persists
- [ ] `exp_validator_audit` F: foreign Distinction no longer constructable ← V1 closed (probe won't compile)
- [ ] `exp_validator_audit` G: nonce desync impossible via new `restore_state` API ← V6 closed

### Theory preserved (validation experiments re-run)

- [ ] `exp18_coding_law`: rho ∈ [0.97, 0.999] across N=1K/10K/100K
- [ ] `exp19_mediated_self_reference`: 0 collisions at depth 10K (T1, T2, T3 all PASS)
- [ ] `exp20_fold_law`: ratio ≥ 40 (likely much higher with byte representation; treatment dominates control)
- [ ] `exp21_cross_engine_determinism`: 5/5 phases CONFIRMED

### Performance (the point of v2.0)

- [ ] `exp14_rehashing` bench re-run — confirm IdentityHasher gives ~6–13× hash speedup (Exp 14 baseline)
- [ ] `exp15_synth_breakdown` bench re-run — confirm synthesize hot path is ~4× faster ([u8;16] vs String per Exp 15)
- [ ] `exp16_clone_cost` bench re-run — confirm Distinction is Copy now (zero clone cost beyond memcpy)
- [ ] `exp01_memory` re-run — confirm ~80 B per distinction (down from 629 B, 8× density per Exp 1)
- [ ] `exp10_throughput` re-run — confirm 8-thread throughput ~7–8M/s (up from 2.6M/s per Exp 10)

If any performance number is materially below projection (>30% miss), investigate before Phase 7. The whole v2.0 value proposition depends on these.

### Documentation

- [ ] `Cargo.toml` still at `1.2.0` (version bump is Phase 7)
- [ ] CHANGELOG `Unreleased` section is populated with one or more lines from every sub-branch, organized by Keep a Changelog category (Added / Changed / Deprecated / Removed / Fixed / Security)
- [ ] CLAUDE.md reflects v2.0 architecture (new APIs: traversal, synthesis log, hex layer; removed: ParallelBatchProcessor, `Distinction::new`)

### Cleanup

- [ ] No untracked or uncommitted changes in `src/`
- [ ] Each sub-branch was merged via `--no-ff` with a thematic merge commit
- [ ] All sub-branches deleted after merge (`git branch` shows only `research/warroom-experiments`, `dev`, `main`, plus any unrelated local branches)
- [ ] All CHECKLIST items in Sections 1.5–1.10 + Section 2 marked `[x]` with EVIDENCE pointer (sub-branch commit hash)
- [ ] Phase 6 row in CHECKLIST execution order marked COMPLETE
- [ ] All work pushed to `origin/research/warroom-experiments`

---

## Risk register

| Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|
| Foundation sub-branch becomes unbuildable mid-flight | M | HIGH | Dispatch a single coordinated specialist (not parallel); incremental commits within sub-branch with `cargo build` after each |
| Two sub-branches conflict on `validator.rs` or `network.rs` | M | MEDIUM | Sequence #6 → #7; #8 depends on #6 (commitment.rs) |
| Probe re-runs show unexpected behavior | M | HIGH | Document; investigate. If probe is wrong, fix it; if code is wrong, fix sub-branch before merge |
| WASM toolchain not installed | M | LOW | Pre-flight check before dispatching #10; auto-install if missing |
| Engine.rs grows beyond manageable size (new fields, methods) | L | M | The traversal API + synthesis log together add ~100 LOC. Engine.rs goes from ~200 LOC to ~300 LOC. Still readable. |
| IdentityHasher used on non-byte keys → catastrophic hash table behavior | L | CRITICAL | Foundation sub-branch tests must include adversarial keys (e.g., all-zero, all-FF) to verify IdentityHasher behavior on byte distributions |
| Distinction::from_hex accepts malformed input silently | L | MEDIUM | from_hex must return `Result<Self, ParseError>`. Tests for length mismatch, non-hex chars, empty string |
| Consumer (ALIS, koru) tests stop passing on v2.0 dependency | H | MEDIUM | Expected per consumer_surface.md. Phase 8 handles. |

---

## Rollback strategy

If a sub-branch lands and is later found broken:

- **Sub-branch is on integration branch alone:** revert the merge commit (`git revert -m 1 <merge-commit>`). Other sub-branches that landed after are unaffected as long as they don't depend on the broken one.
- **Sub-branch is depended upon by later sub-branches:** revert the dependent ones first, then the broken one. Re-implement the broken sub-branch.
- **Foundation sub-branch broken:** roll back to `935edf6` (current head before Phase 6 starts). All Phase 6 work redone.

This is why the foundation lands first: it's the only sub-branch whose breakage cannot be cleanly rolled back without redoing everything. Get it right.

---

## What this plan does NOT contain

- Implementation code (sub-branches produce it)
- Hour-by-hour timeline (specialist availability variable)
- ALIS / koru migration steps (Phase 8)
- Decision on whether to publish a beta of v2.0 for ALIS to test against (Phase 7/8 question)
- CI workflow (assumed pre-existing; verify before Phase 7)
- v2.0 persistence migration tooling (out of scope; both consumers rebuild from scratch on first v2.0 run, since no persistence consumer exists in 1.2.0)

## Critical-path summary (TL;DR)

1. **Run pre-flight checks** (top of plan)
2. **Dispatch #1 foundation** with the 9-step sub-commit strategy. ~2-4 hours. Single specialist.
3. **Re-run Phase 1.5 baseline probes + validation experiments** to establish new (different-IDs-but-same-counts) baseline.
4. **Dispatch parallel cohort:** #2, #3, #4, #5, #10 (5 specialists, can run concurrently).
5. **After cohort merges,** dispatch #6 → #7 → #8 → #9 in sequence within each affected file pair.
6. **Run full acceptance criteria** including performance benches.
7. **Dispatch #11 docs finalize.**
8. **Phase 6 done.** Hand off to Phase 7 (SECURITY.md + version bump + integration PR).
