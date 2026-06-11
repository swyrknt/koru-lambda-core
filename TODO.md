# TODO — koru-lambda-core

Improvements identified through five rounds of ALIS warroom research (50+ experiments, 11 research documents). Organized by priority and breaking-change status.

---

## Version 1.3 — Additive (non-breaking, zero regression)

These expose information the engine already has but keeps private. No existing behavior changes. All 114 tests pass unchanged.

### 1. Parent/child traversal API (~40 LOC)

The engine stores relationships as `DashMap<(String, String), ()>` but provides no way to ask "who are the parents of this distinction?" or "what children does this distinction have?" Consumers (ALIS) are forced to rebuild the entire graph externally.

**Add:**
```rust
impl DistinctionEngine {
    /// The two distinctions whose synthesis produced `d`.
    /// Returns None for primordials (d0, d1 have no parents).
    pub fn parents_of(&self, d: &Distinction) -> Option<(Distinction, Distinction)>;

    /// All distinctions that were produced with `d` as an input.
    pub fn children_of(&self, d: &Distinction) -> Vec<Distinction>;

    /// Number of relationships involving this distinction.
    pub fn degree(&self, d: &Distinction) -> usize;
}
```

**Implementation:** Add a reverse index `DashMap<String, Vec<String>>` (distinction_id → list of distinction_ids it participates in as a relationship member). Populate it inside `synthesize()` at line 126-128 (right after the `add_relationship` calls).

Note on `parents_of`: the relationship set stores `(min(a,b), max(a,b))` — the canonical pair between the NEW distinction and each parent. So for distinction `c`, its two stored relationships are `(min(c, parent_a), max(c, parent_a))` and `(min(c, parent_b), max(c, parent_b))`. To find parents: look up all relationships involving `c.id` via the reverse index, then the other member of each matching pair is a parent. With the reverse index this is O(1). Without it, you'd scan the full relationship set.

