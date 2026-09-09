# Benchmarks

`[SHIPPED]` — capacity and throughput measurements for the current substrate.

**Anti-scope charter.** This document is a *narrow* reference for capacity
arithmetic and throughput numbers, and nothing else. It is not a general
performance-tuning guide, not a design-rationale doc, and not a place for
forward-looking projections. It carries two sections — *Capacity* and
*Throughput* — and every quantitative claim inside those sections cites an
in-crate `benches/*.rs`, `tests/*.rs`, or `examples/*.rs` file at line-level.
Numbers without an in-crate anchor get removed, not extrapolated.

Hard rules:

- **≤ 200 LOC.** If a section grows past its share, split it into a probe
  file under `tests/` or `benches/` — do not fatten this doc.
- **Every number cites an in-crate file:line.** External anchors
  (warroom logs, CHECKLIST notes, experiment IDs) are forbidden. If the
  number cannot be re-derived from a shipped file, it does not belong here.
- **No forward-looking projections.** *"Coming soon"*, *"planned for X"*, or
  *"expected to reach Y"* language is disallowed. Version binding and
  release targets live in `DESIGN.md`; theory-forced consequences live in
  `THEORY.md`.
- **No design rationale.** *Why* the number matters belongs in `DESIGN.md`
  or `THEORY.md`. This doc records *what* was measured, and *where*.
- **Fixture provenance stays canonical in `tests/coding_law.rs`.** The
  exp18 corpus SHA-256 digest constants live at `tests/coding_law.rs:30-31`
  and are the sole source of truth for corpus integrity; this document may
  cite them but never restates them.

---

## Capacity

The substrate's structural laws hold at every scale probed. Two capacity
claims — per-distinction memory and the `r = 2d − 3` invariant at scale —
are re-derivable from shipped code.

**Per-distinction memory footprint.** Measured at 1 M distinctions via the
dhat probe at `examples/dhat_1m.rs:37-86`. The probe allocates a 1 M-chain
under `dhat::Alloc` and prints `max_blocks_size / 1_000_000`. Current
observation on `step/02-subsystems`: ~137.4 B/distinction (release
allocator, aarch64-apple-darwin). Reference targets are pinned in
`DESIGN.md` as gates 13; the observation number lives here.

**Memory ceiling arithmetic (conservative).** With a per-distinction
footprint measured at ~137.4 B (`examples/dhat_1m.rs:37-86`) and ~12 GB of
usable RAM on a 16 GB laptop after OS overhead, the arithmetic ceiling is
12 GB / 137.4 B ≈ 87 M distinctions. Using the more conservative
`DESIGN.md` gate-13 target of ≤ 180 B/distinction, the ceiling drops to
12 GB / 180 B ≈ 70 M. A round number on the conservative side of the
arithmetic is **~80 M**. The math is division; no empirical validation at
80 M is required for the memory claim.

**`r = 2d − 3` at 5 M scale.** The structural-law invariant is verified at
5 M synthesis extensions in `tests/scale.rs:20-52` (the
`r_equals_2d_minus_3_at_5m_synths` test). The test also verifies the
explicit arithmetic `d = N + 2, r = 2 N + 1` and catches per-Nth-synth
edge accounting off-by-ones that only surface deep in a chain.

**Small-scale invariant coverage.** Companion coverage at unit-test scale
runs on every `cargo test` via `law6_r_equals_2d_minus_3_at_small_scale`
in `src/engine.rs:1113`.

---

## Throughput

Two throughput gates are measured under `benches/`. The Coding Law ρ gate
is not a throughput claim; it is a correctness-under-load claim measured
by a test.

**Single-thread `synthesize` throughput.** Measured under Criterion in
`benches/substrate.rs:28-80` (`bench_synth_novel_single_thread`,
`chain_extension` group). Reference target: ≥ 450 K ops/sec (floor
300 K), documented at `benches/substrate.rs:6-7`. Historical steady-state
observation at 1 M–5 M scale: ~500 K ops/sec on M-series silicon in
release mode with thermal-idle discipline.

**Multi-thread `synthesize` throughput.** Measured under Criterion in
`benches/substrate.rs:82-138`. Reference target: ≥ 12 M ops/sec on
8 threads AND ≥ 4× single-thread ratio, documented at
`benches/substrate.rs:8-9`. Historical steady-state observation at
1 M–5 M scale: ~12 M+ ops/sec on 8 threads.

**Saturation fast-path and query throughput.** Secondary measurements for
the API surface — `synthesize` saturation fast-path, `parents_of`, and
`degree` — are captured in `benches/substrate.rs:141-220`.

**Coding Law ρ ≥ 0.985 (correctness-under-load).** The Spearman rank
correlation between engine-observed degree and Zipf-drawn frequency on
the pinned exp18 workload is asserted at ≥ 0.985 in
`tests/coding_law.rs:82-125` (`exp18_rho_clears_gate`). The workload
corpus is pinned by SHA-256 digest constants at `tests/coding_law.rs:30-31`
so a `rand` version drift falsifies the pre-flight gate
(`exp18_corpus_integrity`, `tests/coding_law.rs:48-70`) BEFORE ρ is
reported.

**Ceiling probe at 50 M+ scale.** Not shipped. Whether the single- and
multi-thread numbers hold past L3 cache (~8–32 MB), DashMap shard
collision rate under sustained load, allocator paging near memory
ceiling, and IdentityHasher bucket distribution at large N are all
untested at 50 M+. A ceiling probe belongs on the Step 4 backlog; until
it lands, throughput at ceiling is an open question and no number in
this document extrapolates past the range each cited file exercises.
