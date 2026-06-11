# Checklist — Path to Theory-Aligned, Audited, Validated

**Integration branch:** `research/warroom-experiments`
**Base:** koru-lambda-core 1.2.0 (`src/` unmodified at start)
**Evidence basis:** 17 experiments across `experiments/{runner,qa,rust}/`, summarized in `experiments/findings/`.
**Target release:** 2.0.0 (single cut, no intermediate 1.3)

Status legend: `[ ]` not done · `[~]` partial · `[x]` done

## Working agreements

- **All sub-work branches off `research/warroom-experiments` and merges back into it.** Naming: `fix/*`, `audit/*`, `validate/*`, `impl/*`. The integration branch eventually merges to `dev` as one v2.0 PR.
- **`Cargo.toml` stays at `1.2.0` throughout.** The final commit on the integration branch (right before the PR to `dev`) is the single bump to `2.0.0`. No version edits in any sub-branch.
- **`CHANGELOG.md` grows during the work**, not at the end. Each sub-PR appends to an `## Unreleased` section. The final commit renames it to `## 2.0.0` with the date.
- **Phase 1 (Sections 3 + 4) runs first**, in parallel, before any `src/` edits. We don't refactor on top of unaudited subsystems.
- **No defensive runtime checks (length validation, existence checks) on engine inputs.** Foreign-ID bugs close structurally via the `pub(crate)` constructor change. Theory-pure stays theory-pure.

## Execution order

1. **Phase 1 — Audit + Validate** (Sections 3 + 4 in parallel, read-only)
2. **Phase 2 — Quick wins** (Section 1.4 doc corrections, Section 1.2 hex swap, Section 1.1 snapshot doc)
3. **Phase 3 — ByteMapping fix** (Section 1.1 phantom fix, ~5 LOC additive)
4. **Phase 4 — Settle Section 5 decisions** (3 remaining: snapshot contract, merkle ordering, hex-on-wire)
5. **Phase 5 — Implement v2.0** (Section 2)
6. **Phase 6 — Version bump + CHANGELOG rename + integration PR to `dev`**
7. **Phase 7 — Consumer migration** (ALIS, koru-protocol)

---

## Section 1 — FIX (confirmed bugs and drift, evidence in hand)

### 1.1 Live bugs in engine core
- [ ] **ByteMapping phantom parents** — `primitives.rs:26–38` builds cache against a throwaway engine. Byte-derived IDs exist in relationships but not in `all_distinctions`. Violates `r = 2d − 3` semantically. *(Exp 5, qa)*
  - Fix: register the 8-step chain into the calling engine on first byte use.
  - Scope: ~5 LOC. Zero in-tree tests break (Exp 5).
- [ ] **Foreign-ID acceptance** — public `Distinction::new(String)` (`engine.rs:13`) lets external callers mint IDs. *(Exp 9, qa)*
  - 1MB ID → 7.3 ms/synth (225× DoS slowdown).
  - Cross-engine ID → silent phantom parent in receiver.
  - Empty / colon-containing strings accepted.
  - **Structural fix = TODO #6 (`pub(crate)`)**. Do not patch with runtime length checks (theory drift).
- [ ] **Snapshot tearing** — `get_state_snapshot` (`engine.rs:154–156`) does two independent DashMap iterations. *(Exp 6, qa)*
  - Real, but 0.06–0.08% rate (not the 16.5% CLAUDE.md claims).
  - When it tears, it tears avalanche-sized.
  - Fix: rename to `get_state_snapshot_unsynchronized()` and document, OR add quiesced variant. Decide before any persistence consumer ships.

### 1.2 Performance / cleanup (no theory impact)
- [ ] **`format!("{:x}", digest)` → `hex::encode(digest)`** in `engine.rs` hot path. *(Exp 15, rust)*
  - 213 ns → 113 ns. ~15% end-to-end synth speedup.
  - 2 LOC, additive, ship as 1.2.x patch.

### 1.3 TODO.md design errors (must be corrected before implementation)
- [ ] **TODO #1 API shape** — current proposal returns `Vec`, probes via `synthesize()`. *(Exp 4 + Exp 8)*
  - `children_of` must return `impl Iterator<Item = Distinction>`. d0 at 1M has 102K children; Vec clone costs 13.3 ms.
  - Reverse index must live **inside** the engine. External index races produce orphans (Exp 8: 37 orphans / 20K sweeps).
  - `parents_of` cannot use the "check `synthesize(d, other) == known_child`" trick — synthesize has side effects. Use a forward index `DashMap<Distinction, (Distinction, Distinction)>` populated at synth time.
  - Underlying child storage: `Vec<Distinction>` (DashSet is 5–60× slower; append-only already prevents duplicates).
  - Cache degree separately in `DashMap<Distinction, AtomicUsize>` for O(1) reads.
