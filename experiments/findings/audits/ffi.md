# Audit — `src/ffi.rs` (rust-craftsman)

**Scope:** 899 LOC, 22 `pub extern "C" fn`. Memory safety, panic safety, leak paths, error handling, threading. NOT the ID surface (Exp 17 already covered).
**Method:** Static read-only audit. `cargo clippy`/`cargo test` not run (plan mode).
**Verdict:** **2 HIGH severity findings**, both fixable in ~5 lines. No leak paths. No production `.unwrap()` reachable.

---

## Verdicts

| # | Concern | Severity | Site | v2.0 risk |
|---|---|---|---|---|
| F1 | Panic across `extern "C"` boundary unprotected | **HIGH** | All 22 entrypoints | None |
| F2 | TOCTOU on `*mut KoruAgent`; mutators not Sync | **HIGH** | All `unsafe extern "C"` with `*mut KoruAgent` | None |
| F3 | No `panic="abort"`, no `extern "C-unwind"` | HIGH (supports F1) | `Cargo.toml` | None |
| F4 | Opaque types collapse to `c_void` — pointer-type confusion | MEDIUM | ffi.rs:22-28 | None |
| F5 | `cbindgen` `[export]` omits 18/22 functions from header | MEDIUM | cbindgen.toml:14 | None |
| F6 | "Re-increment ref count" comment is wrong; pattern correct by coincidence | MEDIUM (latent footgun) | 109-113, 235, 281, 290, 299, 391, 406, 438, 463, 486, 544 | LOW |
| F7 | `koru_agent_check_commitment` hash parameter is decorative | MEDIUM | 319-351 + commitment.rs:82-84 | None |
| F8 | Safety contracts under-documented | MEDIUM | all `unsafe extern "C"` | None |
| F9 | `batch_len: usize` unbounded — UB if > isize::MAX | LOW | 266, 375 | None |
| F10 | `(null, 0)` treated as null-error vs empty payload | LOW | 269, 378 | None |
| F11 | Deprecated `koru_agent_propose_batch` still exported in `cdylib` | LOW | 421-465 | None |
| F12 | Missing `Send + Sync` static assertion on raw-pointer types | LOW | 22-28 | None |
| F13 | `koru_agent_state_root` needs `hex::encode` post-v2.0 — 2 LOC change | INFO | 160-172 | **YES — only site** |

---

## F1 + F3 — Panic safety (HIGH)

**No `catch_unwind` anywhere. No `panic = "abort"` in Cargo.toml. No `rust-version`.**

Default `panic = "unwind"`:
- Rust ≥ 1.81: unwinding through `extern "C"` forces `abort()` — process dies hard.
- Rust < 1.81: undefined behavior.

Both unacceptable. Today, no `.unwrap()` is reachable from FFI through engine/network/validator/commitment (verified by trace). But that's discipline, not enforcement.

**Single-line fix:**
```toml
[profile.release]
panic = "abort"
[profile.dev]
panic = "abort"
```

Eliminates F1 and F3 regardless of Rust version.

## F2 — TOCTOU on opaque pointers (HIGH)

Header doc (ffi.rs:6-8) claims "All operations are thread-safe via Arc/DashMap." **Half-true.**

- Engine (`*const KoruEngine`) is shared-thread-safe via DashMap.
- **Agents/validators are NOT.** `koru_agent_join_peer` (219), `koru_agent_propose_commitment` (273), `koru_agent_finalize_batch` (382), `koru_agent_advance_epoch` (480), `koru_agent_restore_nonce` (196), `koru_agent_propose_batch` (432) all form `&mut *(agent as *mut NetworkAgent)`. Two concurrent calls on same agent pointer = two `&mut` aliases = **instant UB.**
- Read-only agent calls form shared `&NetworkAgent`. Safe concurrently with other reads. **UB if concurrent with any `&mut` caller.**

Once C holds the `void*`, no internal synchronization prevents `koru_engine_free` racing against `koru_engine_distinction_count` on the same pointer (use-after-free).

**Doc-only fix** (cheap, additive): thread-safety section in crate header explicitly stating engine is shared-safe, agents/validators require exclusive access.

**Code-level fix** (~30 LOC): wrap agent in `Mutex<NetworkAgent>` inside FFI boundary.

## F4 — Opaque type collapse (MEDIUM)

```rust
pub type KoruEngine = c_void;
pub type KoruAgent = c_void;
pub type KoruValidator = c_void;
```

All three are the same C type. Passing `*mut KoruAgent` where `*const KoruEngine` is expected: silently accepted by both Rust (type alias) and C (decays to `void*`). Runtime cast reinterprets bytes — UB.

**Fix** (additive, non-breaking):
```rust
#[repr(C)]
pub struct KoruEngine { _private: [u8; 0] }
#[repr(C)]
pub struct KoruAgent { _private: [u8; 0] }
#[repr(C)]
pub struct KoruValidator { _private: [u8; 0] }
```

cbindgen emits distinct opaque typedefs. C mismatches become C compile errors.

## F5 — cbindgen export omits 82% of FFI surface

`cbindgen.toml:14`:
```toml
include = ["koru_engine_new", "koru_engine_free", "koru_agent_new", "koru_agent_free"]
```

