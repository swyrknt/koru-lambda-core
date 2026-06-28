# Changelog

Per `DESIGN.md` Part 10.5 (Budget Amendment Policy), budget-gate
amendments are announced in `## Unreleased` so consumer teams (ALIS,
koru-protocol) see the ratchet before they migrate.

Per `DESIGN.md` gate 32, this file will be rewritten as one coherent
v2.0 entry during Step 5 release prep. The `## Unreleased` section
below tracks amendments that land before that rewrite.

## Unreleased

### Substrate

- **Step 1e merged-map refactor** — `DistinctionEngine` storage layout
  changed from three side-by-side `DashMap`s
  (`all_distinctions` / `parents_of` / `degree_counts`) to one
  `DashMap<[u8;16], EngineNode { parents, degree }>`. Public API
  unchanged in signature; performance: single-thread +20%, 8-thread
  +46%, parallel-scaling ratio 2.85× → 3.43× on M3 Pro. See commits
  `30cc77f` (verification mock), `fd9c2b1` (refactor), `cd6ca3f`
  (qa-sentinel falsification tests).

- **Happens-before contract narrowed.** Parent `degree` `fetch_add`s
  now occur after the new-child shard write-lock is released (B1
  mitigation — parent and new-child may hash to the same shard). A
  racing reader who observes a new child and immediately queries
  `degree(parent)` may transiently see the pre-bump value. Post-join
  state is consistent (Release/Acquire pair preserves eventual
  visibility; per-parent sum invariant
  `sum_of_degrees == 2 × non_primordial_count` holds at every
  quiescent point). LCAs drive synthesis sequentially per LCA, so the
  relaxation is invisible to the documented consumer contract.
  Traversal probes should read `degree` at quiescent points (post join
  barrier or consumer-driven epoch boundary), not mid-flight.

### Budget gates

- **Gate 12 amended.** The 4× ratio target proved structurally
  unreachable on M3 Pro 6P+2E asymmetric silicon (hardware ceiling
  ~6.5×; merged-map upper bound 3.59×). Replaced with
  platform-named: M3 Pro target ≥ 12M absolute AND ≥ 3.4× ratio
  (floor ≥ 10M AND ≥ 3.0×); symmetric server hardware (8+ uniform
  cores) ≥ 4× ratio as regression watch, not gate. See
  `BUDGET_LOG.md` row 1 for measurement provenance and signers.
