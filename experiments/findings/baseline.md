# Baseline Measurement — 2026-06-11

**Branch:** `research/warroom-experiments`
**Base commit:** `8fa0e28` (no `src/` changes since)
**Toolchain:** rustc 1.91.0, cargo release profile.

This baseline establishes the known-good state before any Phase 2 fixes land. "No warnings, all tests pass" has a meaning only relative to this baseline.

---

## Summary

| Crate / Feature | Build | Tests | Clippy | Status |
|---|---|---|---|---|
| `koru-lambda-core` (no features) | ✅ release | **103 pass / 0 fail** | **clean** | **GREEN** |
| `koru-lambda-core --features wasm` | ✅ library | **2 wasm tests FAIL/PANIC** | **fails to compile** `tests/falsification/wasm_consistency.rs` | **RED** |
| `experiments/runner` | ✅ | (binaries) | warnings in pre-existing exp01–10; **new exp18, exp19, exp20 clean** | partial |
| `experiments/qa` | ✅ | (binaries) | warnings in pre-existing exp06, 08, 12; **new exp21 + 4 audit probes clean** | partial |

---

## Main crate `koru-lambda-core` (no features) — GREEN

### `cargo test --release`

```
test result: ok. 54 passed; 0 failed   (lib unit tests)
test result: ok.  8 passed; 0 failed   (integration)
test result: ok.  4 passed; 0 failed   (integration)
test result: ok. 16 passed; 0 failed   (integration)
test result: ok.  9 passed; 0 failed   (integration)
test result: ok.  6 passed; 0 failed   (performance_validation)
test result: ok.  5 passed; 0 failed   (throughput_verification)
test result: ok.  1 passed; 0 failed   (doc-test)

TOTAL: 103 tests pass.
```

CLAUDE.md claims "114 tests, zero warnings" — actual count is 103. Either CLAUDE.md is stale or 11 tests have been removed since it was written. Recommend updating CLAUDE.md to "103 tests" as part of Section 1.4 doc drift fixes.

### `cargo clippy --all-targets --release`

```
Finished `release` profile [optimized] target(s) in 4.21s
```

No warnings. Confirms CLAUDE.md's "clippy clean at default level."

---

## Main crate with `--features wasm` — RED

### `cargo build --features wasm --release`

Library builds successfully. Compiles wasm-bindgen + js-sys + serde-wasm-bindgen. Finished in 7.54s.

### `cargo test --features wasm --release --lib wasm`

**FAILURES:**

1. **`test_wasm_engine_primordial_consistency` FAILED** — confirms audit W2 prediction. The test asserts `to_hex(&wasm_engine.d0_id()) == native_engine.d0().id()`. With `d0()` returning `"0"` (string), `id_to_bytes` falls through to `id.as_bytes().to_vec()` → `[0x30]`. `to_hex([0x30]) == "30"`. Native `"0" != "30"`.

2. **`test_wasm_validator_atomic_failure` PANICKED** — confirms audit W13 prediction. wasm-bindgen runtime APIs (`__wbindgen_object_drop_ref`) are stubbed with `panic!("function not implemented on non-wasm32 targets")` on host. Tests labeled `#[test]` instead of `#[wasm_bindgen_test]` cannot exercise the WASM ABI plumbing. Process aborts with SIGABRT.

3. **Passing tests** (sample):
   - `test_wasm_hex_decoding_correctness` ✅
   - `test_wasm_synthesis_determinism` ✅
   - Others not enumerated due to abort before completion

### `cargo clippy --features wasm --all-targets --release`

**BUILD FAILURE** in `tests/falsification/wasm_consistency.rs` (lines 370–372):

```rust
println!("    Network:    {}", &net_r0[..16]);
println!("    Validator:  {}", &val_r0[..16]);
println!("    Commitment: {}", &com_r0[..16]);
```

`[u8]` does not implement `std::fmt::Display`. 18 errors + 10 warnings. The test code expects `*Root()` to return a hex string, but per audit W2 it currently returns `Vec<u8>`. The test was written against a wire-format that changed (or was never coherent — wasm.rs:379-387 heuristic). This is independent of our v2.0 work and predates it.

**Implication:** the wasm feature has never been verified clean. The `cargo clippy --features wasm` failure is a hard blocker for the "zero warnings" promise on the wasm feature. Fix is in Section 1.8 of CHECKLIST (kill `id_to_bytes` heuristic; choose hex-on-wire OR bytes-on-wire and stick to it).

