# Audit — `src/subsystems/network.rs` (qa-sentinel)

**Scope:** 603 LOC. `NetworkAgent`, leader election, epochs, gossip.
**Method:** Static read-only audit. `cargo clippy`/`cargo test` not run (plan mode).
**Verdict:** **CRITICAL FINDINGS.** Five foreign-ID vectors, `leader_id` not bound to commitment hash (forgeable), `previous_root` truncation to 8 bytes (causal-chain collision), undocumented concurrency contract.

---

## Verdicts

| # | Finding | Severity | Lines |
|---|---|---|---|
| N1 | `PeerIdentity::new` accepts unbounded peer-id bytes, folds into engine via byte chain | **HIGH** | network.rs:30-39 |
| N2 | Empty peer-id ("") collapses to `engine.d0()` — **primordial impersonation** | **HIGH** | network.rs:33 |
| N3 | `NetworkAgent::from_state` accepts unvalidated `Distinction` as root | **HIGH** | network.rs:147-164 |
| N4 | `LocalCausalAgent::update_local_root` accepts unvalidated `Distinction` | **HIGH** | network.rs:404-406 |
| N5 | `NetworkAction::BatchProposed` canonicalizes only **first 8 bytes** of `previous_root` — chain-head prefix collision | **HIGH** | network.rs:73-78 (`.iter().take(8)`) |
| N6 | `BatchCommitment::compute` does NOT hash `leader_id` — **leader identity is forgeable post-commitment** | **HIGH** | commitment.rs:46-58 |
| N7 | Leader election: hash uses `distinction_id()`, dedupe uses `peer.id` (String). Different ids, same distinction = both seat. | MEDIUM | network.rs:175 vs 309-310 |
| N8 | No internal sync; `&mut self` everywhere; "network consensus" advertised. Embedder must externally lock; undocumented. | MEDIUM | full file |
| N9 | `BatchCommitment::compute(..., leader_id)` takes leader_id by value, ignores it for hash | LOW (footgun) | commitment.rs:46 |
| N10 | `propose_commitment` epoch field is self-asserted, no peer-verifiable signature binding | MEDIUM | network.rs:202-210 |
| N11 | `pending_commitments` unbounded growth on never-finalized proposals | LOW (mem DoS) | network.rs:121 |

---

## Distinction construction sites in network.rs

| Line | Construction | Provenance | Reaches `engine.synthesize` as parent? |
|---|---|---|---|
| 33 | `engine.d0().clone()` (fold init) | engine-derived | yes |
| 34 | `byte.to_canonical_structure(engine)` per byte of peer-id | **peer-derived** (caller String / wire bytes) | yes |
| 35 | `engine.synthesize(&acc, &byte_d)` | mixed | yes |
| 65 | `engine.d1().clone()` (join_marker) | engine | yes |
| 66 | `engine.synthesize(&join_marker, &peer.distinction)` | mixed | yes |
| 70 | `engine.d0().clone()` (batch_marker) | engine | yes |
| 74-78 | fold over `batch.previous_root.as_bytes()[..8]` (**TRUNCATED**) | **peer-derived** | yes |
| 80 | `engine.synthesize(&batch_marker, &root_distinction)` | mixed | yes |
| 84-91 | fold over `new_epoch.to_le_bytes()` | engine-derived (but `from_state` lets caller seed any u64) | yes |
| 128-132 | `engine.synthesize(d0, d1)` genesis | engine | yes |
| **148** | `local_root: root` from `from_state(root: Distinction, ...)` | **CALLER-SUPPLIED, UNCHECKED** | no — but becomes left operand of all future syntheses |
| 394-395 | `engine.synthesize(&self.local_root, &action_distinction)` | mixed (left operand can be forged via 148/405) | yes |
| **405** | `self.local_root = new_root` from `update_local_root(new_root: Distinction)` | **CALLER-SUPPLIED, UNCHECKED** | no — same as 148 |

**Five peer-derived entry points reach `synthesize()` as operands. Two paths bypass `synthesize()` entirely and just write a forged ID into the agent.**

## N1, N2 — Peer identity attacks

**N1 (DoS):** `PeerIdentity::new(huge_id, &engine)` folds one synthesis per input byte. 1 MB peer-id → 1M syntheses → ~2s CPU per join + 1M permanent distinctions per peer.

**N2 (impersonation):** `PeerIdentity::new("".to_string(), &engine)` folds zero bytes. Fold returns the seed `engine.d0().clone()`. The peer's `distinction_id()` is `"0"`. They appear in `validator_set`, contribute to leader-election hashing, and have impersonated the primordial.

## N3, N4 — Forged root acceptance

`from_state(root: Distinction, ...)` (network.rs:147-164): persistence-load. No provenance check. Stored as `local_root`. Reported by `get_stats().network_root`. All subsequent `engine.synthesize(&self.local_root, ...)` calls (network.rs:395) create real children whose parent is the forged ghost.

`update_local_root(new_root)` (network.rs:404-406): trait method, must be public. Any `&mut NetworkAgent` holder can swap root.

**v2.0 `pub(crate)` closes both at compile time.**

## N5 — `previous_root` 8-byte truncation (causal-chain collision)

```rust
// network.rs:73-78
let root_bytes = batch.previous_root.as_bytes();
let root_distinction = root_bytes.iter()
    .take(8)                       // <-- HERE
    .fold(engine.d0().clone(), |acc, b| { ... });
```

