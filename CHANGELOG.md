# Changelog

All notable changes to `koru-lambda-core` are documented here.

The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## 2.0.0 — 2026-06-16

Single bundled major bump from `1.2.0` (Decision 5.1). No intermediate 1.3.x.

This release closes the entire Phase 6 audit + implementation effort: 11
sub-branches across `research/warroom-experiments`, 161 release tests (up from
103 on the v1.2.0 baseline), clippy clean bare and `--features wasm`. See
`SECURITY.md` for the security-relevant subset and the pinning guidance.

### Added

- **`Distinction::to_hex() -> String`, `Distinction::from_hex(&str) -> Result<Self, ParseError>`,
  `impl Display`, `impl Debug` for `Distinction`** (CHECKLIST 2.1 / sub-branch
  #1). New module `src/distinction_hex.rs` exposes the hex serialization layer.
  The serde adapter is usable via `#[serde(with = "distinction_hex")]` so JSON
  wire formats carry hex strings while bincode keeps raw bytes. Hex parsing
  validates length (32 chars) and charset; malformed input returns a typed
  `ParseError`.
- **`Distinction::as_bytes() -> &[u8; 16]`** — canonical accessor for the 16-byte
  ID (CHECKLIST 2.1 / sub-branch #1).
- **`IdentityHasher`** — a `Hasher` impl that reads the first 8 bytes of each
  16-byte write as a `u64`, XORing into state under a per-write 17-bit rotation
  (CHECKLIST 2.1 / sub-branch #1). Used as `BuildHasherDefault<IdentityHasher>`
  on the engine's internal DashMaps. Per Exp 14: 6–13× hash speedup vs SipHash
  on SHA256-distributed keys. Per Exp 10 re-run on the foundation branch:
  8-thread throughput now 15.3 M ops/s (vs 2.6 M on baseline). Unit tests
  assert: `[u8; 16]` hashes to its leading 8 bytes LE; tuple hashes depend on
  both elements (so `(d0, X)` edges don't bucket-collide); 1 M random SHA256
  prefixes produce 1 M distinct hashes; 10 K `(d0, X)` edges land in 10 K
  distinct buckets.
- **Traversal API.** `DistinctionEngine::degree(&Distinction) -> usize`,
  `parents_of(&Distinction) -> Option<(Distinction, Distinction)>`,
  `children_of(&Distinction) -> impl Iterator<Item = Distinction>`
  (CHECKLIST Section 2.2 / sub-branch #5). All O(1) per call. Backed by three
  new internal DashMap indices populated in `synthesize()` on the
  novel-synthesis path using engine-first ordering per Exp 8 (no orphan IDs in
  indices pointing to unregistered distinctions). `degree()` is O(1) via cached
  `AtomicUsize`; primordials d0 and d1 are seeded at degree 1 to reflect the
  genesis relationship. `children_of` snapshots the children `Vec` under a read
  guard and returns an owned iterator — avoids self-referential guard lifetimes
  and writer-starvation risk. `parents_of` returns the canonical `(min, max)`
  pair regardless of the call order at synthesis time. Eliminates the need for
  external graph trackers (Exp 4: unlocks deletion of ALIS's `tracker.rs` ~600
  LOC by the ALIS team).
- **Append-only synthesis log.** Each novel `synthesize()` call pushes a
  canonical `(min, max)` parent tuple onto `DistinctionEngine`'s internal
  `SegQueue<(Distinction, Distinction)>` (CHECKLIST Section 2.3 / sub-branch
  #4). Canonical ordering (Decision 5.7) enables future Merkle-over-log and
  cross-peer log diffing without rewriting persisted logs.
  `DistinctionEngine::without_log()` constructor opts out for memory-sensitive
  consumers (Exp 7: 1 M synths ≈ 81 MB resident log). Public API:
  `synthesis_log_snapshot() -> Vec<(Distinction, Distinction)>`
  (drain-and-refill, non-destructive), `synthesis_log_len() -> usize`. Replay
  round-trip + shuffled-replay tests assert byte-identical engine-state
  reconstruction (Exp 7, Exp 12 order-independence confirmed). Log entries are
  serde-serializable via `#[serde(with = "distinction_hex")]` on
  consumer-defined entry structs.
- **`BatchSynthesizer`** — concurrent batch-synthesis helper (CHECKLIST 1.10 /
  sub-branch #2). Replaces v1.2.0's `ParallelSynthesizer` with a tighter API:
  `synthesize_batch(Vec<(String, String)>) -> Vec<Option<Distinction>>` returns
  explicit `None` for missing parents instead of v1.2.0's silent empty-string
  fallback. `canonicalize_bytes_batch(Vec<u8>) -> Vec<Distinction>` returns the
  canonical distinctions directly (caller no longer needs to parse hex back).
  Tests added for the unregistered-parent / malformed-hex cases.
- **`DistinctionEngine::check_structural_invariant() -> bool`** (CHECKLIST 2.4 /
  sub-branch #3). Returns whether the engine satisfies `r = 2d - 3` (the
  binary-parentage invariant — every novel synthesis adds 1 node + 2
  relationships). Intended for tests and quiescent diagnostics; explicitly NOT
  a hot-path `debug_assert!` because mid-`synthesize` transients (insert
  distinction → add rel A → add rel B) are visible to a concurrent reader and
  would fire spurious assertions under 8-thread concurrent synthesis. The
  release-mode correctness of the invariant under concurrent synthesis is
  verified at scale by Exp 2 (5 M synths, zero deviations) and
  `tests/falsification/structural_coherence.rs`.
- **`ConsensusValidator::restore_state(engine, root_id, expected_nonce) ->
  Result<Self, String>`** (CHECKLIST 1.6 V6 / sub-branch #7). Atomic
  replacement for the partial-update pair `from_root` + `set_expected_nonce`.
  Refuses fabricated roots not registered in the supplied engine.
- **`NetworkAgent::restore_consensus_validator_state(engine, root_id, nonce)
  -> Result<(), String>`** (CHECKLIST 1.6 V6 / sub-branch #7). Network-agent
  wrapper that constructs a fresh validator via `ConsensusValidator::restore_state`
  and replaces the agent's internal validator atomically.
- **FFI `koru_agent_restore_state(agent, engine, root_hex, nonce) -> i32`**
  (CHECKLIST 1.6 V6 / sub-branch #7). C ABI replacement for the removed
  `koru_agent_restore_nonce`. Takes the 32-char lowercase hex root plus the
  matching nonce; rejects malformed roots with `KORU_ERROR_INVALID_DATA`.
- **WASM `idToHex(arr: Uint8Array) -> string`,
  `idFromHex(s: string) -> Uint8Array`** (CHECKLIST 1.8 / sub-branch #10).
  Freestanding `#[wasm_bindgen]` JS helpers for converting between bytes and
  32-char lowercase hex at human-facing boundaries (logs, URL params, JSON
  debug). The substrate itself never sees hex.
- **`StructuralCompactor::set_thresholds(hot, warm)`,
  `StructuralCompactor::thresholds() -> (usize, usize)`** (CHECKLIST 1.9 /
  sub-branch #9). Symmetric, validated runtime configuration of the thermal
  thresholds. Replaces `set_hot_threshold(N)` which silently inferred
  `warm = hot/2`.
- **`SECURITY.md`** (Decision 5.9). Supported version table, Tier-0
  vulnerability descriptions (N5 / N6 / V5), Tier-1 hardening summary, and
  reporting contact.
- **Dependencies.** `hex = "0.4"` (synthesize hot path);
  `crossbeam-queue = "0.3"` (synthesis log; per Exp 3, `SegQueue` measured at
  −7 % throughput at 8 threads vs −26 % for `RwLock<Vec>`);
  `console_error_panic_hook = "0.1"` (optional, included in the `wasm` feature
  per Decision 5.6); `wasm-bindgen-test = "0.3"` (dev-dependency, drives
  `wasm-pack test --node --features wasm`).

### Changed

- **`src/engine.rs::synthesize`:** swapped `format!("{:x}", Sha256::digest(...))`
  for `hex::encode(Sha256::digest(...))`. Identical output; ~15 % faster
  synthesis hot path (213 ns → 113 ns per Exp 15). All tests continue to pass
  with byte-identical Distinction IDs.
- **`StructuralCompactor::calculate_sis` rewritten** to use `engine.degree`
  directly (CHECKLIST 1.9 #4 / sub-branch #5). Eliminates the full
  state-snapshot clone; each degree lookup is O(1) via the engine's internal
  AtomicUsize cache.
- **`tests/falsification/robustness.rs::test_falsify_random_degree_distribution`
  rewritten** to use `engine.degree` instead of rebuilding a petgraph snapshot
  in the inner loop. Wall time drops from ~107 s (sub-branch #1 regression
  flag) to ~0.02 s — a >5000× speedup. Test logic (preferential attachment
  with degree-weighted parent selection, > 0.45 concentration assertion) is
  preserved.
- **Compactor — `compaction_count` semantics + no self-archive** (CHECKLIST 1.9
  / sub-branch #9). `compact()` is now a pure dry-run that computes
  classification + archived set without advancing `compaction_count`;
  `synthesize_action` is the call that writes the compaction-event distinction
  into the engine and advances the counter. `synthesize_action` no longer
  re-runs `calculate_sis` + `classify_thermal_states` after writing the new
  event — that path classified its own freshly-synthesized event (deg = 2) as
  COLD and added it to `archived_set`. The archived set now reflects what
  `compact()` was told to archive, not what `synthesize_action` accidentally
  swept up after.
- **Docs:** corrected numerical drift in `CLAUDE.md` (memory: 656 → 629
  B/distinction at the v1.2.0 baseline; throughput: 500–900K → 425–540K
  single-thread / 1.85M @ 3.5× → 2.6M @ 4.8× on the v1.2.0 baseline; replay:
  714K → 450K ordered / 367K shuffled; snapshot tearing: 16.5% → ≤0.1% but
  avalanche-sized; test count: 114 → 103 → 161 across the project's evolution;
  compactor described as "append-only via synthesize" rather than
  "non-destructive"; v2.0 framing reworded around memory density not clone
  elimination). README and CLAUDE.md fully rewritten for v2.0 in sub-branch
  #11.

### Changed (breaking)

- **`Distinction` is now `pub struct Distinction { bytes: [u8; 16] }`** —
  16-byte truncated SHA-256 (first 16 bytes, MSB),
  `Clone + Eq + Hash + Display + Debug` (CHECKLIST 2.1 / sub-branch #1). The
  byte field is `pub(crate)`; no public constructor exists. `Distinction::id()
  -> &str` is removed; use `as_bytes() -> &[u8; 16]` or `to_hex() -> String`.
  Primordials Δ₀, Δ₁ are `[0; 16]` and `[1, 0, …, 0]` respectively. Per Exp 13:
  0 collisions in 268 M syntheses at 16-byte truncation.
- **`DistinctionEngine`'s internal DashMaps switched to byte keys with
  `IdentityHasher`** (CHECKLIST 2.1 / sub-branch #1). The `Distinction` store is
  now `DashMap<[u8; 16], Distinction, IdentityBuildHasher>`; the relationship
  set is `DashMap<([u8; 16], [u8; 16]), (), IdentityBuildHasher>`.
  `pub type Relationship = (String, String)` is preserved as a re-exported
  alias; `get_relationships_snapshot()` translates byte keys to hex strings on
  the way out for v1.2.0 callsite compatibility.
- **`DistinctionEngine::get_state_snapshot` renamed to
  `get_state_snapshot_unsynchronized`** (Decision 5.8). Tearing behaviour under
  concurrent writes is unchanged; the new name forces every caller to
  acknowledge at the call site that the two-half snapshot is not atomic.
  Docstring expanded to document the tearing semantics (~0.1 % avalanche-sized
  tear rate per Exp 6) and when the snapshot is safe vs unsafe.
- **`BatchCommitment::compute` output changed** (CHECKLIST 1.5 N6 / sub-branch
  #6). `leader_id` is now hashed into the commitment hash, so all
  `commitment_hash` values differ from v1.2.0 — wire-format break vs v1.2.0.
  Cross-version peer gossip is not supported.
- **`NetworkAction::BatchProposed` action distinction id changed** (CHECKLIST
  1.5 N5 / sub-branch #6). The N5 fix changes how `previous_root` participates
  in the canonical structure; action distinction ids differ from v1.2.0.
- **`PeerIdentity::new` now returns `Result<Self, String>`** (CHECKLIST 1.6 N1 /
  N2; sub-branch #7). Empty ids and ids longer than `MAX_PEER_ID_LEN` (64
  bytes) are rejected before any synth runs into the engine. The WASM binding
  (`joinPeer` / `joinPeers`) now returns a JS rejection; the FFI binding
  (`koru_agent_join_peer`) returns `KORU_ERROR_INVALID_DATA`.
- **`ConsensusValidator::from_root` + `set_expected_nonce` removed** (CHECKLIST
  1.6 V6 / sub-branch #7). Replaced by atomic `restore_state(engine, root_id,
  expected_nonce) -> Result<Self, String>`.
- **`NetworkAgent::restore_consensus_validator_nonce` removed** (CHECKLIST 1.6
  V6 / sub-branch #7). Replaced by `restore_consensus_validator_state(engine,
  root_id, nonce) -> Result<(), String>`.
- **FFI: `koru_agent_restore_nonce(agent, nonce)` removed** (CHECKLIST 1.6 V6 /
  sub-branch #7). Replaced by `koru_agent_restore_state(agent, engine,
  root_hex, nonce) -> i32`.
- **`koru_agent_state_root` C ABI hex width changed** (CHECKLIST 2.1 /
  sub-branch #1). Returns `*mut c_char` via `hex::encode(distinction.as_bytes())`
  — 32 characters lowercase (was 64 under v1.2.0's full-hex Distinction IDs).
  FFI consumers parsing the returned string must update their expected length.
- **FFI opaque types are now `#[repr(C)] struct` placeholders, not `c_void`**
  (CHECKLIST 1.7 F4 / sub-branch #8). C headers declare `struct KoruEngine`,
  `struct KoruAgent`, `struct KoruValidator` — three distinct types. C consumers
  must update header includes; pointer-type mismatches that v1.2.0 silently
  allowed now fail at C compile time.
- **`koru_agent_check_commitment` signature extended** (CHECKLIST 1.7 F7 /
  sub-branch #8). Added two trailing arguments:
  `leader_id: *const c_char, batch_size: u64`. Empty `leader_id` returns
  `KORU_ERROR_INVALID_DATA`. The v1.2.0 "Frankenstein commitment" path with
  empty `leader_id` and `batch_size = 0` is no longer reachable from the FFI.
- **`WasmEngine::synthesize` signature changed** (CHECKLIST 1.8 W9 / sub-branch
  #10). Now `synthesize(id_a: &[u8], id_b: &[u8]) -> Result<Vec<u8>, JsValue>`.
  v1.2.0 took `(id_a: &str, id_b: &str)` (hex). New signature is bytes-canonical
  and symmetric with the output.
- **`WasmNetworkAgent::checkCommitment` signature changed** (CHECKLIST 1.8 W10
  / sub-branch #10). Two trailing arguments added: `leader_id: &str,
  batch_size: u64`. Mirrors the FFI F7 fix.
- **WASM `d0Id()` / `d1Id()` return raw 16-byte `Uint8Array`** (CHECKLIST 1.8
  / sub-branch #10). v1.2.0 emitted UTF-8 hex `"0"` / `"1"` byte strings via
  the `id_to_bytes` heuristic; primordials now ride the same byte path as
  every other distinction.
- **`StructuralCompactor::new` signature changed** (CHECKLIST 1.9 / sub-branch
  #9). Now `new(engine, hot_threshold, warm_threshold)`. The v1.2.0 default
  `(hot, warm) = (3, 1)` was unprincipled; consumers must now pick thresholds
  matching the degree distribution of their engine. `set_hot_threshold(N)`
  removed; replaced by `set_thresholds(hot, warm)` which validates
  `warm <= hot`. `StructuralCompactor::from_root` removed (unused).
- **`CompactionAction::archived_ids` field removed** (CHECKLIST 1.9 / sub-branch
  #9). The field was carried as data but never participated in
  `to_canonical_structure`, so callers relied on reading it back from the
  action rather than computing it from the canonical record. Archived IDs are
  obtained from the compactor itself.

### Removed

- **`Distinction::new(String)`** (CHECKLIST 2.1 / sub-branch #1). No public
  constructor exists. The `Distinction` byte field is `pub(crate)`. External
  code obtains a `Distinction` only through `engine.synthesize`,
  `engine.d0()` / `d1()`, `engine.get_distinction_by_id(hex)`, or
  `Distinction::from_hex(hex)`. Foreign-ID poisoning (Exp 9 / V1 / N3 / N4) is
  closed structurally at compile time — no defensive runtime checks required.
- **`Distinction::id() -> &str`** (CHECKLIST 2.1 / sub-branch #1). Replaced by
  `as_bytes() -> &[u8; 16]` for raw bytes and `to_hex() -> String` for display.
- **`ParallelBatchProcessor` (~250 LOC)** (CHECKLIST 1.10 / sub-branch #2).
  Misnamed Sequential-body wrapper identified by the Phase 1 parallel-audit as
  a duplicate of `ConsensusValidator` with an unused `worker_count` field and a
  single-variant `ProcessingStrategy::Sequential` enum. `ParallelAction`,
  `ProcessingStrategy`, the internal `num_cpus` shim, the `LocalCausalAgent`
  impl, and the 5 associated tests also removed. Callers should use
  `ConsensusValidator` directly.
- **`ParallelSynthesizer`** — renamed to `BatchSynthesizer` (see Added).
  The old name carried "parallel" in user-facing API surface where parallelism
  is an implementation choice; the renaming also drops the silent
  empty-string fallback for missing parents in favour of
  `Vec<Option<Distinction>>`.
- **`ConsensusValidator::from_root`, `set_expected_nonce`** (CHECKLIST 1.6 V6
  / sub-branch #7). Composed independently; replaced by atomic `restore_state`.
- **`NetworkAgent::restore_consensus_validator_nonce`** (CHECKLIST 1.6 V6 /
  sub-branch #7). Replaced by `restore_consensus_validator_state`.
- **FFI `koru_agent_restore_nonce`** (CHECKLIST 1.6 V6 / sub-branch #7).
  Replaced by `koru_agent_restore_state`.
- **`StructuralCompactor::from_root`, `set_hot_threshold`** (CHECKLIST 1.9 /
  sub-branch #9). `from_root` was unused; `set_hot_threshold` silently inferred
  `warm = hot/2`. Use `new(engine, hot, warm)` + `set_thresholds(hot, warm)`.
- **`CompactionAction::archived_ids`** (CHECKLIST 1.9 / sub-branch #9). See
  Changed (breaking).
- **WASM `id_to_bytes` heuristic + internal `hex` helper module** (CHECKLIST 1.8
  / sub-branch #10). The UTF-8 fallback that silently routed primordial IDs
  through a different code path is gone. Every ID-returning WASM method now
  calls `d.as_bytes().to_vec()` directly.

### Fixed

- **`ByteMapping::map_byte_to_distinction` no longer leaves phantom parents in
  the calling engine** (CHECKLIST 1.1 #1; Exp 5, qa-sentinel). v1.2.0 returned
  IDs from a static cache built against a throwaway engine, so the 8-step byte
  fold chain existed in relationships but not in the calling engine's
  `all_distinctions`. The fix now folds each byte through the actual calling
  engine, registering the full chain. Phase 1.5 validator probe Section E
  confirms the phantom count goes from 253 (before) to 0 (after) on the
  256-byte exercise. `synthesize` idempotency ensures subsequent calls for the
  same byte are fast (DashMap hits).

  **Behavioural change:** byte-heavy workloads now grow the engine by the full
  byte chain (up to 8 distinctions per novel byte, ~510 distinct total across
  all 256 bytes due to prefix sharing). This is the correct theory; the prior
  behavior undercounted distinctions and violated `r = 2d − 3` semantically.
  v2.0's `Distinction([u8; 16])` migration recovers the lost throughput (4–26×
  speedups across measured axes per Exp 14, 15).

### Security

The Tier-0 entries below were never deployed to a network (the project has no
production peers as of v2.0). They would have been exploitable in v1.2.x had it
shipped on a network. See `SECURITY.md` for the full posture, supported version
table, and reporting process.

- **N6 / leader-id-forgery (Tier 0, never deployed).** `BatchCommitment::compute`
  now hashes `leader_id.as_bytes()` into the commitment hash (CHECKLIST 1.5 /
  sub-branch #6). Phase 1.5 probe `audit_network_commitment_unbound`
  demonstrated `compute(batch, 7, 3, "alice")` and
  `compute(batch, 7, 3, "EVE")` produced byte-identical `commitment_hash` in
  v1.2.0; post-fix log
  `experiments/findings/run_log/audit_network_commitment_unbound_post_n5_n6_v5_fix.log`
  confirms `hashes equal? false`.
- **N5 / 8-byte previous_root truncation (Tier 0, never deployed).**
  `NetworkAction::BatchProposed::to_canonical_structure` no longer applies
  `.take(8)` to `previous_root.as_bytes()`. Now parses `previous_root` via
  `Distinction::from_hex` and synthesizes the actual parent root; malformed
  inputs (wrong length, non-hex) fall through a deterministic sentinel that
  cannot collide with any well-formed root. Phase 1.5 probe
  `audit_network_foreign_peers` Section D updated to use valid 32-char hex
  inputs; the new well-formed-hex variant demonstrates that two roots sharing
  an 8-char hex prefix now produce distinct action distinctions. Post-fix log
  `…_post_n5_n6_v5_fix.log`.
- **V5 / atomic-failure engine leakage (Tier 0).** `ConsensusValidator::validate_batch`
  now pre-validates the entire batch (previous_root + non-empty + nonce
  sequence + V3 data length) **before** any `engine.synthesize` call. Rejected
  batches leave engine state unchanged. Phase 1.5 probe `exp_validator_audit`
  Section C demonstrated 4 distinctions leaking into the engine on a 3-tx
  out-of-order rejection in v1.2.0; post-fix log confirms `engine distinction
  delta = 0`.
- **F7 / verify_batch leader-id binding** transitively closed by the N6 fix.
  `BatchCommitment::verify_batch` re-derives the commitment hash over
  `self.leader_id`, so a tampered `leader_id` field on a deserialized
  commitment fails verification. The lightweight `BatchCommitment::verify`
  (used by FFI `koru_agent_check_commitment` light-node ping check) remains
  metadata-only by design; both FFI and WASM `checkCommitment` now require the
  caller to supply the real `leader_id` + `batch_size` (closes the v1.2.0
  "Frankenstein commitment" path). Test:
  `verify_batch_rejects_tampered_leader_id`.
- **Foreign-ID poisoning closed structurally** (CHECKLIST 1.1 #2 / 1.5 V1 / 1.5
  N3 / 1.5 N4; sub-branch #1). Removing `Distinction::new(String)` and making
  the byte field `pub(crate)` closes the attack class demonstrated in Exp 9
  (canonical-looking forgery, empty-string IDs, 1 MB DoS-sized IDs,
  colon-separator collisions, cross-engine ghosts). External code obtains
  `Distinction` only through the engine or `Distinction::from_hex` (which
  validates length + charset).
- **Consensus hardening — Tier 1 DoS amplifiers closed at construction time**
  (CHECKLIST 1.6 / sub-branch #7).
  - N1: `PeerIdentity::new` rejects ids longer than `MAX_PEER_ID_LEN` (64
    bytes). The foreign-peers probe confirms a 1 MB peer-id workload returns
    in 6 µs with zero engine growth (was 1.85 s + 1 M leaked distinctions).
  - N2: `PeerIdentity::new` rejects empty ids (was collapsing to Δ₀ —
    primordial impersonation).
  - V3: `TransactionAction.data` greater than `MAX_TX_DATA_BYTES` (4 KiB) is
    rejected at pre-validation; tx-data DoS goes from ~149 ms + 100 K
    distinctions per oversized tx to immediate reject + zero engine growth.
  - V4: `validate_batch` rejection reasons clip oversized `previous_root` to a
    64-char prefix (was a full echo of the untrusted input — 1 : 1
    amplification).
  - V6: see Changed (breaking) — `restore_state` replaces the partial-update
    setters and refuses fabricated roots not registered in the engine.
  - V8: empty-data tx collapse documented as by-design on `TransactionAction`
    (content-addressing produces equal outputs for equal inputs).
  - N7: `NetworkAgent::join_peer` deduplicates on the joint `(id, distinction)`
    key — the same basis the deterministic leader hash uses.
  - N11: `NetworkAgent::pending_commitments` is now a bounded `LruCache<[u8;
    32], BatchCommitment>` (capacity `MAX_PENDING_COMMITMENTS = 256`);
    `advance_epoch` clears the entire set because commitments bind `epoch` in
    their hash and are unfinalizable across the boundary.
- **FFI hardening — UB and TOCTOU closures at the C ABI boundary** (CHECKLIST
  1.7 / sub-branch #8).
  - F1 / F3: `[profile.release] panic = "abort"`. Panics in Rust code
    reachable from `extern "C"` terminate deterministically rather than
    unwind across the foreign-function boundary (UB). Test + bench profiles
    continue to unwind (Cargo ignores the `panic` setting for those).
  - F2 / F8: `NetworkAgent` and `ConsensusValidator` handles are now
    `Box<Mutex<...>>` inside the FFI boundary; concurrent C-thread calls
    targeting the same handle serialize through the internal mutex (closes
    the TOCTOU / `&mut` aliasing class). New test:
    `test_ffi_agent_concurrent_calls_serialize` (8 threads × 500 joins
    through one handle).
  - F4: opaque types are distinct `#[repr(C)] struct { _private: [u8; 0] }` —
    C compile-time type confusion blocked. (See Changed (breaking).)
  - F5: cbindgen `include` allowlist removed; `target/koru.h` now declares
    all 22 entry points + clean type names.
  - F6: every engine borrow uses
    `ManuallyDrop::new(Arc::from_raw(...))` — the v1.2.0
    `Arc::from_raw + Arc::into_raw` re-leak dance is gone.
  - F7: see above; FFI half landed in this sub-branch.
  - F9: `koru_agent_propose_commitment` and `koru_agent_finalize_batch`
    reject `batch_len > isize::MAX as usize` before any
    `slice::from_raw_parts`.
- **WASM hardening (W1/W2/W4/W5/W9/W10/W13)** (CHECKLIST 1.8 / sub-branch #10).
  The WASM FFI is bytes-canonical end to end. Primordials Δ₀, Δ₁ ride the
  same byte path as synthesized IDs (no UTF-8 special-case).
  `console_error_panic_hook` is wired up unconditionally under the `wasm`
  feature via `#[wasm_bindgen(start)] fn _wasm_start()` so Rust panics surface
  as readable stack traces in the JS console (Decision 5.6). All 18 unit
  tests inside `src/wasm.rs` are now `#[wasm_bindgen_test]` and run under
  `wasm-pack test --node --features wasm`; `tests/falsification/wasm_consistency.rs`
  is `#![cfg(feature = "wasm")]`-gated and on the same harness. The v1.2.0
  host-`#[test]` layout that produced "function not implemented on non-wasm32
  targets" aborts on any call returning a `JsValue` is gone.

---

## 1.2.0 — prior

See `git log` prior to `c2d331b` (Update README) for the 1.2.0 release history.
The 1.2.0 line contains the original engine + subsystems shipped on crates.io.

## Process notes

This CHANGELOG is the source of truth for what changes between releases.
Every commit to `research/warroom-experiments` that affects shipped behavior
appended a line under the appropriate `Unreleased` category during Phase 6;
sub-branch #11 + Phase 7 consolidated the resulting `Unreleased` section into
a single ordered release note before renaming it to `## 2.0.0 — 2026-06-16`.
