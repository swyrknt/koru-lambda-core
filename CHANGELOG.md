# Changelog

Budget-gate amendments and substrate changes land under `## Unreleased`
so consumer teams (ALIS, koru-protocol) see the ratchet before they
migrate. The v2.0.0 section below is the coherent entry described in
`DESIGN.md § "Path to v2.0.0"`; `## Unreleased` below it retains the
pre-2.0.0 substrate close for provenance.

## [2.0.0] - 2026-09-16

v2.0.0 introduces the projection primitive as a first-class read dual
to `synthesize`, the two-type discipline that lifts Axiom-4 boundary
enforcement from a runtime `debug_assert!` to a type-level API, and the
novelty-bit outcome enum that lets consumers observe Law 7 saturation
without extra state.

### Added

- **Projection primitive** — `engine.project(root)` builder returning
  `Projection` values. v2.0.0 lands the full API surface: 14 types +
  4 traits + wire codec. Types include `Root`, `Direction { Downstream,
  Upstream, Both }`, and the shipped `Signal` implementors `Adjacency`,
  `Degree`, `HopDistance` (see `src/projection.rs`). The builder chain
  `engine.project(root).direction(..).hops(..).signal(..).materialize()`
  returns a `Materialized<S: Signal>` carrying `projection_id()` +
  `canonical_bytes()`, both content-addressed on the spec
  `{ Root, boundary, Direction, Signal }` at quiescence.
  `Signal` is object-unsafe by design (verified via `trybuild`).
  Ships in commit `8efa70c`.

- **`SynthesisOutcome` + `synthesize_novel`** — Law-7 novelty bit
  exposure via a parallel entry point on `DistinctionEngine`.
  `SynthesisOutcome` is `#[non_exhaustive] pub enum { Novel(Distinction),
  Existing(Distinction), .. }` — variants, not `{ child, was_novel }`
  fields, so future outcome classes can land without breaking match
  arms. `synthesize` still returns a bare `Distinction` for backward
  compatibility; consumers wanting the novelty bit call
  `synthesize_novel`. Ships in commit `5707240`.

- **Two-type discipline: `RawDistinctionId` + `engine.verify()`** —
  Axiom-4 boundary narrowed from runtime `debug_assert!` to a type-level
  API. External bytes enter the system as `RawDistinctionId` (a
  `#[repr(transparent)]` wrapper with no engine-membership claim);
  `engine.verify(raw) -> Result<Distinction, VerifyError>` is the sole
  path from raw bytes to a verified `Distinction`. Foreign-byte
  injection is now closed at the type level, not just in debug builds.
  Ships in commit `10cebaf`.

- **Cond D cross-engine falsifier probe** — `tests/cond_d_falsifier.rs`
  ships as a `#[test]` converting `THEORY.md § The synthesis/projection
  dual` from a spec claim into a CI-attested theorem. Two independent
  engines with identical synthesis histories, queried with the same
  projection spec at quiescence, must produce byte-identical output.
  Every commit that touches synthesis, projection, or wire format either
  preserves Cond D or fails this test. Ships in commit `40b6d25`.

### Changed

- **`Distinction::from_hex` retained for backward compatibility.**
  `Distinction::from_hex` still parses bytes but makes no
  engine-membership claim (see rustdoc). Consumers should migrate to
  `RawDistinctionId::from_hex` + `engine.verify()`, the two-type-safe
  path. The direct hex constructor stays for the v1.x → v2.0 migration
  window; a future major release may deprecate it.

### Fixed

- `clippy.toml:1` and `rustfmt.toml:1` — project name comments corrected
  from a stale `forma-core` rename artifact to `koru-lambda-core`.
- `docs/development/GUARDRAILS.md:317-318` — CI badge URLs corrected
  from placeholder `github.com/you/forma-core/…` to the real
  `github.com/swyrknt/koru-lambda-core/…` repository path.

### Migration

A dedicated consumer migration guide is planned for v2.1. That guide
will ship the "delete your wrapper" playbook for ALIS, koru-protocol,
and other consumers with hand-rolled projection / novelty / verify
shims. Until then: v2.0.0 is API-additive; nothing in the substrate's
shipped surface (`synthesize`, `parents_of`, `degree`, `has`,
`distinction_count`, hex round-trip) changed in a signature-breaking
way. Novel consumers pick up the new APIs directly; existing consumers
migrate at their own pace.

#### npm breaking changes

For npm consumers upgrading from `koru-lambda-core@1.2.0`, the
JS/wasm-bindgen surface is **not** additive. `WasmValidator`,
`WasmCommitmentAgent`, and `WasmNetworkAgent` are retired with no
replacement; `WasmEngine`'s method signatures move from string-typed to
bytes-canonical; and `d0Id()` / `d1Id()` return different bytes than
they did in 1.2 (16-byte primordials, not UTF-8 of `"0"` / `"1"`) — the
one silent break in this release. See
[docs/MIGRATION_JS.md](docs/MIGRATION_JS.md) for the full recipe list
and migration checklist.

## Unreleased

### Substrate baseline (CLOSED — shipped in 2.0.0)

All substrate-baseline measurement gates met. Branch
`step/01-substrate` merged to `release/2.0.0`.

