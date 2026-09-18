# Migrating from koru-lambda-core 1.2 to 2.0 (JavaScript / TypeScript)

`koru-lambda-core@2.0.0` is a **breaking release for npm consumers**. The
Rust-side change surface is additive (see [CHANGELOG.md](../CHANGELOG.md)
§ 2.0.0), but the JS/npm surface is not: one exported class was retained
by name with a completely different API, three exported classes were
retired without replacement, and the byte content returned by the two
primordial accessors changed silently. This document is the migration
playbook for consumers upgrading from `koru-lambda-core@1.2.0` on npm.
Concept-level depth lives in [THEORY.md](../THEORY.md); full version
history lives in [CHANGELOG.md](../CHANGELOG.md).

## Read this first — one silent-breaking change

> **`d0Id()` / `d1Id()` return different bytes in 2.0.** The method name
> lost its `Id` suffix (`d0Id()` → `d0()`, `d1Id()` → `d1()`), but the
> return-type shape is unchanged (`Uint8Array`). Byte **content** did
> change. Any consumer persisting v1.2 primordial bytes to a store, a
> wire, or a cross-process channel and comparing against v2.0 primordial
> bytes will observe false negatives without a compile error.

| Call | v1.2 return bytes | v2.0 return bytes |
|---|---|---|
| `engine.d0Id()` (v1.2) / `engine.d0()` (v2.0) | `[0x30]` — UTF-8 of the literal string `"0"` | `[0x00; 16]` — the 16-byte primordial |
| `engine.d1Id()` (v1.2) / `engine.d1()` (v2.0) | `[0x31]` — UTF-8 of the literal string `"1"` | `[0x01, 0x00, ..., 0x00]` — the 16-byte primordial |

