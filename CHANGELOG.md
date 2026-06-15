# Changelog

All notable changes to `koru-lambda-core` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

Target release: **2.0.0** — first major version bump since the project went public.
Single-cut release. No intermediate 1.3.x. `Cargo.toml` stays at `1.2.0` throughout the
work; the final commit before the integration PR bumps to `2.0.0`.

The Unreleased section grows during the work. Categories follow Keep a Changelog
conventions: Added · Changed · Deprecated · Removed · Fixed · Security.

### Added

### Changed

- **Docs:** corrected numerical drift in `CLAUDE.md` (memory: 656→629 B/distinction;
  throughput: 500–900K → 425–540K single-thread / 1.85M @ 3.5× → 2.6M @ 4.8×;
  replay: 714K → 450K ordered / 367K shuffled; snapshot tearing: 16.5% → ≤0.1% but
  avalanche-sized; test count: 114→103; compactor described as "append-only via
  synthesize" rather than "non-destructive"; v2.0 framing reworded around memory
  density not clone elimination). Numbers now match measured baseline
  (`experiments/findings/baseline.md`).
- **`src/engine.rs::synthesize`:** swapped `format!("{:x}", Sha256::digest(...))`
  for `hex::encode(Sha256::digest(...))`. Identical output; ~15% faster synthesis
  hot path (213 ns → 113 ns per Exp 15, 2026). All 103 tests continue to pass with
  byte-identical Distinction IDs.

### Added

