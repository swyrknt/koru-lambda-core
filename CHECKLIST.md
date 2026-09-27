# koru-lambda-core 2.0.0 — Release Checklist

This is the single source of truth for what 2.0.0 still needs. 2.0.0 ships only when every box is ticked.

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
  - evidence: `.github/workflows/ci.yml` @ 699a872 — 13 jobs; all run locally green via `scripts/check.sh`
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

## PKG — Workspace layout (DEC-9: three crates)

- [x] PKG-1 The workspace publishes four crates from this repo, in one release, at one version: `koru-lambda-core` (engine, projection, replay, recorder, lift, agent contract, distinction_hex), `koru-lambda-consensus` (validator, commitment, network, classifier, batch), `koru-lambda-wasm` (JS/TS bindings for both), `koru-lambda-ffi` (C ABI). Everything depends on `core`; `core` depends on none of them. The wasm bindings were extracted from `core` because `wasm-pack` requires `cdylib`, which would otherwise force a dynamic library on every pure-Rust consumer — the same reasoning that separated the C ABI, and what `ARCHITECTURE.md` already calls the bindings layer.
  - evidence: `Cargo.toml` workspace members + `crates/` @ 57d892f (ffi crate lands with FFI-1..9)
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

## ENG — Engine and trust boundary

- [ ] ENG-1 `synthesize` checks both parents are registered before any state change; an unregistered parent panics with zero state change (documented under `# Panics`), tested in debug and release by comparing full state snapshots
- [ ] ENG-2 `synthesize` throughput: `criterion` `synthesize/hot`, median of 3 runs, ≤ 3% slower than baseline commit `e88e824`, on the pinned CI runner; a regression fails the job
- [ ] ENG-3 `check_structural_invariant` verifies, in one pass:
  - binary parentage
  - every parent registered
  - every id equals the hash of its parents (full content-address recomputation)
  - **per-node** degree recomputed from parentage (not only the sum, which survives a mutant that bumps one parent twice)
  - no registered node has two equal parents (Axiom 3 residue)

  Each violation has its own error variant and a negative test. The docs state whether the genesis edge is materialized or whether Law 6's `+1` is definitional.
- [ ] ENG-4 Only the engine can create a `Distinction`. There is no `Pod`/`Zeroable`, `from_hex`, `FromStr` or `Deserialize` on `Distinction`; external bytes enter only through `RawDistinctionId` → `verify`. Compile-fail tests cover each removed path, and the positive `assert_impl_all!(Distinction: Pod, Zeroable)` at `engine.rs:1161` is inverted to `assert_not_impl_any!`.
- [ ] ENG-5 A `RawDistinctionId` **not registered in the receiving engine** is rejected, and an unregistered `Distinction` reaching `synthesize` panics before any state change. A distinction with equal bytes that *is* registered in the receiving engine is accepted (Law 8). Both directions tested.
- [ ] ENG-6 `IdentityHasher` / `IdentityBuildHasher` are crate-private
- [ ] ENG-7 `relationship_count` is documented as a definitional count; the measured check lives in ENG-3
- [ ] ENG-8 Comments describing foreign-byte behavior match the code in both build profiles

## ENC — Canonical encoding

- [ ] ENC-1 One anchored, order-preserving sequence fold, with a written spec: the anchor is a **parameter**, not a constant; the spec states and proves accumulator/element lineage disjointness **including nested folds** (a fold's result used as another fold's element, which is what the subsystems do); injectivity holds modulo the 128-bit bound. Tests:
  - all 256 byte tips are distinct
  - property test: distinct sequences give distinct results
  - a directly-constructed degenerate case where an accumulator equals the next element tip (a random proptest reaches this with probability 2⁻¹²⁸, so it must be constructed)
  - prefix, permutation and repeat regression vectors
  - golden vectors
