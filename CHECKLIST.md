# koru-lambda-core 2.0.0 — Release Checklist

This is the single source of truth for what 2.0.0 still needs. 2.0.0 ships only when every box is ticked.

> **Scope, settled 2026-09-30.** 2.0.0 publishes the substrate and nothing else:
> `koru-lambda-core`, plus `koru-lambda-wasm` — **a first-class product
> surface, not a test fixture.** It ships to npm alongside the crate.
> Accessibility is the argument: a reader reproduces a child id in a browser
> console with no toolchain at all, which is the shortest path from the claim
> to someone checking it, and a Rust-only release would confine the theory to
> people who already have a Rust toolchain. It also happens to be the second
> target that makes cross-target byte equality demonstrable rather than
> asserted. Everything below the `Not blocking 2.0.0` line stays in the
> repository and ships when it is ready.
>
> The obligation that comes with shipping it: the JS surface has to be good.
> Today a JS consumer cannot get their own data into the engine at all
> (JS-9), which makes the package a demonstration rather than a tool. What we
> do *not* take on is compatibility with v1.2's JS consumers — downstream is
> out of scope, so the surface should be right rather than backward-compatible.
>
> The number that decided it: the published v1.2.0 is 4,489 source lines, of
> which engine, primitives and lib are **409 — nine percent**. The rest is
> consensus machinery, a C ABI and bindings. Carrying that forward would have
> reproduced a crate in which the theory is a rounding error.

## Rules

1. **An unticked box blocks the release.** The release script refuses to publish while any `- [ ]` remains in this file.
2. **A ticked box needs evidence.** Every `- [x]` item must carry an `evidence:` line naming a test (`path::name`), a commit SHA, a CI run URL, or a tracked file. CI rejects ticked items with no evidence or with evidence that doesn't resolve.
3. **Evidence is shown, not claimed.** "Tests pass" is not evidence. The specific test or command that would fail if the item regressed is. An item whose evidence can only be a human opinion is written wrong; rewrite it until a command can fail.
4. **Zero warnings, zero suppression.** No `#[allow]`, `#[expect]`, lint relaxations, `@ts-ignore` or `eslint-disable` anywhere — and no suppression by configuration either (`clippy.toml`, `[lints]`, tool config). Fix the cause.
5. **Nothing is deferred.** Items do not move to a later version. If an item is wrong, it is corrected or removed in a reviewed change, with the reason stated inline.
6. **Decisions are items too.** `DEC-*` items are open questions for the owner; they block like any other item. A decision is closed by a written outcome naming its `docs/SPEC.md` section ("accepted, see SPEC §X") or the item that implements the change ("changed, see ITEM-N"), never by silence.
7. **Findings become items.** Anything a review, mutation run, probe or downstream check turns up is added here as a numbered item before that gate can be ticked. This file is expected to grow during the work; a run that adds nothing is evidence the gate wasn't applied honestly.

Item format:

```
  - [ ] AREA-N Short statement of the required end state
    - evidence: <test path::name | commit | CI run | file>
```

(Indented above so the gates do not read the template as a real item.)

`file:line` references in item text describe the state found on 2026-09-21 and are not live claims; they locate the thing to fix, and go stale once the item is done or the workspace split moves the file. Evidence lines, by contrast, must resolve at the time the box is ticked.

---

## What this crate is (review finding, 2026-09-21)

Five independent reviews, forbidden from looking at downstream usage, agreed on the boundary:

> **koru-lambda-core is the content-addressed graph produced by one commutative irreflexive operator, every read of that graph the axioms force, and the lift that gets outside values into it — and nothing that requires knowing about peers, actors, transactions or time.**

The criterion used: an item belongs in the substrate if deleting it would leave an axiom or law with no referent in the crate. By that test the validator, commitment, network, compactor and batch modules are *reference consumers*, not substrate — which `ARCHITECTURE.md` already states ("These are not the substrate — they USE it") and `src/subsystems/mod.rs` says outright, while `Cargo.toml` and packaging say otherwise.

Three objects are constitutive: **witness** (engine + verify), **vantage** (projection), **lift** (`Canonicalizable`, `ByteMapping`, the anchored fold). The lift is the substrate's only free parameter and is currently its thinnest layer: one stdlib impl (`u8`), no public fold, nothing exposed to JS, no example. Two in-crate consumers hand-rolled the same fold privately and both got the anchor wrong — which is the collision bug.

**Settled 2026-09-21.** Placement (DEC-9): four crates — `koru-lambda-core`, `koru-lambda-consensus`, `koru-lambda-wasm`, `koru-lambda-ffi` — one workspace, one release, one bar (PKG-1..7). Extensibility (DEC-10): `ReadOnlyEngine` unseals so alternative engines remain possible; `CoreSignal` stays sealed; `Boundary`/`Direction` stay closed (SEAL-1..3). Nothing is dropped.

## PROC — Enforcement (first; everything after lands under it)

- [x] PROC-1 CI runs on push to every branch and on every pull request
  - evidence: `.github/workflows/ci.yml` @ 699a872 — `on: push: branches: ['**']`
- [x] PROC-2 CI jobs:
  - evidence: `.github/workflows/ci.yml` @ 699a872 — 16 jobs; all run locally green via `scripts/check.sh` (corrected 2026-10-01: the evidence said 13, the file defines 16)
  - fmt
  - clippy native (`--all-targets --all-features -D warnings`, pedantic)
  - clippy `wasm32-unknown-unknown --features wasm`
  - `cargo test` in debug and release
  - `wasm-pack test --node --features wasm`
  - loom
  - miri subset
  - MSRV 1.80
  - `RUSTDOCFLAGS=-D warnings cargo doc`
  - `cargo audit` + `cargo deny`
  - `cargo publish --dry-run`
  - `wasm-pack build --features wasm` + npm pack dry-run
  - every example runs
  - JS demos typecheck and run
- [x] PROC-3 `scripts/check.sh` runs exactly the CI job set locally (one definition, no divergence)
  - evidence: `scripts/check.sh` @ 699a872 — mirrors the CI job set; `./scripts/check.sh` exits 0
- [x] PROC-4 Suppression gate: CI fails on any lint suppression in tracked files, including configuration-level suppression. `clippy.toml`'s `avoid-breaking-exported-api = true` is removed and the resulting lints fixed (today it hides `to_hex(&self)` on `Copy` types at `distinction_hex.rs:105,133` — exactly the signatures that freeze at 2.0.0).
  - evidence: `scripts/gates/no_suppression.sh` @ 699a872; `clippy.toml` sets the flag false; the two `to_hex` signatures it hid are fixed
- [x] PROC-5 Leak gate: CI fails on internal process references in tracked files (untracked paths, planning IDs, audit tags)
  - evidence: `scripts/gates/no_process_leaks.sh` @ 699a872 — clean at 1e4c6ea
- [x] PROC-6 Checklist gate: CI fails on a ticked item without resolvable evidence; the release gate fails on any unticked item
  - evidence: `scripts/gates/checklist_evidence.sh` (20 ticked, 51 references resolved; it caught three of its own author's evidence lines naming bare filenames), `scripts/gates/release_ready.sh`; `.github/workflows/ci.yml` gates job
- [ ] PROC-7 Test-reference gate: every test named as evidence here or in `docs/CLAIMS.md` exists (`cargo test -- --list`)
- [ ] PROC-8 Link gate: every relative link and file reference in tracked Markdown and rustdoc resolves to a tracked file
- [ ] PROC-9 Doc symbol anchors resolve: every code reference in docs names an existing public item (no `file:line` anchors)
- [ ] PROC-10 Mutation testing: `cargo mutants --in-place -- --all-features` exits 0 in CI on every PR, over every crate in the workspace, with no timeout skips and a committed budget. Each of the six known survivors has a named killing test listed here: restore byte-compare (`projection.rs:1326`), wire version check, `projection_id` derivation, Undirected children, `check_structural_invariant` always-Ok, degree `Release`→`Relaxed`. Equivalent mutants are listed in `docs/MUTANTS.md` with a written argument.
- [ ] PROC-11 Coverage: `cargo llvm-cov --all-features --fail-under-regions 95` (region, not line), debug profile, plus a per-file floor of 90% with no file below it
- [x] PROC-12 Public API snapshot committed and diffed in CI, so a public signature cannot change without changing a committed file a reviewer reads. The snapshots are also the source the changelog and migration guides get derived from, rather than recollection.
  - evidence: `api/koru-lambda-core.txt`, `api/koru-lambda-consensus.txt`, `api/koru-lambda-wasm.txt`; gate `scripts/gates/api_snapshot.sh` (verified to fail on an added public const); `.github/workflows/ci.yml` api-surface job
- [ ] PROC-13b Both existing publish scripts are deleted, not repaired. `build_universal.sh` runs `wasm-pack` against the rlib-only root crate with a `--features wasm` flag that no longer exists, so it cannot succeed; `publish_npm.sh` publishes `../pkg`, which resolves outside the repository entirely. Keeping scripts that cannot work is worse than having none: someone will run them.
- [ ] PROC-21 CI asserts the packed npm tarball's `name` and `version` match the intended package. Nothing checks either today, and the name silently changed from what 1.2.0 shipped.
- [ ] PROC-13 One release script replaces `publish_crates.sh` / `publish_npm.sh`: it runs the full gate, checks this file, builds `pkg/` with `--features wasm`, dry-runs both registries, and asks for confirmation before each real publish
- [x] PROC-14 Feature-matrix builds: `--no-default-features`, each feature alone, `--all-features`, and `wasm32` (making `serde` optional creates combinations nothing builds today)
  - evidence: `.github/workflows/ci.yml` features job @ 699a872 — 4 combinations, `--all-targets` so tests compile too
- [x] PROC-15 `rust-toolchain.toml` and `deny.toml` are committed (neither exists); doctests run under `--all-features` and on the MSRV toolchain
  - evidence: `rust-toolchain.toml`, `deny.toml` @ 699a872
- [x] PROC-16 `[package.metadata.docs.rs]` sets `all-features = true` and the target list, so docs.rs doesn't publish a crate with half the surface missing
  - evidence: `Cargo.toml` `[package.metadata.docs.rs]` @ 57d892f
- [x] PROC-17 Every probe, test or tool named in doc prose resolves to a tracked file (THEORY.md currently cites a self-reference probe whose source is not in the repo)
  - evidence: `scripts/gates/no_process_leaks.sh` @ 699a872 — clean once `docs/SPEC.md` landed @ 1e4c6ea
- [ ] PROC-18 The release script executes publish order — substrate, then consumers, then FFI — each dry-run passing against its already-published predecessor, and refuses to publish out of order
- [ ] PROC-19 Inter-crate dependencies are pinned to an exact version with both `path` and `version`; CI fails on a caret or a path-only dependency
- [ ] PROC-20 Every PROC-2 job runs per crate; a workspace-wide invocation is not sufficient for clippy and feature-matrix gates
- [x] PROC-24 The release order is documented and gated, so a phase cannot be declared finished while its work is open.
  - evidence: `ROADMAP.md` assigns every blocking checklist section to exactly one of six phases and carries **no work items of its own** — two sources of truth for the same work is the defect this release keeps finding. `scripts/gates/roadmap.sh` proves the assignment is total, exclusive and resolvable, and fails if a phase marked complete has open items in its sections. Wired into `scripts/check.sh` and the CI gates job, and verified against a committed file rather than a dirty one.
- [ ] PROC-25 The two checklist sections both named `PKG` are renamed so they are distinguishable. "PKG — The family" and "PKG — Workspace layout" share a prefix, so `roadmap.sh` can only assign them to one phase together, and `release_ready.sh` cannot report them separately. Found by the roadmap gate's duplicate-name check on its first run.
- [x] PROC-22 `scripts/gates/crate_boundary.sh` asserts over the **crate graph**, not over greps: `cargo metadata --format-version 1`, no path from `koru-lambda-core` to any other workspace member in any dependency kind for any target. The current gate reads one `sed` range of `[dependencies]` in one manifest, so it misses `[dev-dependencies]`, `[build-dependencies]` and `[target.'cfg(…)'.dependencies]` — and this repo has *already been bitten there*: the comment at `Cargo.toml` documents an `experiments/runner` edge that created a crate-graph cycle and broke rust-analyzer's macro resolution. It also skips `tests/`, `examples/` and `benches/`, never inspects `crates/*/Cargo.toml` (so "bindings do not depend on agents" is wholly unenforced, which PKG-5's consensus entry point would violate), and its allowlist still names three types that no longer exist. Worst: the regex is `koru_lambda_(consensus|wasm|ffi)`, so **PKG-10's rename to `koru-lambda-agents` silently empties the gate** — it will read green while enforcing nothing. A graph assertion is name-independent, survives the rename, and is shorter than what is there now.
  - evidence: `scripts/gates/crate_boundary.sh` now runs `cargo metadata --format-version 1 --no-deps` and asserts three things, none of them by name: the substrate (identified as the package defined by the **workspace root manifest**) has no workspace-member dependency in any kind or target; the member graph is acyclic; and any member producing a `cdylib` (identified by **crate-type**, so a bindings rename cannot disarm it) depends on the substrate and no other member. Both name-based identifications are structural, so PKG-10's rename cannot empty it. The stale allowlist naming three types that no longer exist is gone, and the source-level grep with it — a `src/` reference to a consumer type cannot compile without a dependency, which the graph now catches. Verified to fail: adding a `[dev-dependencies]` edge from core to the agent crate reports both `substrate has a dev dependency on member` and `dependency cycle: koru-lambda-core -> koru-lambda-consensus -> koru-lambda-core`; the previous gate passed that case silently, and it is the case that actually bit this repo.
