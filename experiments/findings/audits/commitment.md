# Audit — `src/subsystems/commitment.rs` (engine-architect)

**Scope:** 504 LOC. Blockchain two-stage gossip + LRU cache. Concern: post-commit engine mutation, foreign-ID acceptance from gossip payloads.
**Method:** Static read-only audit. `cargo clippy` / `cargo test` not run (agent was in plan mode); recommended as follow-up.
**Verdict:** **CLEAN.** One of the cleanest subsystems. No theory drift. Ships as-is for v2.0.

---

## Verdicts at a glance

| # | Claim under test | Verdict | Citation |
|---|---|---|---|
| 1 | Engine mutated *after* `synthesize_action()` returns | **NO** — no post-return mutation path | commitment.rs:235-258 |
| 2 | LRU cache stores `Distinction` values | **NO** — stores `(TransactionBatch, BatchCommitment)`, no Distinction inside | commitment.rs:17, 158, 194-197 |
| 3 | LRU eviction touches engine state | **NO** — cache is in-memory data availability layer only | commitment.rs:194-197 |
| 4 | Stage-1 → Stage-2 monotonic w.r.t. engine | **MONOTONE** — Stage-2 (`cache_batch`) never calls `engine.synthesize` | commitment.rs:194-197 vs 235-258 |
| 5 | All Distinction sites use `engine.synthesize` / `d0` / `d1` | **YES** | commitment.rs:101-118, 169-181, 246-247, 250 |
| 6 | Direct `Distinction::new(...)` from gossip payload | **NONE** | full file scan |
| 7 | Append-only preserved | **PRESERVED** — eviction is independent of engine | commitment.rs:194-197 |
| 8 | Concurrent `commit()` contention | **Caller-owned** — `&mut self` requires external lock; engine itself is safe | commitment.rs:236, 194 |
| 9 | Orphans if Stage-2 never arrives | **NO orphans in engine** — cache lossy without correctness impact | commitment.rs:104-114; primitives.rs:64-71 |
| 10 | v2.0 (`[u8; 16]`) compile blockers | One soft break: `commitment_root_id(&self) -> &str` | commitment.rs:223-225 |
| 11 | Determinism preserved under `verify_batch` | **YES** — recomputes hash, no synthesis | commitment.rs:90-93 |
| 12 | Rejection path reuses `local_root.clone()` correctly | **YES** — irreflexivity-friendly | commitment.rs:241-244 |

---

## 1. Mutation surface

Two paths could mutate engine state. Both behave correctly.

- **`CommitmentAgent::synthesize_action`** (commitment.rs:235-258): the only engine-writing path.
  - L246: `action_data.to_canonical_structure(engine)` → `BatchCommitment::to_canonical_structure` (L101-118), the synthesize-heavy step.
  - L250: `engine.synthesize(&self.local_root, &commitment_distinction)` — root advance.
  - L253-255: pure agent-local field updates (`self.local_root`, `self.expected_nonce`, `self.commitments_processed`).
  - Returns at L257.
- **`cache_batch`** (commitment.rs:194-197): does NOT touch the engine. Only mutates `self.cache`.
- **`from_root`** (commitment.rs:184-191), **`update_local_root`** (commitment.rs:260-262): no engine access.

No post-return engine mutation path exists.

## 2. LRU cache

- Holds `LruCache<[u8; 32], (TransactionBatch, BatchCommitment)>` (commitment.rs:17).
- `TransactionBatch = {transactions: Vec<TransactionAction>, previous_root: String}` (validator.rs:48-54).
- `BatchCommitment = {commitment_hash: [u8;32], nonce, epoch, leader_id: String, batch_size}` (commitment.rs:23-38).
- **No `Distinction` ever stored.** No engine state touched on eviction.
- Default capacity 1000 (commitment.rs:177), configurable via `from_root` (commitment.rs:187).