- [ ] **TODO #2 log backend** — current proposal is `RwLock<Vec<(String, String)>>`. *(Exp 3)*
  - 26% throughput regression at 8 threads. Mutex is worse (52.7%).
  - Replace with `crossbeam::queue::SegQueue<(Distinction, Distinction)>` (7% at 8 threads, scales with cores).
  - Order-independence of replay confirmed (Exp 7, Exp 12), so SegQueue's weaker ordering is safe.
- [ ] **TODO #2 memory escape hatch** — 1M synths = 81 MB resident log; worst case ~176 B/entry. At 10M = ~1.7 GB.
  - Add `DistinctionEngine::without_log()` constructor or a `log` feature flag.

### 1.4 CLAUDE.md numerical drift (documentation)
- [ ] Memory: `~656 bytes` → `629 B measured at 1M (live heap, dhat)`. *(Exp 1)*
- [ ] Throughput single-thread: `500K–900K/s` → `425–540K/s depending on chain depth`. *(Exp 10)*
- [ ] Throughput 8-thread: `1.85M, 3.5×` → `2.6M, 4.8× on M3 Pro`. *(Exp 10)*
- [ ] Replay: `714K ops/sec` → `450K ordered / 367K shuffled on M3 Pro`. *(Exp 7)*
- [ ] Snapshot tearing: `16.5%` → `rare (~0.1%) but avalanche-sized when it occurs`. *(Exp 6)*
- [ ] v2.0 framing: `clone elimination` → `8× memory density`. Clone is 1.6% of synth cost. *(Exp 11, 16)*

---

## Section 2 — IMPLEMENT (all v2.0, no intermediate release)

All items ship together in 2.0.0. The structural type changes and the additive APIs land in the same release so consumers migrate once.

### 2.1 Type-level changes (structural, theory-strengthening)
- [ ] `Distinction([u8; 16])` — truncated SHA256. Content addressing preserved. *(Exp 13: 0 collisions in 268M; Exp 15: 4.2× faster end-to-end)*
- [ ] `pub(crate)` on the field + private constructor — closes foreign-ID poisoning structurally. No external code can mint IDs.
- [ ] `BuildHasherDefault<IdentityHasher>` on internal DashMaps — 6–13× hash speedup. Safe ONLY with byte keys. *(Exp 14)*
- [ ] FFI: keep public C signatures, swap internal `format!`/parsing for `hex::encode`/`hex::decode`. ~25 LOC. *(Exp 17: 0 signature changes)*

### 2.2 Additive APIs (new public surface)
- [ ] `degree(d: &Distinction) -> usize` — O(1) via `DashMap<Distinction, AtomicUsize>`.
- [ ] `parents_of(d: &Distinction) -> Option<(Distinction, Distinction)>` — forward index populated in synthesize.
- [ ] `children_of(d: &Distinction) -> impl Iterator<Item = Distinction>` — reverse index, in-engine.
- [ ] Streaming `calculate_sis` using the reverse index (removes the full-snapshot clone in compactor).

### 2.3 Synthesis log
- [ ] Append-only log on `SegQueue<(Distinction, Distinction)>` (bytes from day one, not retyped from String).
- [ ] `DistinctionEngine::without_log()` constructor or `log` feature flag for memory-sensitive consumers.
- [ ] Serde derives on log entry type (`bincode` round-trip 25 ms / 14 ms at 1M). *(Exp 7)*

### 2.4 Invariant tripwire
- [ ] `debug_assert!(self.relationship_count() == 2 * self.distinction_count() - 3)` inside `synthesize()` on the novel path. Zero release cost, future-regression tripwire.

### 2.5 Consumer coordination
- [ ] ALIS (`/Users/sawyerkent/Projects/alis-ai/`) — pins `koru-lambda-core = "1.2"`. Bump to 2.0 after engine release.
- [ ] koru-protocol (`/Users/sawyerkent/Projects/koru/`) — same pin, same bump. Check JSON wire formats for embedded distinction IDs.

---

## Section 3 — AUDIT (code paths NOT yet examined for theory drift or correctness)

These subsystems were skimmed for structure during the warroom but never audited for invariant preservation, theory leaks, or correctness under adversarial input. **The project cannot be called "completely theory-aligned" until these are reviewed.**

- [ ] **`src/subsystems/compactor.rs`** (503 LOC) — claimed non-destructive; verify all 11 classification paths preserve append-only.
  - Owner candidate: theory-guardian or engine-architect.
- [ ] **`src/subsystems/validator.rs`** (350 LOC) — blockchain consensus. Does nonce-ordered batch validation construct Distinctions from foreign data? Any path that takes untrusted bytes → ID?
  - Owner candidate: qa-sentinel.
- [ ] **`src/subsystems/network.rs`** (603 LOC) — leader election, epochs, gossip. Network bytes → distinction IDs anywhere?
  - Owner candidate: qa-sentinel.
