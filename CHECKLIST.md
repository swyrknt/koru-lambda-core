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
- [ ] `Distinction(pub(crate) [u8; 16])` newtype with `#[repr(transparent)]` and derives: `Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, bytemuck::Pod, bytemuck::Zeroable`
- [ ] `distinction_hex.rs`: `to_hex` / `from_hex` / `Display` / `Debug` / serde adapter / typed `ParseError` (thiserror)
- [ ] `IdentityHasher`: leading 8 bytes via `u64::from_le_bytes`, `debug_assert!` + `unreachable!()` guards on misuse, `#[inline]` on `write` / `finish`
- [ ] `DistinctionEngine` with 3 fields: `all_distinctions`, `parents_of`, `degree_counts`. No log. No children_of.
- [ ] `DistinctionEngine::new()` seeds d₀/d₁ in `all_distinctions` AND seeds their `degree_counts` entries to 0
- [ ] `synthesize` hot path:
  - entry-gated insert into `all_distinctions`
  - inside closure: insert `parents_of`, pre-seed `degree_counts[new_bytes] = 0`, `fetch_add(1, Release)` on both parents via `get()` (not `entry()`)
  - foreign-byte `debug_assert!` on both parent args
  - `#[must_use]` annotation
  - `expect("degree_counts pre-seeded at parent insertion (invariant)")` instead of `unwrap()` for clearer panic if invariant breaks
