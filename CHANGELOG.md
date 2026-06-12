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