- [ ] PROC-23 No tracked file links to a file the gates do not cover. `doc_anchors.sh` and `no_process_leaks.sh` both enumerate `git ls-files`, so an untracked document is invisible to every doc gate — and `README.md` linked to `ECOSYSTEM.md` while it was untracked, meaning the newest published-intent doc was the one file exempt from the enforcement built for exactly that purpose. The gate checks that every relative Markdown link from a tracked file resolves to a tracked file.

## PKG — The family (settled 2026-10-01)

Three published things, one sentence each, so a stranger lands on the right door:

| Artifact | Is | Ships at 2.0.0 |
|---|---|---|
| `koru-lambda-core` (crate) | the substrate: witness, vantage, lift, replay, trust boundary, vectors | yes |
| `koru-lambda-wasm` (crate) → npm | the same engine with no Rust toolchain | yes |
| `koru-lambda-agents` (repo) | worked consumers of one pattern — validator, commitment, and later network, compactor, batch | **no** |

`agents` versions independently, which is the point: its four wire-facing
defects get fixed on its own schedule, and growing from two consumers back to
five never touches the crate anyone depends on. The substrate stays small
because the interesting work happens next door.

Naming: "agents", not "consensus" — a second claimant in the aisle
`koru-protocol` already holds is the positioning mistake that was vetoed once
already, and "reference" apologises in the title.

- [ ] PKG-10 `koru-lambda-consensus` is renamed `koru-lambda-agents` and moves to its own repository, after its replacement as the substrate's in-repo consumer exists (TST-7). It keeps `publish = false` until its wire defects are closed, and it does not carry 2.0.0.
- [ ] PKG-11 Version policy recorded and enforced: the bindings pin the substrate exactly (`=2.0.0`), because they re-export substrate types across an ABI and a patch can change the generated `.d.ts`; `agents` takes `^2.0`, so it absorbs substrate patches and is the thing that notices when one breaks a consumer.
- [ ] PKG-12 The `agents` repository runs a daily CI job against the substrate's `main` by git dependency. That job failing is how anyone finds out the substrate broke a consumer; colocation was previously doing this job by accident.

## PKG — Workspace layout (DEC-9: three crates)

- [ ] PKG-1 The workspace publishes four crates from this repo, in one release, at one version: `koru-lambda-core` (engine, projection, replay, recorder, lift, agent contract, distinction_hex), `koru-lambda-consensus` (validator, commitment, network, classifier, batch), `koru-lambda-wasm` (JS/TS bindings for both), `koru-lambda-ffi` (C ABI). Everything depends on `core`; `core` depends on none of them. The wasm bindings were extracted from `core` because `wasm-pack` requires `cdylib`, which would otherwise force a dynamic library on every pure-Rust consumer — the same reasoning that separated the C ABI, and what `ARCHITECTURE.md` already calls the bindings layer.
  - **un-ticked 2026-10-01: three crates exist, not four.** `Cargo.toml` members are `.`, `crates/koru-lambda-consensus`, `crates/koru-lambda-wasm`, `experiments/runner`; there is no `koru-lambda-ffi`. `checklist_evidence.sh` resolves evidence *paths*, not claims, so a path that exists let a false count through. Blocked on DEC-1 — this item cannot be restated until the owner's C-ABI decision is closed, and `ECOSYSTEM.md` currently contradicts that decision (see DEC-1).
- [x] PKG-2 CI fails if any `core` module names a `consensus` or `ffi` type
  - evidence: `scripts/gates/crate_boundary.sh` @ 699a872 — clean
- [x] PKG-3 `core` carries crate-level `#![forbid(unsafe_code)]` (unforgeable, not a per-module gate), `crate-type = ["rlib"]`, 5 direct dependencies, and `serde` as an optional feature. `lru` (or its replacement) lives only in `consensus`; `cdylib` only in `wasm` and `ffi`; `staticlib` only in `ffi`.
  - evidence: `Cargo.toml` + `src/lib.rs` @ 57d892f — 5 direct deps, `rlib`, optional serde, crate-level `forbid(unsafe_code)`
- [ ] PKG-4 Each crate has its own description, keywords, categories and README; only `consensus` may use the word "consensus" in its metadata
- [ ] PKG-5 npm ships **one** package with two entry points (core, and consensus), so the wasm binary is not shipped twice; the consensus entry point's size cost is stated
- [ ] PKG-6 Every SUB-*, FFI-*, FFIJS-* item is satisfied inside `consensus` or `ffi`, at the same mutation, coverage and error-variant bar as `core`
- [ ] PKG-8 The published **npm** package is still named `koru-lambda-core`. `wasm-pack` derives the package name from the crate (`koru-lambda-wasm`), so the release script rewrites it and CI asserts the packed tarball's name — otherwise the refactor silently renames the package every JS consumer depends on.
- [ ] PKG-7 `ARCHITECTURE.md`'s three-layer diagram becomes normative: the crate layout matches it, and `lib.rs` re-export order reflects it

## SEAL — Extensibility (DEC-10)

- [ ] SEAL-1 `ReadOnlyEngine` is unsealed: an external crate can implement it. Its method set is reviewed once before publish, since implementors freeze it; later additions must carry defaults.
- [ ] SEAL-2 A test implements `ReadOnlyEngine` from outside the crate (a mock or fixture engine) and materializes a projection over it, proving the unsealing is real
- [ ] SEAL-3 `CoreSignal` stays sealed and `Boundary`/`Direction` stay closed; the docs state that projection guarantees (Cond B/D) hold for substrate-provided engines, and that a consumer engine inherits responsibility for its own correctness

## FRZ — The freeze (identity cannot move; found 2026-10-01)

`ECOSYSTEM.md` promises "identity never moves ... for the life of the major
version". Nothing enforces that today. `api/*.txt` records `pub const
vectors::C_HEX: &str` — the **type**, not the value — so editing a vector
leaves the snapshot byte-identical; `cargo-semver-checks` is a shape tool and
`.github/workflows/ci.yml` declares it a no-op against a 1.2.0 baseline that
has no `vectors` module; and `src/vectors.rs` ships *inside* the crate it
pins, so one commit can change `synth_bytes` and regenerate the vectors
together with every job green. A conformance suite that lives inside the
implementation certifies internal consistency and nothing else. This is the
third instance of the release's root cause — a check that exists and does not
check — and the first one that sits under a published guarantee.

- [ ] FRZ-1 Vectors move out of tree to `conformance/vectors.json` (CONF-1) and a gate fails on any change to it unless `Cargo.toml`'s **major** version also changes. `src/vectors.rs` becomes a generated mirror the gate diffs. This is the only item that converts the promise from cultural to mechanical; every other FRZ item is coverage.
- [ ] FRZ-2 `projection_id()` has a published vector. It is a 32-byte public identifier with its own domain separator, reachable from the shipped wasm surface, and it has **no** value pinned anywhere.
- [ ] FRZ-3 The rustdoc at `src/projection.rs` stops calling `projection_id_is_deterministic_across_engines` "the drift guard". It builds two engines in the same binary, so both drift together and it cannot detect a value change. Either the name and doc describe what it does (cross-engine agreement), or it is replaced by a test that compares against a constant.
- [ ] FRZ-4 `DEGREE_TAG` and `HOPDISTANCE_TAG` have published vectors. `type_id_constants_match_runtime_sha` checks constant == SHA-256(tag), which passes when both change together. Only Adjacency has a wire vector today.
- [ ] FRZ-5 Published vectors cover `Direction::{Downstream, Undirected}` and `Boundary::Saturated`. The single wire vector is Upstream + Hops(2), so three of the shipped axis values produce unpinned bytes.
- [ ] FRZ-6 `domain_anchor()` has a published vector. It is public, identity-producing and unpinned.
- [ ] FRZ-7 A **decode** vector: parse `ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX` from the constant and assert the recovered spec. Every existing projection test encodes and decodes with the same build, so a symmetric loosening of the codec is invisible.
- [ ] FRZ-8 A coverage gate, not just coverage: any public function returning `Distinction`, a projection identifier, or wire bytes must be named in the fixture manifest. Without it the freeze holds for today's surface and lapses for anything added in 2.1.
- [ ] FRZ-9 `Cargo.lock` is committed. It is gitignored and `sha2` floats on `"0.10"`, so CI resolves the hash implementation fresh on every run. The repo already pins `rand` exactly for this class of reason.
- [ ] FRZ-10 `ECOSYSTEM.md` stops counting `api_snapshot.sh` and `cargo-semver-checks` toward the freeze. They guard the public **surface**, which is a different promise and a good one; conflating the two is what hid FRZ-1.

## ENG — Engine and trust boundary

- [x] ENG-1 `synthesize` checks both parents are registered before any state change; an unregistered parent panics with zero state change (documented under `# Panics`), tested in debug and release by comparing full state snapshots
  - evidence: `src/engine.rs` @ 5e9cb8c — `assert_registered` before the insert; tests `engine_tests::foreign_parent_panics_leaving_state_untouched`, `..._rejected_in_either_argument_position`, `..._guard_also_covers_self_synthesis`, `synthesize_with_unregistered_parent_panics` (both profiles)
