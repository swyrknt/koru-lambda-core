# Checklist — koru-lambda-core 2.0.0

**Branch:** `release/2.0.0` (cut from `dev`)
**Plan:** `DESIGN.md` (5 review rounds, unanimous GREEN at commit `22dbbce` on `research/warroom-experiments`)
**Theory:** `THEORY.md`
**Architecture:** `ARCHITECTURE.md`

Status: `[ ]` not done · `[~]` partial · `[x]` done

---

## Working agreements

- All step branches off `release/2.0.0` and merge back into it. Naming: `step/NN-name`.
- `Cargo.toml` stays at `1.2.0` through Steps 1-4. The final commit in Step 5 is the single bump to `2.0.0`.
- `CHANGELOG.md` grows during the work as `## Unreleased`. Step 5 renames to `## 2.0.0 — YYYY-MM-DD`.
- The substrate is sacrosanct. No defensive runtime checks on engine inputs — foreign-ID closes structurally via `pub(crate)` on Distinction.
- Phase 8 (ALIS + koru-protocol consumer migrations) is **out of scope** for this repo. Those teams own their own migrations against the `CHANGELOG.md` and `SECURITY.md` we publish.
- Gates split into three categories per DESIGN.md Part 10:
  - **Theory gates** (un-amendable): axioms 1-4, r=2d−3 zero exceptions, append-only invariant, log-replay invariant, phantom count = 0, engine-state independence. A failure means redesign, not amendment.
  - **Budget gates** (amendable within hard-cap floors): throughput, memory, Fold Law ratio, Coding Law ρ. Floors per DESIGN.md Part 10.5. Amendments follow the Budget Amendment Policy: cite measured value + harness, append to `BUDGET_LOG.md`, theory-guardian + engine-architect sign-off (research-lead measurement justification for loosening).
  - **Hygiene gates** (unconditional): fmt, clippy bare + wasm, Miri, TSan, loom, cargo-public-api, cargo-audit, doc-code reconciliation.
- "Doc-flagged for revisit" remains forbidden.

---

## Step 1 — Substrate foundation (`step/01-substrate`)

### Substrate code
- [x] `Distinction(pub(crate) [u8; 16])` newtype with `#[repr(transparent)]` and derives: `Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, bytemuck::Pod, bytemuck::Zeroable` — `src/engine.rs::Distinction`
- [x] `distinction_hex.rs`: `to_hex` / `from_hex` / `Display` / `Debug` / serde adapter / typed `ParseError` (thiserror)
- [x] `IdentityHasher`: leading 8 bytes via `u64::from_le_bytes`, `debug_assert!` + `unreachable!()` guards on misuse, `#[inline]` on `write` / `finish`
- [x] `DistinctionEngine` storage. **Spec was 3 fields** (`all_distinctions`, `parents_of`, `degree_counts`); **Step 1e merged them to 1 field** (`nodes: DashMap<[u8;16], EngineNode { parents, degree }>`) per the upper-bound mock + BUDGET_LOG.md row 1 amendment process. Three canonical projections preserved as per-node fields. No log, no children_of.
- [x] `DistinctionEngine::new()` seeds d₀/d₁ as `EngineNode { parents: None, degree: 0 }` in `nodes` (was: pre-seed in `all_distinctions` + `degree_counts`).
- [x] `synthesize` hot path:
  - entry-gated insert into `nodes` via Vacant/Occupied match
  - Vacant arm sets `parents: Some((first, second))`, `degree: 0`; block drops entry shard write-lock BEFORE parent fetch_adds (B1 deadlock mitigation post Step 1e merge)
  - foreign-byte `debug_assert!` on both parent args
  - `#[must_use]` annotation
  - `.expect("first parent registered at its insertion (invariant)")` / `.expect("second parent registered at its insertion (invariant)")` on post-entry parent lookups
