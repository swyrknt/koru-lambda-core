# Audit — `src/subsystems/compactor.rs` (engine-architect)

**Scope:** 503 LOC. Documented as "non-destructive — classifies but never mutates the engine" (CLAUDE.md).
**Method:** Static read-only audit. `cargo clippy` / `cargo test` not run (agent was in plan mode); recommended as follow-up.
**Verdict:** **CLAUDE.md is wrong.** Compactor mutates the engine in two paths and has the largest concentration of theory drift in the codebase.

---

## Verdicts at a glance

| # | Claim | Verdict | Evidence |
|---|---|---|---|
| 1 | "Compactor never mutates the engine" | **REFUTED** | `new()` mutates (compactor.rs:104), `synthesize_action` mutates (compactor.rs:275, 278) |
| 2 | `r = 2d − 3` preserved | **CONFIRMED** | All mutations go through `engine.synthesize`; no direct DashMap writes |
| 3 | No foreign `Distinction::new` (v2.0 blocker scan) | **CONFIRMED** | Zero `Distinction::new` calls in `compactor.rs`. Soft churn: String/&str query API will need re-typing. |
| 4 | Free of magic constants / theory drift | **REFUTED** | `hot_threshold: 3`, `warm = hot/2`, unverified "26.6×" claim, JVM HOT/WARM/COLD vocab |
| 5 | Safe under concurrent writers | **REFUTED** | `calculate_sis` uses `engine.get_state_snapshot()` which tears (engine.rs:154-156). Systematically biases new distinctions COLD. |

---

## 1. Public method mutation audit

| Method | Receiver | Touches engine? | Verdict |
|---|---|---|---|
| `new(&Arc<DistinctionEngine>)` | constructor | **`engine.synthesize(d0, d1)` at compactor.rs:104** | **MUTATES** — adds genesis `d0⊕d1` distinction + 2 relationships |
| `from_root(root, hot_threshold)` | constructor | no | Read-only |
| `calculate_sis(&self, engine)` | `&self` | `engine.get_state_snapshot()` at :133 | Read-only (but tears under concurrent writes) |
| `classify_thermal_states(&mut self, sis_map)` | `&mut self` | no | Compactor-only mutation |
| `compact(&mut self, engine)` | `&mut self` | calls `calculate_sis` (snapshot only) | Read-only on engine. Claim holds. |
| `get_stats / is_archived / get_thermal_state / set_hot_threshold` | various | no | Read-only |
| `<StructuralCompactor as LocalCausalAgent>::synthesize_action` | `&mut self` | **`engine.synthesize` twice + canonicalization fold** at :275, :278 (and `to_canonical_structure` fold at :59, :68, :72) | **MUTATES** — writes compaction-event distinction + new root back into engine |
| `<…>::get_current_root / update_local_root` | various | no | Read-only |
| `CompactionAction::to_canonical_structure` | trait impl | **multiple `engine.synthesize` calls** at :59, :68, :72 plus byte folds | **MUTATES** — threshold/count fold chain + final pair |

**Net:** The claim "classifies but never mutates" is true for `compact()` itself. It is FALSE for `new()` and the entire `LocalCausalAgent` trait implementation. A classifier that writes to the thing it classifies has a design contradiction.

The mutations themselves are all `synthesize` calls (append-only, axiom-preserving), so this is documentation drift more than a theory violation — but it is a documentation drift that has been stated in CLAUDE.md and TODO.md as load-bearing fact.

## 2. `r = 2d − 3` invariant

No direct DashMap writes; all engine mutations route through `synthesize`. Invariant is preserved by construction. **Safe.**

## 3. Foreign-data `Distinction` construction (v2.0 blocker scan)

**Hard blockers in `compactor.rs`: zero.** No `Distinction::new(...)` calls.

**Soft churn (porting cost at v2.0):**
- `archived_ids: Vec<String>` (compactor.rs:37)
- `current_root: String` (CompactionStats, :259)
- `is_archived(&self, id: &str)` (:235)
- `get_thermal_state(&self, id: &str)` (:240)
- `HashMap<String, ThermalState>` and `HashSet<String>` internal stores

None reconstruct a `Distinction` from String — they are query-by-ID surfaces. At v2.0 (`Distinction = [u8; 16]` Copy), every `&str` parameter and `String` field should become `Distinction`.

