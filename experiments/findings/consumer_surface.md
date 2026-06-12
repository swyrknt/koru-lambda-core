# Consumer Surface — ALIS + koru-protocol

**Date:** 2026-06-11
**Purpose:** Quantify v2.0 migration cost for the two known consumers before Phase 6 starts.
**Method:** `grep` for affected APIs across each consumer's `src/` and `examples/`.

---

## Headline

**ALIS bears most of the v2.0 migration cost** (24 `Distinction::new` sites; heavy `ByteMapping` use; `get_state_snapshot` callers). **koru is mostly clean** (zero `Distinction::new` sites; uses higher-level subsystem APIs only).

Critical finding: **TODO.md item #1 (traversal API) deletes ~600 LOC from ALIS's `tracker.rs`** — most `Distinction::new` sites in ALIS live in `tracker.rs`, which the v2.0 traversal API replaces wholesale. The migration is therefore *less* than the raw grep count suggests.

---

## Dependency declarations

Both consumers pin koru-lambda-core 1.2:

| Project | `Cargo.toml` line |
|---|---|
| `/Users/sawyerkent/Projects/alis-ai/Cargo.toml` | `koru-lambda-core = "1.2"` |
| `/Users/sawyerkent/Projects/koru/koru-node/Cargo.toml` | `koru-lambda-core = "1.2.0"` |
| `/Users/sawyerkent/Projects/koru/koru-protocol/Cargo.toml` | `koru-lambda-core = "1.2.0"` |

---

## ALIS surface

### Imports

| File | Imports |
|---|---|
| `src/lib.rs` | re-exports `Canonicalizable, Distinction, DistinctionEngine, LocalCausalAgent` |
| `src/alis.rs` | `Distinction, DistinctionEngine` |
| `src/expression.rs` | `Canonicalizable, Distinction, DistinctionEngine, LocalCausalAgent` |
| `src/perception.rs` | `ByteMapping, Canonicalizable, Distinction, DistinctionEngine, LocalCausalAgent` |
| `src/tracker.rs` | `ByteMapping, Distinction, DistinctionEngine` |
| `src/consolidation.rs` | `DistinctionEngine, StructuralCompactor` |

### Affected API call counts