- [ ] ENC-2 Byte folding uses a **per-engine memoized** tip table (registration cached per engine, never identity cached globally — a shared table would hand ENG-1 unregistered parents and panic)
- [ ] ENC-3 `LocalCausalAgent` / `synthesize_causal_action` take `&DistinctionEngine`; roots accepted from outside are engine-verified; the trait is documented as an anchored-fold contract, not a perspective abstraction
- [ ] ENC-4 The anchored fold is **public and re-exported at the crate root**, and `Canonicalizable` ships impls for `u64`, `&[u8]`, `Vec<u8>` and `&str`. Today the only stdlib impl is `u8`, so a consumer with domain data has nothing to call.
- [ ] ENC-5 The two private folds are deleted and call the public one (`validator.rs:97-109`, `commitment.rs:232-238`); a CI gate fails on any remaining hand-rolled `synthesize(acc, …)` left-fold in tracked code
- [ ] ENC-6 `ByteMapping`'s cost (17 syntheses per byte) is measured and either reduced or documented, with the rustdoc number matching the bench

## AGT — The agent contract (write-side discipline)

- [ ] AGT-1 The monotonic-forward rule is enforced, not conventional. `update_local_root`'s docs currently say "Convention (not trait-enforced)": the advance path verifies that the new root's parents include the previous root (one `parents_of` lookup) and rejects a root that does not descend from it. Tests cover both directions.
- [ ] AGT-2 The agent contract advances through the canonical lift only: a multi-element advance uses the anchored fold (ENC-1) seeded at the agent's root, never a fold seeded from its first element. A CI gate rejects the latter shape.
- [ ] AGT-3 The trait and `synthesize_causal_action` ship in `koru-lambda-core` alongside the lift they anchor; their consensus implementations ship in `koru-lambda-consensus`
- [ ] AGT-4 The docs state why the agent is the recommended write path for perspective-carrying consumers **and** why it is not the only one: replay reconstructs from raw parent pairs, cross-runtime convergence compares histories rather than perspectives, and the theory admits multi-root and non-root consumers. A reader should not have to guess whether raw synthesis is a mistake.

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

## SUB — Consensus reference consumers (ships in `koru-lambda-consensus`)

- [ ] SUB-1 Transaction and commitment folds use ENC-1. Tests:
  - regression vectors: nonce 1 vs 256, 2 vs 512; `[0x00]` vs `[0x00,0x00]`
  - pinned roots
  - property test: distinct inputs give distinct roots
- [ ] SUB-2 Wire types deserialize to a **raw form** holding `RawDistinctionId` (via `raw_serde_adapter`); the verified form is produced by an explicit `verify(&engine)` step. Both types are public and named in `docs/MIGRATION_RUST.md`. This resolves the contradiction between "ids must be verified" and "a node booting from disk has no engine yet".
- [ ] SUB-3 Every `Deserialize` in the crate is bounded (`TransactionBatch` cardinality cap on the raw form); each bound is tested with oversized input
- [ ] SUB-4 `verify_batch` does not mutate the caller's engine (tested: engine state byte-identical before and after)
- [ ] SUB-5 `ConsensusValidator`'s agent path enforces nonce validation (no route advances the root without it)
- [ ] SUB-6 `TransactionAction` fields are private, with getters; `CommitmentAgent::with_capacity(NonZeroUsize)`; wire sizes are `u64`; `Debug` on all public types
- [ ] SUB-7 The `lru` advisories are resolved (fixed version or an audited bounded map, reason stated)
- [ ] SUB-8 `NetworkAgent` + `PeerIdentity` ported to 2.0.0, covering the v1.2 surface used downstream (`new`, `from_state`, `join_peer`, `propose_commitment`, `check_commitment`, `finalize_batch`, `advance_epoch`, `get_current_leader`, `current_epoch`, `validator_count`, `validator_set`, `consensus_state_root`, `get_stats`, nonce restore). Required properties:
  - typed peer-id construction with bounds
  - joint (id, distinction) dedupe
  - bounded pending commitments, cleared on epoch advance
  - atomic restore
  - verified wire ids