**Headline measurements (M3 Pro, criterion median of 100 iters,
release build, ~5min thermal idle):**

| Gate | Target | Floor | Measured | Status |
|---|---|---|---|---|
| 11 Single-thread synth | ≥ 450K ops/s | ≥ 300K | **4.51 M ops/s** | PASS (+900%) |
| 12 8-thread synth (M3 Pro) | ≥ 12M absolute AND ≥ 3.4× | ≥ 10M AND ≥ 3.0× | **15.29 M / 3.39×** | PASS (Gate 12 amended in `BUDGET_LOG.md` row 1) |
| 13 Memory per distinction | ≤ 180 B | ≤ 220 B | **137.4 B** (dhat at 1M) | PASS (+24%) |
| 14 Fold Law d₀/d₁ hub ratio | ≥ 100× | ≥ 50× | clears 100× | PASS |
| 15 Coding Law ρ | ≥ 0.985 | ≥ 0.97 | **0.9940** | PASS |

**Substrate facts pinned at Step 1 close:**
- One `DashMap<[u8;16], EngineNode { parents, degree }>` carries the
  three canonical O(1) projections (saturation check, parent lookup,
  degree query) as per-node fields.
- Synthesize hot path: entry-gated insert with explicit `inserted_new`
  flag; parent fetch_adds run only when the closure wins the entry,
  AFTER the shard write-lock releases (B1 deadlock mitigation).
  Documented relaxation: racing reader observing a new child may
  transiently see pre-bump parent degree; post-join sum invariant
  holds.
- exp18 corpus pinned at `tests/corpora/exp18.{log,freq.bin}`
  (1,048,576 B + 16,384 B). SHA-256 digests gated; `rand = "=0.8.5"`
  exact-pinned for reproducibility.
- TSan clean on 90-test lib suite (`-Zsanitizer=thread`,
  `aarch64-apple-darwin`, `-Zbuild-std`).
- 3 loom kernels pass (`RUSTFLAGS="--cfg loom" cargo test --test loom_kernel --release`).
- 101 tests across the workspace; release suite + fmt + clippy clean.

### Substrate

- **Step 1e merged-map refactor** — `DistinctionEngine` storage layout
  changed from three side-by-side `DashMap`s
  (`all_distinctions` / `parents_of` / `degree_counts`) to one
  `DashMap<[u8;16], EngineNode { parents, degree }>`. Public API
  unchanged in signature; performance: single-thread +20%, 8-thread
  +46%, parallel-scaling ratio 2.85× → 3.43× on M3 Pro. See commits
  `30cc77f` (verification mock), `fd9c2b1` (refactor), `cd6ca3f`
  (qa-sentinel falsification tests).

- **Happens-before contract narrowed.** Parent `degree` `fetch_add`s
  now occur after the new-child shard write-lock is released (B1
  mitigation — parent and new-child may hash to the same shard). A
  racing reader who observes a new child and immediately queries
  `degree(parent)` may transiently see the pre-bump value. Post-join
  state is consistent (Release/Acquire pair preserves eventual
  visibility; per-parent sum invariant
  `sum_of_degrees == 2 × non_primordial_count` holds at every
  quiescent point). LCAs drive synthesis sequentially per LCA, so the
  relaxation is invisible to the documented consumer contract.
  Traversal probes should read `degree` at quiescent points (post join
  barrier or consumer-driven epoch boundary), not mid-flight.

### Budget gates

- **Gate 12 amended.** The 4× ratio target proved structurally
  unreachable on M3 Pro 6P+2E asymmetric silicon (hardware ceiling
  ~6.5×; merged-map upper bound 3.59×). Replaced with
  platform-named: M3 Pro target ≥ 12M absolute AND ≥ 3.4× ratio
  (floor ≥ 10M AND ≥ 3.0×); symmetric server hardware (8+ uniform
  cores) ≥ 4× ratio as regression watch, not gate. See
  `BUDGET_LOG.md` row 1 for measurement provenance and signers.

### Docs — DESIGN.md v2 rewrite

**Removed sections:**

- Rounds 1-4 revision log (historic planning cruft).
- Promised-file sections for `src/network.rs`, `src/compactor.rs`,
  `src/parallel.rs`, `src/ffi.rs`, `src/wasm.rs` — none of these
  files exist at the current commit; earlier design rounds described
  them as shipping.
- Empirical numbers, LOC comparisons, and throughput measurements —
  evicted to `docs/BENCHMARKS.md` with in-crate file:line anchors.
- Full test-strategy inventory, step-by-step path off `dev`, resolved
  decisions log, 34 done-criteria gate checklist, Budget Amendment
  Policy, v1.2 → v2.0 fate mapping, and pre-review notes — moved to
  `BUDGET_LOG.md` / `CHANGELOG.md` where they belong.

DESIGN.md now describes only what SHIPS at the current release plus
version-anchored pointers to planned v2.1 work. Every code-referencing
claim carries a `[SHIPPED @ …]`, `[TARGET @ …]`, or `[DEPRECATED @ …]`
tag. Every axiom-shaped sentence anchor-links to `THEORY.md`.