If you have v1.2 primordial bytes at rest anywhere (a database column, a
JSON payload cached on disk, a message queue that hasn't drained),
coordinate the upgrade on both sides before pushing 2.0.

Concrete before/after:

```typescript
// v1.2 — d0Id() returns [0x30] (UTF-8 of "0")
const oldD0: Uint8Array = engine.d0Id();
console.log(oldD0.length);           // 1
console.log(oldD0[0].toString(16));  // "30"

// v2.0 — d0() returns [0x00; 16] (the actual primordial)
const newD0: Uint8Array = engine.d0();
console.log(newD0.length);           // 16
console.log(newD0[0].toString(16));  // "0"

// Byte-comparing across versions returns false silently:
// oldD0.every((b, i) => b === newD0[i])  === false
```

## What changed: at-a-glance

| v1.2 export | v2.0 fate | replacement | why |
|---|---|---|---|
| `WasmEngine` class | **Kept name, different API** | See recipes below | 2.0 is bytes-canonical, not string-typed; every method that took `string` IDs now takes `Uint8Array` |
| `WasmValidator` class | **Retired** | none — build at your layer | subsystem layer no longer at the substrate |
| `WasmCommitmentAgent` class | **Retired** | none — build at your layer | subsystem layer no longer at the substrate |
| `WasmNetworkAgent` class | **Retired** | none — build at your layer | `NetworkAgent` no longer exists at the substrate; SPoC / leader / epoch machinery was consumer-side scaffolding, not substrate primitive |
| `initSync`, default `__wbg_init` export, `InitInput` / `InitOutput` / `SyncInitInput` types | **Kept** | same wasm-bindgen boilerplate | no change to init / loading semantics |
| `WasmEngine.benchmarkSynthesis(iterations)` | **Retired** | run your own timing loop or use the `benches/*.rs` harnesses | benchmarking was consumer-tooling, not substrate; keeping it in the substrate binding conflated concerns |

## Retired with no replacement

`WasmValidator`, `WasmCommitmentAgent`, and `WasmNetworkAgent` are gone
from the substrate. They rode on an earlier substrate shape
(`NetworkAgent` + subsystem trait) that 2.0 deliberately removed. Their
methods — `validateBatch`, `expectedNonce`, `commitmentsProcessed`,
`getLeader`, `joinPeer` / `joinPeers`, `advanceEpoch`, `currentEpoch`,
`consensusRoot`, `finalizeBatch`, `checkCommitment`, `proposeCommitment`,
`benchmarkLeaderElection`, `benchmarkValidation` — have no substrate
analogue.

Options for consumers who were using them:

1. **Don't upgrade yet.** Pin `koru-lambda-core@1.2.x` and continue.
2. **Rebuild the pattern at your own layer** using 2.0 primitives
   (`synthesize`, `synthesizeNovel`, `verify`, `projectAdjacency` /
   `projectDegree` / `projectHopDistance`). See
   [THEORY.md](../THEORY.md) for what the substrate is;
   [ARCHITECTURE.md](../ARCHITECTURE.md) for what it is not.
3. **Wait.** A future release may re-introduce a subsystems layer above
   the substrate. No timeline is committed.

## Recipes: 1.2 → 2.0

### 1. Constructing an engine

The constructor call is unchanged; the module loader still needs `await
init()` (or `initSync(...)`).

```ts
// v1.2 and v2.0 — identical
import init, { WasmEngine } from "koru-lambda-core";

await init();
const engine = new WasmEngine();
```

### 2. `synthesize` — string args become `Uint8Array`

```ts
// v1.2
const child: Uint8Array = engine.synthesize("0", "1");
```

```ts
// v2.0
const child: Uint8Array = engine.synthesize(engine.d0(), engine.d1());
```

Bytes passed to `synthesize` must be witnessed by *this* engine (or a
compatible history). Foreign bytes throw `VerifyError` — this is the
Axiom-4 trust boundary, enforced at the wasm surface.

### 3. `d0Id()` / `d1Id()` — silent byte-content change

```ts
// v1.2
const primordial: Uint8Array = engine.d0Id(); // [0x30]  (UTF-8 of "0")
```

```ts
// v2.0
const primordial: Uint8Array = engine.d0(); // [0x00; 16]  (the primordial)
```

If v1.2 primordial bytes are already on the wire or at rest, the
receiver on v2.0 will not recognize them. Coordinate both sides of the
upgrade before pushing.

### 4. Query methods — `has` / `degree` / `parentsOf` / `distinctionCount` / `relationshipCount`

```ts
// v1.2
engine.distinctionCount();       // number
engine.relationshipCount();      // number
// v1.2 had no `has`, `degree`, or `parentsOf` on WasmEngine.
```

```ts
// v2.0
engine.distinctionCount();       // bigint  (widened for safe range)
engine.relationshipCount();      // bigint
engine.has(bytes);               // boolean
engine.degree(bytes);            // bigint
engine.parentsOf(bytes);         // { min: Uint8Array; max: Uint8Array } | null
```

Note two shape changes: (a) count methods widened from `number` to
`bigint`; (b) `parentsOf` returns a labeled object with the
canonical `min` / `max` parent pair (or `null` for primordials), not a
tuple.

### 5. What you lose: subsystems

If any of the following were in your v1.2 call sites, they are gone at
2.0 and have no drop-in replacement:

- `new WasmValidator(engine)`, `validator.validateBatch(...)`,
  `validator.expectedNonce()`, `validator.currentRoot()`,
  `validator.benchmarkValidation(...)`
- `new WasmCommitmentAgent(engine)`,
  `commitment.commitmentsProcessed()`, `commitment.expectedNonce()`,
  `commitment.currentRoot()`
- `new WasmNetworkAgent(engine)`, `network.getLeader()`,
  `network.joinPeer(...)`, `network.joinPeers(...)`,
  `network.advanceEpoch()`, `network.currentEpoch()`,
  `network.consensusRoot()`, `network.finalizeBatch(...)`,
  `network.checkCommitment(...)`, `network.proposeCommitment(...)`,
  `network.validatorCount()`, `network.benchmarkLeaderElection(...)`

See "Retired with no replacement" above for how to proceed.

## New in 2.0 (adopt as you migrate)

### `verify(bytes)` — the trust boundary

External bytes (from a wire, a database, user input) must cross into a
`Distinction` through `engine.verify(bytes)` before being passed to
`synthesize` or the projection methods.

```ts
// The two-type discipline in JS/TS
declare const bytes: Uint8Array; // from a wire, database, or user input
try {
    const verified = engine.verify(bytes); // Uint8Array (16 bytes)
    engine.synthesize(verified, engine.d0());
} catch (e) {
    // JsError: "VerifyError on bytes: ..." — foreign or malformed
}
```

`synthesize`, `synthesizeNovel`, `has`, `degree`, `parentsOf`, and the
projection methods all verify internally before touching the hot path.
Direct `verify` is the explicit form for consumers who want the trust
boundary crossing to appear in their call graph.

### `synthesizeNovel(a, b)` — novelty bit at operator return

The Law-7 novelty bit is exposed as a discriminated union so consumers
can react to saturation without an extra `has()` query.

```ts
type SynthesisOutcome =
    | { kind: "novel";    distinction: Uint8Array }
    | { kind: "existing"; distinction: Uint8Array };

const outcome: SynthesisOutcome = engine.synthesizeNovel(a, b);
if (outcome.kind === "novel") {
    // First time this pair has been synthesized in this engine
} else {
    // Idempotent replay — distinction bytes still returned
}
```

TypeScript narrows on `kind` because 2.0's wasm-bindgen output ships an
explicit `typescript_custom_section` for the union type.

### Projection primitive — `projectAdjacency` / `projectDegree` / `projectHopDistance`

The read-side dual of `synthesize`. Anchor at a root, pick a direction
and a hop boundary, and materialize.

```ts
// Undirected 2-hop adjacency cone anchored at `child`.
const adj = engine.projectAdjacency(child, "undirected", 2);
adj.entries();          // Array<{ distinction: Uint8Array; parents: { min, max } | null }>
adj.contains(child);    // boolean
adj.canonicalBytes();   // Uint8Array — the sole canonical wire form
adj.projectionId();     // Uint8Array — 32-byte content-addressed id

// Full-reach degree over the upstream cone.
const deg = engine.projectDegree(child, "upstream", undefined);
deg.entries();          // Array<{ distinction: Uint8Array; degree: number }>

// Hop-distance signal, 3 hops downstream.
const hop = engine.projectHopDistance(child, "downstream", 3);
hop.entries();          // Array<{ distinction: Uint8Array; hops: number }>
```

`direction` is one of `"upstream" | "downstream" | "undirected"`.
Passing `hops = undefined` (or omitting it) means "saturated" — traverse
the reachable cone with no hop bound. Concept-level detail is in
[THEORY.md](../THEORY.md) § The synthesis/projection dual.

Materialized projections can be restored from canonical wire bytes:
`engine.restoreProjectionAdjacency(bytes)`,
`engine.restoreProjectionDegree(bytes)`,
`engine.restoreProjectionHopDistance(bytes)`.

### Hex helpers — `idToHex(bytes)` / `idFromHex(str)`

Free functions at the module level, not methods on `WasmEngine`.

```ts
import { idToHex, idFromHex } from "koru-lambda-core";

const hex: string = idToHex(child);           // 32-char lowercase hex
const bytes: Uint8Array = idFromHex(hex);     // 16 raw bytes (unverified)
const verified = engine.verify(bytes);        // cross the trust boundary
```

`idFromHex` validates length and charset only — it makes no
engine-membership claim. Always pass the returned bytes through
`engine.verify()` before feeding them to `synthesize` or a projection.

## Migration checklist

1. Read this document and skim the [CHANGELOG.md](../CHANGELOG.md)
   2.0.0 entry.
2. **Decide if you want to migrate at all.** Pinning
   `"koru-lambda-core": "1.2.x"` is a supported terminal state — v1.2.0
   is a shipped release, still on npm, and will not disappear. If your
   application uses the retired subsystem classes (`WasmValidator` /
   `WasmCommitmentAgent` / `WasmNetworkAgent`) and no v2.0 primitive
   substitutes cleanly, staying on 1.2.x is a legitimate choice. Skip
   the rest of this checklist.
3. **Pin your v1.2 dependency** before starting the migration. Avoid
   mid-upgrade breakage: `"koru-lambda-core": "1.2.x"` until you are
   ready to cut over.
4. **Audit call sites for retired classes.** Any hit is a hard stop:
   ```
   grep -rn "WasmValidator\|WasmCommitmentAgent\|WasmNetworkAgent" src/
   ```
5. **Audit any wire or persistence layer.** v1.2 primordial bytes
   (`d0Id()` / `d1Id()`) are content-different from v2.0 primordials.
   Coordinate with the receiver side before pushing.
6. **Rewrite `synthesize(id_a: string, ...)` call sites** to pass
   `Uint8Array`. Types will guide you.
7. **Rewrite `d0Id()` → `d0()` and `d1Id()` → `d1()`.** Same shape,
   different bytes. See step 5.
8. **Adopt `engine.verify()`** at every trust boundary (deserialize
   from wire, deserialize from disk, user input).
9. **Widen count call sites** — `distinctionCount()` and
   `relationshipCount()` return `bigint`, not `number`.
10. Run your test suite. Fix compile errors first (the visible breaks),
    then rerun to catch semantic regressions.
11. Upgrade to `koru-lambda-core@2.0.0`.

## For depth

- [THEORY.md](../THEORY.md) — the axioms and structural laws
- [CHANGELOG.md](../CHANGELOG.md) — full version history
- [docs/BENCHMARKS.md](./BENCHMARKS.md) — capacity and throughput numbers
- [ARCHITECTURE.md](../ARCHITECTURE.md) — code structure
