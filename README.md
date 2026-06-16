# Koru Lambda Core

[![Crates.io](https://img.shields.io/crates/v/koru-lambda-core.svg)](https://crates.io/crates/koru-lambda-core)
[![Docs.rs](https://docs.rs/koru-lambda-core/badge.svg)](https://docs.rs/koru-lambda-core)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Build Status](https://github.com/swyrknt/koru-lambda-core/actions/workflows/ci.yml/badge.svg)](https://github.com/swyrknt/koru-lambda-core/actions)

A minimal axiomatic system for distributed computation built on **distinction
calculus**: one operator (`synthesize`), four axioms (determinism,
commutativity, irreflexivity, content addressing), two primordials (Δ₀, Δ₁).
From these, all structure emerges deterministically and reproducibly.

> **Status:** v2.0 is queued on `research/warroom-experiments`. The Cargo.toml
> on `main` is still at 1.2.0; v2.0 ships as a single bundled major bump per
> the project's Decision 5.1. The examples in this README target v2.0.

## Quick Start

### Installation

```toml
[dependencies]
koru-lambda-core = "2"
```

### Rust

```rust
use koru_lambda_core::DistinctionEngine;

let engine = DistinctionEngine::new();

// Synthesize the primordials → first emergent distinction.
let existence = engine.synthesize(engine.d0(), engine.d1());
println!("existence = {}", existence.to_hex());

// Build further structure. `synthesize` is &self — no &mut required.
let order  = engine.synthesize(&existence, engine.d0());
let chaos  = engine.synthesize(&existence, engine.d1());
let nature = engine.synthesize(&order, &chaos);

// Traverse: every non-primordial distinction has exactly two canonical parents.
let (a, b) = engine.parents_of(&nature).expect("non-primordial has parents");
assert!(a == order && b == chaos || a == chaos && b == order);

// Degree centrality is O(1).
println!("degree(d0) = {}", engine.degree(engine.d0()));
```

### JavaScript / WASM (`--features wasm`)

The WASM surface is **bytes-canonical**: every distinction ID crossing the JS
boundary is a `Uint8Array` of length 16. Hex is a display format, available via
`idToHex` / `idFromHex` for logs and URLs.

```javascript
import init, {
  WasmEngine, WasmNetworkAgent, idToHex
} from './pkg/koru_lambda_core.js';

await init();
const engine = new WasmEngine();

// d0Id() / d1Id() return raw 16-byte Uint8Arrays.
const d0 = engine.d0Id();
const d1 = engine.d1Id();
const existence = engine.synthesize(d0, d1);  // Uint8Array, 16 bytes
console.log('existence =', idToHex(existence));

// Network consensus.
const agent = new WasmNetworkAgent(engine);
agent.joinPeer('validator_0');
agent.joinPeer('validator_1');
console.log('leader =', agent.getLeader());
```

Build the WASM artifact:

```bash
wasm-pack build --release --features wasm --target web
```

## Core Concepts

### The Five Axioms

1. **Identity** — a distinction is defined solely by its 16-byte ID
2. **Nontriviality** — the system initializes with two primordial distinctions (Δ₀, Δ₁)
3. **Synthesis** — two distinctions combine deterministically to create a third
4. **Symmetry** — `synthesize(a, b) = synthesize(b, a)` (canonical ordering)
5. **Irreflexivity** — `synthesize(a, a) = a` (no new structure)

### Structural Invariants (proven across 50+ experiments)

- **r = 2d − 3** — every novel synthesis adds 1 node + 2 relationships.
- **Average degree → 4.0** — the graph self-balances as it grows.
- **Binary parentage** — every non-primordial has exactly two distinct parents.
- **Content addressing is engine-state-independent** — same chain on different
  engines with different histories → byte-identical IDs.
- **Saturation** — repeating the same synthesis adds zero nodes, zero
  relationships.

## Architecture

```
src/
  engine.rs           Core. DashMap<[u8;16], _, IdentityBuildHasher>. &self everywhere.
                      Traversal indices, append-only synthesis log, structural invariant API.
  primitives.rs       Canonicalizable trait + ByteMapping (folds through caller's engine).
  distinction_hex.rs  to_hex / from_hex / Display / Debug / serde adapter.
                      The only hex-aware module in the crate.
  lib.rs              Public re-exports.
  subsystems/
    local_agent.rs    LocalCausalAgent trait + synthesize_causal_action helper.
    compactor.rs      StructuralCompactor (explicit thresholds, append-only via synthesize).
    validator.rs      ConsensusValidator (pre-validation, V3 data cap, atomic restore_state).
    network.rs        NetworkAgent (peer-id cap, LRU pending_commitments, leader election).
    commitment.rs     CommitmentAgent (BatchCommitment::compute hashes leader_id; LRU cache).
    parallel.rs       BatchSynthesizer (rayon-backed; Vec<Option<Distinction>>).
  ffi.rs              C ABI. Box<Mutex<...>> handles, opaque structs, ManuallyDrop<Arc>,
                      panic = "abort" on release.
  wasm.rs             JS / WASM bindings (feature-gated). Bytes-on-wire end to end.
```

## Testing

```bash
cargo test --release                                  # 161 tests, all green
cargo clippy --all-targets --release                  # clean
cargo clippy --all-targets --features wasm --release  # clean
cargo build --release                                 # rlib + cdylib + staticlib
```

WASM-side tests use `#[wasm_bindgen_test]`:

```bash
wasm-pack test --node --features wasm
```

Tests cover: axiom verification, synthesis determinism, traversal API,
synthesis log replay (ordered + shuffled), structural invariant, compactor
classification, consensus validation with atomic-failure rollback, network
agent epochs / leader election / dedupe, parallel batch synthesis, byte
canonicalization, FFI safety (panic=abort + internal Mutex + opaque types +
length validation), commitment two-stage gossip integrity, and WASM
determinism across the boundary.

## Performance

Measured on Apple M3 Pro (post-v2.0 foundation):

| Surface | Throughput |
|---|---|
| Engine synth, single-thread | ~500K ops/sec |
| Engine synth, 8 threads | **15.3M ops/sec** (was 2.6M v1.2.0 — 5.9× via IdentityHasher + 16-byte keys) |
| Log replay, ordered / shuffled | 450K / 367K ops/sec (perfect fidelity) |
| Memory per distinction | ~80 B (down 8× from v1.2.0's ~629 B; `[u8;16]` instead of String) |
| Ceiling on 16 GB laptop | ~80M distinctions (was ~10M) |

The dominant performance gain in v2.0 comes from cache density, not clone
elimination (clone is ~1.6% of synth cost).

## Documentation

- **[CHANGELOG.md](CHANGELOG.md)** — version-by-version changes; v2.0 ships
  the entire Phase 6 work as a single bundled release.
- **[CHECKLIST.md](CHECKLIST.md)** — per-section v2.0 completion status with
  evidence links.
- **[Documentation Hub](docs/)** — design docs, testing standards.
- **[API Reference](https://docs.rs/koru-lambda-core)** — auto-generated.

## Use Cases

The engine is currently used by two consumers:

- **ALIS** — cognitive architecture, language, self-reference.
- **koru-protocol** — economic consensus, currency, trust (BFT-style
  Structural Proof-of-Causality).

The substrate itself is domain-agnostic. Anything representable as an
append-only graph of content-addressed events — distributed databases,
event-sourced systems, replicated state machines, structurally-aware ML —
fits the shape.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md). The engine core (`src/engine.rs`) is
**sacrosanct**: changes must be additive, axiomatically correct, and
empirically validated. Subsystems are application-layer and accept more
churn.

## License

Dual-licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Links

- [Issue Tracker](https://github.com/swyrknt/koru-lambda-core/issues)
- [Discussions](https://github.com/swyrknt/koru-lambda-core/discussions)
- [Changelog](CHANGELOG.md)

---

**Note**: This is research software implementing distinction theory as a
substrate for distributed computation. The axiomatic foundation is novel; the
engineering of the surrounding subsystems is production-shaped.
