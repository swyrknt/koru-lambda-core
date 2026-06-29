# Corpus Provenance

This file is the **out-of-band authoritative record** for the pinned exp18
corpus (`tests/corpora/exp18.{log,freq.bin}`). It exists because the
in-tree SHA-256 gates in `tests/coding_law.rs` only detect drift — they
cannot distinguish a legitimate, gate-amendment-approved regeneration
from a silent corpus replacement. This file is the trail that survives
either case.

This file closes the "one contributor coordinated-bumps SHA-256 + `rand`
together" hazard by requiring regeneration to leave a visible audit
trail. (Step 1 pre-merge Round 2; see commit log.)

## Authoritative pin (current)

| Field | Value |
|---|---|
| **Generated** | 2026-06-28 |
| **Generator commit** | `7e2127e` (`step1e-corpus-fix: include exp18.log corpus (was silently excluded)`) |
| **Generator binary** | `experiments/runner/src/bin/gen_exp18_corpus.rs` |
| **Workspace `rand` version** | `=0.8.5` (pinned via `[workspace.dependencies]` in root `Cargo.toml`) |
| **RNG seed** | `0xC0DE` |
| **Alpha (Zipf exponent)** | `1.0` |
| **N (chain pool size)** | `4096` |
| **M (pair count)** | `8 * N = 32_768` |

## Pinned files

| File | Size (bytes) | SHA-256 |
|---|---|---|
| `tests/corpora/exp18.log` | `1_048_576` | `9fd8a22b4b89e0937d09af13646af5b797e8369e46c11c3e1f94b3e7169878d9` |
| `tests/corpora/exp18.freq.bin` | `16_384` | `93d08b2fb4e721b679c90a56b70d69de8511a4749a4ee2ff715722efc123ca46` |

## Measured Spearman ρ at the pin

`tests/coding_law.rs::exp18_rho_clears_gate` reports **ρ = 0.9940** on
this corpus against a fresh `DistinctionEngine`. Gate target: ≥ 0.985.
Gate floor: ≥ 0.97.

The exact ρ value above is informational — it lives here, not in the
gate constant, because workload regeneration may legitimately produce
a slightly different ρ at the same gate-clearing tier. The gate is
binary (above target vs below); ρ here is provenance.

## Regeneration policy

The corpus is **not** to be regenerated as part of routine development.
Regeneration requires ALL of:

1. **Written rationale** — substrate-semantics change, gate-tier change
   with measurement, or generator-bug fix. NOT: "test failed and I want
   it to pass" or unrelated `rand` bumps.
2. **`BUDGET_LOG.md` amendment row** if the ρ gate target or floor changes.
3. **Sign-offs**: theory-guardian (workload interpretation) + qa-sentinel
   (process integrity).
4. **Update this file**: new generator commit, new digests, new ρ,
   History row.

CI verifies the SHA-256 gates on every push. A digest mismatch fails
the build BEFORE running ρ — the contributor's first signal is "your
corpus drifted; consult CORPUS_PROVENANCE.md."

## History

| Date | Generator commit | Reason | Signers |
|---|---|---|---|
| 2026-06-28 | `7e2127e` | Initial pinning (Step 1e gate close-out) | swyrknt, qa-sentinel R3 GREEN |