**Phantom node concern (TODO #3):** `to_canonical_structure` on `u8` (compactor.rs:53, 63, 410, 463) routes through `ByteMapping::map_byte_to_distinction` in `primitives.rs`, which builds its cache against a throwaway engine. The compactor inherits this bug — every byte folded through the compactor adds phantom IDs to relationship set. Fixing TODO #3 closes this transitively.

## 4. Theory drift (the worst section)

The largest concentration of magic / drift in the codebase:

| Location | Drift | Recommendation |
|---|---|---|
| compactor.rs:11 | Module docstring: "26.6× storage efficiency" — not reproduced in any test, not derived from theory | Remove or reproduce |
| compactor.rs:110 | `hot_threshold: 3` — no theoretical basis | Derive from degree distribution (percentile) or remove |
| compactor.rs:161 | `warm_threshold = hot_threshold / 2` — integer division, ad hoc | Same |
| compactor.rs:19-28 | HOT/WARM/COLD trichotomy borrowed from JVM GC vocabulary | Either justify trichotomy from theory or use continuous SIS |
| compactor.rs:128 | "SIS is degree centrality" — re-derives Coding Law (rho=0.99) with no additional power | Once TODO #1 lands, `calculate_sis` is one line: `engine.degree(d)` |
| compactor.rs:71 | `archived_ids` dead in canonicalization — two `CompactionAction`s with same threshold+count produce identical distinctions regardless of archive list | Remove field or rethink semantics |
| compactor.rs:401, 453 | Test thresholds empirically tuned (5, 6) to make tests pass at scales where default (3) doesn't | Reveals default threshold is wrong |
| compactor.rs:496-501 | `assert!(compression_ratio > 1.5)` — magic threshold in assertion | Either derive or remove |

## 5. Concurrent-writer safety

`calculate_sis` (compactor.rs:133) uses `engine.get_state_snapshot()` which tears (engine.rs:154-156, ~0.06–0.08% rate per Exp 6).

Tear direction: `synthesize` inserts distinction first (engine.rs:126), then two relationships (engine.rs:127-128). So under tearing:
- Distinction visible, relationships not yet → **degree undercount**, never overcount.
- New hub-in-the-making classified COLD instead of HOT.

**Effect:** Under concurrent load, compactor systematically biases freshly-synthesized nodes COLD and may archive young hot-trajectory distinctions silently. No tests cover concurrent compaction. Restriction (quiesce writers before classifying) is undocumented.

## 6. Other findings

- **compactor.rs:96-113** — `new()` mints `d0 ⊕ d1` as `local_root`. By content addressing, every engine produces the same ID here. Two compactors against two engines have identical `local_root.id()`. Correct, but worth noting for cross-engine reasoning.
- **compactor.rs:211 + :281-283** — `compaction_count` is incremented in both `compact()` and `synthesize_action`. Calling both (as test at :436-444 does) **double-counts**.
- **compactor.rs:284-293** — `synthesize_action` re-runs `calculate_sis` after writing its own work products. The new compaction-event distinctions appear in the next classification with degree 2 (just born) → **archived as COLD by default threshold**. Recursive use: the compactor archives its own outputs.

## 7. Suspected clippy findings (verify with `cargo clippy --all-targets -- -D warnings 2>&1 | grep -i compactor`)

- compactor.rs:139-141, 145-146 — `degree_map.entry(id_a.clone())` where borrowed lookup would suffice. Likely `clippy::map_entry`.
- compactor.rs:191 — Double-clone of every cold ID into both `archived_ids` and `archived_set`. Code smell.
- compactor.rs:289-293 — Duplicated iteration/mutation pattern from `compact()`.

## Recommended actions

**Pre-v2.0 doc fix:**
1. Update CLAUDE.md and TODO.md: compactor is **not** non-destructive; `new()` and `synthesize_action` mutate. Either accept the mutation as correct (synthesize is append-only, axioms preserved) and update the docs, or refactor to actually be read-only.
2. Remove "26.6× compression" from module docstring until reproduced.

**v2.0 cleanup:**
3. Remove magic constants. Derive thresholds from observed degree distribution (e.g., 90th percentile) or drop the trichotomy in favor of raw SIS.
4. Re-type internal `String`/`&str` ID surfaces to `Distinction` (Copy).
5. Replace `calculate_sis` body with `engine.degree(d)` once TODO #1 (traversal API) lands. Eliminates snapshot clone and tear hazard.
6. Fix `compaction_count` double-increment (remove from one site).
7. Either remove the recursive `synthesize_action` re-classify (it's redundant with `compact()`), or exclude compaction-event nodes from re-classification (don't archive your own work).
8. Drop `archived_ids` field from canonicalization OR include it in the structural hash so different archive lists produce different distinctions (revisit semantics).

**Concurrent-safety doc:**
9. Add doc comment to `calculate_sis` warning that the snapshot tears under concurrent writes and biases new distinctions COLD. Recommend quiescence before classification.

---

## Severity ranking

| Issue | Severity |
|---|---|
| Documentation contradicts implementation (non-destructive claim) | **HIGH** (load-bearing for users reading CLAUDE.md) |
| Compactor archives its own work products | **HIGH** (silent correctness bug under repeated calls) |
| Concurrent-write bias toward COLD | **MEDIUM** (rare in practice — 0.06–0.08% tear rate — but silent when it hits) |
| Magic constants without derivation | **MEDIUM** (theory drift; user-tunable mitigates) |
| `compaction_count` double-increment | **LOW** (telemetry only, not correctness) |
| `archived_ids` dead in canonicalization | **LOW** (intentional per :71 comment, but confusing) |
| "26.6×" unverified claim | **LOW** (cosmetic — but exactly the kind of marketing-creep the project should avoid) |

---

**Bottom line:** Compactor's invariants (axioms preserved, append-only) hold. Its documentation and design (non-destructive, threshold-based, recursive synthesize_action) do not. **Largest single theory-drift surface in the project.** No hard v2.0 blockers; significant porting churn around String surfaces.

## Reproduction (todo — agent was in plan mode)

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core
cargo clippy --all-targets 2>&1 | grep -i compactor
cargo test --lib compactor 2>&1 | tail -30
```
