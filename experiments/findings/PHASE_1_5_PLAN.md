# Phase 1.5 Plan — Pre-Registered

**Status:** pre-registered. Written BEFORE any binary is run. Success criteria are locked.
**Branch:** `research/warroom-experiments`
**Base commit:** `3c01a70` (no `src/` changes)
**Date:** 2026-06-11

---

## Why this document exists

Phase 1 produced 11 audit/validation reports through agent reasoning over source. Every HIGH and CRITICAL severity finding is at Level 1 certainty: reasoned, not demonstrated. Phase 1.5 takes them to Level 2: demonstrated by a running probe.

This is gatekeeping for Phase 2-6. Without it, every fix item rests on agent reasoning. If a probe surprises us, the cheap response is to revise the plan now; the expensive response is to ship v2.0 and discover from a consumer that the bug we "fixed" never existed, or that we missed one.

**Pre-registration matters because surprises are the most valuable data and the most easily explained away after the fact.** Writing success criteria before seeing results makes the surprises visible.

---

## Goal

Move every HIGH/CRITICAL audit finding from "reasoned" to "demonstrated by a probe" OR "explicitly marked not-probe-able with reason." Move every CLAUDE.md validation claim from "predicted CONFIRMED" to "measured number." Update CHECKLIST 1.5–1.10 with evidence-grade annotations. Identify the smallest cohesive Phase 2 entry cohort.

---

## Success criteria (locked)

### SC1 — Reproduction status known for every HIGH+ finding

15 findings in scope:
- **Validator:** V1 (`from_root` foreign Distinction), V2 (`update_local_root` foreign), V3 (unbounded `data` DoS)
- **Network:** N1 (peer-id length DoS), N2 (empty peer-id → d0), N3 (`from_state` forged), N4 (`update_local_root` forged), N5 (`previous_root` 8-byte truncation), N6 (`leader_id` not in hash)
- **FFI:** F1 (panic across `extern "C"`), F2 (TOCTOU on `*mut KoruAgent`)
- **WASM:** W1 (`id_to_bytes` heuristic — CRITICAL), W2 (primordials as UTF-8), W4 (silent hex fallback)

Each ends Phase 1.5 in exactly one state:

- **CONFIRMED** — probe demonstrated; log file cited
- **REFUTED** — probe contradicted; finding withdrawn from fix list
- **NOT-PROBEABLE** — requires invocation path we cannot construct in this round (e.g., F1 needs an in-engine panic; F2 needs concurrent C-thread access). Stays in fix list with explicit note.

**Already CONFIRMED at baseline:** W1 (by audit prediction + baseline `cargo test --features wasm` failure of `test_wasm_engine_primordial_consistency`). W2 likewise.

### SC2 — Every CLAUDE.md theory claim has a measured number

Four claims:
1. **Coding Law:** Spearman rho(freq, delta_degree) at N=1K, 10K, 100K — three measured values
2. **Mediated self-reference:** # ID collisions out of 10K mediated-step trajectory — single integer (expected 0)
3. **Fold Law:** deg(d0) treatment/control ratio after equal-size phases — single ratio
4. **Cross-engine determinism:** # divergent snapshots out of 100 (and # divergent final fingerprints out of 3 in phase 5) — two integers (both expected 0)

No "predicted CONFIRMED" survives. Numbers replace prose.

### SC3 — Every CHECKLIST 1.5–1.10 fix item annotated

Each fix item ends Phase 1.5 with exactly one evidence annotation:

- **EVIDENCE: `<log>`** — empirically demonstrated by a specific log file
- **EVIDENCE: implicit** — architectural / cleanup choice that needs no probe (e.g., delete `ParallelBatchProcessor`)
- **EVIDENCE: source-only** — audit reasoning is the only evidence; no probe runs in this round
- **EVIDENCE: REFUTED — remove** — probe contradicted; struck from list with date

### SC4 — `src/` byte-identical to `3c01a70`

Verifiable via `git diff --stat 3c01a70 HEAD -- src/` → empty.

### SC5 — `PHASE_1_5_RESULTS.md` exists with five required sections

1. **Per-binary verdict table** (8 rows)
2. **HIGH+ finding scorecard** (15 rows: status per SC1)
3. **Theory claim scorecard** (4 rows: measured numbers per SC2)
4. **Surprise inventory** — anything observed that was not predicted in Phase 1
5. **Phase 2 entry recommendation** — the smallest cohesive fix set to land first

