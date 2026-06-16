# Security Policy

This document describes the security posture of `koru-lambda-core` and the
process for reporting vulnerabilities.

## Supported Versions

| Version | Supported | Notes |
|---|---|---|
| `2.0.x` | ✅ Yes | Current major. All v2.0 hardening applied. |
| `1.2.x` and earlier | ❌ No | Three demonstrated consensus-correctness bugs (N5, N6, V5) and a class of FFI / foreign-ID poisoning vectors. **Do not deploy to a network.** Migrate to `2`. |

`koru-lambda-core` is not deployed to a public network as of the v2.0 release,
so the v1.2.x bugs below did not affect production peers. They are nonetheless
real and would have been exploitable had v1.2.x shipped on a network.

## v2.0 Security-Relevant Changes (Tier 0)

All three of the following were demonstrated by reproducible probes in v1.2.0
(see `experiments/findings/run_log/`) and are closed in v2.0. The probes have
been re-run against the v2.0 branch and all confirm the closures.

### N5 — `NetworkAction::BatchProposed` `previous_root` truncation

**Class:** Consensus-determinism / causal-chain collision.

**Description:** v1.2.0's `NetworkAction::BatchProposed::to_canonical_structure`
applied `.take(8)` to `previous_root.as_bytes()` before folding. Any two
distinct chain heads sharing an 8-character hex prefix folded to the same
action distinction — two valid forks could converge on identical action IDs.

**Fix:** Parses `previous_root` via `Distinction::from_hex` and synthesizes the
real parent root. Malformed inputs fall through a deterministic sentinel that
cannot collide with any well-formed root. The validator's separate
`previous_root` check rejects malformed inputs upstream on the happy path.

**Evidence:** `experiments/findings/run_log/audit_network_foreign_peers.log`
(v1.2.0) vs `…_post_n5_n6_v5_fix.log` (v2.0).

### N6 — `BatchCommitment::compute` leader-id forgery

**Class:** Cryptographic attribution forgery.

**Description:** v1.2.0's `BatchCommitment::compute` did not hash `leader_id`
into the commitment hash. The same batch with the same nonce and epoch
produced byte-identical `commitment_hash` regardless of who claimed to have
proposed it. Any peer could swap the `leader_id` field on the wire without
invalidating the commitment.

**Fix:** `leader_id.as_bytes()` is now hashed as the final input to the
SHA-256 digest. `BatchCommitment::verify_batch` re-derives the hash via the
same path, so a tampered `leader_id` on a deserialized commitment fails
verification.

**Evidence:**
`experiments/findings/run_log/audit_network_commitment_unbound.log` (v1.2.0:
honest and forged commitments produce identical hash) vs the post-fix log
(`hashes equal? false`). Test: `verify_batch_rejects_tampered_leader_id`.

### V5 — Validator atomic-failure engine leakage

**Class:** Validator-engine state divergence + DoS amplifier.

**Description:** v1.2.0's `ConsensusValidator::validate_batch` synthesized
each transaction's canonical structure mid-loop and only rolled back the
validator's `local_root` / `expected_nonce` fields on rejection. Distinctions
synthesized from the valid prefix of a rejected batch leaked into the engine
permanently. The "atomic failure" docstring was misstated.

The leak is **deterministic across nodes** — every honest node would leak
identically — so consensus determinism itself was preserved. But the leak
constitutes a DoS amplifier (attacker submits a batch with a bad nonce at
position N to force N permanent distinctions per batch) and a
validator-engine consistency issue.

**Fix:** `validate_batch` now runs a pre-validation pass over the entire
batch (previous_root match + non-empty + nonce sequence + V3 data-length cap)
**before** any `engine.synthesize` call. The commit pass only runs if
pre-validation passes.

**Evidence:** `experiments/findings/run_log/exp_validator_audit.log` Section
C (4 distinctions leaked into engine on a 3-tx out-of-order rejection in
v1.2.0) vs the post-fix log (`engine distinction delta = 0`).

## v2.0 Hardening (Tier 1 — defense in depth)

The following are not exploitable consensus bugs in v1.2.0, but were closed
in v2.0 as part of the same effort. They harden the surface against
DoS-amplification patterns and remove undefined-behaviour vectors at the FFI
boundary.