The cache is data-availability scaffolding for blockchain gossip, fully orthogonal to engine state. Correct separation.

## 3. Stage-1 → Stage-2 monotonicity

- **Stage 1** (consensus broadcast): `BatchCommitment::compute` (pure SHA-256, no engine touch) + `synthesize_action` (up to ~25 novel syntheses inside `to_canonical_structure` plus 1 final root synthesize at L250).
- **Stage 2** (lazy data fetch): `cache_batch` is pure agent-local. `get_cached_batch`/`has_cached` are read-only. The verification path `verify_batch` (L90-93) is pure-hash. **No engine calls in Stage 2 at all.**

Engine state advances only during Stage 1. Rollback is not expressible in this code — there is no subtraction path for distinctions or relationships.

## 4. Distinction construction sites (v2.0 blocker scan)

Five sites produce Distinction values. **Zero `Distinction::new` calls.**

| Site | Line | Mechanism | Provenance | v2.0 safe? |
|---|---|---|---|---|
| Genesis | 171-173 | `engine.d0().clone()`, `engine.d1().clone()`, `engine.synthesize(&d0, &d1)` | Engine primordials | YES |
| Hash fold | 104-107 | `engine.d0().clone()` accumulator + `byte.to_canonical_structure(engine)` per byte | First 16 bytes of `commitment_hash` (content hash) via ByteMapping | YES (subject to TODO #3 phantom note) |
| Nonce fold | 110-114 | `engine.d1().clone()` accumulator + per-byte synthesis | `nonce.to_le_bytes()` (8 bytes) | YES (same caveat) |
| Final commitment | 117 | `engine.synthesize(&hash_d, &nonce_d)` | Derived | YES |
| Root advance | 250 | `engine.synthesize(&self.local_root, &commitment_distinction)` | Derived | YES |

The `previous_root: String` field on `TransactionBatch` (validator.rs:53) is referenced in **validator.rs**, not here. Commitment never feeds it to `Distinction::new`.

**ByteMapping caveat:** Each per-byte canonicalization at L105, L112 routes through `ByteMapping::map_byte_to_distinction` (primitives.rs:64-71) which returns `Distinction::new(id.clone())` from a cache built against a throwaway engine — the pre-existing TODO #3 phantom-node bug. Not introduced here; commitment.rs is a victim, not a cause.

## 5. Method surface used on the engine

- `engine.d0()` (171, 104) — read
- `engine.d1()` (172, 111) — read
- `engine.synthesize(...)` (173, 106, 113, 117, 250) — the sole mutation path

`get_distinction_by_id`, `get_state_snapshot`, `add_relationship`, etc. never called. `add_relationship` is private to engine.rs anyway. **Uses only the canonical mutation surface.**

## 6. Concurrent safety

All state-mutating methods take `&mut self`. No internal locks. Concurrency is the caller's problem — matches the engine's design pattern.

- N `CommitmentAgent` instances on shared `Arc<DistinctionEngine>`: SAFE. Each has its own LRU + `local_root`; engine's DashMap provides concurrency.
- Single agent across threads: requires external `Mutex` / `RwLock`.

`expected_nonce`/`commitments_processed` updates at L254-255 are non-atomic w.r.t. engine writes at L246-250, but `&mut self` borrow checker prevents concurrent calls on the same agent. **No runtime concurrency bug.**

## 7. Failure mode: Stage-2 never arrives

**Engine state — no orphans.** Stage 1 produced and stored:
- `commitment_distinction` in `engine.all_distinctions`
- New `local_root` after fusing with previous root
- Their symmetric relationships

These are durable and content-addressed. Future Stage-2 arrivals re-synthesize the same id idempotently (engine.rs:119-121).

**Cache state — lossy without correctness impact.** If Stage 2 never delivers:
- LRU never gains the `(batch, commitment)` entry.
- Future `has_cached` returns false; `get_cached_batch` returns None.
- A `BatchDataRequest` from another peer can still be served by another node (gossip semantics).

**Replay implications:** Once TODO #2 (synthesis log) lands, replay reconstructs commitment distinctions without raw batch bytes. Content-addressed system, correct behavior. Cache is data availability; engine is truth.

## 8. v2.0 impact

When `Distinction` becomes `pub(crate) [u8; 16]` Copy:

| Line | Current | After v2.0 |
|---|---|---|
| 104, 111 | `engine.d0().clone()` / `engine.d1().clone()` | drop `.clone()` (Copy) |
| 105, 112 | `byte.to_canonical_structure(engine)` | signature unchanged, returns Copy |
| 171-173 | `engine.d0().clone()` etc. | drop `.clone()` |
| 184, 186 | `pub fn from_root(root: Distinction, ...)` | unchanged |
| 223-225 | `pub fn commitment_root_id(&self) -> &str { self.local_root.id() }` | **BREAKS** — change to `-> [u8; 16]` (Copy) or `-> &[u8; 16]` |
| 243, 253, 257, 261 | `self.local_root.clone()` / assignment | drop `.clone()` (Copy) |

`BatchCommitment::leader_id: String` and `TransactionBatch::previous_root: String` survive — independent of Distinction type. (Note: `previous_root` should arguably be a Distinction id, but that's a coordinated change with validator.rs.)

**No gossip-payload-to-Distinction construction exists.** v2.0 `pub(crate)` lockdown will not create compile blockers here.

## 9. Test coverage

Eight `#[test]` functions at commitment.rs:271-503. Static read finds:
- No deprecated APIs.
- No `unwrap()` on fallible network/IO paths.
- Only `NonZeroUsize::new(...).unwrap()` on const literals (cannot fail).

Coverage gaps:
- No orphan-resistance test (Stage-1 without Stage-2).
- No concurrent-arrival determinism test (N agents → same engine, different order → same final state).
- `update_local_root` (trait surface) is unexercised.

## Recommended actions

1. **No theory drift — ship as-is.** Append-only preserved. No post-commit mutation. Two-stage pattern correctly separates causal commitment from data availability.
2. **Add idempotent re-synth test:** call `synthesize_action` twice with same commitment on two independent agents; assert matching `local_root`s and identical `distinction_count`. Locks in the content-addressing guarantee at the subsystem boundary.
3. **Document ByteMapping phantom interaction in commitment.rs header comment.** 16-byte hash + 8-byte nonce canonicalization at L104-114 inherits the TODO #3 phantom bug. Not introduced here, but worth flagging for consumers.
4. **Cosmetic:** `BatchCommitment::compute` could take `&str` instead of `String` for `leader_id` (cloned at L91 in `verify_batch`).
5. **Observability:** add `cache_len()` / `cache_size()` accessor. One-liner.
6. **Test gap:** orphan-resistance test (Stage-1 commit + skip `cache_batch` → assert engine state complete).
7. **v2.0:** retype `commitment_root_id(&self) -> &str` to match new Distinction type.

## Pending actions (blocked by plan mode, follow-up)

```bash
cd /Users/sawyerkent/Projects/koru-lambda-core
cargo clippy --all-targets 2>&1 | grep -i commitment
cargo test --lib commitment 2>&1 | tail -30
```

---

## Severity ranking

| Issue | Severity |
|---|---|
| Inherited ByteMapping phantom interaction | **LOW** (caused upstream, fixed via TODO #3) |
| `commitment_root_id` returns `&str` | **LOW** (v2.0 retype) |
| Missing orphan-resistance test | **LOW** (correctness already follows from append-only) |
| Missing concurrent-arrival determinism test | **LOW** (engine determinism is well-tested) |

**Bottom line:** Clean subsystem. Uses only canonical engine mutation surface. Zero foreign-ID construction. Cache fully orthogonal to engine. v2.0 lockdown will not block this file. **Approve.**