`children_of` works the same way: look up all relationships involving `d.id`, filter to those where `d` was an input (not the result). Since relationships are stored as `(result, parent)` pairs, a distinction appears in relationships both as a result (its birth) and as a parent (its children's births). The reverse index captures both — distinguish by checking whether `synthesize(d, other) == some_known_child`.

**Why:** Unlocks deletion of ~600 LOC from ALIS's tracker.rs (parent_map, children_map, all_distinctions duplicate).

---

### 2. Append-only synthesis log (~30 LOC)

Every novel synthesis produces a `(parent_a, parent_b) → child` event. The engine currently discards this history. Recording it enables trivial persistence (save the log, replay to reconstruct).

**Add:**
```rust
impl DistinctionEngine {
    /// The ordered log of all novel synthesis operations.
    /// Each entry is (parent_a_id, parent_b_id). The child is derivable
    /// via SHA256(min:max).
    pub fn synthesis_log(&self) -> &[(String, String)];  // or behind a lock

    /// Number of novel syntheses performed.
    pub fn synthesis_count(&self) -> usize;
}
```

**Implementation:** Add `log: RwLock<Vec<(String, String)>>` to DistinctionEngine. In `synthesize()`, after the "is this new?" check, push `(a.id, b.id)` to the log. Only novel syntheses get logged (idempotent repeats don't).

**Why:** Enables ALIS Phase 6 persistence — save the log as a binary file, replay to reconstruct. Measured: 714K ops/sec replay, perfect fidelity, zero missing distinctions.

---

### 3. Fix ByteMapping phantom nodes (~5 LOC)

`ByteMapping::map_byte_to_distinction()` builds its 256-entry cache against a **throwaway engine** (a private `DistinctionEngine` created inside `lazy_static`). The returned Distinction IDs are correct (content addressing guarantees this) but they are NOT registered in the caller's engine. This means:
- `engine.get_distinction_by_id(byte_id)` returns `None`
- `engine.distinction_count()` undercounts by up to 9 per unique byte used
- Relationships reference IDs that don't exist in `all_distinctions`

**Fix:** Change the byte cache initialization to accept a reference to the actual engine, OR register each byte distinction in the caller's engine on first use, OR expose a `register_byte_primitives(&self)` method that populates all 256 byte distinctions + their 8-step intermediate chains.

**Simplest approach:** Add a method `ensure_byte_registered(&self, byte: u8)` that synthesizes the 8-step chain into `self` if not already present. Call it inside `to_canonical_structure` implementations. Since synthesis is idempotent, this is safe to call repeatedly.

**Why:** Eliminates silent bugs where byte-derived distinctions are "ghosts" in the graph.

---

### 4. Streaming degree calculation (~15 LOC, optional)

`StructuralCompactor::calculate_sis()` currently calls `get_state_snapshot()` (clones the full relationship set) then builds a degree HashMap. With the reverse index from item #1, degree is O(1) per node.

**Add:**
```rust
impl StructuralCompactor {
    /// Compute structural importance using the engine's native degree.
    /// No snapshot clone needed.
    pub fn calculate_sis_streaming(&self, engine: &Arc<DistinctionEngine>) -> HashMap<String, usize>;
}
```

**Why:** Avoids O(n) memory overhead of snapshot-then-rebuild. Important at scale (snapshot of 5M distinctions = 662ms + ~2GB allocation).

---

## Version 2.0 — Breaking Changes

These require a major version bump. Both consumers (ALIS, koru-protocol) update simultaneously. No third-party dependents exist on crates.io.

### 5. `Distinction([u8; 16]) + Copy`

The single highest-leverage change possible.

**Current:** `Distinction { id: String }` — 88 bytes (24 stack + 64 heap), requires Clone, heap-allocates on every construction.

**Target:** `Distinction([u8; 16])` — 16 bytes, inline, Copy, register-passable. Truncated SHA256 (collision probability 1 in 2^64 — effectively zero over any realistic graph size).

**Impact:**
- 5-8× memory reduction per distinction
- Every `.clone()` on Distinction becomes a memcpy (or nothing — it's Copy)
- DashMap keys shrink from String to `[u8; 16]`
- Enables zero-copy mmap persistence (log becomes `&[(Distinction, Distinction)]`)
- ALIS's tracker DashMaps go from `DashMap<String, Vec<String>>` to `DashMap<Distinction, SmallVec<[Distinction; 4]>>`

**Breaking:** Every call site using `.id() -> &str` changes. All DashMap key types change. `Distinction::new(String)` disappears.

---

### 6. `Distinction` field `pub(crate)`

Prevent external construction of Distinction values. Currently `Distinction::new(String)` is public — anyone can mint an ID out of thin air and pass it to `synthesize()`, creating dangling relationships to nonexistent distinctions.

**Target:** Only the engine can construct Distinctions. External code obtains them only through `engine.synthesize()`, `engine.d0()`, `engine.d1()`, or `engine.get_distinction_by_id()`.

**Why:** Type-system validation. Foreign-ID contamination becomes impossible at compile time rather than a runtime discipline problem.

---

### 7. Serde on the synthesis log

With the log from item #2, add `Serialize`/`Deserialize` on the log entry type so consumers can persist it with bincode/serde_json without conversion.

```rust
#[derive(Serialize, Deserialize)]
pub struct SynthesisEvent {
    pub parent_a: DistinctionId,
    pub parent_b: DistinctionId,
}
```

**Why:** Persistence becomes `bincode::serialize(engine.synthesis_log())`. One line.

---

## Not Planned

These were considered and explicitly excluded.

| Item | Why excluded |
|---|---|
| `remove_distinction()` | Violates append-only. A child's identity IS its parents — removing interior nodes breaks content addressing downstream. Theory-forbidden. |
| Fix `ParallelBatchProcessor` to be actually parallel | Blockchain-specific. DashMap already provides concurrency. ALIS uses `rayon::par_iter` directly. |
| Fix `StructuralCompactor` to actually remove nodes | Same as `remove_distinction()`. Classification is correct. Pruning violates theory. |
| `from_snapshot()` constructor | Unnecessary under event-sourced persistence. Replay the synthesis log instead. Also: the snapshot is causally lossy (symmetric relationships, can't recover synthesis direction). |
| `Serialize`/`Deserialize` on `Distinction` | Unnecessary. Persist the log, not individual distinctions. IDs are derivable from the log. |

---

## Measurement Targets

After version 1.3 lands, verify:
- All 114 existing tests still pass
- `synthesize()` throughput unchanged (≥500K ops/sec single-threaded)
- `parents_of()` / `children_of()` are O(1) amortized
- Synthesis log append overhead < 5% of synthesis cost
- Phantom node fix: `distinction_count()` matches actual unique IDs in relationships

After version 2.0 lands, verify:
- Memory per distinction drops from ~656 bytes to ~80 bytes
- 10M distinctions fits in ~800 MB (vs ~6.5 GB before)
- No `.clone()` calls on Distinction in any consumer hot path
- ALIS tracker.rs can be deleted (engine provides traversal natively)
