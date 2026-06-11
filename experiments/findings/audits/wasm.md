# Audit — `src/wasm.rs` (rust-craftsman)

**Scope:** 741 LOC, feature-gated (`wasm` feature → `wasm-bindgen` + `serde-wasm-bindgen` + `js-sys`). Never audited before.
**Method:** Static read-only audit. `cargo build --features wasm` / `cargo clippy` / `cargo test` not run (plan mode).
**Verdict:** **1 CRITICAL** + **1 HIGH** finding. All wire-format related. **Zero memory safety issues. Zero `unsafe`. Zero production `.unwrap()`.**

---

## Verdicts

| ID | Issue | Severity | Lines |
|---|---|---|---|
| W1 | Wire-format heuristic `id.len() == 64` breaks at v2.0 | **CRITICAL** | 379-387 |
| W2 | Primordial IDs leak as `[0x30]`/`[0x31]` UTF-8 bytes, not 32-byte hashes | **HIGH** | 57-66, 379-387 |
| W3 | `Result<_, JsValue>` returns with no error path | LOW | 142, 151, 163 |
| W4 | `hex::decode(id).unwrap_or_else(...)` silently falls back to UTF-8 on bad hex | **HIGH** | 382 |
| W5 | No panic hook — Rust panics surface as opaque `RuntimeError` | MEDIUM | (missing) |
| W6 | No `#[wasm_bindgen(start)]`, no `Default` impl on `WasmEngine` | LOW | 38, (missing) |
| W7 | `previous_root: String` JSON wire — v2.0 decision pending | MEDIUM | 199, 245, 304 |
| W8 | `Vec<String>` parameter forces JS→Rust string copies | LOW | 151 |
| W9 | `synthesize(&str, &str)` accepts JS strings (Exp 9 class — safe today via lookup) | MEDIUM | 71-85 |
| W10 | `check_commitment` builds Frankenstein `BatchCommitment { leader_id: "", batch_size: 0 }` | MEDIUM | 212-235 |
| W11 | `benchmarkSynthesis` is idempotent-only — misnamed | LOW | 95-97 |
| W12 | `WasmCommitmentAgent` lacks engine field | LOW | 343-374 |
| W13 | Tests are `#[test]`, not `#[wasm_bindgen_test]` — never hit WASM runtime | MEDIUM | 405-741 |
| W14 | `currentRoot()` allocates twice per call | LOW | 129, 290, 359 |
| W15 | `validate_batch` rejection collapses to opaque string | LOW | 311-313 |
| W16 | `benchmark_validation` mutates real state | LOW | 320-332 |
| W17 | Hex helper duplicated | LOW | 390-402 |

**Zero `unsafe` blocks. Zero raw pointers. WASM linear memory managed by wasm-bindgen codegen.**

---

## W1 (CRITICAL) — Wire format heuristic breaks at v2.0

```rust
// wasm.rs:379-387
fn id_to_bytes(id: &str) -> Vec<u8> {
    if id.len() == 64 {                                  // SHA256 hex
        hex::decode(id).unwrap_or_else(|_| id.as_bytes().to_vec())
    } else {                                              // Primordial / short
        id.as_bytes().to_vec()
    }
}
```

Central wire converter, called from **8 exported entry points** (every `*Root()`, `d0Id()`, `d1Id()`, `synthesize()` return, `joinPeer()`, `advanceEpoch()`, `finalizeBatch()`, `validateBatch()`).

Post-v2.0 (`Distinction([u8; 16])`):
- Hex returned by `id()` is 32 chars, not 64.
- `id.len() == 64` branch never fires.
- Every distinction falls through to "Primordial or short" → returns **UTF-8 bytes of the hex string** (32 bytes of ASCII characters, not 16 hash bytes).
- JS consumers comparing bytes get garbage. No error signaled.

**Fix path:**
- If hex-on-wire preserved (CHECKLIST §5): change `64` to `32`, decode produces 16 bytes.
- Better: kill the heuristic. Expose `Distinction::as_bytes() -> &[u8; 16]`; `id_to_bytes` collapses to one `.to_vec()`. Primordials get real 16-byte IDs; special case disappears.

## W2 (HIGH) — Primordials leak as raw UTF-8 bytes

`engine.d0().id()` returns `"0"`. `engine.d1().id()` returns `"1"`. `id_to_bytes` else-branch returns `id.as_bytes().to_vec()` → `[0x30]` for d0, `[0x31]` for d1.

JS cannot meaningfully compare primordial to synthesized child via `Uint8Array` equality:
- `d0Id()` → `Uint8Array([48])`
- `synthesize(d0_id_hex, d1_id_hex)` → `Uint8Array([..32 hash bytes..])`

**Never byte-equal**, yet both represent valid distinctions.

Existing tests at wasm.rs:422 appear to encode the bug:
```rust
assert_eq!(to_hex(&wasm_engine.d0_id()), native_engine.d0().id());
```
`to_hex(&[0x30])` = `"30"`. `native_engine.d0().id()` = `"0"`. **Should fail at runtime.** Tests likely never compiled under `--features wasm` (W13).

v2.0 fix: primordials get real 16-byte IDs (e.g., `[0u8; 16]` and `[1u8; 16]` or domain-separated hashes). Special-case disappears.

## W4 (HIGH) — Silent hex fallback to UTF-8

`hex::decode(id).unwrap_or_else(|_| id.as_bytes().to_vec())` (382).

64-char string with non-hex chars → silently returns UTF-8 bytes (64 ASCII bytes) instead of 32 hash bytes. No error. Fail-open on a content-addressed substrate is wrong.

Fix: propagate hex errors as `JsValue::from_str(...)`. Function should fail loudly.

## W5 (MEDIUM) — No panic hook