### SC6 — Clean reproducibility

Every measured number cites a command and log file. Anyone with the repo can re-run.

### SC7 — Honest UNEXPECTED handling

Any observation that doesn't fit the Phase 1 predictions gets its own section in `PHASE_1_5_RESULTS.md`. Not silently absorbed. Not handwaved. If `exp_validator_audit` shows zero engine leakage, that's documented as REFUTING V5 — not as "the probe was buggy."

---

## Explicitly NOT success criteria

- "All probes exit 0." (`exp20_fold_law` exits 1 if treatment/control ratio < 10; that's my opinionated threshold, not theory's. Non-zero exit is signal, not failure.)
- "All audits confirmed." (Refutation is the most valuable outcome of this phase.)
- "Numbers match predictions." (Mismatch is data, not failure.)
- "Phase 1.5 took ≤ 60 sec." (Time budget is irrelevant; rigor isn't.)

---

## Pre-registered predictions

Listed here so post-run surprises are detectable. If a measured outcome contradicts any of these, it's a surprise and gets its own section in `PHASE_1_5_RESULTS.md`.

### Audit probe predictions (CONFIRMED expected)

| Probe | Predicted finding |
|---|---|
| `audit_network_commitment_unbound` | `honest.commitment_hash == forged.commitment_hash` (leader_id flipping doesn't change hash). Both `verify_batch` return true. |
| `exp_validator_audit` A | 1 MB previous_root → rejection reason length ≥ 1 MB |
| `exp_validator_audit` B | 100 KB data → engine grows by ~100K distinctions |
| `exp_validator_audit` C | 3-tx out-of-order batch rejected; engine distinction delta > 0 |
| `exp_validator_audit` D | empty batch + empty data both accepted; degenerate tx-distinction |
| `exp_validator_audit` E | phantoms in relationships (non-zero count) |
| `exp_validator_audit` F | `from_root("deadbeef"×8)` accepted; not registered in engine |
| `exp_validator_audit` G | `set_expected_nonce(100)` then nonce-100 batch ACCEPTED against genesis root |
| `audit_network_foreign_peers` A | 1 MB peer-id constructed; engine grows by ~1M distinctions |
| `audit_network_foreign_peers` B | empty peer-id's `distinction_id() == "0"` |
| `audit_network_foreign_peers` C | `from_state(forged)` reports forged root in stats; not in engine |
| `audit_network_foreign_peers` D | `BatchProposed` distinctions IDENTICAL for two different `previous_root`s sharing 8-char prefix |
| `audit_network_foreign_peers` E | `update_local_root("ATTACKER_PICKED_ROOT")` → reported root is the forged string |
| `audit_network_concurrency` | 8 threads × 50 peers Mutex-serialized → `r == 2d - 3` holds; demonstrates required external lock |

### Validation predictions (numbers)

| Experiment | Predicted measurement |
|---|---|
| `exp18_coding_law` sanity | +1 distinction, +2 relationships, +1 degree each parent after 10K identical synths |
| `exp18_coding_law` N=1K, M=200K, α=1.0 | rho ∈ [0.97, 0.999] |
| `exp18_coding_law` N=10K, M=1M, α=1.0 | rho ∈ [0.97, 0.999] |
| `exp18_coding_law` N=100K, M=5M, α=1.0 | rho ∈ [0.97, 0.999] |
| `exp19_mediated_self_reference` T1 | 1000 iters, 0 deviations, 0 new distinctions, 0 new relationships |
| `exp19_mediated_self_reference` T2 | 10K depth, 0 collisions, all s_i unique |
| `exp19_mediated_self_reference` T3 | 10K depth, 0 collisions, all s_i unique (stronger claim) |
| `exp20_fold_law` control deg(d0) | ≤ 5 |
| `exp20_fold_law` treatment deg(d0) | ≥ 200 |
| `exp20_fold_law` ratio treatment/control | ≥ 40 |
| `exp20_fold_law` random interior at byte 0x2A step 5 | ≤ 10 |
| `exp21_cross_engine_determinism` Phase 1 (100K steps, 100 snapshots) | 0 divergences |
| `exp21_cross_engine_determinism` Phase 2 (50K noise on A) | 0 per-step ID mismatches A vs B |
| `exp21_cross_engine_determinism` Phase 3 (8-thread concurrent ×2) | distinctions equal, relationships equal, fingerprint equal |
| `exp21_cross_engine_determinism` Phase 4 (closed-form predict_id) | 0 mismatches A vs B; 0 mismatches A vs predicted |
| `exp21_cross_engine_determinism` Phase 5 (3 engines majority) | all 3 fingerprints equal |

### Caveats to predictions

- `exp20_fold_law` threshold (`ratio ≥ 40`) is opinionated. If it lands at e.g. 35, that's neither refutation nor surprise; it's "ratio is 35, prediction was ≥ 40, both confirm the qualitative claim." Document as CONFIRMED-with-numerical-correction.
- `exp_validator_audit` Section G prediction (nonce-100 ACCEPTED against genesis) depends on `previous_root` validation also being broken. If validator rejects on previous_root mismatch BEFORE checking nonce, the desync is masked. That's still a valid finding — just a different one.
- Cross-engine fingerprint matching in Phase 3 depends on snapshot quiescence. Barriers in the code enforce this; if a fingerprint mismatch appears, the first suspect is snapshot tearing (Exp 6), not axiom violation.

---

## Execution sequence

### 1.5a — Run (~60 sec)

Run binaries in this order, capturing stdout+stderr to `experiments/findings/run_log/<binary>.log` and exit code:

1. `audit_network_commitment_unbound` (~1 sec; fastest, simplest verdict)
2. `audit_network_concurrency` (~2 sec; uses Mutex, demonstrates pattern)
3. `exp_validator_audit` (~2 sec; 7 sub-findings in one binary)
4. `audit_network_foreign_peers` (~10 sec; 1MB peer-id is the slow part)
5. `exp19_mediated_self_reference` (~1 sec; quick axiom check)
6. `exp20_fold_law` (~5 sec; mechanism isolation)
7. `exp21_cross_engine_determinism` (~5 sec; 5 phases, all axiom-verifying)
8. `exp18_coding_law` (~30 sec; longest; sweep at N=100K, M=5M)

### 1.5b — Interpret

For each binary, write a "Measured Results" subsection in the corresponding audit/validation report containing:
- The command run
- The log file path
- Exit code
- Per-finding verdict (CONFIRMED / REFUTED / UNEXPECTED) keyed to SC1 or SC2
- One-sentence interpretation

### 1.5c — Consolidate

Write `experiments/findings/PHASE_1_5_RESULTS.md` with the five required sections (SC5).

### 1.5d — Propagate

Update `CHECKLIST.md`:
- Mark Section 1.11 final run-step boxes as done
- Add EVIDENCE annotations to each Section 1.5–1.10 item per SC3
- Strike REFUTED items with date
- Add any UNEXPECTED items as new fix candidates

Commit each step as a separate commit with clear lowercase prefix:

- `run: phase 1.5 audit probes + validations`
- `docs: phase 1.5 results synthesis + checklist annotations`

Push to integration branch.

---

## Trigger conditions for re-plan

If any of these happen during 1.5a/b, pause Phase 1.5 and re-plan:

1. **Any HIGH+ finding REFUTED.** Forces audit-report revision and CHECKLIST removal.
2. **More than 2 UNEXPECTED findings emerge.** Suggests audits missed a class of bugs; warrants another audit round.
3. **A binary aborts or hangs > 5 minutes.** Suggests bug in the probe itself.
4. **Memory exhaustion** during 1MB peer-id or 100K-byte data probes. Suggests we underestimated impact; document and continue.

---

## Definition of done

Phase 1.5 is done when all of SC1–SC7 are met AND:

- 8 logs exist in `experiments/findings/run_log/`
- `PHASE_1_5_RESULTS.md` exists with all 5 sections
- `CHECKLIST.md` Section 1.11 final boxes are checked; Sections 1.5–1.10 have EVIDENCE annotations
- 2 commits pushed to `research/warroom-experiments`
- `git diff --stat 3c01a70 HEAD -- src/` is empty

If we can hit Definition of Done, we have **demonstrated** rather than **reasoned** the foundation for Phase 2-6. That's the actual value.

---

## What we are NOT doing in Phase 1.5

- Running the full original 17 experiments (already in WARROOM_FINDINGS.md)
- Fixing any audit-flagged bug
- Modifying any audit report wording
- Touching `src/` in any way
- Beginning Phase 2 quick wins
- Discussing Section 5 decisions
- Coordinating with ALIS or koru-protocol consumers

Each of those is a separate phase. Don't blur boundaries.
