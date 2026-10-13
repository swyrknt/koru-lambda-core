# Koru Lambda Core

[![Crates.io](https://img.shields.io/crates/v/koru-lambda-core.svg)](https://crates.io/crates/koru-lambda-core)
[![Docs.rs](https://docs.rs/koru-lambda-core/badge.svg)](https://docs.rs/koru-lambda-core)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Build Status](https://github.com/swyrknt/koru-lambda-core/actions/workflows/ci.yml/badge.svg)](https://github.com/swyrknt/koru-lambda-core/actions)

A minimal axiomatic substrate for distinction theory. One operator, two
primordials, four axioms enforced inline; correctness properties of
larger distributed or stateful systems emerge as consequences of that
kernel, not as protocol they have to layer on top.

## Install

```toml
[dependencies]
koru-lambda-core = "2.0"
```

No default features. The optional `serde` feature adds hex serde
adapters for `Distinction` and `RawDistinctionId`. The `wasm-bindgen`
surface for browser / Node / Deno targets is not a feature of this
crate: it ships separately as `koru-lambda-wasm`.

## Quickstart (Rust)

```rust
use koru_lambda_core::projection::Adjacency;
use koru_lambda_core::{DistinctionEngine, RawDistinctionId};
use koru_lambda_core::projection::Direction;

let engine = DistinctionEngine::new();

// Write side: synthesize a distinction from the two primordials.
let child = engine.synthesize(engine.d0(), engine.d1());

// Read side: project a 2-hop upstream cone anchored at the child.
let cone = engine.project(child)
    .direction(Direction::Upstream)
    .hops(2)
    .signal(Adjacency)
    .materialize();
assert!(cone.contains(&child));

// Trust boundary: verify raw bytes from a wire / hex source.
let wire = child.to_hex();
let raw = RawDistinctionId::from_hex(&wire).expect("hex parses");
let verified = engine.verify(raw).expect("this engine has these bytes");
assert_eq!(verified.as_bytes(), child.as_bytes());
```

## Quickstart (JS/TS via npm)

```bash
npm install koru-lambda-core
```

```typescript
// `wasm-pack --target bundler` auto-initializes on import — no
// explicit init() call is needed. Bundlers with async-import support
// (Vite + vite-plugin-wasm, webpack 5, esbuild >= 0.20) handle it.
import { WasmEngine, idToHex } from "koru-lambda-core";

const engine = new WasmEngine();

// Write side: synthesize a distinction from the two primordials.
const child = engine.synthesize(engine.d0(), engine.d1());

// Read side: project a 2-hop upstream cone anchored at the child.
const cone = engine.projectAdjacency(child, "upstream", 2);
console.assert(cone.contains(child));

// Trust boundary: raw bytes from a wire / hex source verify back to
// the same distinction.
const verified = engine.verify(child);
console.log("child:", idToHex(verified));
```

Migrating from `koru-lambda-core@1.2.0` on npm? See
the JS migration notes in the repository (https://github.com/swyrknt/koru-lambda-core/blob/main/docs/MIGRATION_JS.md).

## Demos

Four use-cases from simple to complex, ported to both languages as
**non-interactive CLI scripts** — run one, read the output, move on.
Same operations at each tier so the two languages read as parallel
translations of the same behavior:

| Tier | Rust                                       | JS/TS               |
|-----:|:-------------------------------------------|:--------------------|
| 1    | `cargo run --example 01_hello_synth`       | `npm run demo:1`    |
| 2    | `cargo run --example 02_novelty_and_chain` | `npm run demo:2`    |
| 3    | `cargo run --example 03_projection_composition` | `npm run demo:3` |
| 4    | `cargo run --example 04_wire_format_roundtrip`  | `npm run demo:4` |

Tier 4 (`wire_format_roundtrip`) live-verifies **Cond D**: two engines
running the same synthesis history produce byte-identical
`canonical_bytes` on the same projection. The Rust and JS/TS tier-4
demos independently produce the same projection id byte-for-byte —
that's **Cond D′ cross-target byte-equivalence** demonstrated at
runtime. `tests/cond_d_prime.rs` carries the same attestation in CI.

Running the JS/TS side:

```bash
# from the repo root, one-time setup
npm --prefix examples/wasm-demos run build:pkg
cd examples/wasm-demos && npm install

# then any of:
npm run demo:1
npm run demo:2
npm run demo:3
npm run demo:4
npm run demo:all
```

See [`examples/wasm-demos/README.md`](https://github.com/swyrknt/koru-lambda-core/blob/main/examples/wasm-demos/README.md)
for details.

## What's inside

- **4 axioms, 2 primordials, 1 operator** — [THEORY.md](THEORY.md) is
  authoritative. Determinism, commutativity, idempotence, and
  content-addressing are enforced inline in
  [`DistinctionEngine::synthesize`](src/engine.rs).
- **Merged-map engine** — one `DashMap<[u8;16], EngineNode { parents,
  degree }>` carries the three canonical O(1) projections
  (saturation check, parent lookup, degree query) as per-node fields.
  See [ARCHITECTURE.md](ARCHITECTURE.md).
- **Projection primitive** — the axiom-forced read dual of
  `synthesize`. Every projection commits to a 4-axis structure:
  `{ root, direction, boundary, signal }`.
- **Two-type discipline** — external bytes enter as
  `RawDistinctionId` and cross the trust boundary through
  `engine.verify()`; internal code operates on `Distinction`.
  Foreign-byte injection is closed at the type level.
- **Novelty bit at operator return** — `synthesize_novel` returns
  `SynthesisOutcome::{Novel, Existing}` so consumers can react to
  Law-7 saturation without re-querying `has()`.
- **Cond D cross-engine byte-equivalence** — two engines with the
  same synthesis history produce byte-identical output for the same
  projection. CI attests this as a shipping theorem, not a hope.

## Documentation

- [THEORY.md](THEORY.md) — one operator, four axioms, two
  primordials, eight structural laws. Authoritative for the theory.
- [docs/SPEC.md](docs/SPEC.md) — the normative byte-level
  specification: what a conforming implementation must reproduce.
- [DESIGN.md](https://github.com/swyrknt/koru-lambda-core/blob/main/DESIGN.md) — the v2.0 plan. Repository only: it is a
  planning document for this release, so it is not in the published crate
  today, what the epics change, and what v2.0.0 does *not* ship.
- [ARCHITECTURE.md](ARCHITECTURE.md) — code structure, module
  layout, and how the merged-map substrate is organized.
- [ECOSYSTEM.md](ECOSYSTEM.md) — the layers around the substrate,
  which one you want, and how other languages reach it.
- [CHANGELOG.md](CHANGELOG.md) — release history.
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md) — capacity and
  throughput measurements; every number cites an in-crate anchor.
- [docs/MIGRATION_JS.md](https://github.com/swyrknt/koru-lambda-core/blob/main/docs/MIGRATION_JS.md) — JS/TS migration.
  Repository only, and it describes an npm package 2.0.0 does not publish
  guide for consumers upgrading from `koru-lambda-core@1.2.0` on npm.

Auto-generated API docs live at
[docs.rs/koru-lambda-core](https://docs.rs/koru-lambda-core).

## License

Dual-licensed under either:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
