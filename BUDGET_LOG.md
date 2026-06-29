# Budget Amendment Log

Per `DESIGN.md` Part 10.5, every amendment to a budget gate (gates 11–15)
must be recorded here with the columns below. Theory gates (1–10) and
hygiene gates (16–34) are not amendable.

Format: `date | gate | old target | new target | absolute delta | floor delta | author | signers | justification`

---

## Row 1 — Gate 12 (8-thread synthesis throughput), 2026-06-28

| Field | Value |
|---|---|
| **Date** | 2026-06-28 |
| **Gate** | 12 — 8-thread synthesis throughput |
| **Old target** | ≥ 12M ops/sec AND ≥ 4× single-thread (4× ratio non-negotiable) |
| **New target** | M3 Pro (primary): ≥ 12M ops/sec AND ≥ 3.4× single-thread. Symmetric server (8+ uniform cores): ≥ 4× ratio watch-only, not gate. |
| **Old hard-cap floor** | ≥ 8M ops/sec AND ≥ 4× ratio |
| **New hard-cap floor** | M3 Pro: ≥ 10M ops/sec AND ≥ 3.0× ratio |
| **Absolute delta** | Target unchanged (12M). Floor raised: 8M → 10M (+25%). |
| **Floor delta** | Ratio floor lowered: 4× → 3.0× (on M3 Pro only). Symmetric-hardware 4× expectation preserved as regression watch. |
| **Author** | swyrknt (with Claude Opus 4.7 assistance) |
| **Signers** | theory-guardian (conditional GREEN, round 2 pre-merge: requires Y1 loom mutant kernel landing pre-merge — see below), qa-sentinel (GREEN, round 3 + round 2 sharpening: Y1 deferred to Step 2 IF sign-off language reflects "conditional on Y1"; CORPUS_PROVENANCE.md required pre-merge), engine-architect (Round 1 unconditional GREEN withdrawn after panel pushback; Round 2 verdict: YELLOW with merge blockers Y1, rust-craftsman item 5, Y3; Linux 4× watch deferred to Step 4) |
| **Branch** | `step/01-substrate` |
| **Round 2 panel resolution** | Step 1 merge proceeds with the 8-item fix-up commits (CHECKLIST sync, lib.rs lint floor reconcile, recorder #[must_use] + HashSet shadow, engine.rs from_hex doc, rand workspace hoist, CORPUS_PROVENANCE.md, loom kernel 3 reframe + mutant variant). Once those land, Gate 12 amendment is fully ratified by all three signers. |

### Measurement provenance

- **Mock (verification)**: `benches/upper_bound.rs` at commit `30cc77f`. Side-by-side comparison of three-map prod mirror vs single-map merged design. Result: 3-map 2.85× ratio; merged-map upper bound 3.59× on M3 Pro. Established that 4× is unreachable by any axiom-preserving layout on this hardware.
- **Production refactor**: `src/engine.rs` rewritten from three side-by-side DashMaps to one `DashMap<[u8;16], EngineNode { parents, degree }>`. Commit `fd9c2b1`. 93 tests pass (90 prior + 3 qa-sentinel round-3 falsifiers added in commit `cd6ca3f`).
- **TSan**: Full lib test suite under `RUSTFLAGS=-Zsanitizer=thread cargo +nightly test -Zbuild-std --target aarch64-apple-darwin --all-features --lib`. 90/90 tests pass (qa-sentinel's 3 new tests included), no race reports.
- **Production measurement** (`cargo bench --bench substrate`, M3 Pro, criterion median of 100 iters, 5-minute thermal idle):
  - Single-thread novel: 4.44M ops/sec [4.40, 4.50] — clears Gate 11 with ~10× headroom.
  - 8-thread novel: 15.25M ops/sec [14.83, 15.57] — clears the 12M target with +27% headroom.
  - Ratio: 3.43× [3.30, 3.54] — clears the new 3.0× floor; sits at the 3.4× target boundary (within bench CI noise).

### Justification (per Part 10.5 rule 4 — research-lead authored measurement justification for loosening)

The 4× ratio target was set assuming 8 symmetric cores. M3 Pro is
asymmetric (6P + 2E); 8 threads schedule as ~6P + 2E, where E-cores
deliver ~25% the throughput of P-cores. Linear-scaling ceiling on
this hardware is `6 + 0.25 × 2 = 6.5×`, not 8×.

A second cap: per-call SHA-256 is ~75% of single-thread work, and
SHA-256 is **theory-protected** by axiom 4 — substituting (e.g.,
BLAKE3) would break content-addressing equivalence across consumer
ecosystems. This is a load-bearing theory constraint, not a budget
choice.

The mock established the merged-map upper bound at 3.59× ratio on
M3 Pro. Production landed at 3.43× — within measurement noise of
the ceiling. The new 3.4× target reflects measured engineering
reality on the test rig; the 3.0× floor is what would constitute
design failure (any layout regression that pushes the substrate
below the merged-map ceiling).

The original 4× target is preserved as a **regression watch** on
symmetric hardware (CI Linux runners, server `c7i.2xlarge` or
equivalent). When Step 4 generates Linux measurements, the next
amendment row will either ratify the 4× watch as a formal
per-platform gate or revise it.

### Absolute target unchanged

The 12M ops/sec absolute target was the actual consumer-relevant
quantity. Measured 15.25M clears it with +27% headroom. The
hard-cap absolute floor is raised from 8M → 10M to reflect the
merged-map design's improved baseline; an implementation that ships
below 10M absolute is a regression event, not engineering noise.

### What this amendment does NOT change

- Gate 11 (single-thread, ≥ 450K) — clearance unchanged (4.44M >> 450K).
- All theory gates (1–10).
- All other budget gates (13–15).
- All hygiene gates (16–34) — see gate 27 and gate 30 updates in
  DESIGN.md for the merged-map structural changes (1 DashMap field,
  not 3), but the gates themselves were not weakened.