- [ ] ENG-2 `synthesize` throughput: `criterion` `synthesize/hot`, median of 3 runs, ≤ 3% slower than baseline commit `e88e824`, on the pinned CI runner; a regression fails the job
- [x] ENG-3 `check_structural_invariant` verifies, in one pass:
  - evidence: `src/engine.rs` @ bd79412 — four checks in one pass; seven negative tests `engine_tests::auditor_*`, six of which the previous auditor passed
  - binary parentage
  - every parent registered
  - every id equals the hash of its parents (full content-address recomputation)
  - **per-node** degree recomputed from parentage (not only the sum, which survives a mutant that bumps one parent twice)
  - no registered node has two equal parents (Axiom 3 residue)

  Each violation has its own error variant and a negative test. The docs state whether the genesis edge is materialized or whether Law 6's `+1` is definitional.
- [ ] ENG-4 Only the engine can create a `Distinction`. There is no `Pod`/`Zeroable`, `from_hex`, `FromStr` or `Deserialize` on `Distinction`; external bytes enter only through `RawDistinctionId` → `verify`. Compile-fail tests cover each removed path, and the positive `assert_impl_all!(Distinction: Pod, Zeroable)` at `engine.rs:1161` is inverted to `assert_not_impl_any!`.
- [ ] ENG-5 A `RawDistinctionId` **not registered in the receiving engine** is rejected, and an unregistered `Distinction` reaching `synthesize` panics before any state change. A distinction with equal bytes that *is* registered in the receiving engine is accepted (Law 8). Both directions tested.
- [ ] ENG-6 `IdentityHasher` / `IdentityBuildHasher` are crate-private
- [x] ENG-7 `relationship_count` is documented as a definitional count; the measured check lives in ENG-3
  - evidence: `src/engine.rs` @ bd79412 — `relationship_count` documented as definitional arithmetic; the measured equivalent is the auditor's per-node degree recount
- [ ] ENG-8 Documentation describing foreign-byte behavior matches the code in both build profiles — **in the docs as well as the source**. *(Un-ticked 2026-09-30: I ticked this having grepped `src/*.rs` only. `DESIGN.md:141` still calls the guard "the debug-mode `debug_assert!`" and `ARCHITECTURE.md:112,121` say the same, so a reader of either would believe release builds accept foreign parents — the exact opposite of what now ships, and drift in the dangerous direction. Source is clean; the docs are not.)*

## ENC — Canonical encoding

- [ ] ENC-8 **One lift, applied recursively.** `ByteMapping` is re-derived as the anchored fold over the two-letter alphabet `{d₀, d₁}`, so the substrate ships one lift instead of two and `BITS_PER_BYTE` stops being a constant. Measured before adopting: the derived 8-step fold produces the same 256 distinct tips as the shipped 17-step one, with `deg(d₀) = 257` against 512 and 513 nodes against 1023 — the nine extra steps buy no injectivity and exactly double the graph. Published vectors are unaffected (they involve no byte folding; verified). Law 11's magnitude and the corresponding design gate re-baseline; see THY-4.
- [ ] ENC-9 `Canonicalizable::to_canonical_structure` takes the `ByteFold`. The current signature cannot carry one, so every call rebuilds the 256-tip table — and the only real consumer already routed around the trait: `compute_batch_root` calls a `canonicalize_with` helper instead. A trait its own implementors bypass is the wrong shape.
- [ ] ENC-11 **The fold generalises over its alphabet.** `docs/SPEC.md` §4.2 specifies the anchored fold over an arbitrary alphabet with a tip function; the code ships only the concrete 256-byte case. Two voices reached this independently: the general rule exists in prose only, and a consumer folding a composite value has to invent its own framing — the moment a variable-length field appears, that is the collision class `ByteFold` closed, relocated one level up. Ship `fold(anchor, tips, elements)` over any alphabet, with `u8` as its worked instance.
- [ ] ENC-12 Composite framing is answered, not left silent: either a documented field-concatenation rule with its length discipline, or a fold over distinctions. A consumer with a struct containing a `String` must not have to invent this.
- [ ] ENC-10 `DomainLabel` is a newtype with a documented reservation table, not a bare `u8` alias. Two crates picking `0x01` independently is a silent collision.

- [x] ENC-1 One anchored, order-preserving sequence fold, with a written spec: the anchor is a **parameter**, not a constant; the spec states and proves accumulator/element lineage disjointness **including nested folds** (a fold's result used as another fold's element, which is what the subsystems do); injectivity holds modulo the 128-bit bound. Tests:
  - evidence: `src/primitives.rs` @ b3475a0 — `ByteFold`; tests `primitives::tests::all_256_element_tips_are_distinct`, `..._the_cases_that_collided_before`, `..._order_length_and_repetition_all_change_the_result`, `..._prefixes_do_not_fold_alike`, `..._an_anchor_that_is_an_element_tip_is_rejected` (degenerate case constructed directly), `..._the_anchor_changes_the_result`
  - all 256 byte tips are distinct
  - property test: distinct sequences give distinct results
  - a directly-constructed degenerate case where an accumulator equals the next element tip (a random proptest reaches this with probability 2⁻¹²⁸, so it must be constructed)
  - prefix, permutation and repeat regression vectors
  - golden vectors
- [x] ENC-2 Byte folding uses a **per-engine memoized** tip table (registration cached per engine, never identity cached globally — a shared table would hand ENG-1 unregistered parents and panic)
  - evidence: `src/primitives.rs` @ b3475a0 — `ByteFold::new` computes the 256 tips once per engine; test `primitives::tests::tips_are_engine_independent_but_folds_need_their_own_engine` covers the registration trap
- [ ] ENC-3 `LocalCausalAgent` / `synthesize_causal_action` take `&DistinctionEngine`; roots accepted from outside are engine-verified; the trait is documented as an anchored-fold contract, not a perspective abstraction
- [ ] ENC-4 The anchored fold is **public and re-exported at the crate root**, and `Canonicalizable` ships impls for `u64`, `&[u8]`, `Vec<u8>` and `&str`. Today the only stdlib impl is `u8`, so a consumer with domain data has nothing to call.
- [ ] ENC-5 The two private folds are deleted and call the public one (`validator.rs:97-109`, `commitment.rs:232-238`); a CI gate fails on any remaining hand-rolled `synthesize(acc, …)` left-fold in tracked code
- [ ] ENC-6 `ByteMapping`'s cost (17 syntheses per byte) is measured and either reduced or documented, with the rustdoc number matching the bench

## AGT — The agent contract (write-side discipline)

- [ ] AGT-1 The monotonic-forward rule is enforced, not conventional. `update_local_root`'s docs currently say "Convention (not trait-enforced)": the advance path verifies that the new root's parents include the previous root (one `parents_of` lookup) and rejects a root that does not descend from it. Tests cover both directions.
- [ ] AGT-2 The agent contract advances through the canonical lift only: a multi-element advance uses the anchored fold (ENC-1) seeded at the agent's root, never a fold seeded from its first element. A CI gate rejects the latter shape.
- [ ] AGT-3 The trait and `synthesize_causal_action` ship in `koru-lambda-core` alongside the lift they anchor; their consensus implementations ship in `koru-lambda-consensus`
- [ ] AGT-5 The monotonic-forward assertion ships as a generic conformance helper — `assert_forward_monotone<A: LocalCausalAgent>(agent, actions, engine)` — exported from the test-support surface and required of every shipping agent. The assertion already exists but is hardcoded to the test LCA (`lca_monotonicity_previous_root_is_parent`). It fires today on `ConsensusValidator::restore_state`, which assigns a root with no causal link to the prior one.
- [ ] AGT-6 A gate enforces one advance path per agent: a type that `impl LocalCausalAgent` exposes no `pub fn … (&mut self …)` beyond the trait's. This is the mechanical half of `ECOSYSTEM.md`'s "one agent is one causal role"; the other half ("a different advance discipline is a new agent") is a design-review judgment and no gate will carry it, so the doc must say which half is which. It fires today on `ConsensusValidator`, which has **three** advance paths — `validate_batch`, the `synthesize_action` default that moves `local_root` without touching `expected_nonce`, and `restore_state`.
- [ ] AGT-4 The docs state why the agent is the recommended write path for perspective-carrying consumers **and** why it is not the only one: replay reconstructs from raw parent pairs, cross-runtime convergence compares histories rather than perspectives, and the theory admits multi-root and non-root consumers. A reader should not have to guess whether raw synthesis is a mistake.

## TRIM — What leaves the substrate

- [ ] TRIM-1 `SynthesisRecorder` leaves the core. Its log is chronology — exactly the order-bearing state the substrate is defined not to hold — and its own module header already says "Consumer pattern, **not substrate**." Zero consumers in the repo; a consumer rebuilds it in about twenty lines.
- [ ] TRIM-2 `IdentityHasher` and `IdentityBuildHasher` become crate-private. A hashing optimisation is not axiom-bearing, no public signature mentions either, and the bench keeps its own mock copy. Order matters: the recorder is the only non-engine user, so it goes first.
- [ ] TRIM-3 The agent contract leaves the core for the consensus crate. Its own docs say the axioms constrain `synthesize`, not consumer shape, and justify substrate residence with "every consumer we've shipped uses it" — an argument from convenience whose only user is the crate no longer in the release.
- [ ] TRIM-4 `build_children_index` leaves the public surface: its own doc calls it a convenience for consumers who need children iteration.

## PRJ — Projection

- [ ] PRJ-1 `CanonicalBytes` writes into a caller buffer (`write_canonical(&self, out: &mut Vec<u8>)`)
- [ ] PRJ-2 No public trait method that ends in `unreachable!` (`HopDistance` dispatch restructured)
- [ ] PRJ-3 `restore_projection` rejection is fully tested:
  - every `Malformed` offset (magic, version, boundary tag, direction tag, truncation)
  - `SignalMismatch`
  - `IntegerOverflow` where reachable
  - unregistered root
  - one tamper test per wire region

  Byte divergence and foreign engine are distinct error variants.
