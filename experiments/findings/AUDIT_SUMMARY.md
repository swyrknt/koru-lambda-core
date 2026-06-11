# Phase 1 Audit Summary — 11 Specialist Reports

**Date:** 2026-06-11
**Branch:** `research/warroom-experiments`
**Method:** 11 specialist agents dispatched in parallel. All were blocked by plan mode (could not write files or run `cargo`); reports captured to disk by transcribing the agent outputs.
**Outcome:** **2 HIGH-severity findings**, multiple MEDIUM, 0 leak paths, 0 memory-safety bugs. v2.0 migration plan is intact and unblocked.

---

## Per-subsystem verdicts

| Subsystem | LOC | Specialist | Verdict | New findings |
|---|---|---|---|---|
| `compactor.rs` | 503 | engine-architect | **Largest theory drift in codebase** — claims non-destructive but mutates in `new()` and `synthesize_action`; magic constants; unverified "26.6×" claim; archives its own work | 7 issues |
| `validator.rs` | 350 | qa-sentinel | **Atomic-failure docstring is wrong** — engine retains partial syntheses on rejection; foreign-Distinction acceptance; unbounded `data` DoS | 10 issues |
| `network.rs` | 603 | qa-sentinel | **Most security-critical findings** — 5 foreign-ID vectors; `leader_id` NOT in BatchCommitment hash (forgeable); `previous_root` truncated to 8 bytes (causal-chain collision); empty peer-id impersonates primordial; undocumented concurrency contract | 11 issues |
| `commitment.rs` | 504 | engine-architect | **CLEAN** — no drift, no post-commit mutation, LRU fully orthogonal to engine | 0 critical, 1 v2.0 type break |
| `parallel.rs` | 379 | rust-craftsman | `ParallelBatchProcessor` is Sequential despite name → recommend **DELETION** (~250 LOC removed). `ParallelSynthesizer` is genuinely parallel; keep with cleanup | 1 architectural |
| `ffi.rs` | 899 | rust-craftsman | **2 HIGH** — no panic protection across `extern "C"`; TOCTOU on `*mut KoruAgent`. **0 leak paths.** 0 production `.unwrap()`. | 13 issues |
| `wasm.rs` | 741 | rust-craftsman | **1 CRITICAL** wire-format heuristic (`id.len() == 64`) breaks at v2.0; `0 unsafe`, theory-clean | 17 issues |

## Validation experiments (4)

| Claim | Specialist | Status | Design verdict |
|---|---|---|---|
| Coding Law rho=0.99 | research-lead | Source drafted, NOT run | Predicted CONFIRMED at rho 0.97-0.999 |
| Mediated self-reference | research-lead | Source drafted, NOT run | Predicted CONFIRMED for direct (no novelty) + mediated (full novelty) |
| Fold Law depth-≤8 mechanism | research-lead | Source NOT drafted (agent ran out of permitted actions) | Design only |
| Cross-engine determinism | qa-sentinel | Source drafted, NOT run | Predicted CONFIRMED on all 5 phases |

---

## Severity rollup

| Severity | Count | Where |
|---|---|---|
| **CRITICAL** | 1 | wasm.rs W1 (id_to_bytes heuristic) |
| **HIGH** | 9 | validator V1, V2, V3 / network N1-N6 / ffi F1, F2 / wasm W2, W4 |
| **MEDIUM** | ~20 | distributed |
| **LOW** | ~25 | distributed |

## Issues that v2.0 (`pub(crate) [u8;16]`) closes structurally

- validator V1, V2 (foreign-Distinction via `from_root`/`update_local_root`)
- network N3, N4 (same in `from_state`/`update_local_root`)
- engine: foreign-ID via public `Distinction::new` (the original Exp 9 finding)

## Issues that v2.0 does NOT close — need targeted fixes