- [ ] **`src/subsystems/commitment.rs`** (504 LOC) — two-stage gossip + LRU cache. Any post-commit mutation of engine state?
  - Owner candidate: engine-architect.
- [ ] **`src/subsystems/parallel.rs`** (379 LOC) — rayon-based `ParallelBatchProcessor` + `ParallelSynthesizer`. Race conditions when multiple cores synthesize? (Engine itself is reviewed; the parallel wrappers are not.)
  - Owner candidate: rust-craftsman.
- [ ] **`src/ffi.rs`** (899 LOC) — Exp 17 reviewed the **ID surface** (0 signature changes for v2.0). It did NOT review:
  - Memory safety (unsafe blocks, lifetimes of `*mut c_char`).
  - Panic safety across the C boundary.
  - Leak paths (any unfreed allocations?).
  - Error handling under malformed input.
  - Owner candidate: rust-craftsman.
- [ ] **`src/wasm.rs`** (741 LOC) — **completely unaudited**. JS-side ID comparisons (`id === "abc..."`) break post-v2.0. Memory model and panic propagation unreviewed.
  - Owner candidate: rust-craftsman.

---

## Section 4 — VALIDATE (theory claims in CLAUDE.md not reproduced in this repo)

These are imported from the ALIS warroom (5 rounds, 50+ experiments) but were never measured here. They're plausibly true — they're the theory's reason for existing — but for the engine to be self-contained provably theory-aligned, they should be reproduced against this codebase.

- [ ] **Coding Law: degree ↔ usage frequency rho=0.99** — CLAUDE.md asserts this. Not measured locally. Needs experiment that drives synthesis with a known frequency distribution and checks Spearman correlation against final degrees.
  - Owner candidate: research-lead.
- [ ] **Self-reference through mediated observation → infinite novelty** — claim: `synth(synth(self, obs_i), self)` produces unique distinctions at every depth, while direct `synth(self, self)` is irreflexive.
  - Test: assert ID-distinctness across depths 1, 10, 100, 1000.
  - Owner candidate: research-lead.
- [ ] **Fold Law mechanism (depth ≤ 8)** — Exp 4 confirmed d0/d1 are linear-in-N hubs. The depth-8 byte-routing mechanism specifically was not isolated. Need experiment that shows the seed-layer hub effect emerges from ByteMapping's 8-step chain, not from later content concentration.
  - Owner candidate: research-lead.
- [ ] **Cross-engine determinism on arbitrary chains** — Exp 7 showed log replay produces byte-identical state. Stronger version: two cold engines, driven by identical synthesis sequences but never sharing memory, produce byte-identical `all_distinctions` and `relationships` sets at every step.
  - Owner candidate: qa-sentinel.

---

## Section 5 — STILL OPEN (decisions, not work)

- [ ] **Merkle-over-log / log-diffing semantics** — Exp 12 showed canonical ordering is NOT needed for replay. It IS needed if koru-protocol ever hashes the log for consensus or peer-diffs logs. Decide before the log API is finalized.
- [ ] **Snapshot API contract** — pick: rename to `_unsynchronized`, add `_quiesced(barrier)` variant, or document the 0.06–0.08% tear rate at the call site. Required before any persistence consumer adopts it.
- [ ] **Hex on the wire after v2.0** — WASM consumers comparing `id === "abc..."` strings will break if we switch JS-visible IDs to bytes. Decision: keep hex on the wire / serialize bytes / both? Affects Section 3 audit of `wasm.rs`.

**Decided** (record only):
- ~~v1.3 vs v2.0 boundary~~ → single v2.0 cut. One migration for consumers.

---

## Quick stats

| Category | Items |
|---|---|
| Section 1 — FIX (concrete bugs/drift) | 14 |
| Section 2 — IMPLEMENT (all v2.0) | 13 |
| Section 3 — AUDIT (unreviewed code) | 7 subsystems |
| Section 4 — VALIDATE (unreproduced theory) | 4 claims |
| Section 5 — DECIDE (open questions) | 3 |

**To call the project "completely theory-aligned, clean, high-quality, bug-free":**
- All of Section 1 must be closed.
- All of Section 3 must be audited (and any findings closed).
- Section 4 should be closed for the engine to be self-validating.
- Section 2 is the new value being shipped; Section 5 are gating decisions.

## Evidence index

- `experiments/findings/WARROOM_FINDINGS.md` — top-level synthesis
- `experiments/findings/SUMMARY_research_lead.md` — Exp 1, 2, 3, 7, 10, 11
- `experiments/findings/SUMMARY_qa_sentinel.md` — Exp 5, 6, 8, 9, 12
- `experiments/findings/SUMMARY_rust_craftsman.md` — Exp 4, 13, 14, 15, 16, 17
- Reproduction commands in each summary file.