---

## `experiments/runner` crate

### `cargo clippy --release --all-targets` summary

Pre-existing warnings (NOT new code):
- `exp01_memory.rs` — 2 warnings (likely `dead_code` from unused `common::*` import + `manual_div_ceil` or `type_complexity`)
- `exp02_invariants.rs` — 2 warnings
- `exp03_log_ab.rs` — 3 warnings (`type_complexity` × 2 on `RwLock<Vec<(String,String)>>`, `manual_div_ceil`)
- `exp07_replay.rs` — 2 warnings
- `exp10_throughput.rs` — 1 warning (`manual_div_ceil`)
- `common.rs` — `dead_code` on `build_chain_engine` (only some binaries use it)

**Our new files:**
- `exp18_coding_law.rs` ✅ **clean**
- `exp19_mediated_self_reference.rs` ✅ **clean**
- `exp20_fold_law.rs` ✅ **clean** (after removing unused `degree_map` helper)

---

## `experiments/qa` crate

### `cargo clippy --release --all-targets` summary

Pre-existing warnings (NOT new code):
- `exp06_tearing.rs` — 2 warnings
- `exp08_concurrent_read.rs` — 1 warning
- `exp12_log_commutativity.rs` — 10 warnings (`type_complexity` on `HashMap<(String,String), i64>` × multiple, `manual_div_ceil`)

**Our new files:**
- `exp21_cross_engine_determinism.rs` ✅ **clean** (after 7 fixes: 6 hex-literal byte groupings + 1 type alias)
- `exp_validator_audit.rs` ✅ **clean**
- `audit_network_foreign_peers.rs` ✅ **clean**
- `audit_network_commitment_unbound.rs` ✅ **clean**
- `audit_network_concurrency.rs` ✅ **clean**

---

## Fixes applied during baseline establishment

In `exp21_cross_engine_determinism.rs`:
- `0xDEC0DE_DEEPu64` → `0x00DE_C0DE_DEAF_u64` (`P` is not valid hex; compile error)
- 5 other hex literals reformatted to even-byte groups (clippy `unusual_byte_groupings`)
- `Vec<Vec<(usize, usize)>>` extracted to `type IdxPair = (usize, usize)` (clippy `type_complexity`)

In `exp20_fold_law.rs`:
- Removed unused `degree_map` helper function (`dead_code`)

---

## Implications for "no warnings, all tests pass" bar

The "no warnings, all tests pass" promise applies to:
1. **Main crate (no features)** — already met. Don't regress.
2. **Main crate `--features wasm`** — **currently RED**. Must be fixed before v2.0 ships. Section 1.8 work.
3. **Experiments crates** — pre-existing warnings exist in non-new code. Either:
   - (a) Fix them as cleanup (low priority, ~20 warnings of `type_complexity` + `manual_div_ceil`)
   - (b) Scope the promise to `src/` + `tests/` only; experiments are throwaway harness.
   - Recommended: (a) for hygiene, but not blocking v2.0.

---

## Open questions surfaced by baseline

- [ ] CLAUDE.md says "114 tests" but actual is 103. Either remove 11 tests, add 11, or update doc. **Add to Section 1.4.**
- [ ] `tests/falsification/wasm_consistency.rs` has 18 compile errors under `--features wasm`. Was this ever working? Or has the wire format changed underneath?
- [ ] Should `cargo test --features wasm` be replaced with `wasm-pack test --node` (per audit W13)? If yes, the host-side `#[test]` blocks in `wasm.rs` should be migrated.

---

## Reproduction

```bash
# Main crate baseline
cd /Users/sawyerkent/Projects/koru-lambda-core
cargo test --release
cargo clippy --all-targets --release

# WASM feature baseline (currently red)
cargo build --features wasm --release
cargo test --features wasm --release --lib wasm
cargo clippy --features wasm --all-targets --release

# New experiment files (clean)
cargo clippy --release --all-targets --manifest-path experiments/runner/Cargo.toml \
    --bin exp18_coding_law --bin exp19_mediated_self_reference --bin exp20_fold_law
cargo clippy --release --all-targets --manifest-path experiments/qa/Cargo.toml \
    --bin exp21_cross_engine_determinism --bin exp_validator_audit \
    --bin audit_network_foreign_peers --bin audit_network_commitment_unbound \
    --bin audit_network_concurrency
```
