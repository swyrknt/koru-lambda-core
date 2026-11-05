# Changelog

Budget-gate amendments and substrate changes land under `## Unreleased`
so consumers see the ratchet before they migrate; `## Unreleased` below the
v2.0.0 section retains the pre-2.0.0 substrate close for provenance.

The breaking changes in the v2.0.0 section are derived from a mechanical
public-API diff against the published 1.2.0, not from recollection. That diff
is `api/DELTA-from-1.2.0.txt`, which is not part of the published package;
regenerate it with
`cargo public-api --simplified -p koru-lambda-core diff 1.2.0`.

## [2.0.0] - 2026-09-16

v2.0.0 introduces the projection primitive as a first-class read dual
to `synthesize`, the two-type discipline that lifts Axiom-4 boundary
enforcement from a runtime `debug_assert!` to a type-level API, and the
novelty-bit outcome enum that lets consumers observe Law 7 saturation
without extra state.

### Added

- **Projection primitive** — `engine.project(root)` builder returning
  `Projection` values. v2.0.0 lands the full API surface: 17 types +
  4 traits + wire codec, counted from the repository's public-API snapshot
  (`api/koru-lambda-core.txt`, not part of the published package). Types
  include `Boundary`, `Direction { Upstream, Downstream, Undirected }`, and
  the shipped `Signal` implementors `Adjacency`, `Degree`, `HopDistance`
  (see `src/projection.rs`). The builder chain
  `engine.project(root).direction(..).hops(..).signal(..).materialize()`
  returns a `Projection<'_, S: Signal>` carrying `projection_id()` +
  `canonical_bytes()`, both content-addressed on the spec
  `{ root, boundary, direction, signal }` at quiescence.

  This entry named three things that do not exist: a `Root` type (the
  builder takes a `Distinction`), a `Materialized<S>` return type (it is
  `Projection<'e, S>`), and a `Direction::Both` variant (it is
  `Undirected`). The count was 14. All four were written from the design
  rather than from the surface.
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

- **Cond D cross-engine falsifier probe** — `tests/projection_is_engine_independent.rs`
  ships as a `#[test]` converting `THEORY.md § The synthesis/projection
  dual` from a spec claim into a CI-attested theorem. Two independent
  engines with identical synthesis histories, queried with the same
  projection spec at quiescence, must produce byte-identical output.
  Every commit that touches synthesis, projection, or wire format either
  preserves Cond D or fails this test. Ships in commit `40b6d25`.

### Changed

The five entries below were absent from this section until 2026-11-04. They
are the largest breaks in the release, and they were missing because the
section was written from memory of recent work rather than from the diff.
Counts are unique items from `api/DELTA-from-1.2.0.txt`; the `--simplified`
diff lists each item once per public re-export path, so the raw line count is
roughly double.

- **BREAKING: the `subsystems` module is removed in full** — six submodules
  (`commitment`, `compactor`, `local_agent`, `network`, `parallel`,
  `validator`) and 21 public types, including `CommitmentAgent`,
  `ConsensusValidator`, `NetworkAgent`, `StructuralCompactor`,
  `ParallelBatchProcessor`, `ParallelSynthesizer` and their action, stats
  and batch types. This is ~500 of the 590 removed items on its own.

  Nothing replaces them in this crate, and that is the point of the release:
  a consensus validator, a network agent and a batch compactor are consumers
  of the substrate, not parts of it. `scripts/gates/crate_boundary.sh`
  asserts the dependency direction over the crate graph so the boundary is a
  property of the build rather than a sentence in a document.

- **BREAKING: the `ffi` module is removed in full** — 22 `extern "C"`
  functions, the five `KORU_*` status constants, and the three opaque
  `c_void` handle aliases `KoruEngine`, `KoruAgent`, `KoruValidator`.
  2.0.0 publishes no C ABI. A C consumer stays on 1.2.0.

- **BREAKING: a distinction's identity is sixteen bytes, not a `String`** —
  `Distinction::id(&self) -> &str` and `Distinction::new(String)` are both
  removed, and `Distinction` is now `#[repr(transparent)]` over `[u8; 16]`.
  Every identifier that crossed an API boundary as text in 1.2.0 crosses as
  bytes in 2.0.0; `to_hex()` and `RawDistinctionId::from_hex()` are the
  display and parse boundaries. `Relationship`, which was
  `pub type Relationship = (String, String)`, and `StateSnapshot` are
  removed with it.