- [ ] SUB-9 Compaction resolved honestly. The v1.2 `StructuralCompactor` never removed anything from the engine — it classified nodes by degree threshold and tracked its own archived set. So it is a read over a degree signal, which is what a projection is. Either port it as an explicitly consumer-side classifier, or express it as a projection boundary/signal; the "compactor" name goes if it evicts nothing. Required: classification is a pure function of engine state (shuffled replay yields identical stats).
- [ ] SUB-10 `BatchSynthesizer` ported; the result representation (`Option` vs sentinel) is justified; concurrency is tested. *(No downstream consumer today; owner decided 2026-09-21 to ship it anyway — it meets the same bar as everything else.)*
- [ ] SUB-11 Every subsystem meets the same mutation, coverage and error-variant bar as the core
- [x] SUB-12 Consensus tests build distinctions through the engine, not through an internal constructor. Found when the crate split made `Distinction::from_bytes_unchecked` unreachable from outside the substrate — three tests had been fabricating identifiers from raw bytes.
  - evidence: `crates/koru-lambda-consensus/src/{validator,commitment}.rs` @ 57d892f — no `from_bytes_unchecked` remains; the crate compiles only against the public API

## JS — wasm / TypeScript surface

- [ ] JS-1 Counts and degrees are `number` everywhere (not `bigint`); the 2^53 bound is documented; `docs/MIGRATION_JS.md` is corrected in the same change (it currently documents `bigint` as the 2.0 shape)
- [ ] JS-2 Every thrown error has a stable, documented `name` / `code`; tests assert them
- [ ] JS-3 A hex helper for 32-byte ids (projection ids)
- [ ] JS-4 `snapshotParentage`, `WasmEngine.replay`, `checkStructuralInvariant` and the sequence fold are exposed and tested
- [ ] JS-5 Every rejection path is tested from JS (invalid direction, each `restoreProjection*` failure, verify, hex parse)
- [ ] JS-6 Projection wrappers don't clone their full contents on every getter call
- [ ] JS-7 The generated `.d.ts` is reviewed; the demos typecheck against it under `strict`
- [ ] JS-8 The published wasm target is a deliberate, documented decision and is verified against a real consumer. v1.2 shipped `--target web` (default `init` export); 2.0.0 currently ships `--target bundler` (no such export), which silently breaks the only npm consumer (`canon-cpg/cli/engine.js` calls `await initWasm({module_or_path})`). Whatever is chosen — single target or multiple entry points — a test consumes the packed tarball the way that consumer does. See DEC-7.
- [x] JS-10 The bindings handle `#[non_exhaustive]` substrate enums across the crate boundary: an unknown `SynthesisOutcome` variant surfaces to the caller instead of failing to compile against a future substrate release.
  - evidence: `crates/koru-lambda-wasm/src/lib.rs` @ 57d892f — wildcard arm returns a JS error
- [ ] JS-9 The lift layer is reachable from JS: the fold, `Canonicalizable`-style byte/string lifting, and an agent advance are exposed and tested. Today a JS consumer cannot get domain data into the engine at all.

## FFI — C ABI (ships in `koru-lambda-ffi`; restored by owner decision 2026-09-21)

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

v1.2 shipped `WasmNetworkAgent`, `WasmValidator` and `WasmCommitmentAgent` (741-line `wasm.rs`) over string ids and JSON strings. They are restored on the 2.0.0 types, not copied forward.

- [ ] FFIJS-1 The three classes are rebuilt on the 2.0.0 surface: `Uint8Array` ids, the raw/verified split, typed errors with stable `name`/`code` (no JSON-string or hex-string parameters carried over)
- [ ] FFIJS-2 Every method has a wasm test, including every rejection path
- [ ] FFIJS-3 The generated `.d.ts` covers them and the demos typecheck against it
- [ ] FFIJS-4 They are covered by the same mutation, coverage and error-variant bars as the core
- [ ] FFIJS-5 The `pkg/` package size impact is measured and recorded (the wasm binary grows; LAND-* depends on the landing page staying honest about it)

## TST — Test suite