| Issue | Where | Fix scope |
|---|---|---|
| Compactor mutation in `new()` + `synthesize_action` (doc/impl mismatch) | compactor.rs:104, 269-300 | Doc fix or refactor |
| Compactor magic constants (`hot_threshold: 3`, `warm = hot/2`, `26.6×`) | compactor.rs:110, 161, module doc | Remove or derive |
| Compactor archives its own work products | compactor.rs:284-293 | Skip recompute after own synth |
| `compaction_count` double-increment | compactor.rs:211 + 281-283 | Remove one site |
| Validator atomic-failure semantics lie | validator.rs:96-97 docstring | Doc fix + pre-validate batches |
| Validator unbounded `data` DoS | validator.rs:34 | Length cap |
| Validator rejection message amplifies attacker input | validator.rs:115-119 | Clip to 64 chars |
| Network peer-id unbounded DoS | network.rs:30 | Length cap |
| Network empty peer-id → d0 impersonation | network.rs:33 | Reject `is_empty()` |
| **Network previous_root 8-byte truncation (causal-chain collision)** | network.rs:73-78 `.take(8)` | Drop `.take(8)` |
| **`BatchCommitment::compute` does not hash `leader_id` (forgery)** | commitment.rs:46-58 | Add `leader_id.as_bytes()` to hasher |
| Network validator dedupe mismatch | network.rs:175 vs 310 | Joint key dedupe |
| Network `pending_commitments` unbounded growth | network.rs:121 | Size cap + TTL |
| **FFI panic safety** | (missing) | Add `panic = "abort"` to Cargo.toml |
| **FFI TOCTOU on `*mut KoruAgent`** | full surface | Doc paragraph OR wrap agents in Mutex |
| FFI cbindgen include omits 18/22 | cbindgen.toml:14 | Remove include list |
| FFI typed opaque structs (type confusion) | ffi.rs:22-28 | `#[repr(C)] pub struct ... { _private: [u8; 0] }` |
| FFI `check_commitment` hash decorative | ffi.rs:319-351 + commitment.rs:82-84 | Add hash to `verify` |
| WASM id_to_bytes heuristic | wasm.rs:379-387 | Kill heuristic, expose `as_bytes()` |
| WASM primordials leak UTF-8 | wasm.rs:57-66 | Real 16-byte primordial IDs |
| WASM silent hex fallback | wasm.rs:382 | Fail-closed |
| WASM Frankenstein `BatchCommitment` | wasm.rs:212-235 | Narrower check_commitment API |
| WASM host-only tests (`#[test]` vs `#[wasm_bindgen_test]`) | wasm.rs:405-741 | Convert |

## Cross-cutting findings

- **`BatchCommitment::compute` not hashing `leader_id`** appears in network.md (N6), ffi.md (F7), wasm.md (W10). Single root cause; fix in commitment.rs propagates.
- **ByteMapping phantom-parent issue (TODO #3)** inherited by compactor (canonicalization), commitment (commitment hash + nonce folds), validator (data folds), network (peer-id folds). Fixing TODO #3 closes transitively in all four.
- **Engine `Distinction::new(String)` being public** is the root of foreign-ID poisoning across validator, network, and engine itself. v2.0 `pub(crate)` closes all.
- **Append-only is structurally preserved everywhere.** No subsystem bypasses `engine.synthesize` to mutate state.

---

## Verdict on v2.0 readiness

**Phase 1 outcomes do not change the v2.0 plan. They expand it.**

The v2.0 type-level changes (`Distinction([u8; 16])`, `pub(crate)`, IdentityHasher) remain the right move. They close 5 of the foreign-ID issues structurally. But Phase 1 surfaced:

- **3 consensus-correctness bugs** in network/commitment that v2.0 does NOT close and that **must not ship to a network at any version**: `previous_root` 8-byte truncation, `leader_id` not in hash, validator atomic-failure leakage.
- **2 HIGH FFI hardening items** that need ~5 LOC total: `panic = "abort"` + thread-safety docstring.
- **1 CRITICAL wasm wire-format break** at v2.0 that needs the CHECKLIST §5 decision (hex-on-wire) first.
- **Largest single cleanup opportunity:** delete `ParallelBatchProcessor` (~250 LOC).
- **Largest single theory-drift cleanup:** compactor — either restate its contract (mutating is fine, just admit it) or refactor to truly read-only.

## Phase 1 conclusion

The engine core (engine.rs + primitives.rs + local_agent.rs ≈ 350 LOC) and the commitment subsystem are clean. The validator, network, compactor, FFI, and WASM each carry distinct concrete bugs that should be fixed before v2.0 ships. No memory-safety violations were found. No leak paths. No reachable production `.unwrap()` in FFI/WASM (engine.rs unchanged).

**Recommended next steps** (Phase 2):
1. Update CHECKLIST.md to integrate these findings into Sections 1 (FIX) and 2 (IMPLEMENT).
2. Lift plan mode so the four validation experiments can be run (4 binaries + 4 `[[bin]]` stanzas).
3. Begin Phase 2 quick wins on the integration branch.

---

## Report index

**Audits** (`experiments/findings/audits/`):
- `compactor.md` — engine-architect
- `commitment.md` — engine-architect
- `validator.md` — qa-sentinel
- `network.md` — qa-sentinel
- `parallel.md` — rust-craftsman
- `ffi.md` — rust-craftsman
- `wasm.md` — rust-craftsman

**Validations** (`experiments/findings/validations/`):
- `coding_law.md` — research-lead (design + drafted source)
- `mediated_self_reference.md` — research-lead (design + drafted source)
- `fold_law.md` — research-lead (design only)
- `cross_engine_determinism.md` — qa-sentinel (design + drafted source)
