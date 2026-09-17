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

No default features. The `wasm` feature gates the `wasm-bindgen`
surface for browser / Node / Deno targets.

## Quickstart

```rust
use koru_lambda_core::{Adjacency, DistinctionEngine, RawDistinctionId};
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

## What's inside

- **4 axioms, 2 primordials, 1 operator** — [THEORY.md](THEORY.md) is
  authoritative. Determinism, commutativity, irreflexivity, and
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
- [DESIGN.md](DESIGN.md) — shipping story: what the substrate ships
  today, what the epics change, and what v2.0.0 does *not* ship.
- [ARCHITECTURE.md](ARCHITECTURE.md) — code structure, module
  layout, and how the merged-map substrate is organized.
- [CHANGELOG.md](CHANGELOG.md) — release history.
- [docs/BENCHMARKS.md](docs/BENCHMARKS.md) — capacity and
  throughput measurements; every number cites an in-crate anchor.

Auto-generated API docs live at
[docs.rs/koru-lambda-core](https://docs.rs/koru-lambda-core).

## License

Dual-licensed under either:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).