| ID | Closure | Source |
|---|---|---|
| N1 | `PeerIdentity::new` rejects ids longer than 64 bytes (was unbounded; 1 MB id = 1.85 s of CPU + 1 M permanent distinctions per join) | `audit_network_foreign_peers` A |
| N2 | `PeerIdentity::new` rejects empty ids (was collapsing to Δ₀ — primordial impersonation) | `audit_network_foreign_peers` B |
| N3 / N4 | Foreign-ID poisoning closed structurally by `Distinction` `pub(crate)` constructor; no public `Distinction::new` | Foundation #1 |
| N7 | `NetworkAgent::join_peer` dedupes on joint `(id, distinction)` key — same basis as leader hash | source-only audit |
| N11 | `pending_commitments` is now a bounded `LruCache` (cap 256); cleared on epoch advance | source-only audit |
| V3 | `TransactionAction.data` capped at 4 KiB; pre-validation rejects oversized payloads | `exp_validator_audit` B |
| V4 | Rejection messages clip oversized `previous_root` to a 64-char prefix (no more 1:1 amplification) | `exp_validator_audit` A |
| V6 | `ConsensusValidator::restore_state(engine, root_id, nonce)` replaces the partial-update pair `from_root` + `set_expected_nonce`; verifies root is registered in engine | `exp_validator_audit` G |
| F1 / F3 | `panic = "abort"` on release profile — no UB from unwinding across `extern "C"` | source-only audit |
| F2 / F8 | FFI agent / validator handles wrap `Box<Mutex<...>>` — concurrent C-thread calls serialize internally; no `&mut` aliasing UB | source-only audit |
| F4 | Opaque types are distinct `#[repr(C)] struct { _private: [u8; 0] }` — C pointer-type confusion fails at compile time | source-only audit |
| F6 | FFI engine borrows use `ManuallyDrop<Arc<DistinctionEngine>>` — no panic-window ref-count leak | source-only audit |
| F7 | `koru_agent_check_commitment` requires real `leader_id` + `batch_size`; no "Frankenstein commitment" with empty fields | source-only audit |
| F9 | Length-taking FFI entry points reject `len > isize::MAX` before `slice::from_raw_parts` | source-only audit |
| W10 | WASM `checkCommitment` requires real `leader_id` + `batch_size` (mirror of F7) | source-only audit |

Full per-item evidence and post-fix logs in `experiments/findings/`.

## Migration from v1.2.x to v2.0.0

Pin `koru-lambda-core = "2"`. The v2.0 release is a single bundled major
version: there is no v1.3.x patch line for the N5 / N6 / V5 bugs. Consumers
running v1.2.x in any networked context must migrate.

All breaking changes are documented in [`CHANGELOG.md`](CHANGELOG.md) under
`## 2.0.0`. The highest-impact migrations:

- `Distinction::id() -> &str` removed → use `as_bytes() -> &[u8; 16]` or
  `to_hex() -> String`.
- `Distinction::new(String)` removed; no public constructor → use
  `engine.synthesize`, `engine.d0()` / `d1()`, `engine.get_distinction_by_id`,
  or `Distinction::from_hex`.
- `PeerIdentity::new` now returns `Result<Self, String>` — empty / oversized
  ids return `Err`.
- `ConsensusValidator::from_root` + `set_expected_nonce` removed → use
  `restore_state(engine, root_id, expected_nonce)`.
- `NetworkAgent::restore_consensus_validator_nonce` removed → use
  `restore_consensus_validator_state(engine, root_id, nonce)`.
- FFI: `koru_agent_restore_nonce` removed → `koru_agent_restore_state`.
  `koru_agent_check_commitment` gains `leader_id` + `batch_size` args. C
  opaque types are now `struct KoruEngine` / `struct KoruAgent` /
  `struct KoruValidator` (was `void *`).
- WASM: `WasmEngine.synthesize` takes `Uint8Array` arguments (was hex
  strings); `WasmEngine.d0Id()` / `d1Id()` return raw 16-byte arrays (was
  UTF-8 hex strings); `WasmNetworkAgent.checkCommitment` gains `leader_id` +
  `batch_size`. Use the new `idToHex` / `idFromHex` JS helpers for display.
- `StructuralCompactor::new(engine)` → `new(engine, hot_threshold,
  warm_threshold)`. `set_hot_threshold` removed → `set_thresholds(hot, warm)`.
  `CompactionAction.archived_ids` field removed.
- `ParallelBatchProcessor` removed entirely (was a single-variant
  Sequential-body wrapper). Use `ConsensusValidator` directly.
- `ParallelSynthesizer` renamed to `BatchSynthesizer`; return type is
  `Vec<Option<Distinction>>` instead of `Vec<String>` with silent
  empty-string fallback.
- `DistinctionEngine::get_state_snapshot` renamed to
  `get_state_snapshot_unsynchronized` — every caller must acknowledge the
  tearing behaviour at the call site.

The full list, with rationales and Decision IDs, is in the v2.0 entry of
`CHANGELOG.md`.

## Reporting a Vulnerability

Please report security issues privately. Open a draft security advisory on
the GitHub repository, or email `sawyerkent.me@gmail.com` directly. Include:

- A minimal reproducer (or the relevant subsystem and conditions).
- Affected versions.
- Suggested severity (consensus-correctness, DoS amplifier, UB, etc.).

We aim to acknowledge within 48 hours and to ship a fix within 30 days for
consensus-correctness or UB issues, longer for hardening / defense-in-depth
items.

Do not file public GitHub issues for security-sensitive reports.