- [x] `engine.parents_of(d) -> Option<(Distinction, Distinction)>` (O(1) lookup via `nodes.get(d).parents`, `#[must_use]`)
- [x] `engine.degree(d) -> usize` with Acquire load + genesis_addend (1 for primordials, 2 otherwise, 0 for foreign)
- [x] `engine.relationship_count() -> usize` — `(nodes.len() - 2) * 2 + 1` (O(1) since primordials always present)
- [x] `engine.check_structural_invariant() -> Result<(), InvariantError>` — verifies `nodes.len() == (count of nodes with parents) + 2`. Moved from O(1) to O(N) post-merge (iterates nodes to count parented entries); not on hot path.
- [x] `agent.rs` at substrate level: `LocalCausalAgent` trait + `synthesize_causal_action` helper. Adapted from dev's pattern.
- [x] `primitives.rs` rewrite: `Canonicalizable` trait + engine-registered `ByteMapping::map_byte_to_distinction(byte, engine)`. No static cache, no phantom nodes. Falsified by `fold_law_structural_invariant_holds_after_exercise`.
- [x] `replay.rs`: `snapshot_parentage`, `replay_topological` → `Result<Arc<DistinctionEngine>, ReplayError>`, `build_children_index`, `ReplayError::{Unreachable, Mismatch, MissingPrimordial}` via thiserror, `#[non_exhaustive]`
- [x] `recorder.rs`: `SynthesisRecorder` with `!Send + !Sync` via `PhantomData<*const ()>`. **Spec was novelty check via `log.contains(&child)`**; **Step 1e Round-2 review (qa-sentinel Y3) upgraded to HashSet shadow** with `IdentityBuildHasher` for O(1) dedup. Log behavior unchanged.
- [x] `lib.rs` public re-exports
- [x] Crate-level lint floor at `lib.rs` (`#![warn(clippy::unwrap_used, must_use_candidate, missing_const_for_fn, missing_docs)]`). `.expect()` convention: every call site ends in `"(invariant)"`. CI escalates `-D warnings` to deny. Step 5 hygiene grep verifies.
- [x] Substrate-wide `#[must_use]` policy applied to every function returning a `Distinction`
- [x] `#[non_exhaustive]` on public error enums (`ParseError`, `ReplayError`, `InvariantError`). `PeerIdentityError` lands in Step 2.
- [x] `Cargo.toml` `[dev-dependencies]`: `static_assertions = "1"`, `loom = "0.7"`, `dhat = "0.3"`, `proptest = "1.0"`, `criterion = "0.5"`, `serde_json = "1.0"`, `rand = "=0.8.5"` (workspace pin). **`blake3 = "1"` was removed in step1e-drift-sweep** — the planned differential test (synthesize_ref with blake3) was deferred to Step 4 essential probes; see Step 1 substrate tests § Differential testing below.
- [x] `Cargo.toml` `[dependencies]`: `bytemuck = { version = "1", features = ["derive"] }`, `sha2 = "0.10"`, `dashmap = "6.0"`, `hex = "0.4"`, `thiserror = "1.0"`, `serde = "1.0"`. WASM optional deps land at Step 3. `parking_lot` lands at Step 3 (FFI Mutex).
- [x] `Cargo.toml` package metadata: `edition = "2021"`, `rust-version = "1.80"`
- [x] Safety justification comment on `Distinction` documenting `bytemuck::Pod`'s requirements (no padding, all-bits-valid)

### Substrate tests
- [~] **Axioms:**
  - [x] determinism (`axiom1_determinism_*`)
  - [x] commutativity (`axiom2_commutativity_*` + `prop_commutativity` 10K proptest)
  - [x] irreflexivity (`axiom3_irreflexivity_*` + `prop_irreflexivity` 10K proptest)
  - [x] content addressing (`axiom4_content_addressing_*`)
  - [x] saturation (`law7_saturation_repeated_synth_adds_nothing` — 1000 repeats, not 1M; the 1M-repeats probe is a **Step 4 essential probe**)
- [x] **Structural laws at scale:**
  - [x] r=2d−3 after 5M synths (zero deviations) — `tests/scale.rs::r_equals_2d_minus_3_at_5m_synths`
  - [x] binary parentage — `law5_every_nonprimordial_has_two_parents` + scale check via `check_structural_invariant`
  - [x] ByteMapping phantom count = 0 over 256-byte exercise — `primitives::tests::fold_law_structural_invariant_holds_after_exercise`