- **BREAKING: `DistinctionEngine` loses the string-keyed and snapshot
  accessors** — `get_distinction_by_id(&str)`,
  `get_distinctions_snapshot()`, `get_relationships_snapshot()`,
  `get_state_snapshot()` and `new_shared()` are removed. The replacements
  are `has`, `snapshot_distinctions`, `snapshot_parentage` and
  `Arc::new(DistinctionEngine::new())`.

- **BREAKING: `synthesize`, `d0` and `d1` changed signature.** 1.2.0 took
  and returned references — `synthesize(&self, &Distinction, &Distinction)`,
  `d0(&self) -> &Distinction`. 2.0.0 takes and returns values:
  `synthesize(&self, Distinction, Distinction) -> Distinction`,
  `d0(&self) -> Distinction`. `Distinction` is `Copy` and sixteen bytes
  wide, so passing it by reference costs an indirection and buys nothing.
  Call sites drop their `&`.

  `primitives::Canonicalizable::to_canonical_structure` likewise moved from
  `&self` to `self` in the same pass, and has since gained a `&ByteFold`
  parameter.

- **BREAKING: `LocalCausalAgent` and `synthesize_causal_action` are not in
  the substrate** — both moved to the agent layer, along with
  `build_children_index`. The substrate ships `parents_of`, and
  agent-ness is a reading of the graph rather than a type: every
  non-primordial distinction is an advance of each of its parents, so a
  trait asserting that only some types have it was the wrong shape.

- **BREAKING: `Distinction::from_hex` is removed**, along with
  `impl FromStr`, `bytemuck::Pod` and `bytemuck::Zeroable`. External
  bytes enter as `RawDistinctionId` and become a `Distinction` only
  through `engine.verify()`. The `serde` adapter for `Distinction` is
  serialize-only; `RawDistinctionId` carries the deserializing half.

  This entry previously said `from_hex` was "retained for backward
  compatibility" and "makes no engine-membership claim". Both halves
  were a problem: the claim was enforced by a `debug_assert!` at the
  next `synthesize`, so it did not hold in release builds at all, and
  the entry went stale when ENG-4 removed the path. A parse cannot
  establish membership, which is why the parse now lives on the
  unverified type.

- **BREAKING: `project` and `project_arc` panic on a root this engine has
  not witnessed**, matching `synthesize`'s treatment of a foreign parent.
  Previously they accepted one and produced an empty cone, which serialized
  to a well-formed 55-byte §7 frame that `restore_projection` refused on
  every engine **including the one that produced it**. §7 defines canonical
  bytes as what this engine would produce, so emitting a frame the engine
  cannot reproduce broke the definition rather than merely answering oddly.

  Reachable in intended use, not only through misuse: a `Distinction` is
  `Copy` and carries no engine tag, and Law 8 makes holding a value another
  engine produced the designed case. The guard tests witness state rather
  than provenance — a root both engines have witnessed still projects, which
  is what content addressing requires.

  Found by a Python/Rust differential campaign
  (`experiments/python/diff_harness.py`), not by review.

- **`conformance/vectors.json` digest repinned** (`749a8d15…` →
  `bd8bdad3…`), and again no published identifier moved — one added
  `reject.cases` entry, `saturated_with_nonzero_boundary_payload`. §7 pins
  offset 22 to zero when the boundary tag is saturated, and nothing probed
  that byte. `conformance/reference/substrate.py` **accepted** such a frame:
  it rebuilt the projection from the payload it parsed and re-serialized the
  tampered byte, so its own comparison could not see it. The accepted frame
  carried a §8 identity no legitimate specification produces, so the two
  sides held different identities over bytes the receiver had accepted.
  Fixed there and pinned here, so both implementations are gated rather than
  one being fixed: reject cases 11 → 12, reference implementation 103 → 104.

- **BREAKING: `CanonicalBytes` requires `write_canonical(&self, &mut
  Vec<u8>)`** instead of `canonical_bytes(&self) -> Vec<u8>`.
  `canonical_bytes` remains as a provided method, so callers are
  unaffected; implementors move one line. The owned return allocated
  once per node in the cone, which measured 557–592 µs of a
  20,000-entry `Projection::canonical_bytes` against 104–118 µs
  writing in place — about 5×.

  The buffer passed to an implementation is a scratch buffer, not the
  wire frame. Given the frame, an encoder calling `out.clear()` would
  discard every entry already written and underflow the length
  arithmetic; the scratch copy costs one `memcpy` per entry and keeps
  that unreachable.

### Fixed

- `clippy.toml:1` and `rustfmt.toml:1` — project name comments corrected
  from a stale `forma-core` rename artifact to `koru-lambda-core`.
- `docs/development/GUARDRAILS.md:317-318` — CI badge URLs corrected
  from placeholder `github.com/you/forma-core/…` to the real
  `github.com/swyrknt/koru-lambda-core/…` repository path.