`.take(8)` discards everything past byte 8. SHA256 hex ids are 64 chars. Only the first 8 ASCII chars feed canonicalization. **Two completely different chain heads sharing a hex-prefix produce identical action distinctions** — identical post-synthesis network roots. The causal chain is supposed to be tamper-evident; every byte after 8 doesn't matter.

This is correctness, not just security. Two distinct causal histories converge.

## N6 — `BatchCommitment::compute` does not hash `leader_id`

`commitment.rs:46-58`: the SHA256 hasher consumes `batch_root || nonce || epoch`. `leader_id` is stored in the struct but **excluded** from `commitment_hash`. `verify_batch` (commitment.rs:90-93) recomputes via `Self::compute(batch, nonce, epoch, self.leader_id.clone())` — `compute` ignores it again.

**Result:** An attacker can flip `leader_id` to any string in flight; `verify_batch` still returns true. Leader attribution is not cryptographically bound to the batch.

Trivial fix: include `leader_id.as_bytes()` in the hasher.

## N7 — Validator dedupe mismatch

`join_peer` dedupes on `peer.id` String equality (network.rs:175). `get_current_leader` hashes `peer.distinction_id()` (network.rs:310). Different `id` strings whose byte folds collide on the same distinction both seat AND both contribute `distinction_id().as_bytes()` twice to the leader-election hash. Combined with N2, an attacker with `id=""` doubles their hash weight.

Fix: dedupe on `(peer.id, peer.distinction_id())` joint key.

## N8 — Undocumented concurrency contract

`network.rs` has no async, no tokio, no Mutex, no RwLock. Every state-modifying method takes `&mut self`. The agent assumes "the runtime (Go/Kotlin/Swift) broadcasts" (doc on `propose_commitment`, network.rs:194-196) and serializes calls. **Embedder must wrap in external Mutex; nothing documents this.**

FFI consumers in particular: two threads calling `koru_agent_join_peer` on the same `*mut KoruAgent` = two `&mut` aliases = UB at the FFI boundary.

## N10 — Epoch self-assertion

`propose_commitment` (network.rs:197-217) sets `epoch: self.current_epoch` — read from the leader's own state. No peer-signed pre-commit binding leader to epoch. A byzantine peer can construct a `BatchCommitment` with arbitrary `epoch` (struct fields are `pub`); `check_commitment` (network.rs:229-234) only checks `commitment.epoch == self.current_epoch`. Pre-broadcasting commitments for future epochs is accepted by light nodes when those epochs arrive.

## N11 — `pending_commitments` unbounded

`pending_commitments: HashMap<[u8; 32], BatchCommitment>` (network.rs:121) inserted in `propose_commitment` (network.rs:214), removed only on **successful** `finalize_batch` (network.rs:272). If leaders propose-but-never-finalize (invalid batch data), map grows forever. Memory DoS on followers.

Fix: bound size, add TTL eviction.

## v2.0 (`pub(crate) [u8;16]`) impact

**Closed structurally:**
- N3 (`from_state(root: Distinction, ...)`) — caller cannot mint Distinction.
- N4 (`update_local_root(new_root: Distinction)`) — same.

**NOT closed by v2.0; need targeted fixes:**
- N1 (peer-id length): bound at `PeerIdentity::new` (suggest 64 bytes max).
- N2 (empty peer-id → d0): reject `id.is_empty()`.
- N5 (`previous_root` truncation): remove `.take(8)`; either hash full string or validate as 64-hex-char SHA256.
- N6 (`leader_id` not in hash): include in `BatchCommitment::compute`.
- N7 (dedupe mismatch): dedupe on `(id, distinction_id)` jointly.
- N8 (concurrency doc): documentation + optionally wrap in internal Mutex.
- N10 (epoch self-assertion): peer-signed pre-commit, out-of-scope here.
- N11 (pending_commitments unbounded): size cap + TTL.

## Recommended actions (must-fix before v2.0)

1. `commitment.rs:46-58`: include `leader_id` in SHA256 hasher.
2. `network.rs:73-78`: drop `.take(8)` from previous_root canonicalization.
3. `network.rs:30-39`: cap peer-id length; reject empty.
4. `network.rs:147-164`: `from_state` must validate `root` via `engine.get_distinction_by_id(...)`.
5. `network.rs:404-406`: `update_local_root` must validate, or remove from public trait.
6. `network.rs:175`: dedupe on `(id, distinction_id)`.
7. `network.rs:121`: bound `pending_commitments` + TTL/eviction.
8. Crate header: document the embedder-must-lock contract, or wrap in internal Mutex.

## Repro probes (drafted, blocked from writing)

Three programs designed; full source in agent transcript:
- `audit_network_foreign_peers.rs` — demonstrates N1, N2, N3, N4, N5
- `audit_network_commitment_unbound.rs` — demonstrates N6 (leader_id forgeable)
- `audit_network_concurrency.rs` — demonstrates N8 (external lock required)

## Bottom line

`network.rs` has **the most security-critical findings** in the codebase. v2.0's `pub(crate)` closes 2 of the 7 HIGH severity issues. The remaining 5 (peer-id length, empty peer-id, previous_root truncation, leader_id forgery, epoch self-assertion) need targeted fixes that **do not depend on v2.0** and could ship as 1.2.x patches if a release window appeared. For the koru blockchain use case, **N5 and N6 are correctness bugs that affect consensus integrity** and should not ship to a network at any version without being fixed first.
