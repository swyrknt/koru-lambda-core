[SHIPPED @ release/2.0.0]
Manifest version: 2.0.0

# koru-lambda-core v2.0.0 — Naming the projection dual

The substrate has always produced two structural objects: the append-only
graph the operator writes, and any consistent read over it — the projection.
[`THEORY.md` names both](THEORY.md#the-synthesis-projection-dual). v2.0.0
makes that naming operational: at E02 completion, the substrate exposes an
API surface for the projection dual. Six independent koru projects have
built projection-shaped wrappers under different names — Fields (alis-ai),
Fields (koru-engine), Sessions (koru-wave), Peers (koru-mesh), Workspaces
(koru-delta), PlayerState (game-studio consumers). What they share is what
the axioms forced them to build. v2.0.0's contribution is that the
substrate now provides it directly.

Byte-identical across engines. No consensus. No handshake. Consumer
migration to the substrate primitive is the subject of E04, sequenced
after E02 completion.

---

## How this document relates to the others

Three shipping documents share the substrate's authoritative surface,
each with a narrow charter:

- **`THEORY.md`** — one operator, four axioms, two primordials, eight
  structural laws. Any claim about *what the substrate is* lives here,
  or nowhere. Downstream docs paraphrase; they do not restate.
- **`DESIGN.md`** (this file) — the shipping story: what the substrate
  ships today at the current commit, what E02/E03/E04 change, and the
  anti-scope that keeps v2.0.0 honest. Version binding lives here and
  nowhere else.
- **`docs/BENCHMARKS.md`** — capacity and throughput measurements, with
  every quantitative claim citing an in-crate `benches/*.rs`,
  `tests/*.rs`, or `examples/*.rs` file at line-level. Numbers without
  an in-crate anchor get removed, not extrapolated.

Every axiom-shaped sentence in this document is an anchor-link back to
`THEORY.md`. Every code-referencing claim in this document carries a
`[SHIPPED @ …]`, `[TARGET @ …]`, or `[DEPRECATED @ …]` tag. Any
sentence that violates either rule is a drift report against this file.

**Anchor slug convention.** The `THEORY.md#section-slug` links in this
document use a simplified GitHub-style slug: heading text lowercased,
non-alphanumeric characters (including em-dashes, equals, and minus
signs) dropped, remaining runs of whitespace converted to single
hyphens. This is not always identical to GitHub UI's canonical
`github-slugger` output for headings containing spaced punctuation
(e.g., "Law 8 — Engine independence"). The full inventory and
convention are documented in
`.claude/warroom/epics/E01-v2-baseline-alignment/S01-design-doc/phase-5-execute/anchor-budget.md`.
The real enforcement tool (`theory_anchor_check.sh`) lands in E01-S05
and will normalize slugs to whichever canonical form the trio of docs
adopts. Between now and S05, readers may encounter dead links in the
GitHub UI for headings with spaced punctuation; content is still
findable by search.

---

## Who this release is for

The v2.0.0 preparation on `release/2.0.0` is for two audiences:

- **Contributors** — the substrate's docs now match its code.
  `THEORY.md` is authoritative for axioms and laws; this document is
  authoritative for the shipping story; `docs/BENCHMARKS.md` is
  authoritative for capacity and throughput. A new contributor can
  read the three docs in order and produce a mental model that
  survives contact with `src/`.

- **Downstream consumer teams building on 1.x** — the projection dual
  you have been reinventing is being named. v2.0.0 does not yet
  require you to migrate; E04 will offer the migration guide when the
  API surface (E02) is stable. Between now and then: pin to `1.2.0`,
  read [`THEORY.md § synthesis/projection dual`](THEORY.md#the-synthesis-projection-dual)
  for the concept, watch E02 for the API.

This is not a "delete your wrapper" promise — that pitch belongs in E04's
migration guide, after the API is stable enough to migrate onto. This is
the release that *names the object* your wrapper has been implementing.

---

## Path to v2.0.0

Current state: `release/2.0.0`. Manifest version 2.0.0 (bump landed
at E02 completion). What shipped at E02 completion: projection
primitive API surface, `Cargo.toml` version bump 1.2.0 → 2.0.0, no C
ABI added.

Six preconditions for the version bump:

1. E02 primitive API surface lands.
2. `clippy.toml:1` `forma-core` string corrected. `[SHIPPED @ E02-S06]`
3. `Cargo.lock` policy affirmed (currently checked in for the library —
   retained; policy note added to the release commit).
4. All E01 stories closed (S01 + S03 + S04 + S05).
5. E03 BLOCKING closed (`TransactionBatch` deserialize cardinality cap).
6. `theory_anchor_check.sh` real implementation lands (E01-S05).

*This section describes preconditions for a version bump; nothing here
is a shipping commitment.*

Release mechanics — dates, coordination, tag sequencing, rollback
plan — live in
[`RELEASE_PLAN.md`](.claude/warroom/epics/E01-v2-baseline-alignment/RELEASE_PLAN.md).

---

## The substrate

`koru-lambda-core` implements one operator, four axioms, two primordials,
and eight structural laws that follow as consequences. Theory content
below is anchor-linked to [`THEORY.md`](THEORY.md), which is authoritative;
this section describes what the code ships today plus the anchor-tagged
targets that land in E02/E03.

### The operator

The substrate exposes one write-side entry point:
[`synthesize(a, b)`](THEORY.md#the-operator) — two distinctions in, one
distinction out, with a structural `novel?` bit distinguishing a
first-time derivation from a Law 7 saturated repeat. The runtime path
enforces [the four axioms](THEORY.md#the-four-axioms) directly:

- Foreign-byte guard on both parents (Axiom 4 boundary enforcement)
- Irreflexivity: `a == b` returns `a` directly
- Commutativity: canonical `(min, max)` ordering on raw bytes before hashing
- Content addressing: 16-byte SHA-256 prefix of the canonical pair
- Saturation: `contains_key` fast path before entry lock

`pub fn synthesize` `[SHIPPED @ src/engine.rs:605-611]`. The bare
`synthesize` API returns `Distinction`, discarding the novelty bit;
the parallel `synthesize_novel` entry point returns
`SynthesisOutcome::Novel(Distinction) | Existing(Distinction)`
`[SHIPPED @ src/engine.rs:708-713]`. Both route through the shared
`synthesize_inner` implementation `[SHIPPED @ src/engine.rs:740-840]`.

**Two-type discipline (Axiom-4 closure).** Bytes typed as `Distinction`
that were not produced by the operator are structurally illegitimate.
[`THEORY.md § The operator`](THEORY.md#the-operator) names the API-level
convention: raw bytes admitted only as `RawDistinctionId`, converted to
`Distinction` via `engine.verify()`, foreclosing foreign-byte injection at
the type level. That API surface is `[SHIPPED @ src/engine.rs:158-177]`
(`RawDistinctionId`) and `[SHIPPED @ src/engine.rs:991]` (`verify`).
The runtime closure remains as the debug-mode `debug_assert!`
foreign-byte guard at `[SHIPPED @ src/engine.rs:747-756]`; release
builds trust the contract.

### The Distinction type

`pub struct Distinction(pub(crate) [u8; 16])` `[SHIPPED @ src/engine.rs:71]`
— a 16-byte newtype with:

- `#[repr(transparent)]` — zero-copy FFI / persistence layout.
  `*const Distinction` and `*const [u8; 16]` are layout-compatible.
- `Copy + Clone + Eq + Hash + Ord` — by-value passage across threads,
  no allocations. `Ord` via derived lexicographic byte comparison
  matches the canonical `(min, max)` ordering used inside `synthesize`
  so consumers get `BTreeMap<Distinction, _>` and `slice.sort()`
  without boilerplate.
- `bytemuck::Pod + Zeroable` derives — zero-copy `&[Distinction] ↔ &[u8]`
  conversions without `unsafe` on the consumer side. Free because the
  layout is just 16 bytes of plain data.
- `pub(crate)` field — no public constructor. Foreign-ID poisoning
  closed structurally at compile time.
- Primordials: `d0 = Distinction([0; 16])`, `d1 = Distinction([1, 0, …, 0])`.

Hex round-trip via `to_hex` / `from_hex` / `Display` / `Debug` at
`[SHIPPED @ src/distinction_hex.rs]`, with typed `ParseError` returned
from `from_hex` (length + charset validation). `from_hex` parses bytes
but does not verify engine registration — callers must obtain
`Distinction` values from the engine they intend to use. Cross-engine
byte injection is caught by the two-type discipline above.

**Substrate-wide `#[must_use]` policy.** Every function that produces a
`Distinction` is `#[must_use]`. Dropping a synthesis result is always a
bug (you computed a distinction and threw it away). Applies to
`engine.synthesize`, `engine.parents_of`, `engine.degree`, `engine.d0`,
`engine.d1`, `Distinction::as_bytes`, `Distinction::from_hex`,
`Distinction::to_hex`, `LocalCausalAgent::get_current_root`,
`LocalCausalAgent::synthesize_action`, `synthesize_causal_action`,
`snapshot_parentage`, `replay_topological`, `build_children_index`,
`SynthesisRecorder::new`, `SynthesisRecorder::log`.

### Engine state

One `DashMap<[u8; 16], EngineNode { parents, degree }>` plus two
primordial constants:

- `pub struct DistinctionEngine` `[SHIPPED @ src/engine.rs:494]`
- `struct EngineNode` `[SHIPPED @ src/engine.rs:453]`
- `pub struct IdentityHasher` `[SHIPPED @ src/engine.rs:206]`

`EngineNode`'s two fields serve the three canonical O(1) projections
the theory names:

| Projection | Theory anchor | Field access |
|---|---|---|
| Saturation check | [Law 7](THEORY.md#law-7-saturation) | `nodes.contains_key(id)` |
| Parent lookup | [Law 5](THEORY.md#law-5-binary-parentage) | `nodes.get(id).parents` |
| Degree query | [Law 11](THEORY.md#law-11-fold-law) + [Law 12](THEORY.md#law-12-coding-law) | `nodes.get(id).degree.load(Acquire)` + genesis addend |

Three side-by-side maps from an earlier design (`all_distinctions` /
`parents_of` / `degree_counts`) collapsed into this single map at
Step 1e (public API signatures unchanged; single-thread +20%,
8-thread +46% on M3 Pro). See `CHANGELOG.md § Step 1e merged-map
refactor` for the provenance.

### IdentityHasher

DashMap uses a specialized hasher `[SHIPPED @ src/engine.rs:206]` that
takes the leading 8 bytes of the 16-byte SHA-256 prefix directly as the
u64 hash — no XOR, no rotation, no diffusion math. SHA-256 prefixes are
already uniformly distributed; a general-purpose hasher would waste
cycles re-mixing entropy. Speedup numbers live in `docs/BENCHMARKS.md § Throughput`.

The hasher carries structural guards: `debug_assert!` on 16-byte keys
plus `unreachable!()` on every non-`write` `Hasher` method (write_u8,
write_u16, …, write_length_prefix). Misuse — hashing a slice, a
non-16-byte key, or a typed integer — triggers an immediate panic
instead of silently corrupting state. v2.0.0 has no tuple keys, so the
hasher only ever sees single 16-byte writes.

`pub type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>`
`[SHIPPED @ src/engine.rs:274]` is what the `DashMap` field type
parameterizes on.

### Synthesize hot path — concurrency contract

`pub fn synthesize` `[SHIPPED @ src/engine.rs:605-611]` and
`synthesize_novel` `[SHIPPED @ src/engine.rs:708-713]` share the
`synthesize_inner` hot path `[SHIPPED @ src/engine.rs:740-840]` and
are race-free under concurrent synthesis. Shape:

1. **Foreign-byte guard** on both parents (`debug_assert!` on
   `nodes.contains_key(&parent.0)`). Debug-only enforcement of the
   two-type discipline described above; release builds trust the
   contract.
2. **Irreflexivity check** — `a == b` returns `a` directly, before any
   hashing (Axiom 3).
3. **Canonical ordering** — `(first, second) = if a.0 <= b.0 { (a, b) } else { (b, a) }`
   (Axiom 2).
4. **Content addressing** — SHA-256 over the canonical pair, leading
   16 bytes as the child's identity (Axioms 1 + 4).
5. **Saturation fast path** — `contains_key(&new_bytes)` check before
   the entry lock (Law 7). Hot-path optimization: avoids any shard
   write-lock traffic on repeat calls.
6. **Entry-gated insert** — `nodes.entry(new_bytes)` on `Vacant` runs
   the closure that inserts the new `EngineNode`. Exactly one thread
   wins the entry-vacant dispatch per novel child.
7. **Parent degree bumps** — after the child's shard write-lock
   releases, both parents' `degree.fetch_add(1, Ordering::Release)`
   fire (B1 mitigation — parent and child may hash to the same shard;
   holding the child-shard lock across a parent-shard operation could
   deadlock if a peer thread races the mirror pair).

**Memory ordering contract.** `fetch_add(1, Release)` on parent degree
pairs with `Ordering::Acquire` loads in `pub fn degree` so probes
reading `node.degree` directly (without first observing the new child)
still get a happens-before edge to the writing synthesis. The
Release/Acquire kernel is
`[SHIPPED @ tests/loom_kernel.rs]` — loom verifies the abstract
memory-model interleavings; TSan on the concurrent-write
byte-equivalence test verifies the DashMap-shard side. Neither alone
covers both; both are required.

**Law 8 quiescence qualifier.** Two engines processing the same
operations produce byte-identical state
[at quiescence](THEORY.md#law-8-engine-independence); mid-flight
transient divergence is permitted while writes are in progress. The
claim is post-processing convergence, not instantaneous equality.
LCAs drive synthesis sequentially per LCA, so the relaxation is
invisible to the documented consumer contract; probes reading
`node.degree` mid-flight are responsible for their own quiescence
boundary (post-join barrier, epoch boundary, etc.).

**Why `AtomicUsize::fetch_add` not `Vec::push`.** Earlier design rounds
kept a `children_of: DashMap<[u8;16], Vec<Distinction>>` for
enumerating a distinction's children. d₀ and d₁ accumulate millions of
children via the Fold Law; a `Vec<Distinction>` reallocates O(log N)
times under the shard write-lock and stalls every other thread trying
to synthesize against d₀ or d₁. `AtomicUsize::fetch_add` is
constant-cost, lock-free, and the count itself is what
[Coding Law](THEORY.md#law-12-coding-law) actually names. Consumers
wanting children iteration call `build_children_index` on a snapshot
(see below).

### Structural invariants surfaced

`pub fn check_structural_invariant` `[SHIPPED @ src/engine.rs:1068]`
returns `Result<(), InvariantError>`
`[SHIPPED @ src/engine.rs:319]`. The check tests
[Law 5 binary parentage](THEORY.md#law-5-binary-parentage) via the
[Law 6 r = 2d − 3](THEORY.md#law-6-r--2d--3) accounting:
`nodes.len() == parents_of_count + 2` (every non-primordial has parents
recorded once; +2 accounts for the primordials, which by definition have
no parents). A failure is a theory event — the engine no longer
satisfies Law 6 and must be redesigned, not amended.

### LocalCausalAgent (substrate-level trait)

`pub trait LocalCausalAgent` `[SHIPPED @ src/agent.rs]` captures the
substrate's reference consumption pattern:

- an LCA anchors to a **local root distinction** — its perspective;
- state transitions are **causal syntheses** from local root + the
  canonical structure of the action being taken;
- LCAs **update their perspective forward** as their causal chain
  advances.

The trait exposes `get_current_root` and `update_local_root` as the
core surface, plus a default `synthesize_action` method that composes
them via the helper `pub fn synthesize_causal_action`
`[SHIPPED @ src/agent.rs]`. The helper canonicalizes the action's data
through the engine (via `Canonicalizable`) and synthesizes the result
with the local root, returning a new root.

See
[`THEORY.md § what it means to use the substrate`](THEORY.md#what-it-means-to-use-the-substrate)
for the pattern's theoretical status — reference, not axiom. Consumers
with different shapes (multi-perspective, non-root, no-perspective) can
exist and still receive the substrate's axiom-level correctness
guarantees; the trait exists so LCA-shape consumers interoperate.

The trait lives at `[SHIPPED @ src/agent.rs]` (substrate level, not
under `subsystems/`) because it formalizes what it means to *use* the
substrate. Subsystems are *implementers* of the trait, not the trait's
home.

### Reference observers (consumer-side, not engine state)

Three reference implementations ship in-crate demonstrating consumer
patterns; none extend engine state. Consumers wanting none of these
pay nothing.

- **`SynthesisRecorder`** `[SHIPPED @ src/recorder.rs]` — chronological
  record of novel syntheses through a single engine. Consumer
  constructs one, routes `synthesize` calls through it; the recorder
  pushes to its internal `Vec` only on novel results (deduped via
  membership check). `!Send + !Sync` by construction via
  `PhantomData<*const ()>` marker — single-thread use enforced at
  compile time. This is NOT an LCA (LCAs anchor to a local_root and
  evolve; the recorder is a passive observer with no perspective of
  its own).

- **`snapshot_parentage` + `replay_topological`**
  `[SHIPPED @ src/replay.rs]` — engine persistence + round-trip
  reconstruction. `snapshot_parentage(engine)` dumps the parent map;
  `replay_topological(snapshot)` rebuilds a fresh engine by repeatedly
  calling `synthesize` on entries whose parents are already
  registered, until quiescent. Returns `Result<_, ReplayError>` with
  release-safe content-address mismatch detection: the release build
  checks that each `synthesize` result matches the recorded child
  bytes and returns `ReplayError::Mismatch` on tampered parentage,
  `ReplayError::Unreachable` on cycles or missing parents, and
  `ReplayError::MissingPrimordial` when the input cannot root in d₀
  or d₁. The round-trip identity is
  [Law 9 order-independent reconstruction](THEORY.md#law-9-order-independent-reconstruction):
  replaying a shuffled parentage snapshot produces byte-identical state.

- **`build_children_index`** `[SHIPPED @ src/replay.rs]` — materializes
  the inverse of `parents_of` in O(N) once for consumers that need to
  iterate children rather than just count them. Takes a parentage
  snapshot, returns `HashMap<Distinction, Vec<Distinction>>`.
  Snapshot-in-time semantics — concurrent syntheses against the live
  engine after the snapshot was taken do not appear.

These aren't substrate. They're worked examples of how to use it. All
three are pure functions or small structs that consume the substrate;
the engine is unaware of them.

### Reference subsystems

Two reference LCA implementations demonstrate the pattern with running
consensus code at the current commit:

- `[SHIPPED @ src/subsystems/validator.rs]` — `ConsensusValidator`
  with V3 (data cap), V4 (oversized-root clipping), V5 (atomic-failure
  pre-validation), V6 (atomic `restore_state`), V8 (empty-data
  by-design) designed-in as non-existent bugs. Pre-validation pass
  before any `synthesize` call — engine state is invariant on
  rejection. Typed errors on the public surface; explicit thresholds;
  no magic constants.

- `[SHIPPED @ src/subsystems/commitment.rs]` — `CommitmentAgent` +
  `BatchCommitment`. `BatchCommitment::compute` hashes `leader_id` (N6
  designed-in); `TransactionBatch::previous_root` typed as
  `Distinction` — the N5 String-truncation bug literally cannot exist
  because there is no string to truncate. `pending_commitments` LRU
  cache with documented cap derivation
  (`COMMITMENT_CACHE_CAP` pinned in-file). `TransactionBatch`
  deserialize cardinality cap is `[TARGET @ E03]` (BLOCKING for tag).

Common design principles across subsystems:

1. **Each subsystem IS an LCA.** Implements
   [the reference consumption pattern](THEORY.md#what-it-means-to-use-the-substrate)
   directly; the trait's semantics are the only contract.
2. **Bug-correct from the start.** Audit-discovered bugs from earlier
   design rounds are designed-in as non-existent (typed fields,
   pre-validation, atomic restore, hashed inputs) rather than fixed
   retroactively.
3. **Typed errors throughout.** No `Result<T, String>` on public
   surfaces. `#[non_exhaustive]` on public error enums.
4. **No magic constants.** Every threshold is a named `const` with a
   docstring explaining its source.
5. **`#[serde(try_from = "Raw")]`** on any type with construction
   invariants — deserialization can't bypass the constructor.

**Subsystems are NOT** production consensus (use `koru-protocol`),
configuration knobs (parameters are consumer choices), or optimal
under every workload (they optimize for clarity). If a subsystem is
growing past budget, either it's accreting bugs (fix at design level)
or accreting features (belong in the consumer's own LCA implementation).

### Direct dependencies

Beyond what dev already pulls (`dashmap`, `sha2`, `serde`, `lru`,
`rayon`, `hex`):

- `bytemuck = { version = "1", features = ["derive"] }` — `Pod` +
  `Zeroable` derives on `Distinction` for zero-copy slice views
  without `unsafe`. `derive` feature is NOT default; pinning is
  mandatory or the derive macros are unavailable and the substrate
  will not compile.
- `thiserror = "1"` (already in dev) — typed errors (`ParseError`,
  `ReplayError`, `PeerIdentityError`, `InvariantError`).
- `static_assertions = "1"` (dev-dep) — compile-time trait assertions
  for `Distinction: Copy + Send + Sync + Pod` and
  `SynthesisRecorder: !Send + !Sync`.
- `loom = "0.7"` (dev-dep) — memory-ordering model checker for the
  Release/Acquire kernel in `synthesize` + `degree`.
- `dhat = "0.3"` (dev-dep) — per-distinction memory probe. See
  [`docs/BENCHMARKS.md § Capacity`](docs/BENCHMARKS.md).
- `blake3 = "1"` (dev-dep) — alternative hash for differential-test
  reference against SHA-256; the substrate ships SHA-256 (Axiom 4
  content-addressing contract) but the differential test catches
  bytes-injection regressions.

FFI / WASM heavyweight dependencies (`parking_lot`, `console_error_panic_hook`,
`wasm-bindgen`, `wasm-bindgen-test`) are NOT pulled at this commit — see
anti-scope. The `wasm` feature flag remains declared in `Cargo.toml` for
future E-series work; no `src/` code is currently gated on it.

No new heavyweight deps. Every addition serves a specific design goal.

### Edition, MSRV, workspace

- **Edition:** Rust 2021. v2.0.0 does not bump to 2024 — an orthogonal
  concern that would expand the migration surface for consumers
  without delivering substrate value.
- **MSRV:** `rust-version = "1.80"` in `Cargo.toml`. Pinned because
  `DashMap 6` requires 1.71 and `bytemuck::Pod` derive is stable on
  1.74; 1.80 leaves headroom for `LazyLock`/`OnceLock` usage without
  surprising consumers. Bumping MSRV is a breaking change requiring
  its own minor-version release after v2.0.0.
- **Workspace:** single-crate, not a workspace member. `experiments/`
  has its own `Cargo.toml` and does not ship in the published crate.
  The published `koru-lambda-core` is one `Cargo.toml`, one crate,
  one published artifact.

### Test strategy — one-paragraph reference

The substrate ships with axiom-verification tests (isolated
determinism / commutativity / irreflexivity / content-addressing),
scale probes ([`r = 2d − 3`](THEORY.md#law-6-r--2d--3) at 5 M synths
with zero deviations, saturation at 1 M repeats), concurrency probes
(8-thread byte-equivalence, loom kernel for Release/Acquire, TSan on
the FFI concurrent test in E03), and misuse-detection tests
(cross-engine foreign-byte injection debug panic, `SynthesisRecorder`
`!Send + !Sync` compile-time assertion, `replay_topological`
release-safe mismatch/unreachable/missing-primordial errors). Full
test inventory lives in `src/**/tests` inline modules and in
`tests/`. Coding Law ρ ≥ 0.985 is asserted at
`[SHIPPED @ experiments/runner/tests/coding_law.rs:74-127]` against a
pinned exp18 corpus.

### Why the code looks the way it does — grounded decisions

Four structural decisions shape the current substrate. Each is
recorded so a new contributor can distinguish "the code is like this
because the theory forces it" from "the code is like this because an
earlier alternative was rejected." Only the second class is amendable
without a theory change.

1. **No engine-side synthesis log.** The engine carries no ordered
   history of syntheses. Rationale:
   [the substrate is timeless](THEORY.md#what-it-means-to-use-the-substrate);
   order is what consumers do. Chronological observation is a
   consumer concern — `SynthesisRecorder` is the reference
   implementation, not engine state. This decision is theory-forced
   (Laws 8 + 9); reversing it would require a theory amendment.

2. **`degree_counts` over `children_of`.** The engine records a
   per-node `AtomicUsize` participation count instead of a per-node
   `Vec<Distinction>` children list. Rationale:
   [Coding Law](THEORY.md#law-12-coding-law) names *degree* — the
   count — explicitly; children enumeration is the derived form.
   Every audit-verified caller (probes, subsystems, tests) uses
   `degree(d)`; zero use children iteration. `Vec::push` under the
   d₀/d₁ shard write-lock reallocs O(log N) times up to Fold-Law
   scale, stalling every peer thread. `AtomicUsize::fetch_add` is
   constant-cost and lock-free. Reversing this decision means
   accepting the reallocation stall for a use case no consumer has;
   theoretically legal, engineeringly wrong.

3. **`LocalCausalAgent` at substrate level, not under `subsystems/`.**
   The trait IS substrate — it formalizes what it means to use the
   substrate. Filing it under `subsystems/` misnames its role.
   `[SHIPPED @ src/agent.rs]` is the correct home.

4. **`previous_root: Distinction` on `TransactionBatch`.** The N5
   String-truncation bug from v1.2.x cannot exist because there is no
   string to truncate. Content addressing (Axiom 4) says identity is
   bytes; string typing an identity field is a category error.
   Reversing this decision reintroduces N5 by construction; it is
   not on any table.

Non-decisions worth naming (things the theory permits but the
engine does not currently expose):

- The engine does not expose `remove_distinction`, `clear`, or any
  non-monotone mutation. Append-only is a *design choice* enabling
  Laws 8 + 9 to hold cleanly; the axioms
  [don't forbid removal](THEORY.md#what-follows-from-the-theory).
  Garbage collection by partial-reroot replay is theoretically
  permissible; v2.0.0 deliberately excludes it.
- The engine does not expose `SynthesisOutcome`; the novelty bit is
  discarded at the current commit. E02 changes this.
- The engine does not accept `RawDistinctionId` at the API surface;
  foreign-byte injection is caught by debug-mode `debug_assert!`
  today. E02 promotes this to type-level.

### API surface at the current commit

Public entry points a v1.x consumer would touch. Every method carries
a `[SHIPPED]` tag by construction (release/2.0.0); items added at
later epics carry a `[TARGET]` tag on the relevant line-item.

- `DistinctionEngine::new()` / `Default::default()` — bootstrap;
  inserts d₀, d₁; asserts `distinction_count() == 2`,
  `relationship_count() == 1`.
- `engine.synthesize(a, b) -> Distinction` — the write path.
- `engine.parents_of(d) -> Option<(Distinction, Distinction)>` — O(1)
  parent lookup; `None` for primordials and unregistered bytes.
- `engine.degree(d) -> usize` — [Law 12](THEORY.md#law-12-coding-law)
  degree query; participation count + genesis addend.
- `engine.distinction_count() -> usize` — O(1) size.
- `engine.relationship_count() -> usize` —
  `(nodes.len() - 2) * 2 + 1`; direct
  [Law 6](THEORY.md#law-6-r--2d--3) accounting.
- `engine.check_structural_invariant() -> Result<(), InvariantError>` —
  release-safe theory-event probe.
- `engine.d0()` / `engine.d1()` — primordial accessors.
- `engine.has(d) -> bool` — registration probe; drives
  `replay_topological`'s topological pass.
- `Distinction::as_bytes(&self) -> &[u8; 16]` — canonical bytes.
- `Distinction::from_hex(s: &str) -> Result<Self, ParseError>` — hex
  round-trip parse. Does not verify engine registration.
- `Distinction::to_hex(&self) -> String` — 32-char lowercase hex.
- `snapshot_parentage(&engine) -> Vec<(Distinction, ParentPair)>` —
  persistence snapshot.
- `replay_topological(snapshot) -> Result<Arc<DistinctionEngine>, ReplayError>` —
  round-trip reconstruction.
- `build_children_index(&snapshot) -> HashMap<Distinction, Vec<Distinction>>` —
  inverse projection for children-iteration consumers.
- `SynthesisRecorder::new()` / `SynthesisRecorder::synthesize(&mut self, &engine, a, b)` /
  `SynthesisRecorder::log(&self) -> &[Distinction]` — chronological
  observer.
- `LocalCausalAgent` trait — reference consumer pattern.
- `synthesize_causal_action(root, action, &engine) -> Distinction` —
  LCA helper.

Every entry above resolves to a `[SHIPPED @ src/…]` file path per the
substrate description sections above. `SynthesisOutcome` and the
`RawDistinctionId → engine.verify() → Distinction` type-level API
extend this surface at E02.

---

## What v2.0.0 changes (TARGET-tagged forward-pointers)

This section names the deltas from the current shipping state to v2.0.0.
Each item points to the epic that lands it. Tags follow the
`[TARGET @ Enn]` convention on any code-referencing claim.

### E02 — projection primitive API surface

The substrate has, until now, exposed only the write dual
(`synthesize`). The read dual has lived in six ad-hoc reinventions
across the ecosystem (see the headline). E02 exposes the projection
API surface directly, respecting
[the projection dual as theory-forced](THEORY.md#the-synthesis-projection-dual):
a projection is not new structure; it is the graph as viewed from
somewhere. Cross-engine projection independence is the falsifier —
two engines with the same synthesis history queried with the same
projection `{ Root, boundary, Direction, Signal }` at quiescence must
produce byte-identical output. `[SHIPPED @ E02-S05]`.

**Novelty bit home.** The operator produces `(child, novel?)` per
[`THEORY.md § The operator`](THEORY.md#the-operator); the bare
`synthesize` API still returns `Distinction`, discarding the novelty
bit. v2.0.0 exposes the novelty bit via a parallel `synthesize_novel`
entry point returning `SynthesisOutcome::Novel(Distinction) |
Existing(Distinction)` (a `#[non_exhaustive]` enum, not a struct with
fields — the enum shape lets future variants land without breaking
match arms). `[SHIPPED @ E02-S03]`. This is one paragraph in one place
— not a section header — matching Logic Enforcer's T13 placement
discipline: the
theory-side status of the novelty bit is already fixed in
[`THEORY.md § Implications not yet materialized`](THEORY.md#implications-not-yet-materialized);
DESIGN.md merely names the API surface that materializes it.

**Two-type API surface.** `RawDistinctionId → engine.verify() →
Distinction`. The runtime closure is `[SHIPPED @ src/engine.rs:747-756]`;
the type-level API is `[SHIPPED @ src/engine.rs:158-177]`
(`RawDistinctionId`) and `[SHIPPED @ src/engine.rs:991]` (`verify`) —
see substrate description above.

**Signal axis forward-compat.** The projection dual carries a `Signal`
axis (see [`THEORY.md § synthesis/projection dual`](THEORY.md#the-synthesis-projection-dual)).
E02's initial API surface will expose a bounded set of signals
sufficient for the flagship consumers named above (adjacency, degree,
hop-distance). Future signals — novelty rate, saturation-boundary
probes, cross-vantage intersection, structural attention — are
exploration land, not shipping targets. Naming them here prevents
v2.0.0 from hard-closing the read dual and forcing a v3.0.0 bump when
the next novel signal lands.

### E03 — subsystems hardening

Two hardening deliverables land at E03; both are BLOCKING for the
v2.0.0 tag:

- **`TransactionBatch` deserialize cardinality cap** `[TARGET @ E03]` —
  the current `TransactionBatch::previous_root: Distinction` typing
  closes the N5 String-truncation class at the type level, but a
  hostile-input `Vec<Transaction>` field can still allocate
  unbounded memory during deserialization. E03 adds a
  `#[serde(try_from = "Raw")]` wrapper with a documented cardinality
  cap so `Deserialize` can't produce an outsized batch. The cap value
  is derived from the same `MAX_LEADER_ID_LEN`-anchored arithmetic
  the LRU cache uses.
- **Concurrent-read visibility contract doc** `[TARGET @ E03]` — the
  Release/Acquire kernel described above ships today; formalizing the
  "read `node.degree` at quiescent points" contract in an in-crate
  ADR (`docs/adr/`) lets consumers reason about traversal probe
  correctness without re-reading the substrate source.

Validator + commitment paths get a hardened review pass at the same
time — no new features, only bug-class closures a pre-tag audit
surfaces.

### E04 — consumer migration

Full migration guide from v1.x public API to v2.0.0. Includes
per-version-pin (0.1.0 / 1.1.0 / 1.2.0) diffs, worked-example wrapper
deletions for the six ecosystem projections named in the headline, and
the "delete your wrapper" pitch after the API is stable enough for that
claim to be honest. `[TARGET @ E04]`. Sequenced after E02 completion.

E04's structural role is that it is the *only* place downstream teams
should read migration prose. This document names categories (see the
Migration categories section below); THEORY.md names axioms and
laws; E04 names diffs. Any migration-shaped sentence outside E04 is
either a category summary (belongs here) or a drift-report.

---

## Anti-scope — what v2.0.0 does NOT ship

Enumerated. No hedging.

### Deleted files and non-claims

- **`network.rs`, `compactor.rs`, `parallel.rs`, `ffi.rs`, `wasm.rs`**
  — earlier design rounds described these files as shipping. They do
  not exist in `src/` at commit `7549860`; every prior section
  describing them has been removed. See `CHANGELOG.md § Removed
  sections` for the excision record.

- **v2.0.0 does not add a C ABI.** The `cdylib` / `staticlib`
  crate-types in `Cargo.toml` produce empty C-boundary artifacts at
  this commit; they are retained for a future FFI epic but claim no
  C API surface today.

- **No axiom or law claims not already in `THEORY.md`.** This document
  makes no theoretical claims of its own; every axiom-/law-shaped
  sentence in DESIGN.md is an anchor-link to a heading in `THEORY.md`.
  If a sentence looks like a theory claim without an anchor, treat it
  as a drift-report against this file.

### Direction non-claims (Visionary Phase 1, verbatim)

1. **No substrate-side event bus / pub-sub / reactive framework.** If
   perspective ships an `observe` verb at all, it is a bounded
   per-handle ring, not a global stream.
2. **No DID / crypto identity / X3DH / DoubleRatchet.** Peer identity
   remains a bounded byte string; cryptographic identity is
   koru-liberation / koru-crypto-strategy.
3. **No live `children_of` as a hot-path query.** Engine keeps degree
   as the Coding Law count; consumers who need enumeration call
   `replay::build_children_index` on a snapshot.
4. **No ephemeral state (WavePool, TTL) as substrate.** If perspective
   lands, "wave = perspective + TTL" is a consumer subsystem.
5. **No persistence rewrite.** `snapshot_parentage` +
   `replay_topological` are the persistence surface, unchanged.
6. **No byte-fold / Fold Law changes.** `ByteMapping` semantics stay
   what Step 1c shipped.
7. **No production BFT consensus layer.**
   `[SHIPPED @ src/subsystems/validator.rs]` and
   `[SHIPPED @ src/subsystems/commitment.rs]` are reference LCA
   implementations, not the pitch. `koru-protocol` remains the home
   for production consensus.
8. **No networking stack.** Production networking is downstream of
   this crate.
9. **No substrate-native cryptographic proofs.**
   `BatchCommitment::compute` hashing `leader_id` (N6 closure) is
   reference-subsystem behavior, not a substrate primitive. Substrate
   primitive remains distinction synthesis.
10. **No enumeration of dropped v1.2 APIs as "features."** Removals
    are cleanup, not selling points.

---

## Migration categories

Full migration guide lands in E04 with diffs and per-version-pin
(0.1.0 / 1.1.0 / 1.2.0) instructions. DESIGN.md carries only the
category-level summary of what will change; downstream consumers use
this section to bucket their audit surface, then follow E04 for the
per-item diff.

- **v1.x → v2.0.0 breaking public-API changes.** Three categories:
  - *Type-rename* — String-typed IDs become byte-typed
    `Distinction`. Every construction site (`Distinction::new(String)`,
    `String::to_distinction()`, JSON schemas serializing `id: String`)
    changes. `#[serde(with = "distinction_hex")]` is the wire-format
    adapter.
  - *Method-remove* — `get_distinction_by_id`, `Distinction::id()`,
    `Distinction::new(String)`, `ParallelBatchProcessor`,
    `ParallelAction`. Each has a replacement documented in E04; none
    is a silent deletion.
  - *Field-visibility* — `Distinction`'s inner byte field becomes
    `pub(crate)`. Consumers reaching into the field switch to
    `as_bytes()` or `to_hex()`.

- **Optional adoption.** Features consumers can start using
  immediately at v2.0.0 without changing existing code:
  - `bytemuck::Pod`-derived zero-copy slice views on `[Distinction]`
    for persistence and wire-format code.
  - `snapshot_parentage` + `replay_topological` for engine
    persistence and round-trip reconstruction.
  - `SynthesisRecorder` for chronological observation without paying
    for engine-side state.
  - `build_children_index` for consumers that need children iteration
    (rare; the theory-named primitive is `degree`).
  - `InvariantError` from `check_structural_invariant` for downstream
    theory-event detection.

- **Behavior-preserved.** Nothing changes for consumers who ignore
  the new features and adopt the type-rename cleanup — same
  synthesize path, same axiom guarantees, same LCA pattern. Adoption
  of optional features is opt-in.

Per-category diffs and worked-example wrapper deletions for each of
the six ecosystem projections named in the headline live in E04's
migration guide. This document deliberately carries no diffs — a diff
in DESIGN.md invites drift the moment E04 lands its authoritative
version.

---

## Downstream drift prevention

Three checks close the drift class that motivated this rewrite:

1. **`theory_anchor_check.sh`** `[TARGET @ E01-S05]` — falsifier that
   fails CI when a downstream doc paraphrases a THEORY.md
   concept-term without an anchor-link. Stub lives at
   `.claude/warroom/checks/theory_anchor_check.sh`; real
   implementation lands with E01-S05.
2. **`cargo public-api` snapshot** — pre-tag hygiene ensuring every
   `[SHIPPED]` claim in this document resolves to an actually-exported
   symbol. Baseline lands with the v2.0.0 tag.
3. **Anchor-budget review at each phase gate** — the manifest at
   `.claude/warroom/epics/E01-.../S01-.../phase-5-execute/anchor-budget.md`
   lists every THEORY.md anchor DESIGN.md uses; a PR that adds a
   new anchor must extend the budget.

---

*Empirical evidence: `docs/BENCHMARKS.md`. Theory: `THEORY.md`.
Architecture cross-reference: `ARCHITECTURE.md`.*