### Migration

A dedicated consumer migration guide is planned for v2.1. That guide
will ship the "delete your wrapper" playbook for consumers carrying
hand-rolled projection / novelty / verify shims.

**v2.0.0 is not API-additive.** This section said it was, and that it was
safe to "migrate at their own pace" — both were written before the three
breaking changes in *Changed* above landed, and neither was revisited. What
actually breaks:

- `Distinction::from_hex`, `impl FromStr`, `Pod` and `Zeroable` are gone.
  Hex parsing moves to `RawDistinctionId::from_hex` followed by
  `engine.verify()`. The `serde` adapter for `Distinction` is
  serialize-only.
- `CanonicalBytes` implementors move from `canonical_bytes(&self) ->
  Vec<u8>` to `write_canonical(&self, &mut Vec<u8>)`. *Callers* are
  unaffected — `canonical_bytes` remains as a provided method.
- `project` and `project_arc` now panic on a root the engine has not
  witnessed. Code that passed a foreign root was already producing frames
  nothing would accept, so nothing working breaks — but it fails loudly now
  instead of quietly.

- The `subsystems` and `ffi` modules are gone in full. There is no shim; a
  consumer of either stays on 1.2.0 or moves to the agent layer.
- `Distinction` no longer has a `String` identity. `id()` and
  `new(String)` are gone, `Relationship` and `StateSnapshot` with them.
- `synthesize`, `d0` and `d1` take and return `Distinction` by value.
  Drop the `&` at the call site; the compiler points at every one.

What genuinely did not change in signature: `parents_of`, `degree`, `has`
and `distinction_count`.

This list previously also named `synthesize`, and said a consumer using only
these is unaffected. Both were wrong, and the parenthetical said why without
noticing: the check was that each was "verified **present** in
`api/koru-lambda-core.txt`". Present is not unchanged. `synthesize` is in the
diff's *removed* set, because 1.2.0's took two `&Distinction` and 2.0.0's
takes two `Distinction` — a break at every call site of the most-used
function in the crate, reported as a compatibility guarantee.

#### npm breaking changes

For npm consumers upgrading from `koru-lambda-core@1.2.0`, the
JS/wasm-bindgen surface is **not** additive. `WasmValidator`,
`WasmCommitmentAgent`, and `WasmNetworkAgent` are retired with no
replacement; `WasmEngine`'s method signatures move from string-typed to
bytes-canonical; and `d0Id()` / `d1Id()` return different bytes than
they did in 1.2 (16-byte primordials, not UTF-8 of `"0"` / `"1"`) — the
one silent break in this release.

This paragraph linked `docs/MIGRATION_JS.md` for "the full recipe list".
That document is not in the published package and still describes an
`init`/`initSync` surface that the npm package deferred with the bindings,
so the link sent a reader of the shipped changelog to a file they do not
have, for guidance that no longer applies. The three retired classes and
the `d0Id()` / `d1Id()` byte change above are the whole of the break.

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

**Substrate facts pinned at the substrate milestone:**
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

- **`conformance/vectors.json` digest repinned** (`9acd5c50…` →
  `749a8d15…`), and no published identifier moved. The change is in
  `reject.cases` only: the case named `tampered_entry_output_len`
  altered byte 63, which `docs/SPEC.md` §7 places inside the first
  entry's 16-byte identifier (55..71), not its `u32 output_len`
  (71..75) — the header is 55 bytes, not 47, because the entry count
  follows the signal identity.

  Both implementations rejected it, so both looked correct, and the
  entry array's own length field had nothing probing it in either one.
  The case is renamed `tampered_entry_identifier` — which is what it
  tests — and a real length probe at offset 71 added beside it. The
  Rust test carried the same wrong offset with a `47 + 16` comment and
  now asserts both. Reject cases 10 → 11; the reference
  implementation reproduces 103/103.

- **Single-map storage refactor** — `DistinctionEngine` storage layout
  changed from three side-by-side `DashMap`s
  (`all_distinctions` / `parents_of` / `degree_counts`) to one
  `DashMap<[u8;16], EngineNode { parents, degree }>`. Public API
  unchanged in signature; performance: single-thread +20%, 8-thread
  +46%, parallel-scaling ratio 2.85× → 3.43× on M3 Pro. See commits
  `30cc77f` (verification mock), `fd9c2b1` (refactor), `cd6ca3f`
  (falsification tests).

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

### Docs — design-document rewrite

This subsection described the rewrite of `DESIGN.md`, which is not part of
the published package. It is kept for provenance, with the file named rather
than linked.

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