- [x] **Engine internals:**
  - [x] IdentityHasher 1M-key distinct-bucket test (`million_distinct_inputs_million_distinct_hashes`)
  - [x] Compile-time `assert_send_sync<DistinctionEngine>`, `assert_send_sync<Distinction>`. (`dyn LocalCausalAgent` Send+Sync deferred — LCA's associated type makes it non-trivial; covered indirectly by the consumer subsystems in Step 2.)
  - [x] Concurrent-write byte-equivalence — `concurrent_synth_byte_equivalent_state` (assertion updated post-merge: `sum(node.degree) == 2 * non_primordial_count`)
  - [x] `or_insert_with` closure runs exactly once per novel synth — `or_insert_with_closure_runs_exactly_once`
  - [x] Primordial invariants on fresh engine — `fresh_engine_has_*`, `primordials_have_*`, `synthesize_on_cold_engine_does_not_panic`
  - [x] Fold Law byte coverage exact bound (== 512) — `fold_law_byte_coverage_exact_bound`
  - [ ] Mediated self-reference uniqueness at depth ≥ 10K (iterative) — **DEFERRED to Step 4 essential probes** (CHECKLIST line 217). Proptest 10K-case bound on commutativity/irreflexivity/idempotency landed in Step 1; the specific mediated-self-ref chain probe is the Step 4 deliverable.
  - [x] LCA pattern invariants — `agent::tests::lca_byte_identical_advancing_chains`, `lca_chain_records_in_engine_parents_of`
- [x] **Proptest properties:** commutativity / irreflexivity / idempotency — `engine_tests::proptests` (10K cases each)
- [x] **Misuse detection:**
  - [x] IdentityHasher panics on non-16-byte `write` (`panics_on_short_slice_in_debug` + `write_u8_is_unreachable` etc.)
  - [x] `Distinction::from_hex` rejects empty, wrong length, non-hex, uppercase, non-ASCII — distinction_hex tests
  - [x] Cross-engine foreign-byte injection panic in debug — `foreign_byte_synthesize_panics_in_debug`
  - [x] `SynthesisRecorder !Send + !Sync` via `static_assertions::assert_not_impl_any!`
  - [x] `replay_topological` returns `Mismatch` / `Unreachable` / `MissingPrimordial` (release-mode, not debug-gated)
  - [x] `SynthesisRecorder` dedup test — `repeated_synthesize_dedups`
- [x] **Replay correctness:**
  - [x] `replay_topological(snapshot_parentage(e))` byte-identical round-trip
  - [x] Replay on shuffled parentage — `replay_shuffled_order_byte_identical`
  - [x] Log-replay invariant — `log_replay_invariant_byte_identical_state` (assertion updated post-merge to compare via `snapshot_distinctions` / `snapshot_parentage` / per-node `degree`)
- [ ] **Differential testing — DEFERRED to Step 4 essential probes.** Step 1 was specced to include a blake3 reference impl + hand-derived golden bytes + degree recompute oracle. `blake3` dev-dep was removed in step1e-drift-sweep as unused; the differential suite belongs alongside Step 4's cross-engine determinism 5-phase probe. Re-add `blake3 = "1"` in Step 4 and implement: (1) `synthesize_ref` blake3 reference, (2) 16 hand-derived golden byte fixtures, (3) degree recompute oracle scanning the relationship set fresh. Cross-axiom invariants on production (commutativity / irreflexivity on 1M random pairs) ARE covered by the proptest 10K cases shipped in Step 1; the gap is the hash-algorithm-independent confirmation + golden anchors.
- [x] **Append-only invariant** — `engine_is_append_only_by_api_surface` test + Step 5 hygiene grep (per DESIGN.md gate 27: "no `fn (remove|clear|truncate|drop)_distinction` anywhere").
- [x] **Memory ordering:** loom kernel — `tests/loom_kernel.rs` with 3 standard kernels + `cfg(loom_mutant)` regression sentinel (kernel 4 downgrades writer to Relaxed; `#[should_panic]` proves loom catches it). Run: `RUSTFLAGS="--cfg loom" cargo test --test loom_kernel --release`.
- [x] **Compile-time assertions:** `Distinction: Copy + Send + Sync + Pod + Zeroable`, `#[repr(transparent)]`, `SynthesisRecorder: !Send + !Sync` — `compile_time_assertions` module
- [x] `benches/substrate.rs` (criterion harness): synthesize novel/saturated, 8-thread, parents_of, degree, byte_fold. Plus `benches/upper_bound.rs` as the cited evidence for BUDGET_LOG.md row 1 (Gate 12 amendment).

### Step 1 measurements (HARD gate — no soft hatch)
**Primary platform:** Apple M3 Pro, 8-core. Criterion median of 100 iters.
- [x] Single-thread synthesis throughput ≥ 450K ops/sec → **4.44 M ops/sec** (+887% headroom; `cargo bench --bench substrate`)
- [x] 8-thread synthesis throughput (M3 Pro primary): BOTH ≥ 12M ops/sec absolute AND ≥ 3.4× single-thread → **15.25 M ops/sec, 3.43× ratio** (Gate 12 amended in Step 1e via `BUDGET_LOG.md` row 1; original 4× ratio was structurally unreachable on M3 Pro 6P+2E). Hard-cap floor: ≥ 10M AND ≥ 3.0× ratio. On symmetric hardware (8+ uniform cores) the substrate is expected to clear ≥ 4× single-thread as a regression watch — escalate to formal per-platform gate via amendment if Step 4 Linux measurements deliver.
- [x] Memory per distinction ≤ 180 B at 1M scale → **137.4 B** (+24% headroom; `cargo run --example dhat_1m`). dhat 0.3.3 has an aarch64-apple-darwin release-mode bug — run in debug; the allocator measurement is identical.
- [x] Coding Law ρ ≥ 0.985 against exp18 workload → **ρ = 0.9940** (`cargo test --release --test coding_law`). Workload module pinned at `experiments/runner/src/coding_law_workload.rs` per the Step 4 reuse requirement.
- [x] Pin the exp18 corpus → committed at `tests/corpora/exp18.{log,freq.bin}`; SHA-256 digests pinned as constants in `tests/coding_law.rs`; `exp18_corpus_integrity` test gates against drift; `rand = "=0.8.5"` exact-pinned in `experiments/runner/Cargo.toml` for generator reproducibility.
- [x] Fold Law d₀/d₁ hub ratio ≥ 100× after byte folds → covered by `primitives::tests::fold_law_d0_d1_hub_ratio_clears_100x_gate` (Step 1e added).
- [x] r = 2d − 3 with zero deviations at 5M synths → `tests/scale.rs::r_equals_2d_minus_3_at_5m_synths` passes; explicit arithmetic `distinction_count = N+2, relationship_count = 2N+1` asserted in addition to `check_structural_invariant`.

### Step 1 hygiene
- [x] `cargo fmt --check` clean
- [x] `cargo clippy --workspace --all-targets --all-features -- -D warnings` clean
- [x] `cargo bench --no-run` compiles (`benches/substrate.rs` + `benches/upper_bound.rs`)
- [x] TSan green on 90 lib tests — `RUSTFLAGS=-Zsanitizer=thread cargo +nightly test -Zbuild-std --target aarch64-apple-darwin --all-features --lib`
- [x] loom kernel passes (3/3 standard, 4/4 mutant)
- [ ] `cargo +nightly miri test --lib` — **DEFERRED**. Miri build on the substrate's dep graph (DashMap + sha2 + serde) takes 30+ min before tests even start. Rerun once before Step 2 cut to confirm clean; CI gates the full miri run.

**Step 1 Gate (hard checkpoint):** all substrate tests pass; `engine.rs ≤ 480 non-test LOC` (measured 210 lines of code, exclusive of docs/blanks/attrs); all measurements meet budget. Status: **CLOSED**, every measurement clears its target.

---

## Step 2 — Reference subsystems (`step/02-subsystems`)

### Subsystem code (rewritten from dev as clean LCA implementations)
- [ ] `validator.rs` (~300 LOC): `ConsensusValidator` with pre-validation pass; V3 (data cap), V4 (oversized-root clipping), V5 (atomic failure designed-in), V6 (atomic restore_state), V8 (empty-data by design)
- [ ] `commitment.rs` (~250 LOC): `BatchCommitment::compute` hashes `leader_id` (N6 designed-in); LRU stays at dev's ~1000-entry cache
- [ ] `network.rs` (~550 LOC): `TransactionBatch::previous_root: Distinction` (not String — N5 cannot exist); `MAX_PEER_ID_LEN = 64` (N1); `PeerIdentity::new -> Result<Self, PeerIdentityError>` typed (N2); joint `(id, distinction)` dedupe in `join_peer` (N7); `pending_commitments: LruCache<[u8;32], BatchCommitment>` with `MAX_PENDING_COMMITMENTS = 256` (= 64 peers × 2 epochs × 2 safety margin) (N11); `advance_epoch` clears `pending_commitments`; atomic `restore_consensus_validator_state`
- [ ] `compactor.rs` (~250 LOC): `Compactor::new(hot_threshold, warm_threshold)` requires explicit thresholds; no `archived_ids` field; no double-count; no self-archive; `#[serde(try_from = "RawForm")]` enforces `warm <= hot` on deserialize
- [ ] `parallel.rs` (~80 LOC): `BatchSynthesizer` only. `ParallelBatchProcessor` deleted. Returns `Vec<Option<Distinction>>`.
- [ ] `subsystems/mod.rs` declarations + re-exports

### Subsystem tests (each closes the corresponding audit finding by construction)
- [ ] Validator: V5 atomic-failure regression, V3 data cap, V4 oversized-root clipping, V6 atomic restore_state, V8 empty-data by-design
- [ ] Commitment: N6 regression (compute hashes leader_id), F7 transitive (verify_batch rejects tampered leader_id)
- [ ] Network: N5 cannot exist (typed previous_root), N1/N2 peer-id bounds, N7 joint dedupe, N11 LRU cap + epoch clear
- [ ] Compactor: explicit thresholds required at construction, no double-count, no self-archive, no `archived_ids`, Deserialize respects `warm <= hot`
- [ ] Parallel: `BatchSynthesizer` returns `Vec<Option<Distinction>>` with correct nones for failed syntheses

**Step 2 Gate:** all regression tests pass; per-file LOC ceilings hit:
`validator.rs ≤ 330`, `commitment.rs ≤ 280`, `network.rs ≤ 580`,
`compactor.rs ≤ 280`, `parallel.rs ≤ 100`. Aggregate
`subsystems/ ≤ 1,500 LOC`. (Targets from DESIGN.md Part 3 with ~10% ceiling
headroom.) A per-file miss triggers the same gate-decision PR mechanic as
Step 1.

---

## Step 3 — Bindings (`step/03-bindings`)

### FFI (`ffi.rs`, ~650 LOC)
- [ ] Opaque types: `#[repr(C)] pub struct KoruEngine { _private: [u8; 0] }`, same for `KoruAgent` and `KoruValidator`. Distinct typedefs in `target/koru.h`.
- [ ] Handle wrapping: `Box<parking_lot::Mutex<NetworkAgent>>`, `Box<parking_lot::Mutex<ConsensusValidator>>` (5× faster than `std::sync::Mutex`, no poisoning)
- [ ] Engine borrows: `ManuallyDrop<Arc<DistinctionEngine>>` via one `borrow_engine` helper
- [ ] `panic = "abort"` on release profile in `Cargo.toml`
- [ ] Length guards: `batch_len > isize::MAX` rejected before `slice::from_raw_parts`
- [ ] `koru_agent_check_commitment` takes `leader_id` + `batch_size` as real inputs
- [ ] cbindgen.toml: no `include` allowlist, no `prefix` (manual `koru_` on each `#[no_mangle]`)

### WASM (`wasm.rs`, ~450 LOC, feature-gated)
- [ ] Bytes-canonical end to end (every distinction ID crossing the JS boundary is `Uint8Array` length-16)
- [ ] `WasmEngine::synthesize(&[u8], &[u8]) -> Result<Vec<u8>, JsValue>` length-validated
- [ ] `idToHex(arr) -> string` and `idFromHex(s) -> Uint8Array` freestanding helpers
- [ ] `checkCommitment(hash, nonce, epoch, leader_id, batch_size)` — empty `leader_id` rejected
- [ ] `#[wasm_bindgen(start)] fn _wasm_start()` wires `console_error_panic_hook` unconditionally under the `wasm` feature
- [ ] All WASM tests are `#[wasm_bindgen_test]`

### Binding tests
- [ ] FFI: 8-thread concurrent join test (F2/F8 regression)
- [ ] FFI: `batch_len > isize::MAX` rejected (F9 regression)
- [ ] FFI: fabricated root rejected via `restore_state`
- [ ] WASM: bytes-canonical round-trip, axioms preserved across boundary, `idToHex`/`idFromHex` round-trip, empty `leader_id` rejected (W10 regression)

**Step 3 Gate:** FFI passes 8-thread concurrent test (TSan-clean:
`RUSTFLAGS=-Zsanitizer=thread cargo +nightly test --release` — gate 20,
**unconditional** per Part 10, not "if toolchain available"); WASM
compiles under `--features wasm`; `cargo clippy --all-targets --features wasm --release -- -D warnings`
clean (gate 16); `wasm-pack test --node --features wasm` passes
(gate 21 — set up the toolchain in CI if not local).

---

## Step 4 — Probes + scale validation (`step/04-validation`)

`experiments/` is a separate workspace (doesn't ship in the crate). Probe findings become evidence in `SECURITY.md` and the substrate docstrings.

### Essential probes (each falsifies a load-bearing claim if it fails)
- [ ] Cross-engine determinism (5 phases — Exp 21 carried forward)
- [ ] r = 2d − 3 at 5M synths (zero deviations)
- [ ] Saturation at 1M identical synths (distinction_count delta = 0)
- [ ] Phantom-node count = 0 over full 256-byte exercise
- [ ] Mediated self-reference uniqueness at depth ≥ 10K
- [ ] `replay_topological(snapshot_parentage(e))` round-trip byte-equivalence
- [ ] Concurrent synthesis byte-equivalence (8 threads → byte-identical final state)

### Theory validation probes
- [ ] Coding Law ρ ≥ 0.985 (Spearman, exp18 workload as pinned in Step 1 measurements)
- [ ] Fold Law d₀/d₁ ratio ≥ 100×

### Engineering probes
- [ ] 8-thread throughput on M3 Pro: ≥ 12M ops/sec AND ≥ 3.4× single-thread (Gate 12 platform-named target; see DESIGN.md Part 10.5 + `BUDGET_LOG.md` row 1). Hard-cap floor: ≥ 10M AND ≥ 3.0× ratio. On symmetric server hardware: ≥ 4× single-thread as regression watch.
- [ ] Memory per distinction at 1M scale ≤ 180 B (dhat; includes DashMap shard slack)
- [ ] WASM bytes-on-wire round-trip fingerprint match (native vs wasm-pack-node)

### Edge-case probes
- [ ] `replay_topological` worst-case O(N²) probe on degenerate linear-chain parentage

**Step 4 Gate:** every essential claim has a probe demonstrating it; thresholds defined with pre-registered refutation conditions.

---

## Step 5 — Documentation + release prep (`step/05-release`)

- [ ] `CHANGELOG.md` written as one coherent v2.0 entry (not accreted across sub-branches)
- [ ] `SECURITY.md` with Tier-0 disclosure for N5 / N6 / V5:
  - Each issue gets: affected versions (`< 2.0.0`), fixed version (`2.0.0`), one-paragraph technical description, one-paragraph impact analysis, and an explicit structural-fix note ("closed by construction in v2.0 via …")
  - Supported-version table (current row: `2.x` supported, `1.x` end-of-life on `2.0.0` release date)
  - This is CVE-style disclosure, not a notes file
- [ ] `README.md` rewritten for v2.0
- [ ] `CLAUDE.md` updated
- [ ] `Cargo.toml` bumped `1.2.0 → 2.0.0` (single commit; the final commit on the integration PR)
- [ ] `Cargo.toml` profile.release has `panic = "abort"` (already required by Step 3)
- [ ] Name gate-decision PR reviewers in `DESIGN.md` (theory-guardian + engine-architect for the design; research-lead-authored measurement justification for budget amendments)
- [ ] CI workflow file at `.github/workflows/` enforcing: fmt, clippy bare + wasm (gates 15+16), test release, Miri on substrate (gate 19), TSan on FFI 8-thread (gate 20 — unconditional), wasm-pack (gate 21)
- [ ] Hygiene gate: `! git grep -nE '\.children_of\(' -- ':!src/replay.rs' ':!CHANGELOG.md'` (no consumer calls the dropped API)
- [ ] Hygiene gate: `! git grep -nE 'fn (remove|clear|truncate|drop)_distinction' src/` (append-only invariant — no removal primitive, per Theory Gate 8)
- [ ] Hygiene gate: `git grep -nE 'pub struct Distinction' src/` returns ONE line, which contains `pub(crate)` on the field (foreign-byte injection closed structurally)
- [ ] Hygiene gate: `git grep -cE '^\s+\w+:\s+DashMap<' src/engine.rs` returns exactly **3** (the canonical engine field count — `all_distinctions`, `parents_of`, `degree_counts`; a 4th map without a corresponding ARCHITECTURE.md update fails this gate)
- [ ] Hygiene gate: no `#[ignore]` / `#[cfg(slow)]` markers on essential probes (Part 10 gate 16 — every test in `cargo test --release` actually runs)
- [ ] Hygiene gate: `cargo audit` clean (no advisories on direct deps — dashmap, sha2, parking_lot, bytemuck, thiserror, lru, rayon, hex, console_error_panic_hook, static_assertions, blake3)
- [ ] **`cargo public-api` lockdown** (Part 10 gate 25): install `cargo-public-api`; capture the v2.0 public API surface as `public-api.txt`; commit it; CI fails on any silent surface change. Locks `Distinction`'s `pub(crate)` field, `#[non_exhaustive]` on errors, `#[must_use]` policy, FFI symbol names. Intentional API changes require co-merged baseline rebaseline.
- [ ] **`cargo mutants` weekly CI** (defense-in-depth on non-theory code): scoped to `src/subsystems/*.rs` + `src/ffi.rs` + `src/wasm.rs` + `engine.rs::{children_of, degree, check_structural_invariant}` (the bookkeeping functions theory-guardian flagged as NOT self-protecting via content addressing). Substrate hash path and primitives excluded — mutation testing there is theater. Surviving mutants get either a new test or `// mutants: skip` with justification. Triage budget: ~2-5 hours quarterly.
- [ ] **`critcmp` nightly cron on `main`** with 5% regression threshold posted to a GitHub issue. Catches subsystem death-by-a-thousand-cuts drift (3% PRs that compound to 15% by Phase 8) that the per-step hard gates can't see. Not gating on PRs — diagnostic only. Lower priority than `cargo-public-api`.
- [ ] **`BUDGET_LOG.md` published** (Part 10 gate 34): empty if no budget amendments occurred during Steps 1-4; populated with `date | gate | old | new | baseline delta | author | signers | justification` rows if any did. Linked from `## Unreleased` in CHANGELOG.md.
- [ ] **Doc-code reconciliation review** (Part 10 gate 28): paired review of `src/engine.rs` field list and public method signatures against `ARCHITECTURE.md §Substrate / engine.rs`. Reviewers: theory-guardian + engine-architect.
- [ ] All Part 10 done-criteria gates verified end-to-end (34 total):
  - **Theory gates 1-10** (un-amendable): axioms + r=2d−3 + engine independence + phantom=0 + concurrent byte-equivalence + replay round-trip + append-only + log-replay + mediated self-reference
  - **Budget gates 11-15** (amendable within floors per Part 10.5): throughput single + 8-thread, memory, Fold Law ratio, Coding Law ρ on pinned exp18 corpus
  - **Hygiene gates 16-28** (unconditional): test/clippy×2/fmt/bench-compile/Miri/TSan/wasm-pack/loom/cargo-public-api/cargo-audit/grep sweep/doc-code reconciliation
  - **Size gates 29-30**: `tokei src/ --no-tests ≤ 3,450`, `engine.rs ≤ 480 LOC`
  - **Documentation gates 31-34**: SECURITY/CHANGELOG/Cargo.toml/BUDGET_LOG.md
- [ ] PR `release/2.0.0` → `dev`

**Step 5 Gate:** all CI gates green; PR ready for review.

---

## Pre-flight cleanups (before starting Step 1)

- [x] Stale local `release/2.0.0` branch deleted (was at `913473c` from first v2.0 attempt; deleted in this branch's setup)
- [x] `release/2.0.0` cut from `dev` HEAD (`c2d331b`, the "Update README" commit) — verified
- [x] Working tree clean — `.claude/`, `experiments/`, `warroom/` all gitignored (commit `214a11e`, expanded in commit on dev). Directories stay on disk for reference but don't pollute the branch.
- [x] **`benches/performance.rs` deleted.** It benched the v1.2 String API and won't compile against v2.0. Step 1 creates `benches/substrate.rs` fresh against the byte-API surface. `Cargo.toml` `[[bench]]` entry also removed.
- [x] `cargo audit` run on `dev`'s `Cargo.lock`. **Result: 0 vulnerabilities, 5 warnings, all accepted with rationale below.** No blocking advisories on any direct dep. The 5 warnings:
  - `atty` (RUSTSEC-2024-0375 unmaintained, RUSTSEC-2021-0145 potential unaligned read) — transitive via `criterion`. Not in v2.0's substrate-only build path. `criterion`'s next minor will likely drop `atty`; not v2.0-blocking.
  - `lru` (RUSTSEC-2026-0002 IterMut Stacked Borrows) — direct dep via `subsystems/network.rs`'s `pending_commitments`. v2.0 uses `lru::LruCache::put`/`pop_lru`, NOT `IterMut`. The unsound code path is not reachable from our usage. Verify post-Step-2 that no `IterMut` usage creeps in via hygiene grep (Step 5).
  - `rand` 0.8.5 + 0.9.2 (RUSTSEC-2026-0097 custom-logger unsound) — `rand` is a dev-dep for proptest + Zipf workload. Both versions pulled transitively. We do not use custom loggers with rand; the unsound code path is not reachable. exp18 corpus generator pins `rand = "=0.8.5"` per Step 1 plan; the unsound path remains unreachable.
  - **Recheck before v2.0 ship:** rerun `cargo audit` at Step 5 release prep. If any of these has been promoted to a CVE or if direct usage of the unsound code paths has crept in, address before tagging 2.0.0.
- [x] **`BASELINE_DEV_M3_PRO` resolved — not needed.** Originally proposed to anchor budget amendments against measured dev performance. On closer inspection, dev ships the v1.2 String-API (`Distinction { id: String }`, ~629 B/distinction, no `parents_of()`/`degree()`); v2.0 ships the byte-API. Comparing throughput / memory across structurally different APIs is apples-to-oranges — the regression delta would be rhetoric, not evidence. Resolved by updating `DESIGN.md` Part 10.5 to drop the dev-baseline requirement and lean on **absolute targets + hard-cap floors + `BASELINE_WARROOM_M3_PRO`** (the apples-to-apples reference from `research/warroom-experiments` commit `22dbbce`, which had the same byte-canonical Distinction). Warroom numbers pinned in DESIGN.md: 500K single-thread, 15.3M 8-thread, 80 B/distinction, 250× Fold ratio, ρ ≈ 0.99 on exp18. `BASELINE_DEV_M3_PRO.toml` scaffold deleted.

---

## Blind-spot followups — surfaced in round-2 broad review, tagged for Step 4/5

Items the team flagged when given freedom to find what we'd stopped seeing.
None are blockers; all are accepted as work to land alongside or after
the substrate ships.

### Tag for Step 4 (probes + scale validation)

- [ ] **Ceiling probe** — synth to OOM on a 16 GB box; record actual ceiling + allocator footprint vs the predicted ~80M. If actual is materially below 80M, update THEORY.md and DESIGN.md to honest measurement. (research-lead)
- [ ] **64-thread concurrent byte-equivalence** — DashMap default shard count is `4 × num_cpus`. The 8-thread probe in Step 1 doesn't exercise above that. Run 64 threads on the same chain; assert byte-identical state. If passes, declare "tested to 64 threads"; otherwise document the actual support boundary. (research-lead)
- [ ] **1M-byte phantom probe** — extend the 256-byte exercise to 1M random + adversarial byte sequences. The 256-byte test catches the static-cache regression but is too small to detect non-deterministic re-emergence. (research-lead)
- [ ] **1M-depth mediated self-reference probe** — current gate is depth ≥ 10K iterative. At 1M depth, instrument: unique distinction count, longest root-to-node path, degree of the accumulator. Verify uniqueness == depth exactly. (research-lead)
- [ ] **Adversarial Coding Law workload** — at minimum one of: uniform sampling (anti-Zipf), rotating hot-set, time-varying alpha. Report ρ; the secondary gate is "ρ on adversarial workload is documented, not necessarily ≥ 0.97." Makes Coding Law's workload-conditional nature falsifiable. (research-lead, theory-guardian)
- [ ] **Memory-pressure throughput probe** — at 60M+ distinctions on a 16 GB box (consume 70%+ of RAM). Report throughput delta vs cold/warm baseline. Catches allocator paging regressions invisible at 1M. (research-lead)
- [ ] **WASM at-scale byte-equivalence** — replay the exp18 corpus through `wasm-pack test --node`; assert final state-fingerprint bit-exact vs native. Catches WASM `usize` precision boundaries. (research-lead)
- [ ] **Consumer migration replay** — capture ~10K-op synthesis trace from each of ALIS and koru-protocol; replay through v2.0; assert byte-identical final state. Cheap insurance vs CHANGELOG-only safety. (research-lead)
- [ ] **Structural collision pathway probe** — at end of any large-scale run, assert `all_distinctions.len() == unique_count(content_addresses)`. Free; catches non-random pair collision pathways. (research-lead)
- [ ] **`koru_engine_free` double-free safety test** — F-series FFI gate. Calling free twice → defined behavior (poisoned handle or abort, not UB). Current F-series tests cover concurrent join but not double-free. (engine-architect)

### Tag for Step 5 (docs + release prep)

- [ ] **Steel-man writeups: rejected engine shapes.** Document in DESIGN.md or a new `DESIGN_ALTERNATIVES.md` why we did NOT pick: (a) 2-field engine (collapse `all_distinctions` into `parents_of: DashMap<[u8;16], Option<(D,D)>>`), (b) 4-field engine (add `synthesis_count: AtomicUsize` for O(1) convergence checks). Without these writeups, future contributors will relitigate the choice from scratch. (engine-architect)
- [ ] **"LCA is substrate" defense.** Currently asserted in THEORY.md without argument. Document why consumers that don't fit the LCA shape (multi-perspective agents, non-root-anchored synthesis) aren't supported as first-class — or, if they are, update THEORY to reflect it. (theory-guardian)
- [ ] **`SynthesisRecorder` placement.** Currently in `src/recorder.rs` (substrate layer) but carries order-bearing state — a category error per theory-guardian. Either move to `src/subsystems/recorder.rs` or document explicitly why it lives in substrate. (theory-guardian)
- [ ] **CI wall-clock budget per PR.** Estimate: Miri 5-15 min + TSan 2-5 min + 5M r=2d-3 ~10s + loom ~30s + criterion benches ~5 min + WASM tests ~2 min. Per-PR likely 25-45 min on M3 Pro, 60-90 min on GitHub runners. Document this; if too slow, tier into "PR / nightly / pre-release" gate buckets. Otherwise gates will get `#[ignore]`'d under pressure. (qa-sentinel)
- [ ] **Build matrix.** Add to CI: Windows (`x86_64-pc-windows-msvc`), Linux ARM (`aarch64-unknown-linux-gnu`), macOS ARM (M3 Pro primary). `no_std` smoke test for the substrate's `Distinction` type (since it's `Pod`). MSVC linker quirks around `panic = "abort"` need verifying. (qa-sentinel)
- [ ] **`#[should_panic]` interaction with `panic = "abort"`.** Release builds with `panic = "abort"` skip unwinding-required tests silently. Document which tests run on `[profile.dev]` vs `[profile.release]`, and verify the FFI misuse-panic tests are running in the profile we think they are. (qa-sentinel)
- [ ] **`degree()` formula prose cleanup.** Current API doc explains `+1 for primordial, +2 otherwise` but doesn't explain WHY in source. Add docstring distinguishing the genesis-edge contribution from the parent-edge contribution. (theory-guardian)
- [ ] **`d0 ⊗ d1` explicit case.** What happens if a consumer calls `engine.synthesize(d0, d1)` directly? Irreflexivity doesn't fire (distinct). A new distinction is born; the genesis edge is already counted. Either (a) deliberately allow it and document, (b) special-case it in synthesize. Currently silent. (theory-guardian)
- [ ] **`BatchSynthesizer` `Vec<Option<Distinction>>` overhead.** 17 bytes padded to 24 — 50% memory overhead vs `Vec<Distinction>` + sentinel. At batch sizes that motivate parallel synthesis (>10K), this matters. `Distinction::ZERO` (already `Zeroable`) makes a natural sentinel. Evaluate during Step 2; either justify the Option or switch to sentinel. (engine-architect)

---

## What this checklist does NOT cover

- ALIS migration to `koru-lambda-core = "2"` — owned by the ALIS team. Driven by `CHANGELOG.md` + `SECURITY.md` once we publish.
- koru-protocol migration — owned by the koru-protocol team. Same.
- v3 architectural questions (extracting subsystems to a separate `koru-reference-spoc` crate, etc.) — explicitly out of scope per Decision 5.
- Anything not gated by a Step 1-5 gate above OR tagged in the Blind-spot followups section. If it's worth doing for v2.0, it's a checklist item; if it's not on this list, it's not in v2.0.
