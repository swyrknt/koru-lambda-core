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