| API | Sites | Files | Phase 6 impact |
|---|---|---|---|
| **`Distinction::new(String)`** | **24** | `tracker.rs` (12), `examples/persistence_experiments.rs` (6), `expression.rs` (2), `showcase.rs` (2), `context_awareness_experiments.rs` (1), `self_experiments.rs` (1) | **BREAKS** — Distinction field becomes `pub(crate)`. Most `tracker.rs` sites delete entirely when ALIS adopts v2.0 traversal API per TODO #1. |
| `ByteMapping::map_byte_to_distinction(byte, engine)` | ~20 sites | `perception.rs`, `tracker.rs`, all examples | **SIGNATURE UNCHANGED.** Behavior is now correct (Phase 3 fix registers full chain). ALIS will see more distinctions per byte input; this matches theory. |
| `get_state_snapshot()` | 3 sites | `src/lib.rs`, `examples/theory_experiments.rs` (2) | **BREAKS** — renamed to `get_state_snapshot_unsynchronized` in Phase 2. Trivial sed-style update. |
| `LocalCausalAgent::update_local_root` (impl'd) | 2 sites | `expression.rs:327`, `perception.rs:254` | **BREAKS** — trait signature changes for V6 fix (joint `restore_state`). May need to add new method, deprecate old, or remove from trait. |
| `StructuralCompactor` | ~uses (count not exhaustive) | `consolidation.rs` | Should be unchanged in v2.0 (compactor design accepted per Decision 5.2). |

### `tracker.rs` is the load-bearing surface

`tracker.rs` accounts for **12 of 24** `Distinction::new` calls in ALIS. TODO.md item #1 (traversal API: `parents_of`, `children_of`, `degree`) explicitly says:

> "Unlocks deletion of ~600 LOC from ALIS's tracker.rs (parent_map, children_map, all_distinctions duplicate)."

So most `Distinction::new` usage in ALIS is in code that v2.0's new traversal API makes obsolete. The ALIS migration is therefore predominantly *replacement* (delete tracker, use engine native API), not *rewriting*.

### Estimated ALIS migration cost

| Type | Sites | Effort |
|---|---|---|
| Delete (replaced by traversal API) | ~12 in `tracker.rs` + tests/examples that exercise it | LOW (mechanical) |
| Migrate (`get_state_snapshot` rename) | 3 | TRIVIAL (sed) |
| Migrate (`Distinction::new` → engine API outside tracker) | ~12 in expression/examples | MODERATE (each site needs to be re-thought against the engine's new API) |
| Migrate (`update_local_root` trait change) | 2 | LOW (one-time pattern) |

**Total ALIS effort: ~1–2 days for an experienced developer.**

---

## koru surface

### Imports

| File | Imports |
|---|---|
| `koru-protocol/src/lib.rs` | re-exports `Distinction, DistinctionEngine, LocalCausalAgent, BatchCommitment, NetworkStats, PeerIdentity, TransactionBatch` |
| `koru-protocol/src/account.rs` | `LocalCausalAgent, PeerIdentity, Canonicalizable, Distinction, DistinctionEngine` |
| `koru-protocol/src/identity.rs` | `Canonicalizable, Distinction, DistinctionEngine` |
| `koru-protocol/src/transfer.rs` | `Canonicalizable, Distinction, DistinctionEngine` |
| `koru-protocol/src/node.rs` | `BatchCommitment, CompactionStats, StructuralCompactor, LocalCausalAgent, NetworkAgent, NetworkStats, PeerIdentity, TransactionAction, TransactionBatch, Canonicalizable, Distinction, DistinctionEngine` |
| `koru-node/*` | `DistinctionEngine`, `PeerIdentity`, batch log helpers |

### Affected API call counts

| API | Sites | Files | Phase 6 impact |
|---|---|---|---|
| **`Distinction::new(String)`** | **0** | none | **CLEAN.** koru does not mint Distinctions externally. |
| `PeerIdentity::new(id, engine)` | 4 | `account.rs:101`, `node.rs:1309`, `node_cmd.rs:177, 199, 258` | **SIGNATURE UNCHANGED.** Behavior changes: peer-id length cap + empty-id rejection (Section 1.6 N1/N2). May surface tests where empty/long peer-ids are used. |
| `LocalCausalAgent::update_local_root` (impl'd) | 3 | `account.rs:209`, `node.rs:489, 1991` | **BREAKS** — trait signature changes for V6 fix. Same pattern as ALIS. |
| `BatchCommitment::compute(...)` | implicit via `propose_commitment` | `node.rs` | **WIRE FORMAT BREAK** — `leader_id` enters the SHA256 hash (N6 fix). Existing persisted commitments will not validate post-v2.0. |
| `TransactionAction { ... }` field-struct construction | many | `node.rs:845, 924, 956, 984, 1265, ...` | **SIGNATURE UNCHANGED.** But validator now pre-validates (V5 fix) — any test sending invalid batches expects engine state unchanged on rejection; current behavior leaks 4-distinctions-per-pre-failure-tx. The fix makes the validator more strictly atomic. |
| `Canonicalizable for X` impls | 5 | `account.rs:22 (AccountAction)`, `transfer.rs:130 (Transfer)`, `transfer.rs:248 (KeyRotation)`, `node.rs:76 (NodeAction)`, `node.rs:207 (PeerAction)` | **POTENTIALLY UNCHANGED.** Trait signature `fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction` stays. The Distinction *output* changes representation if v2.0 bytes-on-wire affects them externally; internally to koru it should not matter. |
| `StructuralCompactor::new(&engine)` | 2 | `node.rs:627, 676` | **UNCHANGED** (compactor design accepted per Decision 5.2). |

### koru wire format concerns

The N6 fix (hash `leader_id` in `BatchCommitment::compute`) breaks wire format:

- Any node with persisted commitments computed against the old hash will see those commitments fail to verify against the new hash.
- Cross-version peer gossip: a 1.2.0 node and a 2.0.0 node cannot exchange commitments — different hashes for the same logical batch.
- **Mitigation:** v2.0 is the first major bump. Both sides upgrade simultaneously per Decision 5.1 (bundle into v2.0). No mixed-version testnet should exist anyway (per user: "no active users today").

The N5 fix (drop `.take(8)` from `previous_root`) similarly changes the action distinction id. Same mitigation.

### Estimated koru migration cost

| Type | Sites | Effort |
|---|---|---|
| `update_local_root` trait change | 3 | LOW |
| `Distinction` field changes (just type, no field access) | indirect | LOW (compiler-driven) |
| Re-test all consensus paths against the new wire format | full integration test suite | MODERATE |
| Wire-format documentation update | 1 file | LOW |

**Total koru effort: ~1 day for an experienced developer.**

---

## Cross-consumer summary

| Concern | ALIS | koru |
|---|---|---|
| `Distinction::new` exposed (becomes `pub(crate)`) | **24 sites** (mostly tracker.rs, deleted by traversal API) | 0 sites |
| `get_state_snapshot` rename | 3 sites | 0 sites |
| `update_local_root` trait change | 2 sites | 3 sites |
| Wire format break (N5/N6) | n/a (no peer-to-peer gossip) | YES (all commitments and BatchProposed actions) |
| Distinction byte representation change (`String` → `[u8; 16]`) | indirect (auto-handled by compiler if APIs stay) | indirect |
| Hex display/parse layer | use it directly (`Distinction::to_hex()`, `from_hex`) | use it for wire format adapters |

---

## Phase 6 implications

1. **ALIS migration is bigger than I estimated** (~24 `Distinction::new` sites, not "a handful"). But ~half disappear when tracker.rs is replaced by the traversal API. Net effort moderate.
2. **koru migration is smaller than I estimated** — no `Distinction::new` use at all. Main concern is wire format break (managed because no active users).
3. **`update_local_root` trait change** affects 5 sites across both consumers (2 ALIS + 3 koru). Worth designing the v2.0 API carefully:
   - Option A: rename `update_local_root` → `restore_state(engine, root_id, nonce)` (or split into validator vs network agent versions)
   - Option B: add `restore_state` alongside, deprecate `update_local_root`
   - Recommend A in v2.0 since it's a breaking release anyway.
4. **`Canonicalizable` trait users** (5 koru sites) are stable post-v2.0 as long as the trait signature stays the same. Confirms Decision 5.5 (bytes-on-wire) is consumer-safe — they implement, they don't observe.
5. **`PeerIdentity::new` callers** (4 sites in koru) become subject to N1/N2 validation. If anyone tests with empty peer-id or oversized peer-id, those tests now fail (correctly).

---

## What to add to CHECKLIST

Add these items to Phase 8 (consumer migration):

- [ ] ALIS: rename `get_state_snapshot` → `_unsynchronized` (3 sites)
- [ ] ALIS: delete `tracker.rs` and replace with engine's native traversal API (~600 LOC removal per TODO #1)
- [ ] ALIS: rewrite remaining `Distinction::new` sites outside `tracker.rs` (~12 sites) to obtain Distinctions through the engine
- [ ] ALIS: migrate `update_local_root` trait impl to new `restore_state` API (2 sites)
- [ ] koru: migrate `update_local_root` trait impl (3 sites)
- [ ] koru: verify no test uses empty or oversized peer-id (would fail post-N1/N2 fix)
- [ ] koru: update wire-format documentation for the N5/N6 hash changes (commitment_hash and BatchProposed action distinction)
- [ ] koru: re-run integration test suite against v2.0; any test asserting specific commitment hashes needs new expected values

---

## What to add to Section 2.5 of CHECKLIST

Replace the placeholder "ALIS migration plan" / "koru migration plan" with concrete grep-counted scope above.

---

## Reproduction

```bash
# ALIS
grep -rn "Distinction::new\|\.get_state_snapshot\b\|ParallelBatchProcessor\|ParallelSynthesizer\|update_local_root\|ByteMapping::map_byte_to_distinction" \
  /Users/sawyerkent/Projects/alis-ai/src/ /Users/sawyerkent/Projects/alis-ai/examples/

# koru
grep -rn "Distinction::new\|\.get_state_snapshot\b\|ParallelBatchProcessor\|ParallelSynthesizer\|BatchCommitment::compute\|PeerIdentity::new\|update_local_root" \
  /Users/sawyerkent/Projects/koru/koru-protocol/src/ /Users/sawyerkent/Projects/koru/koru-node/src/
```
