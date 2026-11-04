# Koru Lambda Core

[![Crates.io](https://img.shields.io/crates/v/koru-lambda-core.svg)](https://crates.io/crates/koru-lambda-core)
[![Docs.rs](https://docs.rs/koru-lambda-core/badge.svg)](https://docs.rs/koru-lambda-core)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Build Status](https://github.com/swyrknt/koru-lambda-core/actions/workflows/ci.yml/badge.svg)](https://github.com/swyrknt/koru-lambda-core/actions)

Fold values into an append-only graph of content-addressed identifiers;
any two conforming runtimes compute the same sixteen bytes without
coordinating.

```rust
use koru_lambda_core::DistinctionEngine;

let engine = DistinctionEngine::new();
let child = engine.synthesize(engine.d0(), engine.d1());
println!("{}", child.to_hex());
```

```
6cc4f0e930b34481d03a4134331852ea
```

Those thirty-two hex characters are not this machine's answer. The same
synthesis prints the same value in JS/TS through the wasm bindings and in
the [Python reference implementation](conformance/reference/substrate.py),
and you can reproduce it below with `shasum` and no toolchain at all —
because the identifier is a hash of its two parents and nothing else.

## Which layer do you want?

Most readers want the layer above this one.

- **Building something that keeps state** — an agent, a validator, a log
  that has to agree with another copy of itself: you want the agent layer,
  [`koru-lambda-agents`](https://github.com/swyrknt/koru-lambda-core/tree/main/crates/koru-lambda-agents).
  It carries the patterns this crate deliberately does not.
- **Building another agent layer, a binding, or a second implementation of
  the specification**: you want this crate directly, plus
  [`docs/SPEC.md`](docs/SPEC.md).

[`ECOSYSTEM.md`](ECOSYSTEM.md) describes how the layers divide and why the
substrate is kept small enough to freeze.

## What this is not

- **Not a general CRDT.** This is a grow-only set. Merging is union and
  nothing is ever retracted, so it needs no conflict resolution — and it
  cannot express a counter that goes down, a register that gets
  overwritten, or anything else that forgets.
- **Not consensus.** Two engines that fold the same values reach the same
  identifiers because the identifiers are Merkle-determined, not because
  anyone voted. Nothing here decides *which* history is the real one; that
  is the question a consensus protocol answers and this substrate does not.
  The consensus agents in
  [`koru-lambda-agents`](https://github.com/swyrknt/koru-lambda-core/tree/main/crates/koru-lambda-agents)
  are reference consumers built on top, not part of the substrate.
- **Not a 256-bit identifier.** Identifiers are 128 bits — SHA-256
  truncated to its first 16 bytes. The collision margin is a birthday bound
  on 128 bits, not on 256. `docs/SPEC.md` §11 records the limit.

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

## Lifting your data

Your values are not distinctions, and `synthesize` will not take them. To
get a distinction from a domain value, fold its bytes onto an anchor:

```rust
use koru_lambda_core::{domain_anchor, ByteFold, Canonicalizable, Distinction,
                       DistinctionEngine, DomainLabel};

const SENSORS: DomainLabel = DomainLabel::new(b'S');

struct Reading { sensor: u8, celsius: i16 }

impl Canonicalizable for Reading {
    fn to_canonical_structure(&self, fold: &ByteFold, engine: &DistinctionEngine)
        -> Distinction
    {
        let mut bytes = vec![self.sensor];
        bytes.extend_from_slice(&self.celsius.to_be_bytes());
        fold.fold(domain_anchor(engine, fold, SENSORS), &bytes, engine)
    }
}
```

Two things a stranger gets wrong here, both of which the substrate will
tell you about loudly:

- **Anchor on a synthesized root, never on an element tip.** `fold` panics
  on a tip, because the first step could collapse through idempotence and
  silently drop an element. `domain_anchor` returns a safe anchor.
- **The anchor is what separates domains.** The same bytes under a
  different `DomainLabel` fold to a different identifier. The label is a
  parameter of the anchor, never a tag inside the hash — `docs/SPEC.md`
  §4.2.

A stateful consumer then carries one root forward, synthesizing each new
value into it. That is the agent layer's job, not the substrate's:

```rust
use koru_lambda_agents::Lca;
use std::sync::Arc;

let engine = Arc::new(DistinctionEngine::new());
let fold = Arc::new(ByteFold::new(&engine));
let seed = domain_anchor(&engine, &fold, SENSORS);

let mut lca = Lca::with_fold(&engine, Arc::clone(&fold), seed);
let root = lca.advance(Reading { sensor: 1, celsius: 21 });
println!("{}", root.to_hex());
```

```
5cd3a11a987b003855a585697e3db48e
```

A second engine, built independently and given the same reading, advances
to the same sixteen bytes. That is the whole point: the root is a function
of the history, not of the process that walked it.

## Check an identifier yourself, with no toolchain

Identity is `SHA-256(min(a,b) ‖ max(a,b))` truncated to the first 16 bytes,
and nothing else. The two primordials are fixed constants, so the first
child this substrate can produce is computable with a command you already
have:

```bash
printf '%s' \
  '0000000000000000000000000000000001000000000000000000000000000000' \
  | xxd -r -p | shasum -a 256 | cut -c1-32
```

```
6cc4f0e930b34481d03a4134331852ea
```

That is `synthesize(d₀, d₁)`. The same value appears in
[`conformance/vectors.json`](conformance/vectors.json), which every
conforming implementation certifies against, and a live engine reproduces it
in `tests/conformance_fixture.rs`. Three sources, one number — and you did
not install anything to check the third against the first.

`min` and `max` are over the raw bytes, which is what makes the operator
commutative: `synthesize(a, b)` and `synthesize(b, a)` hash the same input.

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
`canonical_bytes` on the same projection. The Rust and JS/TS tier-4 demos
produce the same projection id byte-for-byte, and `tests/cond_d_prime.rs`
asserts the same constants under both targets in CI.

State that result narrowly, because it is easy to overstate. The JS/TS side
is this crate compiled to `wasm32`, so the two runs are **two compilations
of one source, not two independent witnesses**. What they establish is that
identity survives a 32-bit pointer width and a different codegen backend —
continuously, on every commit. They do not establish byte-order
independence: both targets are little-endian, CI has no big-endian and no
32-bit native target, so byte order rests on the encoding being *specified*
(`docs/SPEC.md` §7) rather than on being tested. The claim that the
specification alone is sufficient can only be falsified by an
implementation written from it in another language, which is what
`conformance/vectors.json` exists for.

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