- [ ] `engine.parents_of(d) -> Option<(Distinction, Distinction)>` (O(1) lookup, `#[must_use]`)
- [ ] `engine.degree(d) -> usize` with Acquire load + genesis_addend (1 for primordials, 2 otherwise)
- [ ] `engine.relationship_count() -> usize`
- [ ] `engine.check_structural_invariant() -> Result<(), InvariantError>` (asserts `all_distinctions.len() == parents_of.len() + 2`)
- [ ] `agent.rs` at substrate level: `LocalCausalAgent` trait + `synthesize_causal_action` helper. (Move + adapt from dev's `subsystems/local_agent.rs`.)
- [ ] `primitives.rs` rewrite: `Canonicalizable` trait + engine-registered `ByteMapping::map_byte_to_distinction(byte, engine)`. No static cache. No phantom nodes.
- [ ] `replay.rs`: `snapshot_parentage`, `replay_topological` returning `Result<Arc<DistinctionEngine>, ReplayError>`, `build_children_index` helper, `ReplayError::{Unreachable, Mismatch, MissingPrimordial}` via thiserror
- [ ] `recorder.rs`: `SynthesisRecorder` with `!Send + !Sync` via `PhantomData<*const ()>`, novelty check via `log.contains(&child)`
- [ ] `lib.rs` public re-exports
- [ ] Crate-level lint floor at `lib.rs`: `#![warn(clippy::unwrap_used, clippy::must_use_candidate, clippy::missing_const_for_fn)]`. **`clippy::expect_used` deliberately NOT in the floor** — the hot path mandates `.expect("…invariant")` to encode invariant-proof obligations in source. Compromise: every `.expect()` call site in the substrate must use a message ending in `"(invariant)"` identifying the load-bearing precondition. CI grep gate (Step 5): `! git grep -nE '\.expect\("[^"]*"\)' src/ | grep -vE '\(invariant\)"'` — every expect either has the invariant marker, or it's a violation. Gives tighter discipline than `expect_used` without self-contradiction.
- [ ] Substrate-wide `#[must_use]` policy applied to every function returning a `Distinction` (per the inventory in DESIGN.md Part 2)
- [ ] `#[non_exhaustive]` on public error enums (`ParseError`, `ReplayError`, `PeerIdentityError`) so future variants don't break semver
- [ ] `Cargo.toml` `[dev-dependencies]` adds: `static_assertions = "1"` (for compile-time trait assertions), `loom = "0.7"` (for memory-ordering test), `dhat = "0.3"` (for per-distinction memory probe), `blake3 = "1"` (for hash-algorithm cross-check differential test)
- [ ] `Cargo.toml` `[dependencies]` adds: `bytemuck = { version = "1", features = ["derive"] }` (Pod/Zeroable derive — the `derive` feature is NOT default; omitting it makes `#[derive(bytemuck::Pod, Zeroable)]` fail to compile), `parking_lot = "0.12"` (FFI Mutex), `console_error_panic_hook = { version = "0.1", optional = true }` (under `wasm` feature). thiserror / dashmap / sha2 / serde / lru / rayon / hex already in dev.
- [ ] `Cargo.toml` package metadata: `edition = "2021"`, `rust-version = "1.80"` (MSRV pinned — DashMap 6 needs 1.71, bytemuck derive needs 1.74, 1.80 leaves headroom for `LazyLock`).
- [ ] Safety justification comment on `Distinction` documenting `bytemuck::Pod`'s requirements (no padding, all-bits-valid — both satisfied by `#[repr(transparent)] [u8; 16]`)

### Substrate tests
- [ ] **Axioms:** determinism, commutativity (+ 10K proptest), irreflexivity (+ proptest), content addressing, saturation (1M repeats → delta=0)
- [ ] **Structural laws at scale:** r=2d−3 after 5M synths (zero deviations), binary parentage (`parents_of.len() == all_distinctions.len() − 2`), ByteMapping phantom count = 0 over 256-byte exercise
- [ ] **Engine internals:**
  - IdentityHasher 1M-key distinct-bucket test
  - Compile-time `assert_send_sync<DistinctionEngine>`, `assert_send_sync<Distinction>`, `assert_send_sync<dyn LocalCausalAgent>`
  - **Concurrent-write byte-equivalence** (8 threads same chain → byte-identical state; `sum(degree_counts) == 2 * parents_of.len()`)
  - **`or_insert_with` closure runs exactly once per novel synth** — pre-bind `x = synthesize(d0, d1)`, snapshot `d0_before` + `x_before`, race N threads on `synthesize(d0, x)`, assert PARENT degrees increment by exactly 1 (not 2N). The child's degree alone can't catch the bug.
  - **Primordial invariants on a fresh engine:** `distinction_count() == 2`, `parents_of(d0).is_none()`, `parents_of(d1).is_none()`, `degree(d0) == 1`, `degree(d1) == 1`, `check_structural_invariant().is_ok()`. Also: `synthesize(d0, fresh_x)` on a freshly-constructed engine doesn't panic on the hot-path `unwrap`/`expect`.
  - **Fold Law byte coverage lower bound:** after 256-byte exercise, `degree(d0) >= 256 * 8` AND `degree(d1) >= 256 * 8`. The `8` is the per-bit fold step count in `ByteMapping::map_byte_to_distinction`. Step-1 *implementation* of `primitives.rs` must use 8-step folds (one per bit, both primordials at every step); the probe assertion is locked at `>= 256 * 8` and the implementation conforms to it, not the other way around.
  - Mediated self-reference uniqueness at depth ≥ 10K (iterative, not recursive)
  - **LCA pattern invariants** (Theory §13-15): two `LocalCausalAgent` instances initialized with the same local root, processing the same canonical action sequence, produce byte-identical advancing root chains at every step. An LCA whose root advances forward never regresses to a prior parent (`update_local_root` is monotonic in the synthesis graph). Falsifies a future LCA implementation that introduces nondeterminism into causal action synthesis.
- [ ] **Proptest properties:** commutativity / irreflexivity / synth idempotency on engine state
- [ ] **Misuse detection:**
  - IdentityHasher panics on non-16-byte `write`
  - `Distinction::from_hex` rejects empty, wrong length, non-hex, uppercase, non-ASCII
  - Cross-engine foreign-byte injection panic in debug builds
  - `SynthesisRecorder !Send + !Sync` via `static_assertions::assert_not_impl_any!`
  - `replay_topological` returns `Mismatch` on tampered, `Unreachable` on cyclic, `MissingPrimordial` on can't-bootstrap (each release-mode, not debug-gated)
  - `SynthesisRecorder` dedup test (re-recording same child doesn't double-append)
- [ ] **Replay correctness:**
  - `replay_topological(snapshot_parentage(e))` byte-identical round-trip
  - Replay on shuffled parentage (Exp 12 carried forward)
  - **Log-replay invariant** (theory-guardian round-2): rebuild engine from
    `parents_of.iter()` into a fresh `DistinctionEngine`; assert byte-equality
    of `all_distinctions`, `parents_of`, and `degree_counts` against the
    live engine. Catches hidden state fields that aren't pure functions of
    the synthesis log — falsifies a future "4th DashMap" that would survive
    content-addressing checks.
- [ ] **Differential testing:**
  - **Hash-algorithm cross-check** (rust-craftsman round-2, qa-sentinel
    round-2 critique applied): a ~70 LOC naive reference impl
    `synthesize_ref(a, b)` using `blake3::Hasher` + canonical `[u8;16]`
    byte ordering + leading 16 bytes. The test asserts THREE things over
    1M random `(a, b)` pairs, NONE of which are properties of the
    reference alone:
    1. **Hand-derived golden bytes:** for 16 hardcoded `(a, b)` pairs
       (covering: both primordials, irreflexive cases, byte-ordering
       boundary cases like `[0xFF, 0x00...]` vs `[0x00, 0xFF...]`),
       assert `engine.synthesize(a, b)` produces specific expected bytes
       computed by hand from the SHA-256 spec. Falsifies any ordering
       inversion or prefix-truncation drift by anchoring at known points.
    2. **Cross-axiom invariants on PRODUCTION engine** (not the reference):
       1M random pairs assert `engine.synthesize(a, b) == engine.synthesize(b, a)`
       (commutativity) and `engine.synthesize(a, a) == a` (irreflexivity).
       The blake3 reference is used to confirm these properties are
       independent of hash choice — if they fail on production but hold
       on reference, production has a non-hash bug.
    3. **Independence check:** assert `engine.synthesize(a, b) != synthesize_ref(a, b)`
       for the 1M random pairs (different hashes ⟹ different outputs).
       This catches the degenerate case where someone accidentally writes
       a test that compares production to itself.
    The earlier "both must agree on commutativity" framing was insufficient
    — it tested properties of the reference, not the production engine.
    The hand-derived golden anchors are the actual canonical-ordering /
    prefix-truncation falsifier.
  - **Traversal/degree recompute oracle** (engine-architect round-2):
    a ~35 LOC reference that computes `degree(d)` by scanning the
    relationship set fresh on every call (no cache). Run 1M proptest
    synthesis operations; assert `engine.degree(d) == reference.degree(d)`
    for every `d` after every op. Catches any future cache-staleness bug
    in `degree_counts` or `parents_of` under concurrent insert.
- [ ] **Append-only invariant** (engine-architect round-2 promotion to
  theory gate): a compile-time test (or unit test) verifying the
  `DistinctionEngine` API exposes no `remove_*` / `clear` / `truncate` /
  `drop_distinction` method. Backed by the Step 5 hygiene grep.
- [ ] **Memory ordering:** `loom` model-checker test over a minimal kernel (writer `fetch_add(Release)` / reader `load(Acquire)`); falsifies missing-Acquire regardless of host architecture
- [ ] **Compile-time assertions:** `Distinction: Copy + Send + Sync + Pod + Zeroable`, `#[repr(transparent)]`, `SynthesisRecorder: !Send + !Sync`
- [ ] `benches/substrate.rs` (criterion harness): synthesize cold/warm, parents_of, degree, byte folding. Do NOT carry forward dev's `benches/performance.rs` (it benches the String API).

### Step 1 measurements (HARD gate — no soft hatch)
**Primary platform:** Apple M3 Pro, 8-core. Criterion median of 100 iters.
- [ ] Single-thread synthesis throughput ≥ 450K ops/sec
- [ ] 8-thread synthesis throughput: BOTH ≥ 12M ops/sec absolute AND ≥ 4× single-thread (ratio carries the floor to non-M3 hardware)
- [ ] Memory per distinction ≤ 180 B at 1M scale (dhat live-heap, steady state; including DashMap shard capacity slack — see DESIGN.md gate 13 arithmetic. The earlier 140 B target was arithmetic-only and didn't account for shard slack. Hard-cap floor at 220 B.)
- [ ] Coding Law ρ ≥ 0.985 against exp18-as-implemented workload: length-N chain pool, Zipf `alpha=1.0`, `seed=0xC0DE`, `N=4096`, `M=8N`, Spearman of `freq[k]` vs `degree_after[k] − degree_before[k]`. **Implementation note:** Step 1 must produce the workload as a reusable module (e.g., `experiments/runner/src/coding_law_workload.rs`) so Step 4 reruns the exact same code at scale instead of reimplementing it. Same constants, same chain builder, same Zipf, same seed.
- [ ] **Pin the exp18 corpus** at `/tests/corpora/exp18.log` + `/tests/corpora/exp18.freq.bin` — capture the deterministic `(min, max)` pair log AND the Zipf-draw frequency array produced by running the workload module above with the pinned constants. Freeze both files. Step 4 replays from these corpora bit-exactly. **Integrity gate:** compute SHA-256 of each file; commit the hex digests as constants in `tests/coding_law.rs` (`EXP18_LOG_SHA256 = "..."`, `EXP18_FREQ_SHA256 = "..."`). The test asserts the file contents match before computing ρ. Catches accidental regeneration with shifted dependency defaults (e.g. `rand` minor version changes). Also pin `rand = "=0.8.5"` (or whichever version produced the corpus) with exact-version `=` syntax in `[dev-dependencies]` so the corpus generator stays reproducible across `cargo update`.
- [ ] Fold Law d₀/d₁ hub ratio ≥ 100× after byte folds
- [ ] r = 2d − 3 with zero deviations at 5M synths

### Step 1 hygiene
- [ ] `cargo fmt --check` clean
- [ ] `cargo clippy --all-targets --release -- -D warnings` clean (gate 15)
- [ ] `cargo bench --no-run` compiles (gate 18) — verifies `benches/substrate.rs` is wired in
- [ ] `cargo +nightly miri test --lib` clean on substrate (gate 19)

**Step 1 Gate (hard checkpoint):** all substrate tests pass; `engine.rs ≤ 480 LOC`; all measurements meet budget. If a measurement misses budget: stop, open a gate-decision PR. Do not proceed to Step 2 with an unresolved miss.

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
- [ ] 8-thread throughput on M3 Pro ≥ 12M ops/sec
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
- [ ] Clean working-tree from dev's leftover untracked dirs (`.claude/`, `experiments/`, `warroom/`) — decide which to keep on `release/2.0.0` before Step 1's first commit. Notes: `.claude/` is editor config (probably gitignore); `experiments/` is the warroom probe workspace (rebuild fresh per `engine-architect` rather than carry forward — v1.2 API mismatch); `warroom/` is research artifacts (review for archival).
- [ ] Decide fate of dev's `benches/performance.rs` — it benches the String API. Either delete (Step 1 replaces with `benches/substrate.rs`) or keep as `benches/_archive_performance.rs.bak` for reference. Recommend delete.
- [ ] `cargo audit` clean on `dev`'s `Cargo.lock` before Step 1 starts. Any advisories on direct deps must be either patched on `dev` first or explicitly accepted with a note in this checklist.
- [ ] **Capture `BASELINE_DEV_M3_PRO`** — run criterion (synthesize cold/warm, parents_of, degree) + dhat (memory per distinction at 1M) on `dev` HEAD as a pre-flight measurement. Pin the numbers in `DESIGN.md` Part 10.5 as the regression-delta baseline. Every Step 1+ budget amendment cites this baseline. Without it, "engineering reality" is rhetoric.

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