- [ ] PRJ-4 Downstream, Undirected and `Saturated` traversals are tested against hand-computed expected sets
- [ ] PRJ-5 `projection_id` is pinned by golden vectors for every signal; changing any single axis changes the id (tested)
- [ ] PRJ-6 Read-only guarantee stated precisely: enforced for core signals, a documented contract for consumer `Signal` impls
- [ ] PRJ-7 Downstream/Undirected materialize cost (O(N) index build) is documented and measured
- [ ] PRJ-8 Read-only measured, not asserted: engine state is byte-identical before and after every core projection, including `materialize`; projecting twice yields identical bytes and an unchanged `distinction_count`. Shipped as a consumer self-test for custom signals.
- [ ] PRJ-9 The degree-threshold classifier (today's "compactor") is expressed as a projection with a consumer `Signal` that returns the thermal class — demonstrated working from an external crate, so this holds whatever DEC-9 decides. A *threshold boundary* is not expressible (closed enum, see DEC-10); the classifier uses an existing boundary. It evicts nothing, so the name goes either way.

## REP — Replay and recording

- [ ] REP-1 `replay_topological` is O(n) (Kahn's algorithm) and returns `DistinctionEngine`. Tests: shuffled, reverse-order, duplicate and tampered input, plus a scaling test.
- [ ] REP-2 One children-index implementation shared by replay and projection
- [ ] REP-3 `ReplayError` has no redundant fields
- [ ] REP-4 `SynthesisRecorder` → replay round-trip is tested; the docs state that the log holds children only

## JS — wasm / TypeScript surface

- [ ] JS-1 Counts and degrees are `number` everywhere (not `bigint`); the 2^53 bound is documented; `docs/MIGRATION_JS.md` is corrected in the same change (it currently documents `bigint` as the 2.0 shape)
- [ ] JS-2 Every thrown error has a stable, documented `name` / `code`; tests assert them
- [ ] JS-3 A hex helper for 32-byte ids (projection ids)
- [ ] JS-4 `snapshotParentage`, `WasmEngine.replay`, `checkStructuralInvariant` and the sequence fold are exposed and tested
- [ ] JS-5 Every rejection path is tested from JS (invalid direction, each `restoreProjection*` failure, verify, hex parse)
- [ ] JS-6 Projection wrappers don't clone their full contents on every getter call
- [ ] JS-7 The generated `.d.ts` is reviewed; the demos typecheck against it under `strict`
- [ ] JS-10 `examples/wasm-demos/package.json` depends on `"koru-lambda-core": "file:./pkg-node"`, but `pkg-node/package.json` is `"name": "koru-lambda-wasm"` — the repo's own demo scaffold names a package that does not exist at the path it points to. Found while verifying the npm consumer count. The only external npm consumer (`canon-cpg`) pins `koru-lambda-core@^1.2.0`, the retired name, so the rename needs a decision in both places and PROC-21 is what would have caught this.
- [ ] JS-8 The published wasm target is a deliberate, documented decision and is verified against a real consumer. v1.2 shipped `--target web` (default `init` export); 2.0.0 currently ships `--target bundler` (no such export), which silently breaks the only npm consumer (`canon-cpg/cli/engine.js` calls `await initWasm({module_or_path})`). Whatever is chosen — single target or multiple entry points — a test consumes the packed tarball the way that consumer does. See DEC-7.
- [x] JS-10 The bindings handle `#[non_exhaustive]` substrate enums across the crate boundary: an unknown `SynthesisOutcome` variant surfaces to the caller instead of failing to compile against a future substrate release.
  - evidence: `crates/koru-lambda-wasm/src/lib.rs` @ 57d892f — wildcard arm returns a JS error
- [ ] JS-9 The lift layer is reachable from JS: the fold, `Canonicalizable`-style byte/string lifting, and an agent advance are exposed and tested. Today a JS consumer cannot get domain data into the engine at all.

## TST — Test suite

- [ ] TST-7 **The substrate keeps a consumer that applies pressure.** When `agents` leaves, nothing in this repo calls the lift. An example is not a substitute: the trait defect was found because a real consumer folds *N* values in a loop and the per-call table cost became visible enough to route around — a 40-line example folds once and the cost stays invisible. Ship an integration test defining a domain type with a non-trivial `Canonicalizable` impl and a batch loop, plus a `trybuild` **pass** case, since the defect was expressible rather than runtime.
- [ ] TST-6 `tests/scale.rs` stops claiming a falsification it does not perform: the message reads "r = 2d − 3 holds with zero deviations at 5M synths" on an assertion that checks parentage, identity and degree — not that relation.

- [ ] TST-1 One golden-vector file, used by both native and wasm tests (child ids; projection ids and canonical bytes for every signal × direction × boundary; subsystem roots)
  - **un-ticked 2026-10-01: the evidence does not support the text.** `src/vectors.rs` has nine constants and **zero** projection ids (`grep -c projection_id src/vectors.rs` → 0). Of "every signal × direction × boundary" — 3 × 3 × 2 — exactly one combination is pinned (Adjacency / Upstream / Hops(2)). No subsystem roots. The file is real and the two suites do assert against it, which is why this read as green; the item claims three things it does not contain. This is the pattern Rules 2 and 3 exist to catch, in the item CONF-1 and CONF-2 were about to be built on top of.
- [ ] TST-2 No test compares a value with the formula that produced it (the `r = 2d−3` via `relationship_count` checks are replaced by the measured ledger)
- [ ] TST-3a The loom model covers the real `synthesize` check-then-insert ordering (the mutant kernel must still fail loom)
- [ ] TST-3b A real-code concurrency stress test: N threads, exactly one `Novel` per child, final state equal to a serial run
- [ ] TST-4 Every public error variant is constructed and asserted by at least one test
- [x] TST-5 The miri subset passes and is listed in CI
  - evidence: `.github/workflows/ci.yml` miri job @ 699a872; `cargo +nightly miri test -p koru-lambda-core --lib` exits 0

## API — Public surface

- [ ] API-1 Every public error enum is `#[non_exhaustive]` and asserted mechanically (`assert_impl_all!(E: Debug + Display + Error + Send + Sync + 'static)`), with each `Display` string pinned against a golden so message drift fails a test
- [ ] API-2 `InvariantError` field names describe what they hold
- [ ] API-3 `Debug` on every public type (asserted mechanically); `Hash` on `Boundary` and `Direction`
- [ ] API-4 `serde` is an optional feature; adapters deserialize owned and escaped strings (tested with `from_reader` / `from_value`)
- [ ] API-5 `crate-type` carries only what the crate needs (`rlib` for a library; `cdylib`/`staticlib` belong with the C ABI, wherever DEC-9 puts it); no documentation relies on `panic = "abort"`; the release profile's `panic` setting is reconciled with ENG-1/ENG-5's "catchable panic, zero state change" claim, since a consumer building with `abort` gets no unwind
- [x] API-6 `cargo semver-checks` runs in CI against the published baseline, and the 2.0.0 snapshot becomes the baseline for future releases. *(Corrected 2026-09-26: this item assumed semver-checks would enumerate the v1.2 → 2.0.0 delta. It cannot — because the bump is already major, all 254 of its lints skip. It is future-facing only, preventing accidental breakage within 2.0.x. The delta comes from an API-snapshot diff instead; see PROC-12 and `api/DELTA-from-1.2.0.txt`.)*
  - evidence: `.github/workflows/ci.yml` semver job; `cargo semver-checks check-release --baseline-version 1.2.0` runs clean (0 checks, 254 skip)
- [ ] API-7 `#[must_use]` audit across every public function returning a value (today: 21 in `engine.rs`, 20 in `projection.rs`, 2 in `replay.rs` — the asymmetry is unexamined)
- [ ] API-8 `unsafe` is confined and locked: `#![forbid(unsafe_code)]` everywhere except the C ABI module, which carries `#![deny(unsafe_op_in_unsafe_fn)]`, a `# Safety` contract on every function, and a CI gate that no other module gains `unsafe`. Plus `assert_impl_all!` for `Send + Sync + UnwindSafe + RefUnwindSafe` on public types.
- [ ] API-9 No `impl Drop` in the crate, gated in CI (a panic in `Drop` under `panic = "abort"` is unrecoverable for consumers)

## THY — Theory alignment

- [ ] THY-1 Every law in `THEORY.md` states its status: definitional, derived (with proof sketch), property of an encoding, empirical (with probe), or conjecture
- [ ] THY-2 *(not blocking)* `docs/CLAIMS.md`: every claim in THEORY, DESIGN, ARCHITECTURE, README and rustdoc maps to a definition, proof sketch, named test or named probe
- [ ] THY-3 Law 10 (mediated self-reference) has a runnable probe with a recorded result. The v1-era probe source exists at commit `2b1c323` (`experiments/runner/src/exp19_mediated_self_reference.rs`) and is restored and ported, not rewritten from scratch. THEORY's citation resolves to it.
- [ ] THY-4 Law 11 text matches the shipped fold (syntheses per byte, depth). Sequenced **after** ENC-1/ENC-2, since the fold shape changes the hub gates.
- [ ] THY-5 *(not blocking)* Law 8 says "execution interleavings of the same operations"; Law 7's projection-stability claim is scoped to where it holds
- [ ] THY-6 *(not blocking)* THEORY states that Laws 5–12 continue the axiom numbering
- [ ] THY-7 No shipped capability is still marked as future work
- [ ] THY-8 *(not blocking)* Coding Law evidence paths are correct
- [ ] THY-9 *(not blocking)* Subsystem theory review has named tests as its evidence: subsystem-derived roots are order-independent, and no read path mutates engine state (before/after snapshot equality)
- [ ] THY-10 *(not blocking)* Every empirical claim has a runnable probe that names its **falsifying observation**, runs in CI, and records commit and seed
- [ ] THY-11 *(not blocking)* **Sequence Law.** THEORY states order encoding as a derived law: in a commutative, irreflexive, content-addressed substrate, sequence injectivity requires an anchor disjoint from element lineage, and nesting depth carries length. ENC-1 is its implementation, not a private utility.
- [ ] THY-12 *(not blocking)* Each of Axioms 1–4 has a named test that fails if only that axiom breaks, listed in `docs/CLAIMS.md`
- [ ] THY-13 THEORY and SPEC state which laws are conditional on id injectivity, and that an in-engine collision is undetectable (it presents as saturation, and content-address recomputation still passes)
- [ ] THY-14 *(not blocking)* Law 10's precondition is stated (the observation must lie outside the chain, or `synth(synth(x,obs),x)` collapses to a fixed point when `obs == x`), or Law 10 is labelled a conjecture
- [ ] THY-15 *(not blocking)* **Degree conservation.** Σ degree = 4N−6, so mean degree → 4 at every scale under every workload; hub degree is paid for by leaves, and the Coding Law is a variance claim under a fixed mean. Definitional, with one test.
- [ ] THY-16 **Lift Law.** THEORY names the lift — the map from non-distinction values into the graph — as the substrate's third forced object and its only free parameter, with two sub-parameters: the seed-anchor (which buys injectivity) and the alphabet (which shapes topology). `Canonicalizable` is its trait, the anchored fold its instance. Status: definitional.
- [ ] THY-17 *(not blocking)* THEORY's two statements about the LCA are reconciled: it is called "a pattern, not an axiom" at one point and "a substrate-level construct" at another. The resolution states which is binding and why, since it decides where the trait ships.
- [ ] THY-18 Law 11 is re-derived after ENC-1/ENC-2 land: under a fixed Σdeg, the Fold Law names where degree concentrates, and PRB-2 tests whether that is the alphabet rather than primordiality. Independent-law status is withdrawn if it is a corollary.
  - **PRB-2 has now run and the answer is: it is a corollary.** Independent-law status is withdrawn. THEORY states Law 11 as a corollary of the Fold Law applied to the lift's routing table, with no independent content, and either renumbers or keeps the number with that status attached.
- [ ] THY-19 THEORY stops claiming "the two primordials become topological hubs" without bound. Replace with what was measured: the byte lift's primordial degree is capped at `2^8 + 1 = 257` because the fold reaches only 256 accumulator states; folding data adds **zero**; the primordials hold top rank only while the corpus is under roughly 1 KiB, and at 64 KiB d₀ is rank 42.
- [ ] THY-20 THEORY states the **anchor ceiling** as a consequence of the lift's shape: an anchor touched only at the fold's first step takes at most `|alphabet|` novel participations, independent of corpus size. This is what makes the Law 11 demotion structural rather than workload-dependent, and it is the sharper claim the measurement bought us.
- [ ] ENC-13 The three doc blocks in `src/primitives.rs` that generalise alphabet-construction cost into a Fold Law claim are corrected: `ByteMapping`'s "Fold Law contribution" ("they accumulate degree far faster than anything else … a ratio of about 64×"), the "why not two calls per bit" note, and `ByteFold`'s preamble. The 64× figure holds at the 256-byte exercise and is **0.02×** at 64 KiB.
- [ ] TST-9 `fold_law_hub_ratio_characterises_this_encoding` in `src/primitives.rs` is rescoped and renamed. It folds **no corpus at all** — only the 256 `map_byte_to_distinction` calls — so its `ratio >= 50.0` floor measures alphabet construction, not degree concentration under data. Its rustdoc says "what survives as a claim is the shape", and PRB-2 showed the shape is the alphabet's. Either it asserts the 257 ceiling it actually measures, or it folds a corpus and asserts what happens then.
- [ ] DOC-23 `DESIGN.md` and `ARCHITECTURE.md` disclose that `domain_anchor` derives the anchor from `fold.tip(domain)`, so in the shipped configuration the lift's two free parameters — anchor and alphabet — **covary**. This is not an axiom violation (the anchor is a child of a tip and the fold's guard still holds), but it means no in-tree measurement can separate the two factors, which is why Law 11 stood unexamined.

## PRB — Probes

- [x] PRB-1 Mediated self-reference: 100K depth, with constant-observation, cycling-observation and observation-drawn-from-chain variants. Falsified by any repeat or fixed point before the birthday bound.
  - evidence: `tests/mediated_self_reference.rs` — all three regimes reach **100,001 distinct distinctions at depth 100,000**, with no fixed point and no repeat. Beyond the stated bar it also asserts, at every step, that `parents_of(next)` contains the previous element: without that the chain would be merely novel rather than self-referential, and the law would be vacuous. Carries its own contrast case (`synth(x, x) == x`), so mediation is shown to be doing work rather than decorating a construction that would have worked anyway. Verified to fail: replacing the mediated construction with direct self-synthesis trips the fixed-point assertion at step 1 in all three regimes.
- [x] PRB-6 Order emerges from an order-blind operator — the empirical content of "structure falls out of synthesis", stated as a counted prediction rather than an impression.
  - evidence: `tests/order_emergence.rs` — the operator is commutative, so it cannot represent an ordered pair; a flat left-associated fold therefore loses exactly the leading transposition, and **120 orderings of five distinct bytes collapse to 60** identifiers. The anchored fold recovers **all 120**, with no position index stored anywhere: order falls out of nesting, because composing commutative operations is not commutative. The `n!/2` figure was derived from the axiom before being measured, and it came back exact. Also pins why the anchor must not be an element tip — that is the single thing breaking the symmetry.
- [x] PRB-2 Anchor-hub: the same corpus folded under k different seeds **and** alphabets, varied independently, measuring whether top-degree rank tracks the alphabet. If it does, Law 11 demotes to a corollary of THY-16 and THEORY says so.
  - evidence: `experiments/runner/src/anchor_hub_probe.rs`, run via `cargo run -p koru-experiments-runner --release --bin anchor_hub_probe`. **The hub tracks the alphabet.** 6 anchor levels × 6 alphabet levels fully crossed, identical corpus per cell: the top-degree node changed in **0 of 90** alphabet-varying pairs and **90 of 90** anchor-varying pairs — `agree(vary anchor) = 1.000`, `agree(vary alphabet) = 0.000`, at every corpus size ≥ 1536 B. Three exact results, not trends: (1) `degree(d₀)` and `degree(d₁)` gain **zero** from folding data in all 648 cells — their 257 is one-time alphabet-construction cost that never grows; (2) `anchor.fold_delta == distinct_first_steps` exactly, so an anchor touched only at the first step is **ceilinged at 256 regardless of corpus size** — forced by the encoding's shape, not a workload accident; (3) `degree == novel_participations` held for every node in every cell, so Law 12's structural part fully accounts for every figure Law 11 quotes. Crossover at ~1.2 KiB: "the primordials are the hubs" is true only below that, and at 64 KiB d₀ is rank **42**. Discriminating: an anchor-routed control on the same corpus and factors returns `TracksAnchor` at every size with the ceiling broken by two orders of magnitude. Cell (0,0) is asserted byte-identical to the shipped encoding, so the probe measures the lift rather than a re-implementation.
  - note: the shipped configuration could never have shown this. `domain_anchor` derives the anchor from `fold.tip(domain)`, so anchor and alphabet **covary** in-tree; decoupling them is the whole experiment.
- [ ] PRB-3 Projection idempotence for an arbitrary signal (see PRJ-8), shipped as the consumer self-test
- [x] PRB-4 THEORY's own falsifier, run and recorded: two engines fed the same parent pairs in different orders, queried at quiescence with the same `{root, direction, boundary, signal}`, produce byte-identical `canonical_bytes`. THEORY names this as the experiment that would refute its central claim, and nothing in the crate ran it.
  - evidence: `tests/order_independence.rs::same_pairs_in_different_orders_produce_identical_bytes` — 600 pairs, permuted to a different topological order that differs from the original at **595 of 600 positions** (the test asserts the divergence is real, so it cannot silently degenerate into re-running the same order). Compares distinction counts, relationship counts, the full distinction set, parentage edge-for-edge, and `projection_id` + `canonical_bytes` across all 18 axis combinations — 3 directions × 3 signals × 2 boundary kinds. Verified to fail: injecting one novel distinction into the second engine trips the byte assertion at `Upstream/Degree/hops(2)`.
- [x] PRB-8 **Clustering is mediated self-reference.** The only edges in the graph are child-parent plus the single genesis edge, so a triangle needs one of its pairs to be an edge in its own right — meaning `c = synth(a, b)` closes a triangle exactly when `a` and `b` were already adjacent, i.e. when one is the other's parent. That is Law 10's shape: for `m = synth(x, obs)` and `r = synth(m, x)`, `parents(r) = (m, x)` and `x` is a parent of `m`. So the local clustering coefficient of the parentage graph measures the density of mediated self-reference and measures **nothing else** — a structural identity between a standard graph statistic and a law of this theory.
  - evidence: `tests/parentage_topology.rs::every_triangle_is_a_mediated_self_reference` — 548 distinctions, 1,093 relationships, **546 triangles: 545 mediated self-reference, 1 genesis, 0 unexplained.** A closed classification with no residue, asserted alongside `r = 2d − 3`. Verified to fail: injecting one triangle among three distinctions that are not each other's parents is reported as exactly 1 of 547 unexplained. This generalises `tests/mediated_self_reference.rs` from a single chain to a population.
- [x] PRB-9 **Synthesis is strictly local — no action at a distance.** For two neighbourhoods that are disjoint *and non-adjacent*, work confined to one perturbs no clustering coefficient in the other, exactly.
  - evidence: `tests/parentage_topology.rs::synthesis_confined_to_one_neighbourhood_does_not_reach_another` — 400 syntheses confined to a 37-node neighbourhood changed **0 of 12** coefficients in the far neighbourhood, compared bit-for-bit. Verified to fail: drawing one parent from the far neighbourhood instead moves **12 of 12**. This is a live regression gate, not a philosophical claim: a reverse index, a degree cache that rewrites a third party, or any rebalancing pass would break it, and locality is what "every distinction is an advance of its own parents" rests on. Note that v1.2's deleted information-dynamics test asserted the **negation** of this — it tried to show coupling between separated subprocesses, and the theory's own locality commitment requires there to be none.
- [ ] PRB-10 Coherence mean-reversion under confinement, with recorder/replay branching so every arm starts from one recorded history. Measured out-of-tree: confining synthesis to a radius-2 ball contracts coherence spread — sd 0.1294 → 0.0778, slope of final on initial +0.29, `r(initial, Δ) = −0.80`. **The caveat must ship in the test body:** the attractor (~0.25) and the apparent global constant (mean coherence stable at ~0.156 across a 3× scale range) are properties of the growth policy, not of the substrate. Asserting them as substrate constants would repeat exactly the mistake the deleted v1.2 probes made.
- [ ] PRB-7 The Law 12 ρ gate gets a negative control. `experiments/runner/tests/coding_law.rs` asserts `rho >= 0.985`, but degree-delta is incremented by the very draws that frequency counts, so ρ ≈ 0.99 may be close to arithmetically forced rather than an empirical finding about the substrate. `THEORY.md` presents it as measured and workload-conditional ("settles around ρ ≈ 0.99 with run-to-run noise of ~0.005"), which is strictly stronger than "a near-tautology clears its own threshold". One shuffled-frequency run — same degree deltas, permuted frequency vector — shows what ρ a *dead* substrate yields and makes the gate mean something. This is the headline empirical number in THEORY.md, and it is the same structural defect as the 1.2.0 information-dynamics test: a correlation with no null model. Better executed here; identical missing piece.
- [x] TST-11 Axioms 2 and 3 are tested **through the WebAssembly boundary**, not only natively.
  - evidence: `crates/koru-lambda-wasm/tests/wasm_smoke.rs` — `axiom2_commutativity_holds_across_the_boundary`, `axiom3_irreflexivity_holds_across_the_boundary`, `law7_saturation_adds_no_structure_across_the_boundary`. 21 wasm tests pass, up from 18. The gap predates 2.0.0: the published 1.2.0 tarball's falsification suite carried a wasm-consistency module with a `test_falsify_wasm_axiom_violations` written to assert exactly this, and **that module never compiled once before it was published** (18 hard compile errors under `--features wasm`, verified by building the published tarball), so the axioms had never been checked through the bindings in any released version. Not redundant with the native tests: the bindings cross an ABI, marshal through `JsValue`, and compile under a different backend at 32-bit pointer width — argument-order canonicalisation and the irreflexive fast path are both things a marshalling layer can disturb with no native test noticing.
- [ ] TST-10 `snapshot_distinctions()` and `snapshot_parentage()` are tested under concurrent writers, or their doc comments state the quiescence precondition. `snapshot_parentage` is documented as "the canonical persistence dump: combined with `replay_topological`, it reconstructs a byte-identical engine" — and that guarantee is sound only for a quiescent engine. The two maps are read independently, so a concurrent dump can contain a child whose parents are absent, which is structurally the same failure the v1.2 compactor hit. There are 8 threaded tests in `src/engine.rs` and **not one takes a snapshot while writers run**. Right now the doc promises more than the code delivers.
- [ ] PRB-5 `tests/cond_d_falsifier.rs` is renamed and its rustdoc corrected to say what it tests. It drives both engines from one closure over one corpus in **one order** — its own comment says "identical synthesis histories" — so it establishes determinism (Axiom 1), not the order-independence of Laws 8 and 9 that its name and position imply. It is a good test under the wrong name, and the name is why nobody noticed the real falsifier was missing for three months.

## CONF — Conformance (the polyglot mechanism, settled 2026-10-01)

The strategy, recorded in `ECOSYSTEM.md`: the substrate's algorithm is small
enough that a conforming implementation is tens of lines in any language, so
a *conformance suite* reaches more languages than bindings do, at a fraction
of the cost. These items are what make that claim true rather than stated. A
foreign-function interface stays out of scope because it answers a different
question — a shared engine instance, not agreement — and nobody has asked for
one.

- [ ] CONF-1 `conformance/vectors.json` is the single source for every published vector: operator cases, byte-lift tips, the anchored fold, and the **full cross product** of projection wire forms — 3 signals × 3 directions × 2 boundary kinds — each with its `projection_id`. Language-neutral: hex strings and integers, no Rust types. *(Widened 2026-10-01: "a full projection wire form", singular, pinned 1 of 18+ and constrained neither the `Degree`/`HopDistance` per-node output encodings, nor the 16-byte signal identity per signal, nor the `Downstream`/`Undirected` node sets, nor `Saturated`'s zero-payload rule. A foreign implementation could pass every vector and still produce different bytes for any of those.)*
- [ ] CONF-2 `src/vectors.rs` is generated from, or test-asserted against, `conformance/vectors.json`. Two sources of truth for the same bytes is the drift that content addressing exists to prevent, and a gate fails if they disagree.
- [ ] CONF-3 `conformance/README.md` states what an implementation must reproduce to call itself conforming, in a numbered list keyed to `docs/SPEC.md` sections, with no reference to Rust.
- [ ] CONF-4 A non-Rust reference implementation ships under `conformance/`, reading `vectors.json` and asserting every case, with a provenance header naming the `docs/SPEC.md` commit it was written against. It replaces an unverifiable transcript with something a reader runs, and it makes "tens of lines in any language" a measured number. **It is not a substitute for DEMO-5**: once it is in-repo and gated, the maintenance incentive is to fix it until it matches Rust, at which point it proves the fixtures are self-consistent rather than that the specification is sufficient. Only an implementation written from the document by someone without the Rust source can establish sufficiency.
- [ ] CONF-8 `vectors.json` carries a `reject` section: byte sequences plus the expected rejection reason, mirroring PRJ-3's enumerated offsets — wrong magic, wrong version, out-of-range direction tag, truncated entry array, signal mismatch, unregistered root, well-formed forgery. The trust boundary is *rejection* semantics and cannot be expressed as a positive vector, so a positive-only suite certifies "agrees on good input", which is not what a trust boundary is.
- [ ] CONF-9 `docs/SPEC.md` §10 is generated from `conformance/vectors.json`, and the CONF-2 gate covers all three edges. Otherwise §10 is a third source of truth: CONF-3 tells an implementer to work from SPEC while CONF-5 certifies them against the JSON, so when those drift the suite says "conforming" about a document that no longer describes the implementation.
- [ ] CONF-10 ENC-11 and ENC-12 land **before** CONF-1 freezes, and `vectors.json` carries a non-`u8` alphabet case plus a composite with a length-prefixed field. `docs/SPEC.md` §4.2 specifies the fold over an arbitrary alphabet, but only the `Σ = {d₀,d₁}` byte case ships with vectors, and composite framing is unspecified — so two implementations that pass every vector will disagree on the first variable-length composite either one lifts. That is the collision class `ByteFold` closed, relocated one level up and out of reach of the suite.
- [ ] CONF-11 `domain_anchor()` has a vector per reserved label, and `conformance/README.md` carries the reservation table. DEC-4 makes domain separation an anchor parameter, so anchor derivation is identity-bearing; `DomainLabel` is a bare `u8` alias (ENC-10), so two implementations can agree on every vector and still derive the same anchor for different domains.
- [ ] CONF-12 The conformance suite covers the npm/wasm surface, which is the artifact most readers will actually run. CONF-1..5 as written reach only the Rust crate, and the lift is not reachable from JS at all (JS-9).
- [ ] CONF-5 CI runs CONF-4 against `vectors.json` on every push. An implementation nobody runs is a claim, not a check.
- [ ] CONF-6 `docs/SPEC.md` §10 publishes the byte tips and the anchored sequence fold. §4.3 already tells the reader that §10 "include[s] byte tips and a sequence fold" and §10 contains neither — only `c`, `d`, `e` and the adjacency bytes. The constants exist in `src/vectors.rs` (`TIP_*`, `FOLD_HI_ANCHORED_AT_D_HEX`) but were never published, so an independent implementer cannot verify the lift "from this document alone", which is §10's stated contract.
- [ ] CONF-7 The cross-target check (Cond D′) covers the lift. `crates/koru-lambda-wasm/tests/cond_d_prime_wasm.rs` asserts only `C_HEX`, `D_HEX`, `E_HEX` and the adjacency bytes, and the wasm surface exports no tip or fold entry point — so the substrate's **one free parameter** is the one thing byte-equality is not checked on. `docs/SPEC.md` §9's D′ row claims verification "against §10", which is narrower than its wording implies. Requires JS-9 (a JS consumer can lift its own data) to be reachable.

## DOC — Documentation

- [ ] DOC-20 The docs stop claiming a **type-level** trust boundary while four public bypasses exist. `README.md` says "Foreign-byte injection is closed at the type level" and `docs/SPEC.md` §3 says "the type system distinguishes the two", but `Distinction` publicly exposes `from_hex`, `FromStr`, `Pod` and `Zeroable` — each produces one with no engine, from an external crate. The real boundary is ENG-1's runtime `assert_registered`, which is engine-witnessed and correct; the docs should say that until ENG-4 removes the paths. Note the second-order consequence: with `panic = "abort"` in the release profile (API-5) the guard is a process abort rather than a catchable error, so "trust boundary" means "the process dies" for release consumers.
- [ ] DOC-21 `ARCHITECTURE.md` and `ECOSYSTEM.md` describe the same three layers. ARCHITECTURE says the *crate* divides into Bindings / Reference subsystems / Substrate with the substrate including "the LCA trait, and consumer helpers (replay, recorder, hex display)"; ECOSYSTEM names the recorder, the hashing optimisation and the children index as excluded. ARCHITECTURE describes what ships, ECOSYSTEM describes what TRIM-1..4 would produce, and neither says so. `ARCHITECTURE.md` also still shows `src/wasm.rs ~680 LOC`, which no longer exists.
- [ ] DOC-22 `ECOSYSTEM.md` stops overstating what two compilation targets prove. It says a second target "is what makes cross-target agreement checkable"; DOC-19 and DEMO-5 exist to narrow exactly that — two compilations of one source are not two witnesses, and what the wasm suite establishes is 32-bit pointer width plus an independent codegen backend. Same for "on every target": CI covers one native target and wasm32, with no big-endian and no 32-bit native, so byte-order independence rests on the little-endian encoding being specified, not on being tested.
- [ ] DOC-16 `docs/SPEC.md` covers the Fold Law and the Coding Law. It currently mentions neither — two of the twelve laws are absent from the document shipped as the theory instrument.
- [ ] DOC-17 The 23 stale claims found by the claims inventory are corrected. The dangerous one: `DESIGN.md:141` and `ARCHITECTURE.md:112,121` still describe the foreign-byte guard as a debug-only `debug_assert!`, so a reader would conclude release builds accept foreign parents — the opposite of what ships.
- [ ] DOC-18 Six of eight `[SHIPPED @ src/…:NNN]` anchors in DESIGN are off by 70–110 lines, and nine code comments cite design gates that no longer exist. Symbol anchors replace line anchors (see DOC-2).
- [ ] DOC-19 The cross-target claim states only what it falsifies. Two compilations of one source are not two witnesses: what the wasm suite actually establishes is 32-bit pointer width and an independent codegen backend, continuously. Byte-order independence rests on explicit little-endian encoding, not on a big-endian target.

- [x] DOC-1 `docs/SPEC.md`:
  - evidence: `docs/SPEC.md` @ 1e4c6ea — all 15 citations repointed; independently reimplemented from the document alone, reproducing every golden vector and the 150-byte wire form
  - exact synthesis hash (concatenation, no separator, lexicographic min, 16-byte prefix, collision bound)
  - primordials
  - the anchored sequence fold and the byte-tip derivation (without these, commitment roots cannot be reproduced by a second implementation)
  - wire format
  - projection conditions A–F and the cross-target condition
  - golden vectors

  Every spec reference in source, tests and Cargo.toml points to it.
- [ ] DOC-2 `DESIGN.md` uses symbol anchors only; the type-level trust-boundary claim is true; the subsystem sections match the code; nothing is marked for a later version; and its non-goals do not contradict shipped items (non-goal 6 "no byte-fold changes" contradicts ENC-1; non-goal 8 "no networking stack" contradicts SUB-8)
- [ ] DOC-3 `README.md` claims match `docs/CLAIMS.md`; the trust-boundary wording is exact
- [ ] DOC-4 `docs/MIGRATION_JS.md` is correct for the shipped target (no stale `init` / `initSync` guidance), covers every JS change, and does not contradict the shipped surface
- [ ] DOC-5 `docs/MIGRATION_RUST.md`: v1.x → 2.0.0 for Rust consumers, including the subsystem API mapping, the raw/verified type split, and the commitment-hash change with before/after vectors
- [ ] DOC-6 `CHANGELOG.md` lists every breaking change, derived from `api/DELTA-from-1.2.0.txt` rather than from recollection
- [ ] DOC-7 rustdoc: no broken links; `# Errors` / `# Panics` on every public function; no stale comments
- [ ] DOC-8 `ARCHITECTURE.md` matches the code
- [ ] DOC-9 Example comments are accurate (tier 3 cone anchor, tier 4 header), in both languages
- [ ] DOC-10 `docs/BENCHMARKS.md` numbers re-measured after all changes
- [~] DOC-11 **[cut]** `docs/development/*` is accurate and current (restored 2026-09-21: nothing is dropped; HYG-4 decides whether it ships, this item keeps it correct either way)
  - cut 2026-09-30: Merged into HYG-4, which already decides what ships file by file.
- [ ] DOC-12 `docs/SPEC.md` states the engine model explicitly: an engine is a witness of what it has synthesized, not a namespace; primordials are the same constants everywhere; identity is engine-independent by construction; ids carry no origin and are not scoped to an application; an unregistered distinction reaching `synthesize` is a caller error caught before any state change
- [ ] DOC-13 `docs/SPEC.md` records the outcome and reasoning of every closed `DEC-*` item, so a reader finds the limits stated rather than discovering them

- [ ] DOC-14 `docs/MIGRATION_JS.md` stops describing the subsystem classes as retired, since FFIJS-* restores them, and documents their new shape
- [ ] DOC-15 README and rustdoc carry a "lifting your data" section: a domain value folded to a root, then used to advance an agent — the path a stranger needs and cannot currently find

## LAND — The published artifact

- [ ] LAND-11 A "Built on this" section names the **domains**, not the repositories — physics, cognition, space, ephemeral state, notarisation. Twenty `koru-*` repos by one author reads as a hobby; the same facts as five domains read as five independent confirmations. Same evidence, opposite verdict.
- [ ] LAND-12 *(superseded by LAND-11)* A "Built on this" section names the downstream projects — spatial, cognitive, physical — with links. It shows the substrate's reach without shipping any of their weight, which is the whole argument for the minimal core.

- [ ] LAND-1 README first screen, in order: the ≤25-word claim (LAND-4), a runnable example with its printed output, then the cross-runtime byte-equality claim in one sentence. Theory is linked below the fold, not led with.
- [ ] LAND-2 DEMO-1 and DEMO-4 run against the **published** npm tarball and crates.io package in CI, with no `wasm-pack` step. Today every demo requires a Rust toolchain and a local build, so no demo is runnable by someone who installs the package.
- [ ] LAND-3 The npm landing page (`pkg/README.md`) is JS-first, not the Rust README
- [ ] LAND-4 One claim, ≤ 25 words, naming content-addressed folding and cross-runtime byte-equality, identical in README, crate description, npm description, npm landing page and crate rustdoc; a gate diffs all five
- [ ] LAND-5 "What this is not": grow-only set rather than a general CRDT; Merkle determinism rather than consensus; 128-bit ids; and not a consensus protocol — the consensus agents are reference consumers, named and linked as such
- [ ] LAND-6 A 60-second quickstart, timed by someone who has not seen the repo
- [ ] LAND-7 npm keywords, description and rendering verified on the real package page before publish
- [ ] LAND-8 Crate description, keywords and categories name the capability, not the theory and not a protocol category. Today the description is "a minimal axiomatic system for distributed computation" and the keywords include "consensus", which files the substrate in the wrong aisle and invites comparison to protocols it is not.
- [ ] LAND-9 The hook runs with no toolchain: a reader reproduces a child id against their own SHA-256 in under 60 seconds from a browser or a single command, without installing Rust
- [ ] LAND-10 Neither "distinction theory" nor "axiom" appears above the first runnable example in the README, the npm landing page or the crate-level rustdoc
- [ ] LAND-13 The README carries a layer signpost: which readers want the agent layer, which want the substrate directly, and a link to `ECOSYSTEM.md`. Sixteen downstream projects depend on the substrate directly and two re-implemented a shipped agent, because nothing in the README said where to go. The signpost is the only fix for that specific failure, and the README has none today.

## HYG — Hygiene and packaging

- [x] HYG-1 No internal planning or audit tags in source, tests, benches or manifests
  - evidence: `scripts/gates/no_process_leaks.sh` clean @ 1e4c6ea
- [ ] HYG-2 `scripts/build_universal.sh` removed
- [x] HYG-3 `cargo audit` and `cargo deny` clean (crossbeam-epoch ≥ 0.9.20; rand advisory; duplicate versions resolved)
  - evidence: `cargo audit` reports 0 @ e201dc1 — crossbeam-epoch 0.9.21, lru 0.18, rand pin =0.9.5
- [ ] HYG-4 Crate tarball and npm package contents reviewed file by file, including whether `docs/development/*` ships
- [ ] HYG-5 The whole workspace (including `experiments/`) builds and tests in CI
- [x] HYG-7 The corpus generator writes where the gate reads. The two had drifted apart when the gate test moved crates, so a regeneration would have written to the workspace root — a path nothing reads — leaving the pinned corpus stale while appearing to succeed.
  - evidence: `experiments/runner/src/bin/gen_exp18_corpus.rs` @ e201dc1; regeneration reproduces the pinned digests in place
- [~] HYG-6 **[cut]** `cargo publish --dry-run` output is byte-compared against a committed manifest; `Cargo.lock` is committed; a clean checkout reproduces the same tarball
  - cut 2026-09-30: Reproducible-tarball byte-comparison for a crate that publishes a few times a year.

## DEC — Owner decisions

- [ ] DEC-1 **C FFI. Decided (owner, 2026-09-21): restore to the full standard.** Nothing is dropped, regardless of current consumer count. Implemented by FFI-1..9. Ticks when the reasoning is recorded per DOC-13.
- [ ] DEC-2 **JS bindings for the subsystems. Decided (owner, 2026-09-21): build them to the full standard.** Implemented by FFIJS-1..5 and DOC-14. Ticks when the reasoning is recorded per DOC-13.
- [ ] DEC-3 **Demo scope. Decided (owner, 2026-09-21): keep all four; nothing folded.** DEMO-4 is the flagship and DEMO-1 the hook, but DEMO-3 stays a separate demo.
- [ ] DEC-4 **Domain separation.** The hash-tag branch is **closed on theory grounds** (theory veto, 2026-09-21): a domain tag in the hash is exactly the separate naming layer Axiom 4 excludes. Domain separation is an anchor parameter of the fold. ANA-1 documents the consequences; it does not reopen the choice.
  - requires: ANA-1
- [ ] DEC-5 **Identifier width.** Ids are SHA-256 truncated to 128 bits (collision search bounded near 2^64), and the width is **unversioned**: the wire format has a version field, an id does not, so a later widening cannot be migrated. Decide: accepted with a written threat model and that non-goal stated, or widened now.
  - requires: ANA-2
- [ ] DEC-6 **Unbounded growth.** The eviction branch is foreclosed by ENG-3 (evicting an interior node fails the engine's own auditor), so the real question is whether a peer can drive an engine out of memory. Decide: accepted and documented with measured costs and the recommended consumer pattern, or hostile-input memory bounds become their own items.
  - requires: ANA-3
- [ ] DEC-7 **Published wasm target.** *(Framing settled 2026-09-30: the wasm crate ships to npm as a product, so this is decided on technical merit — which target best serves a reader with no toolchain — and not by what v1.2 consumers used. Verify a packed tarball before publishing; npm's unpublish window is 72 hours.)* v1.2 shipped `--target web`; 2.0.0 ships `--target bundler`, which breaks the only npm consumer. Decide the target strategy (single target, or multiple entry points) — see JS-8.
- [ ] DEC-8 **`BatchSynthesizer` (SUB-10). Decided (owner, 2026-09-21): port it to the full standard.** No consumer today; shipped anyway. Ticks when the reasoning is recorded per DOC-13.
- [ ] DEC-9 **Crate boundary. Decided (owner, 2026-09-21): four crates**, one workspace, one release, same bar — `koru-lambda-core` (engine, projection, replay, lift), `koru-lambda-consensus` (validator, commitment, network, classifier, batch), `koru-lambda-wasm` (JS/TS bindings; extracted 2026-09-22 because `wasm-pack` requires `cdylib`), `koru-lambda-ffi` (C ABI). Nothing is dropped. Implemented by PKG-1..7. Viability was measured first: the classifier and batch paths both compile against the public API from an external crate, with no internal access. Ticks when the reasoning is recorded per DOC-13.
  - requires: ANA-4 (records the measured basis: 7→5 direct deps, `lru` and mandatory `serde` leave the substrate, ~37 KB of wasm no longer paid by browser users who don't use consensus, and a consumer-layer bug stops forcing a substrate major)
- [ ] DEC-10 **Sealing.** Measured 2026-09-21 from an external crate against the public API only: a sibling **can** define its own `Signal`, materialize it, get canonical bytes and a projection id, achieve cross-engine byte equality, and `restore_projection` its own type — no escape hatch needed. Sealing blocks exactly three things, permanently at 2.0.0:
  1. implementing `CoreSignal` (substrate-internal dispatch)
  2. implementing `ReadOnlyEngine` — so no alternative engine ever: no mock, no disk-backed, no remote
  3. adding `Boundary` / `Direction` variants — so no threshold or other custom traversal bound

  **Decided (owner, 2026-09-21): unseal `ReadOnlyEngine`; `CoreSignal` stays sealed; `Boundary`/`Direction` stay closed.** Implemented by SEAL-1..3. Ticks when the reasoning is recorded per DOC-13.

## ANA — Analyses backing the decisions

- [~] ANA-1 **[cut]** Domain separation: what collides today across applications, what an anchor-based separation gives, what a hash-level domain tag would cost (identity change, vector churn, loss of cross-application convergence, naming layer), and which downstream projects are affected
  - cut 2026-09-30: Retroactive minute-taking: DEC-4's hash-tag branch is closed on theory grounds and needs no separate analysis.
- [ ] ANA-2 Collision analysis: the birthday bound at 128 bits, accidental-collision probability at realistic and extreme counts, the adversarial case, what an attacker gains, and the consequence of ids being unversioned and unmigratable
- [~] ANA-3 **[cut]** Growth analysis: measured bytes per distinction and per relationship at 1K/1M scale, peak allocation for a maximum-size hostile batch, whether a peer can drive an engine past available memory, and the recommended consumer pattern
  - cut 2026-09-30: Folded into DEC-6's written outcome; a separate growth analysis for an append-only graph is ceremony.
- [~] ANA-4 **[cut]** Crate-boundary analysis: what each crate's name promises and what a stranger searching for it finds; the semver blast radius of a consumer-layer bug under each layout; the dependency surface (`lru`, `serde`, `unsafe`) each layout forces on a substrate user; feature-matrix and wasm size effects; publish mechanics; and the cost of the migration the split itself creates
  - cut 2026-09-30: The crate-boundary decision is made and implemented; an analysis justifying it after the fact serves nobody.

## REV — Independent review (after everything above)

- [ ] REV-1 Code review from reviewers who see only the code: no must-fix findings
- [ ] REV-2 Test-quality review: no must-fix findings; mutation report clean
- [~] REV-3 **[cut]** Theory alignment: a reviewer given only `THEORY.md` and the code produces a written finding list with zero must-fix items
  - cut 2026-09-30: Theory alignment is covered by THY-1 and the probes, which are falsifiable where a review pass is not.
- [~] REV-4 **[cut]** Drift sweep: no stale names, paths or claims in tracked files
  - cut 2026-09-30: Drift is now gated mechanically (`no_process_leaks`, `api_snapshot`, link and anchor checks).
- [~] REV-5 **[cut]** Every finding raised by REV-1..4, the mutation run, the probes and the downstream check has become an item here and that item is ticked. No open finding remains at release.
  - cut 2026-09-30: A closure loop that cannot terminate on its own terms: it requires every finding to become a ticked item, including findings about itself.

## DEMO — Demonstrations (Rust and TypeScript, identical output, run in CI)

- [ ] DEMO-1 Predict-and-confirm: child ids reproduced with a plain SHA-256 call (the hook — checkable by hand in seconds)
- [ ] DEMO-2 *(not blocking: ships when ready)* Trust boundary: verify rejection, projection restore round-trip, tamper rejection
- [ ] DEMO-3 *(not blocking: ships when ready)* Order independence: shuffled, repeated and argument-swapped histories give identical bytes
- [ ] DEMO-4 Cross-runtime convergence (**flagship**): Rust writes the history; a separate Node process replays it shuffled and reproduces the same projection id; tampering fails
- [ ] DEMO-5 *(not blocking: ships when ready)* An independent re-implementation written from `docs/SPEC.md` alone (a standalone script, not this crate) reproduces every golden vector: child ids, fold roots, projection ids. This is the only item that can falsify "the spec is sufficient"; DEMO-4 compiles the same source twice and cannot.
- [ ] DEMO-6 Lifting domain data: a domain struct folded to a root and used to advance an agent, in Rust and TypeScript, with identical output. Every demo today synthesizes primordials; none shows a reader how their own data gets in.

## REL — Release (each step needs explicit owner approval)

- [ ] REL-1 `pkg/` rebuilt with `--features wasm`; version and exports verified
- [ ] REL-2 `cargo publish`
- [ ] REL-3 `npm login` (owner) and `npm publish`
- [ ] REL-4 Annotated tag `v2.0.0` created and pushed
- [ ] REL-5 Yank criteria agreed in writing before publish: what would justify yanking 2.0.0 and who decides

---

# Not blocking 2.0.0

Retained in the repository, out of the release. Nothing here is abandoned; it is unscheduled.

## SUB — Consensus reference consumers (ships in `koru-lambda-consensus`)

*Deferred 2026-09-30: `koru-lambda-consensus` is a reference consumer, not the substrate. It keeps its collision fix and carries `publish = false`; its remaining wire-facing defects (SUB-2..6) are recorded and unfixed, and it must not be published until they are.*


- [x] SUB-1 Transaction and commitment folds use ENC-1. Tests:
  - evidence: `crates/koru-lambda-consensus/src/{validator,commitment}.rs` @ e354591 — both folds anchored; tests `validator::tests::nonce_byte_order_is_not_lost`, `..._a_repeated_leading_byte_is_not_absorbed`, `..._nonce_and_data_do_not_bleed_into_each_other`, `..._distinct_transactions_have_distinct_roots`, `..._distinct_inputs_give_distinct_roots` (proptest); reproduction program now reports all three cases distinct
  - regression vectors: nonce 1 vs 256, 2 vs 512; `[0x00]` vs `[0x00,0x00]`
  - pinned roots
  - property test: distinct inputs give distinct roots
- [ ] SUB-2 Wire types deserialize to a **raw form** holding `RawDistinctionId` (via `raw_serde_adapter`); the verified form is produced by an explicit `verify(&engine)` step. Both types are public and named in `docs/MIGRATION_RUST.md`. This resolves the contradiction between "ids must be verified" and "a node booting from disk has no engine yet".
- [ ] SUB-3 Every `Deserialize` in the crate is bounded (`TransactionBatch` cardinality cap on the raw form); each bound is tested with oversized input
- [ ] SUB-4 `verify_batch` does not mutate the caller's engine (tested: engine state byte-identical before and after)
- [ ] SUB-5 `ConsensusValidator`'s agent path enforces nonce validation (no route advances the root without it)
- [ ] SUB-6 `TransactionAction` fields are private, with getters; `CommitmentAgent::with_capacity(NonZeroUsize)`; wire sizes are `u64`; `Debug` on all public types
- [x] SUB-7 The `lru` advisories are resolved (fixed version or an audited bounded map, reason stated)
  - evidence: `crates/koru-lambda-consensus/Cargo.toml` @ e201dc1 — lru 0.18; `cargo audit` reports 0
- [ ] SUB-8 `NetworkAgent` + `PeerIdentity` ported to 2.0.0, covering the v1.2 surface used downstream (`new`, `from_state`, `join_peer`, `propose_commitment`, `check_commitment`, `finalize_batch`, `advance_epoch`, `get_current_leader`, `current_epoch`, `validator_count`, `validator_set`, `consensus_state_root`, `get_stats`, nonce restore). Required properties:
  - typed peer-id construction with bounds
  - joint (id, distinction) dedupe
  - bounded pending commitments, cleared on epoch advance
  - atomic restore
  - verified wire ids
- [ ] SUB-9 Compaction resolved honestly, **derived rather than ported** (amended 2026-09-29).
  The v1.2 `StructuralCompactor` is 503 lines that read the graph their own way. Reading the
  graph by root, direction, boundary and signal is what a projection *is*, and the classification
  was demonstrated working from an external crate against the public API alone: a thermal signal
  over `degree` materialises, produces canonical bytes and a projection id, achieves cross-engine
  byte equality, and restores. The threshold belongs to the signal, not the boundary, so the
  closed `Boundary` enum (DEC-10) does not block it.
  **Nothing is dropped and no consumer breaks:** the name and the five methods actually used
  downstream — `new`, `set_hot_threshold`, `compact`, `get_stats`, `synthesize_action` — are kept
  as the facade. Only the implementation changes: a projection for the classification, plus the
  threshold and archived set, which are genuinely consumer state. Expect roughly 100 lines in
  place of 503.
  Original requirements still apply: The v1.2 `StructuralCompactor` never removed anything from the engine — it classified nodes by degree threshold and tracked its own archived set. So it is a read over a degree signal, which is what a projection is. Either port it as an explicitly consumer-side classifier, or express it as a projection boundary/signal; the "compactor" name goes if it evicts nothing. Required: classification is a pure function of engine state (shuffled replay yields identical stats).
- [ ] SUB-10 `BatchSynthesizer` ships **as a thin helper, not a port** (amended 2026-09-29). Batching is scheduling, not substrate: the engine is already `Send + Sync`, so a batch is a fold over calls across threads. The v1.2 type was 379 lines wrapping what a consumer writes in a handful; expect roughly 30. The result representation (`Option` vs sentinel) is justified, and concurrency is tested to the same bar as everything else. *(Zero downstream consumers found in any sibling project; the owner decided 2026-09-21 to ship it regardless, and it keeps its name.)*
- [ ] SUB-11 Every subsystem meets the same mutation, coverage and error-variant bar as the core
- [x] SUB-12 Consensus tests build distinctions through the engine, not through an internal constructor. Found when the crate split made `Distinction::from_bytes_unchecked` unreachable from outside the substrate — three tests had been fabricating identifiers from raw bytes.
  - evidence: `crates/koru-lambda-consensus/src/{validator,commitment}.rs` @ 57d892f — no `from_bytes_unchecked` remains; the crate compiles only against the public API

## FFI — C ABI (ships in `koru-lambda-ffi`; restored by owner decision 2026-09-21)

*Deferred 2026-09-30: No consumer exists, and the C ABI was 20% of the published v1.2 by line count. Restoring it would rebuild the ratio that made v1.2 nine percent substrate.*


v1.2 shipped `src/ffi.rs`: 22 `extern "C"` functions over the engine and the network agent, using `Arc::into_raw` / `Box::into_raw`, `CString` returns, and **no `catch_unwind` anywhere**. It is restored to the same standard as the rest of the crate, not copied forward.

- [ ] FFI-1 Every `extern "C"` function catches unwinding at the boundary (`catch_unwind`) and returns an error code instead. A Rust panic must never cross into C, which is undefined behavior; today nothing catches.
- [ ] FFI-2 Every pointer parameter is null-checked and its ownership rule is documented: who allocates, who frees, and what double-free or use-after-free is prevented by construction
- [ ] FFI-3 Every returned `char*` has a matching free function, and a test proves the round trip leaks nothing
- [ ] FFI-4 A C header is generated (cbindgen) and committed, and CI fails if it drifts from the source
- [ ] FFI-5 An integration test compiles and runs a real C program against the built library, covering every exported function including the error paths
- [ ] FFI-6 The FFI tests run under miri (or an equivalent) for the pointer paths that miri can check; what cannot be checked is stated
- [ ] FFI-7 The FFI surface is rebuilt on the 2.0.0 API (two-type boundary, verified ids), not on the v1.2 string-id API
- [ ] FFI-8 The FFI is covered by the same mutation, coverage and error-variant bars as the core
- [ ] FFI-9 `docs/MIGRATION_RUST.md` (or a C-specific note) documents the FFI surface, its ownership rules and its threading guarantees

## FFIJS — JS bindings for the consensus consumers (ships in `koru-lambda-consensus`; restored by owner decision 2026-09-21)

*Deferred 2026-09-30: Mirrors of a consensus layer that is itself no longer in the release.*


v1.2 shipped `WasmNetworkAgent`, `WasmValidator` and `WasmCommitmentAgent` (741-line `wasm.rs`) over string ids and JSON strings. They are restored on the 2.0.0 types, not copied forward.

- [ ] FFIJS-1 The three classes are rebuilt on the 2.0.0 surface: `Uint8Array` ids, the raw/verified split, typed errors with stable `name`/`code` (no JSON-string or hex-string parameters carried over)
- [ ] FFIJS-2 Every method has a wasm test, including every rejection path
- [ ] FFIJS-3 The generated `.d.ts` covers them and the demos typecheck against it
- [ ] FFIJS-4 They are covered by the same mutation, coverage and error-variant bars as the core
- [ ] FFIJS-5 The `pkg/` package size impact is measured and recorded (the wasm binary grows; LAND-* depends on the landing page staying honest about it)

## MIG — Downstream migration

*Deferred 2026-09-30: Downstream projects are another team's concern as of 2026-09-30. The generated `api/DELTA-from-1.2.0.txt` stays in the repo as the raw material whenever someone wants it.*


- [ ] MIG-1 Following only `docs/MIGRATION_RUST.md`, scratch copies of `koru-node`, `koru/koru-protocol`, `koru-protocol`, `hashy-protocol`, `koru-mesh` and `stele` compile and pass their tests against 2.0.0 (the consumer repos themselves are not modified)
- [ ] MIG-2 Per-consumer breakage inventory: for every downstream project pinned to a 0.x/1.x version, the APIs it uses that changed, and the migration step for each. Anything with no migration path is raised as its own item.
- [ ] MIG-3 Persisted-data note, stated honestly: the fold fix changes `batch_root`, and therefore `commitment_hash`. A v1-era batch log still passes its stored signature check, but **recomputation under 2.0.0 will not match, and the leader's signature cannot be reproduced** without the leader's key. Re-derivation is impossible for signed or externally-attested values. The note says which values are recoverable (hashes) and which are not (signatures over old hashes).
- [ ] MIG-4 A v1.x batch-log reader (in-crate or a documented external tool) so `koru-node` can still read pre-2.0 history even where it can no longer re-verify it
- [ ] MIG-5 `docs/MIGRATION_JS.md` and `docs/MIGRATION_RUST.md` agree with each other and with the shipped surface; a gate diffs both against the public API snapshot (PROC-12)
- [ ] MIG-6 Every type re-exported by a downstream project's public API is inventoried (e.g. `koru-mesh` re-exports `NetworkAgent`), so transitive semver breaks are counted
- [ ] MIG-7 `docs/MIGRATION_RUST.md` states the crate rename for every consumer-layer import, with a before/after `Cargo.toml`