Rust panic on wasm32 → `unreachable` trap → JS `RuntimeError: unreachable executed`. **Not UB** (wasm32 guarantees memory safety on trap), but opaque (no Rust file/line/message).

Fix:
```rust
#[wasm_bindgen(start)]
pub fn init() {
    console_error_panic_hook::set_once();
}
```
Add `console_error_panic_hook` under the `wasm` feature.

## W7 (MEDIUM) — JSON wire format depends on §5 decision

JSON wire embeds distinction IDs as strings. Post-v2.0:
- Hex-on-wire (likely): `Distinction.id()` returns 32-char hex, JSON stays `String`, **wire-compatible**.
- Bytes-on-wire: JSON must use base64 / explicit hex.

**Hex-on-wire keeps internal wasm.rs changes small** (≤ 10 LOC in `id_to_bytes`). Decide §5 before further wasm.rs work.

## W9 (MEDIUM) — `synthesize` lookup-first pattern

```rust
let a = self.inner.get_distinction_by_id(id_a)
    .ok_or_else(|| JsValue::from_str(&format!("Distinction not found: {}", id_a)))?;
```

Today: lookup-first short-circuits foreign-ID exposure. Unregistered IDs return `None`, surface as JS error. **WASM does not amplify the Exp 9 problem.**

Post-v2.0 `pub(crate)`: pattern stays correct.

**However:** wire format does not round-trip. `id_a` came from `d0Id()` (returns `[0x30]` bytes, see W2), but engine's `d0` ID is `"0"`, not `"30"`. Inputs/outputs use different encodings.

Fix at v2.0: `synthesize(&[u8], &[u8])`. Symmetric.

## W10 (MEDIUM) — Frankenstein `BatchCommitment`

```rust
let commitment = BatchCommitment {
    commitment_hash: hash, nonce, epoch,
    leader_id: String::new(),    // empty
    batch_size: 0,                // zero
};
Ok(self.inner.check_commitment(&commitment))
```

Safe today — `NetworkAgent::check_commitment` only inspects nonce/epoch. Risk: if anyone ever swaps `verify` for `verify_batch` or adds hash-verification, this path silently breaks. Plus reinforces network.md N6 finding (`leader_id` not in `compute` hash).

Fix: add `leader_id: &str`, `batch_size: usize` parameters or narrower API.

## W13 (MEDIUM) — Tests don't hit WASM runtime

All 14 tests are `#[test]`, run on host arch. Do NOT exercise JS shim, `Uint8Array` conversion, exception bridge. Several look broken on static read (encoding W2 bug).

Standard practice: `wasm-bindgen-test` + `#[wasm_bindgen_test]` + `wasm-pack test --node` or `--headless --firefox`.

## v2.0 migration summary

| Today | v2.0 | Effort |
|---|---|---|
| `id_to_bytes` heuristic | `Distinction::as_bytes().to_vec()` | 10 LOC |
| Primordials as `"0"` / `"1"` | Real 16-byte IDs | upstream (engine) |
| `synthesize(&str, &str)` | `synthesize(&[u8], &[u8])` | 20 LOC — **breaking on JS** |
| `previous_root: String` JSON | Keep if hex-on-wire | 0 LOC if hex preserved |
| `BatchCommitment` Frankenstein | Add proper params | 10 LOC + upstream |
| Tests asserting hex(d0_id) == "0" | Update for real bytes | 5 LOC |
| Missing hook, Default, host-only tests | Add hook, Default, wasm-bindgen-test | 30 LOC + Cargo.toml |

**Total wasm.rs churn at v2.0:** ~75 LOC. Critical path: W1 + W2.

## Theory drift (Q5)

- `WasmEngine::synthesize` accepts JS strings but lookup via `get_distinction_by_id` rejects unknown → theory-clean.
- `PeerIdentity::new(peer_id.to_string(), &engine)` at 143, 154 synthesizes canonical structure from string seed → not foreign-ID poisoning.
- `serde_json::from_str::<TransactionBatch>` at 199, 245, 304 takes `previous_root: String`; validator rejects mismatches → theory-clean.

**Net:** WASM does not introduce theory drift beyond engine core. Wrapper is disciplined.

## Memory & panic safety

- **Zero `unsafe`** confirmed by grep.
- All linear-memory marshalling is wasm-bindgen codegen.
- `&[u8]` parameters borrow into linear memory for call duration. Length check on `hash_bytes` (219) happens BEFORE `copy_from_slice` (224). No OOB.
- **All `.unwrap()` inside `#[cfg(test)]`.** Production code is unwrap-free.
- Panic on wasm32 traps, not UB. Default trap is opaque (no message).

## Recommendations, ranked

1. **W1 + W2 (CRITICAL/HIGH):** kill `id_to_bytes` heuristic. Decide CHECKLIST §5 first.
2. **W4 (HIGH):** make `id_to_bytes` fail-closed on bad hex.
3. **W13 (MEDIUM):** convert to `wasm-bindgen-test`. Verify host tests pass.
4. **W5 (MEDIUM):** add `console_error_panic_hook` + `#[wasm_bindgen(start)]`.
5. **W7 (MEDIUM):** resolve JSON wire alongside W1.
6. **W9 (MEDIUM):** `synthesize(&[u8], &[u8])` at v2.0.
7. **W10 (MEDIUM):** proper params on `checkCommitment`.
8. **W3, W6, W11-W17 (LOW):** cleanup, ergonomics.

## Bottom line

Nothing in wasm.rs is theory-drifting. Nothing is memory-unsafe. No `unsafe`, no production `.unwrap()`. The bugs are all wire-format and ergonomics — exactly the class CHECKLIST §5 anticipates. v2.0 plus a CHECKLIST §5 decision cleans this up in ~75 LOC.