Only 4 of 22 functions appear in generated `koru.h`. The other 18 export as linker symbols but C consumers must hand-roll declarations — easy to mismatch signatures.

**Fix:** remove the `include` list (cbindgen exports every `pub extern "C"` it finds) or extend to all 22.

## F6 — Latent Arc bookkeeping footgun

Pattern at 109-113 (repeated at 220-235, 274-299, 383-406, 433-463, 481-486, 540-544):

```rust
let engine_arc = Arc::from_raw(engine as *const DistinctionEngine);
let agent = Box::new(NetworkAgent::new(&engine_arc));
// Re-increment ref count (we borrowed it)
let _ = Arc::into_raw(engine_arc);
```

**Comment is wrong.** `Arc::from_raw → Arc::into_raw` is refcount-preserving, not incrementing. Correct today only because `NetworkAgent`/`ConsensusValidator`/`CommitmentAgent` **do not store** the Arc (verified: no `engine: Arc<DistinctionEngine>` field).

If any refactor adds `engine: Arc<DistinctionEngine>` as a subsystem field, refcount math is short by one → `koru_engine_free` use-after-frees engine while agent still references it.

**Idiomatic fix** using `ManuallyDrop`:
```rust
let engine_arc = ManuallyDrop::new(Arc::from_raw(engine as *const DistinctionEngine));
let agent = Box::new(NetworkAgent::new(&engine_arc));
Box::into_raw(agent) as *mut KoruAgent
```

Removes the dance entirely. Error paths collapse to plain `return`.

## F7 — `check_commitment` hash is decorative

ffi.rs:319-351 builds a `BatchCommitment` from C-supplied hash + nonce + epoch, calls `agent.check_commitment(&commitment)` which delegates to:

```rust
// commitment.rs:82-84
pub fn verify(&self, expected_nonce: u64, expected_epoch: u64) -> bool {
    self.nonce == expected_nonce && self.epoch == expected_epoch
}
```

**`commitment_hash` is never inspected.** `koru_agent_check_commitment(agent, [random_garbage; 32], 0, 0)` returns 1 at genesis just as readily as the real hash.

Not a memory-safety bug; API correctness/security smell. Function name and docstring imply hash validation; implementation provides none. (Related to network.md N6 — `leader_id` also not in `BatchCommitment::compute` hash.)

## v2.0 migration (refines Exp 17)

Exp 17 concluded: 0 C signature changes, ~25 LOC internal hex transcoding. **This audit confirms and refines.**

| Site | Today | Post-v2.0 |
|---|---|---|
| `koru_agent_state_root` (166) | `agent.consensus_state_root()` returns `&str` | `state_root_id()` returns `[u8;16]`; FFI calls `hex::encode(root)` then `CString::new`. **+2 LOC** |
| `koru_agent_get_leader` (502) | Returns peer `id` (human String) | **Unchanged** — peer id, not a Distinction |
| `propose_commitment`, `finalize_batch`, `propose_batch` JSON | serde parses `TransactionBatch.previous_root: String` (hex today) | Decision in validator.rs / CHECKLIST §5 |
| 22 opaque pointer types | Pointer-sized | **Unchanged** |
| `[u8; 32]` commitment hashes | Raw bytes | **Unchanged** |

**None of F1-F12 complicates the migration.** F4 and F6 would make migration *safer* by removing latent footguns.

### Suggested ordering

1. **Pre-v2.0 (1.x patch):** add `panic = "abort"` (F1, F3), threading-safety doc (F2, F8), fix cbindgen include (F5). All non-breaking.
2. **v2.0:** migrate `Distinction → [u8; 16]`, internal `hex::encode`, typed opaque structs (F4). The opaque-struct change is technically a C-source break (`KoruEngine` becomes `struct KoruEngine` not `void`), so belongs in major bump.
3. **Post-v2.0 hardening:** `ManuallyDrop` refactor (F6).

## Test gaps

11 `#[test]` functions (ffi.rs:577-898). Cover lifecycle, allocation pairing, two-stage commitment, nonce restore, null-pointer safety. **Do NOT cover:**
- Concurrent calls on same `*mut KoruAgent` (would surface F2)
- Free-during-use TOCTOU (F2)
- Mismatched opaque-pointer passing (F4)
- `batch_len > isize::MAX` (F9)
- Panic crossing FFI boundary (F1)
- Hash-tamper on `check_commitment` (F7)

## Bottom line

- **2 HIGH** (F1 panic propagation, F2 TOCTOU + agent-not-Sync). Both addressable with one-line Cargo.toml change (F1) and documentation paragraph (F2). Neither blocks v2.0.
- **6 MEDIUM** (F3-F8).
- **4 LOW** (F9-F12) — defense in depth.
- **0 leak paths.** Allocation/free pairing correct on happy and error paths.
- **0 panic sites reachable today.**
- **0 v2.0 migration complications.** Exp 17's "25 LOC internal" estimate stands; only FFI site that changes is `koru_agent_state_root` (+2 LOC).

**Single highest-leverage fix:** `panic = "abort"` (one line). Eliminates F1 and F3 regardless of Rust version or future `.unwrap()` slippage.

**Second-highest:** thread-safety docstring (one paragraph). Eliminates F2/F8 confusion.

**~5 lines total → eliminates both HIGH findings.**