- [x] TST-1 One golden-vector file, used by both native and wasm tests (child ids; projection ids and canonical bytes for every signal × direction × boundary; subsystem roots)
  - evidence: `src/vectors.rs` @ 57d892f — native `tests/cond_d_prime.rs` and wasm `crates/koru-lambda-wasm/tests/cond_d_prime_wasm.rs` both assert against it
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
- [ ] THY-2 `docs/CLAIMS.md`: every claim in THEORY, DESIGN, ARCHITECTURE, README and rustdoc maps to a definition, proof sketch, named test or named probe
- [ ] THY-3 Law 10 (mediated self-reference) has a runnable probe with a recorded result. The v1-era probe source exists at commit `2b1c323` (`experiments/runner/src/exp19_mediated_self_reference.rs`) and is restored and ported, not rewritten from scratch. THEORY's citation resolves to it.
- [ ] THY-4 Law 11 text matches the shipped fold (syntheses per byte, depth). Sequenced **after** ENC-1/ENC-2, since the fold shape changes the hub gates.
- [ ] THY-5 Law 8 says "execution interleavings of the same operations"; Law 7's projection-stability claim is scoped to where it holds
- [ ] THY-6 THEORY states that Laws 5–12 continue the axiom numbering
- [ ] THY-7 No shipped capability is still marked as future work
- [ ] THY-8 Coding Law evidence paths are correct
- [ ] THY-9 Subsystem theory review has named tests as its evidence: subsystem-derived roots are order-independent, and no read path mutates engine state (before/after snapshot equality)
- [ ] THY-10 Every empirical claim has a runnable probe that names its **falsifying observation**, runs in CI, and records commit and seed
- [ ] THY-11 **Sequence Law.** THEORY states order encoding as a derived law: in a commutative, irreflexive, content-addressed substrate, sequence injectivity requires an anchor disjoint from element lineage, and nesting depth carries length. ENC-1 is its implementation, not a private utility.
- [ ] THY-12 Each of Axioms 1–4 has a named test that fails if only that axiom breaks, listed in `docs/CLAIMS.md`
- [ ] THY-13 THEORY and SPEC state which laws are conditional on id injectivity, and that an in-engine collision is undetectable (it presents as saturation, and content-address recomputation still passes)
- [ ] THY-14 Law 10's precondition is stated (the observation must lie outside the chain, or `synth(synth(x,obs),x)` collapses to a fixed point when `obs == x`), or Law 10 is labelled a conjecture
- [ ] THY-15 **Degree conservation.** Σ degree = 4N−6, so mean degree → 4 at every scale under every workload; hub degree is paid for by leaves, and the Coding Law is a variance claim under a fixed mean. Definitional, with one test.
- [ ] THY-16 **Lift Law.** THEORY names the lift — the map from non-distinction values into the graph — as the substrate's third forced object and its only free parameter, with two sub-parameters: the seed-anchor (which buys injectivity) and the alphabet (which shapes topology). `Canonicalizable` is its trait, the anchored fold its instance. Status: definitional.
- [ ] THY-17 THEORY's two statements about the LCA are reconciled: it is called "a pattern, not an axiom" at one point and "a substrate-level construct" at another. The resolution states which is binding and why, since it decides where the trait ships.
- [ ] THY-18 Law 11 is re-derived after ENC-1/ENC-2 land: under a fixed Σdeg, the Fold Law names where degree concentrates, and PRB-2 tests whether that is the alphabet rather than primordiality. Independent-law status is withdrawn if it is a corollary.

## PRB — Probes

- [ ] PRB-1 Mediated self-reference: 100K depth, with constant-observation, cycling-observation and observation-drawn-from-chain variants. Falsified by any repeat or fixed point before the birthday bound.
- [ ] PRB-2 Anchor-hub: the same corpus folded under k different seeds **and** alphabets, varied independently, measuring whether top-degree rank tracks the alphabet. If it does, Law 11 demotes to a corollary of THY-16 and THEORY says so.
- [ ] PRB-3 Projection idempotence for an arbitrary signal (see PRJ-8), shipped as the consumer self-test
- [ ] PRB-4 THEORY's own falsifier, run and recorded: two engines fed the same parent pairs in different orders, queried at quiescence with the same `{root, direction, boundary, signal}`, produce byte-identical `canonical_bytes`. THEORY names this as the experiment that would refute its central claim, and nothing in the crate runs it.

