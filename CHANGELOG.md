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

### Fixed

### Security

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