- `hex = "0.4"` direct dependency (used by the synthesize hot path).
- **`Distinction::to_hex() -> String`, `Distinction::from_hex(&str) -> Result<Self, ParseError>`,
  `impl Display`, `impl Debug` for `Distinction`** (CHECKLIST 2.1 / Phase 6
  sub-branch #1). New module `src/distinction_hex.rs` exposes the hex
  serialization layer. The serde adapter is usable via
  `#[serde(with = "distinction_hex")]` so JSON wire formats carry hex strings
  while bincode keeps raw bytes. Hex parsing validates length (32 chars) and
  charset; malformed input returns a typed `ParseError`.
- **`Distinction::as_bytes() -> &[u8; 16]`** — canonical accessor for the
  16-byte ID (CHECKLIST 2.1 / Phase 6 sub-branch #1).
- **`IdentityHasher`** — a `Hasher` impl that reads the first 8 bytes of each
  16-byte write call as a `u64`, XORing into state under a per-write bit
  rotation (CHECKLIST 2.1 / Phase 6 sub-branch #1). Used as
  `BuildHasherDefault<IdentityHasher>` on the engine's internal DashMaps.
  Per Exp 14 (2026): 6–13× hash speedup vs SipHash on SHA256-distributed keys.
  Per Exp 10 re-run on the foundation branch: 8-thread throughput now
  15.3 M ops/s (vs 2.6 M on baseline). Unit tests assert: (a) `[u8; 16]`
  hashes to its leading 8 bytes LE; (b) tuple hash depends on both elements
  (so `(d0, X)` edges don't bucket-collide); (c) 1 M random SHA256 prefixes
  produce 1 M distinct hashes; (d) 10 K `(d0, X)` edges land in 10 K
  distinct buckets.
- **Traversal API.** `DistinctionEngine::degree(&Distinction) -> usize`,
  `parents_of(&Distinction) -> Option<(Distinction, Distinction)>`,
  `children_of(&Distinction) -> impl Iterator<Item = Distinction>`
  (CHECKLIST Section 2.2 / Phase 6 sub-branch #5). All O(1) per call.
  Backed by three new internal DashMap indices populated in `synthesize()`
  on the novel-synthesis path using engine-first ordering per Exp 8 (no
  orphan IDs in indices pointing to unregistered distinctions). `degree()`
  is O(1) via cached `AtomicUsize`; primordials d0 and d1 are seeded at
  degree 1 to reflect the genesis relationship. `children_of` snapshots
  the children Vec under a read guard and returns an owned iterator —
  avoids self-referential guard lifetimes and writer-starvation risk.
  `parents_of` returns the canonical (min, max) pair regardless of the
  call order at synthesis time. Eliminates the need for external graph
  trackers (Exp 4: unlocks deletion of ALIS's `tracker.rs` ~600 LOC).
- **Append-only synthesis log.** Each novel `synthesize()` call pushes a
  canonical `(min, max)` parent tuple onto `DistinctionEngine`'s internal
  `SegQueue<(Distinction, Distinction)>` (CHECKLIST Section 2.3 / Phase 6
  sub-branch #4). Canonical ordering (per Decision 5.7) enables future
  Merkle-over-log and cross-peer log diffing without rewriting persisted
  logs. `DistinctionEngine::without_log()` constructor opts out for
  memory-sensitive consumers (Exp 7: 1M synths ≈ 81 MB resident log). Public
  API: `synthesis_log_snapshot() -> Vec<(Distinction, Distinction)>`
  (drain-and-refill, non-destructive), `synthesis_log_len() -> usize`.
  Replay round-trip + shuffled-replay tests assert byte-identical engine
  state reconstruction (Exp 7, Exp 12 order-independence confirmed). Log
  entries are serde-serializable via a `#[serde(with = "distinction_hex")]`
  wrapper on consumer-defined entry structs.
- `crossbeam-queue = "0.3"` direct dependency (backs the synthesis log;
  per Exp 3, `SegQueue` measured at −7% throughput at 8 threads vs −26%
  for `RwLock<Vec>`).
- **`BatchSynthesizer`** — concurrent batch-synthesis helper (CHECKLIST
  Section 1.10 / Phase 6 sub-branch #2). Replaces the v1.2.0
  `ParallelSynthesizer` with a tighter API: `synthesize_batch(Vec<(String,
  String)>) -> Vec<Option<Distinction>>` returns explicit `None` for
  missing parents instead of the v1.2.0 silent empty-string fallback that
  the Phase 1 parallel-audit flagged. `canonicalize_bytes_batch(Vec<u8>)
  -> Vec<Distinction>` returns the canonical distinctions directly
  (caller no longer needs to parse hex back). Tests added for the
  unregistered-parent / malformed-hex cases.
- **`DistinctionEngine::check_structural_invariant() -> bool`** — returns
  whether the engine satisfies `r = 2d - 3` (the binary-parentage invariant
  — every novel synthesis adds 1 node + 2 relationships) (CHECKLIST 2.4 /
  Phase 6 sub-branch #3). Intended for tests and quiescent diagnostics;
  explicitly NOT a hot-path `debug_assert!` because mid-`synthesize`
  transients (insert distinction → add rel A → add rel B) are visible to a
  concurrent reader and would fire spurious assertions under 8-thread
  concurrent synthesis. The structural correctness of the invariant under
  release-mode concurrent synthesis is verified at scale by Exp 2 (2026,
  zero deviations at 5M synths) and `tests/falsification/structural_coherence.rs`.

### Changed (breaking)

- **`BatchCommitment::compute` output changed.** Adding `leader_id` to the
  hash input (N6 fix) changes all commitment_hash values — wire-format break
  vs v1.2.0 (CHECKLIST 1.5 / Phase 6 sub-branch #6). Per Decision 5.1:
  bundled into v2.0; no users on a network today, so no separate v1.2.1
  patch needed. Cross-version peer gossip is not supported.
- **`NetworkAction::BatchProposed` action distinction id changed.** The fix
  for N5 changes how `previous_root` participates in the canonical structure;
  action distinction ids differ from v1.2.0. Same disposition as N6 above.
- **`Distinction` is now `pub struct Distinction { bytes: [u8; 16] }`** —
  16-byte truncated SHA256 (first 16 bytes, MSB), `Clone + Eq + Hash +
  Display + Debug`. The byte field is `pub(crate)`; no public constructor
  exists (CHECKLIST 2.1 / Phase 6 sub-branch #1). `Distinction::id() -> &str`
  is removed; use `as_bytes() -> &[u8; 16]` or `to_hex() -> String`.
  Primordials Δ₀, Δ₁ are `[0; 16]` and `[1, 0, …, 0]` respectively. Per Exp 13
  (2026): 0 collisions in 268 M syntheses at 16-byte truncation.
- **`DistinctionEngine`'s internal DashMaps switched to byte keys with
  `IdentityHasher`** (CHECKLIST 2.1 / Phase 6 sub-branch #1). The `Distinction`
  store is now `DashMap<[u8; 16], Distinction, IdentityBuildHasher>`; the
  relationship set is now
  `DashMap<([u8; 16], [u8; 16]), (), IdentityBuildHasher>`.
  `pub type Relationship = (String, String)` is preserved as a re-exported
  alias; `get_relationships_snapshot()` translates byte keys to hex strings
  on the way out for v1.2.0 callsite compatibility.
- **FFI: `koru_agent_state_root` returns `*mut c_char` via
  `hex::encode(distinction.as_bytes())`** (CHECKLIST 2.1 / Phase 6 sub-branch
  #1). C signature is unchanged; the internal path is now explicitly
  bytes → hex → CString. The hex strings are 32 characters (lowercase),
  reflecting the truncated 16-byte SHA256 (vs 64 characters under v1.2.0's
  full-hex Distinction IDs). FFI consumers parsing the returned string
  must update their expected length.

### Changed

- **`StructuralCompactor::calculate_sis` rewritten** to use `engine.degree`
  directly (CHECKLIST Section 1.9 #4 / Phase 6 sub-branch #5). Eliminates
  the full state-snapshot clone; each degree lookup is O(1) via the engine's
  internal AtomicUsize cache.
- **`tests/falsification/robustness.rs::test_falsify_random_degree_distribution`
  rewritten** to use `engine.degree` instead of rebuilding a petgraph
  snapshot in the inner loop. Wall time drops from ~107 s (Phase 6 #1
  baseline regression flag) to ~0.02 s — a >5000× speedup. The test logic
  (preferential attachment with degree-weighted parent selection,
  > 0.45 concentration assertion) is preserved.

### Changed (breaking)

- **`DistinctionEngine::get_state_snapshot` renamed to
  `get_state_snapshot_unsynchronized`** (Decision 5.8). The method's existing
  tearing behavior under concurrent writes is unchanged; the new name forces
  every caller to acknowledge at the call site that the two-half snapshot is
  not atomic. Docstring expanded to document the tearing semantics (~0.1%
  avalanche-sized tear rate per Exp 6) and when the snapshot is safe vs unsafe.
  External callers must rename: this is a v2.0 breaking change.

### Fixed

- **`ByteMapping::map_byte_to_distinction` no longer leaves phantom parents in
  the calling engine** (CHECKLIST 1.1 #1; Exp 5, qa-sentinel). Previously, the
  function returned IDs from a static cache built against a throwaway engine,
  so the 8-step byte fold chain existed in relationships but not in the calling
  engine's `all_distinctions`. The fix now folds each byte through the actual
  calling engine, registering the full chain. Phase 1.5 validator probe
  Section E confirms the phantom count goes from 253 (before) to 0 (after) on
  the 256-byte exercise. `synthesize` idempotency ensures subsequent calls for
  the same byte are fast (DashMap hits).

  **Behavioral change:** byte-heavy workloads now grow the engine by the full
  byte chain (up to 8 distinctions per novel byte, ~510 distinct total across
  all 256 bytes due to prefix sharing). This is the correct theory; the prior
  behavior undercounted distinctions and violated `r = 2d - 3` semantically.
  v2.0's `Distinction([u8; 16])` migration recovers the lost throughput
  (4–26× speedups across measured axes per Exp 14, 15).

### Deprecated

### Removed

- **`ParallelBatchProcessor` (~250 LOC removed)** — misnamed Sequential-body
  wrapper identified by the Phase 1 parallel-audit as a duplicate of
  `ConsensusValidator` with an unused `worker_count` field and a
  single-variant `ProcessingStrategy::Sequential` enum (CHECKLIST Section
  1.10 / Phase 6 sub-branch #2). `ParallelAction`, `ProcessingStrategy`,
  the internal `num_cpus` shim, the `LocalCausalAgent` impl, and the 5
  associated tests also removed. Callers should use `ConsensusValidator`
  directly — the duplicated `validate_batch` semantics live there.
- **`ParallelSynthesizer`** — renamed to `BatchSynthesizer` (see Added).
  The old name carried "parallel" in user-facing API surface where
  parallelism is an implementation choice; the renaming also drops the
  silent empty-string fallback for missing parents in favour of
  `Vec<Option<Distinction>>`.

- **`Distinction::new(String)` removed** (CHECKLIST 2.1 / Phase 6 sub-branch #1).
  No public constructor exists. The Distinction byte field is `pub(crate)`.
  External code obtains a Distinction only through `engine.synthesize`,
  `engine.d0()`/`d1()`, `engine.get_distinction_by_id(hex)`, or
  `Distinction::from_hex(hex)`. Foreign-ID poisoning (Exp 9 / V1 / N3 / N4) is
  closed structurally at compile time — no defensive runtime checks required.
- **`Distinction::id() -> &str` removed** (CHECKLIST 2.1 / Phase 6 sub-branch #1).
  Replaced by `Distinction::as_bytes() -> &[u8; 16]` for raw bytes and
  `Distinction::to_hex() -> String` for display. Routine `.id() -> &str` callers
  in subsystems / tests / examples / benches were rewritten to one of these.

### Fixed

### Security

- **N6 / leader-id-forgery (CVE-class, never deployed).** `BatchCommitment::compute`
  now hashes `leader_id.as_bytes()` into the commitment hash (CHECKLIST 1.5 /
  Phase 6 sub-branch #6). Phase 1.5 probe `audit_network_commitment_unbound`
  demonstrated `compute(batch, 7, 3, "alice")` and `compute(batch, 7, 3, "EVE")`
  produced byte-identical `commitment_hash` in v1.2.0; post-fix log at
  `experiments/findings/run_log/audit_network_commitment_unbound_post_n5_n6_v5_fix.log`
  confirms `hashes equal? false`.
- **N5 / 8-byte previous_root truncation (CVE-class, never deployed).**
  `NetworkAction::BatchProposed::to_canonical_structure` no longer applies
  `.take(8)` to `previous_root.as_bytes()`. Now parses `previous_root` via
  `Distinction::from_hex` and synthesizes the actual parent root; malformed
  inputs (wrong length, non-hex) fall through a deterministic sentinel that
  cannot collide with any well-formed root. Phase 1.5 probe
  `audit_network_foreign_peers` Section D updated to use VALID 32-char hex
  inputs (the v1.2.0 demo strings were only 24 chars and now fall through
  the sentinel); the new well-formed-hex variant demonstrates that two roots
  sharing an 8-char hex prefix now produce distinct action distinctions.
  Post-fix log at
  `experiments/findings/run_log/audit_network_foreign_peers_post_n5_n6_v5_fix.log`.
- **V5 / atomic-failure engine leakage.** `ConsensusValidator::validate_batch`
  now pre-validates the entire batch (nonce sequence + previous_root) BEFORE
  any `engine.synthesize` call. Rejected batches leave engine state unchanged.
  Phase 1.5 probe `exp_validator_audit` Section C demonstrated 4 distinctions
  leaking into the engine on a 3-tx out-of-order rejection in v1.2.0; post-fix
  log at `experiments/findings/run_log/exp_validator_audit_post_n5_n6_v5_fix.log`
  confirms `engine distinction delta = 0`.
- **F7 / verify_batch leader-id binding** transitively closed by the N6 fix.
  `BatchCommitment::verify_batch` re-derives the commitment hash over
  `self.leader_id`, so a tampered `leader_id` field on a deserialized
  commitment fails verification. The lightweight `BatchCommitment::verify`
  (used by FFI `koru_agent_check_commitment` light-node ping check) is
  unchanged — it cannot hash-verify without the batch data and is documented
  accordingly. Test: `verify_batch_rejects_tampered_leader_id`.

- **Foreign-ID poisoning closed structurally** (CHECKLIST 1.1 #2 / 1.5 V1 / 1.5 N3
  / 1.5 N4; Phase 6 sub-branch #1). Removing `Distinction::new(String)` and
  making the byte field `pub(crate)` closes the attack class demonstrated in
  Exp 9 (canonical-looking forgery, empty-string IDs, 1 MB DoS-sized IDs,
  colon-separator collisions, cross-engine ghosts). Any external code that
  constructed Distinction values out-of-tree must obtain them through the
  engine or via `Distinction::from_hex` (which validates length + charset).

---

## 1.2.0 — prior

See `git log` prior to `c2d331b` (Update README) for the 1.2.0 release history.
The 1.2.0 line contains the original engine + subsystems shipped on crates.io.

## Process notes

This CHANGELOG is the source of truth for what changes between releases.
Every commit to `research/warroom-experiments` that affects shipped behavior
should append a line under the appropriate `Unreleased` category. The final
v2.0 commit renames `## Unreleased` to `## 2.0.0 — YYYY-MM-DD`.

For demonstrated security vulnerabilities (N5, N6, V5 — see `SECURITY.md` once
published), the `Security` section also references the consensus-correctness
patches landing in v2.0.