## DOC — Documentation

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
- [ ] DOC-11 `docs/development/*` is accurate and current (restored 2026-09-21: nothing is dropped; HYG-4 decides whether it ships, this item keeps it correct either way)
- [ ] DOC-12 `docs/SPEC.md` states the engine model explicitly: an engine is a witness of what it has synthesized, not a namespace; primordials are the same constants everywhere; identity is engine-independent by construction; ids carry no origin and are not scoped to an application; an unregistered distinction reaching `synthesize` is a caller error caught before any state change
- [ ] DOC-13 `docs/SPEC.md` records the outcome and reasoning of every closed `DEC-*` item, so a reader finds the limits stated rather than discovering them

- [ ] DOC-14 `docs/MIGRATION_JS.md` stops describing the subsystem classes as retired, since FFIJS-* restores them, and documents their new shape
- [ ] DOC-15 README and rustdoc carry a "lifting your data" section: a domain value folded to a root, then used to advance an agent — the path a stranger needs and cannot currently find

## LAND — The published artifact

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
- [ ] HYG-6 `cargo publish --dry-run` output is byte-compared against a committed manifest; `Cargo.lock` is committed; a clean checkout reproduces the same tarball

## MIG — Downstream migration

- [ ] MIG-1 Following only `docs/MIGRATION_RUST.md`, scratch copies of `koru-node`, `koru/koru-protocol`, `koru-protocol`, `hashy-protocol`, `koru-mesh` and `stele` compile and pass their tests against 2.0.0 (the consumer repos themselves are not modified)
- [ ] MIG-2 Per-consumer breakage inventory: for every downstream project pinned to a 0.x/1.x version, the APIs it uses that changed, and the migration step for each. Anything with no migration path is raised as its own item.
- [ ] MIG-3 Persisted-data note, stated honestly: the fold fix changes `batch_root`, and therefore `commitment_hash`. A v1-era batch log still passes its stored signature check, but **recomputation under 2.0.0 will not match, and the leader's signature cannot be reproduced** without the leader's key. Re-derivation is impossible for signed or externally-attested values. The note says which values are recoverable (hashes) and which are not (signatures over old hashes).
- [ ] MIG-4 A v1.x batch-log reader (in-crate or a documented external tool) so `koru-node` can still read pre-2.0 history even where it can no longer re-verify it
- [ ] MIG-5 `docs/MIGRATION_JS.md` and `docs/MIGRATION_RUST.md` agree with each other and with the shipped surface; a gate diffs both against the public API snapshot (PROC-12)
- [ ] MIG-6 Every type re-exported by a downstream project's public API is inventoried (e.g. `koru-mesh` re-exports `NetworkAgent`), so transitive semver breaks are counted
- [ ] MIG-7 `docs/MIGRATION_RUST.md` states the crate rename for every consumer-layer import, with a before/after `Cargo.toml`

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
- [ ] DEC-7 **Published wasm target.** v1.2 shipped `--target web`; 2.0.0 ships `--target bundler`, which breaks the only npm consumer. Decide the target strategy (single target, or multiple entry points) — see JS-8.
- [ ] DEC-8 **`BatchSynthesizer` (SUB-10). Decided (owner, 2026-09-21): port it to the full standard.** No consumer today; shipped anyway. Ticks when the reasoning is recorded per DOC-13.
- [ ] DEC-9 **Crate boundary. Decided (owner, 2026-09-21): four crates**, one workspace, one release, same bar — `koru-lambda-core` (engine, projection, replay, lift), `koru-lambda-consensus` (validator, commitment, network, classifier, batch), `koru-lambda-wasm` (JS/TS bindings; extracted 2026-09-22 because `wasm-pack` requires `cdylib`), `koru-lambda-ffi` (C ABI). Nothing is dropped. Implemented by PKG-1..7. Viability was measured first: the classifier and batch paths both compile against the public API from an external crate, with no internal access. Ticks when the reasoning is recorded per DOC-13.
  - requires: ANA-4 (records the measured basis: 7→5 direct deps, `lru` and mandatory `serde` leave the substrate, ~37 KB of wasm no longer paid by browser users who don't use consensus, and a consumer-layer bug stops forcing a substrate major)
- [ ] DEC-10 **Sealing.** Measured 2026-09-21 from an external crate against the public API only: a sibling **can** define its own `Signal`, materialize it, get canonical bytes and a projection id, achieve cross-engine byte equality, and `restore_projection` its own type — no escape hatch needed. Sealing blocks exactly three things, permanently at 2.0.0:
  1. implementing `CoreSignal` (substrate-internal dispatch)
  2. implementing `ReadOnlyEngine` — so no alternative engine ever: no mock, no disk-backed, no remote
  3. adding `Boundary` / `Direction` variants — so no threshold or other custom traversal bound

  **Decided (owner, 2026-09-21): unseal `ReadOnlyEngine`; `CoreSignal` stays sealed; `Boundary`/`Direction` stay closed.** Implemented by SEAL-1..3. Ticks when the reasoning is recorded per DOC-13.

## ANA — Analyses backing the decisions

- [ ] ANA-1 Domain separation: what collides today across applications, what an anchor-based separation gives, what a hash-level domain tag would cost (identity change, vector churn, loss of cross-application convergence, naming layer), and which downstream projects are affected
- [ ] ANA-2 Collision analysis: the birthday bound at 128 bits, accidental-collision probability at realistic and extreme counts, the adversarial case, what an attacker gains, and the consequence of ids being unversioned and unmigratable
- [ ] ANA-3 Growth analysis: measured bytes per distinction and per relationship at 1K/1M scale, peak allocation for a maximum-size hostile batch, whether a peer can drive an engine past available memory, and the recommended consumer pattern
- [ ] ANA-4 Crate-boundary analysis: what each crate's name promises and what a stranger searching for it finds; the semver blast radius of a consumer-layer bug under each layout; the dependency surface (`lru`, `serde`, `unsafe`) each layout forces on a substrate user; feature-matrix and wasm size effects; publish mechanics; and the cost of the migration the split itself creates

## REV — Independent review (after everything above)

- [ ] REV-1 Code review from reviewers who see only the code: no must-fix findings
- [ ] REV-2 Test-quality review: no must-fix findings; mutation report clean
- [ ] REV-3 Theory alignment: a reviewer given only `THEORY.md` and the code produces a written finding list with zero must-fix items
- [ ] REV-4 Drift sweep: no stale names, paths or claims in tracked files
- [ ] REV-5 Every finding raised by REV-1..4, the mutation run, the probes and the downstream check has become an item here and that item is ticked. No open finding remains at release.

## DEMO — Demonstrations (Rust and TypeScript, identical output, run in CI)

- [ ] DEMO-1 Predict-and-confirm: child ids reproduced with a plain SHA-256 call (the hook — checkable by hand in seconds)
- [ ] DEMO-2 Trust boundary: verify rejection, projection restore round-trip, tamper rejection
- [ ] DEMO-3 Order independence: shuffled, repeated and argument-swapped histories give identical bytes
- [ ] DEMO-4 Cross-runtime convergence (**flagship**): Rust writes the history; a separate Node process replays it shuffled and reproduces the same projection id; tampering fails
- [ ] DEMO-5 An independent re-implementation written from `docs/SPEC.md` alone (a standalone script, not this crate) reproduces every golden vector: child ids, fold roots, projection ids. This is the only item that can falsify "the spec is sufficient"; DEMO-4 compiles the same source twice and cannot.
- [ ] DEMO-6 Lifting domain data: a domain struct folded to a root and used to advance an agent, in Rust and TypeScript, with identical output. Every demo today synthesizes primordials; none shows a reader how their own data gets in.

## REL — Release (each step needs explicit owner approval)

- [ ] REL-1 `pkg/` rebuilt with `--features wasm`; version and exports verified
- [ ] REL-2 `cargo publish`
- [ ] REL-3 `npm login` (owner) and `npm publish`
- [ ] REL-4 Annotated tag `v2.0.0` created and pushed
- [ ] REL-5 Yank criteria agreed in writing before publish: what would justify yanking 2.0.0 and who decides
