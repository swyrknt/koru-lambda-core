// Tier 4 — wire_format_roundtrip. Content addressing at three scales, and
// the cross-process hand-off:
//
//   1. Distinction: `idToHex` → wire → `idFromHex` → `verify`.
//   2. Projection: the **same parent pairs in a different order** give
//      byte-identical `canonicalBytes` and an identical `projectionId`
//      (Laws 8 and 9, live). Replaying one fixed order on two engines in
//      this process would only show Axiom 1 (determinism).
//   3. Tamper rejection at the wire boundary, including the one tamper
//      that is accepted and what it costs.
//   4. The hand-off: this process reads the history a *separate Rust
//      process* wrote, replays it in an order it computes itself, and
//      reproduces the projection id Rust recorded.
//
//   cargo run --example 04_wire_format_roundtrip   # writes the artifact
//   npm run demo:4                                  # this file reads it
//
// What section 4 does not establish: both sides are this one crate, one
// compiled native and one to wasm32. It witnesses that an artifact crosses
// a process boundary intact, not that docs/SPEC.md is sufficient to
// reimplement from. DEMO-5 is the item that would.
//
// Mirrors examples/04_wire_format_roundtrip.rs.

import { readFileSync } from "node:fs";
import { WasmEngine, idToHex, idFromHex } from "koru-lambda-core";

function toHex(bytes: Uint8Array): string {
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function fromHex(hex: string): Uint8Array {
    const out = new Uint8Array(hex.length / 2);
    for (let i = 0; i < out.length; i++) {
        out[i] = Number.parseInt(hex.slice(i * 2, i * 2 + 2), 16);
    }
    return out;
}

function bytesEqual(a: Uint8Array, b: Uint8Array): boolean {
    if (a.length !== b.length) return false;
    for (let i = 0; i < a.length; i++) {
        if (a[i] !== b[i]) return false;
    }
    return true;
}

function assert(cond: boolean, msg: string): void {
    if (!cond) throw new Error(msg);
}

function refuses(f: () => unknown): boolean {
    try {
        f();
        return false;
    } catch {
        return true;
    }
}

// The 64-bit LCG of tests/order_independence.rs, in BigInt so the wrap is
// the same wrap. `Number` would lose the low bits that drive the choice.
const MUL = 6364136223846793005n;
const INC = 1442695040888963407n;
const MASK = (1n << 64n) - 1n;

function seededPairs(seed: bigint, count: number): Array<[number, number]> {
    let state = seed;
    const next = (): number => {
        state = (state * MUL + INC) & MASK;
        return Number(state >> 33n);
    };
    return Array.from({ length: count }, () => [next(), next()] as [number, number]);
}

type Pair = [Uint8Array, Uint8Array];

/** Produce a different topological order of the same parent pairs. */
function reorder(history: Pair[], d0: Uint8Array, d1: Uint8Array): Pair[] {
    const witnessed = new Set<string>([toHex(d0), toHex(d1)]);
    const pending: Array<Pair | null> = [...history];
    const out: Pair[] = [];

    while (out.length < history.length) {
        let progressed = false;
        // Back-to-front: take the last eligible pair, which maximises
        // divergence from the order the producer used.
        for (let i = pending.length - 1; i >= 0; i--) {
            const pair = pending[i];
            if (pair === null) continue;
            if (witnessed.has(toHex(pair[0])) && witnessed.has(toHex(pair[1]))) {
                pending[i] = null;
                out.push(pair);
                progressed = true;
                break;
            }
        }
        if (!progressed) {
            // The rest need children not yet produced. Replay the consumed
            // prefix through a scratch engine to learn what is witnessed now.
            const scratch = new WasmEngine();
            for (const [l, r] of out) witnessed.add(toHex(scratch.synthesize(l, r)));
            const eligible = pending.some(
                (p) => p !== null && witnessed.has(toHex(p[0])) && witnessed.has(toHex(p[1])),
            );
            assert(eligible, "reorder: pairs remain and none are eligible — not a DAG");
        }
    }
    return out;
}

function buildHistory(engine: WasmEngine, pairs: Array<[number, number]>): [Pair[], Uint8Array] {
    const chain: Uint8Array[] = [engine.d0(), engine.d1()];
    const history: Pair[] = [];
    for (const [a, b] of pairs) {
        const left = chain[a % chain.length];
        const right = chain[b % chain.length];
        history.push([left, right]);
        chain.push(engine.synthesize(left, right));
    }
    return [history, chain[chain.length - 1]];
}

function chainHistory(engine: WasmEngine): Uint8Array {
    const c = engine.synthesize(engine.d0(), engine.d1());
    const d = engine.synthesize(c, engine.d0());
    engine.synthesize(d, engine.d1());
    return c;
}

// ---- (1) distinction wire round-trip through the trust boundary
const engine = new WasmEngine();
const child = chainHistory(engine);
const wire = idToHex(child);
const verified = engine.verify(idFromHex(wire));
assert(bytesEqual(verified, child), "distinction round-trip failed");

console.log("distinction wire round-trip:");
console.log(`  child  = ${wire}`);
console.log(`  verify(idFromHex(wire)) == child  ✓`);
console.log();

// ---- (2) order independence — same pairs, different order
const engineA = new WasmEngine();
const [history, root] = buildHistory(engineA, seededPairs(20260101n, 24));
const shuffled = reorder(history, engineA.d0(), engineA.d1());
const moved = history.filter((p, i) => toHex(p[0]) + toHex(p[1]) !== toHex(shuffled[i][0]) + toHex(shuffled[i][1])).length;
assert(moved > 0, "the reordering degenerated to the original order");

const engineB = new WasmEngine();
for (const [l, r] of shuffled) engineB.synthesize(l, r);

const projA = engineA.projectAdjacency(root, "undirected", 2);
const projB = engineB.projectAdjacency(root, "undirected", 2);
const bytesA = projA.canonicalBytes();
const idA = projA.projectionId();

assert(bytesEqual(bytesA, projB.canonicalBytes()), "Laws 8/9 violated: canonical bytes diverged under reorder");
assert(bytesEqual(idA, projB.projectionId()), "projection id diverged: the two engines asked different questions");

console.log(`order independence (${history.length} pairs, ${moved} of them in a new position):`);
console.log(`  root  = ${idToHex(root)}`);
console.log(`  axis  = direction=undirected, boundary=hops(2), signal=Adjacency`);
console.log(`  entries                            = ${(projA.entries() as unknown[]).length}`);
console.log(`  canonicalBytes.length              = ${bytesA.length}`);
console.log(`  bytewise original == reordered     ✓`);
console.log(`  projectionId matches               ✓  (necessary, not sufficient — it`);
console.log(`                                         hashes the axes, not the output)`);
console.log();

// ---- (3) tamper at the wire boundary
//
// The spec §10.1 history, so these are the offsets and outcomes pinned in
// tests/projection_wire_tamper.rs. The 55-byte header puts the first
// entry's identifier at 55..71 and its u32 output length at 71..75.
const engineC = new WasmEngine();
const c = chainHistory(engineC);
const sent = engineC.projectAdjacency(c, "upstream", 2);
const clean = sent.canonicalBytes();

const restored = engineC.restoreProjectionAdjacency(clean);
assert(bytesEqual(restored.projectionId(), sent.projectionId()), "clean restore changed the id");

const badId = Uint8Array.from(clean);
badId[63] = (badId[63] + 1) & 0xff;
const badLen = Uint8Array.from(clean);
badLen[71] = (badLen[71] + 1) & 0xff;
const badBoundary = Uint8Array.from(clean);
badBoundary[22] = 0x03; // Hops(2) -> Hops(3)

assert(refuses(() => engineC.restoreProjectionAdjacency(badId)), "a tampered entry identifier must be refused");
assert(refuses(() => engineC.restoreProjectionAdjacency(badLen)), "a tampered output length must be refused");
const accepted = engineC.restoreProjectionAdjacency(badBoundary);

console.log("tamper rejection (restoreProjectionAdjacency):");
console.log(`  clean bytes                        restored ✓`);
console.log(`  entry identifier, byte 63          refused  ✓`);
console.log(`  declared output length, byte 71    refused  ✓`);
console.log(`  boundary payload, byte 22          ACCEPTED`);
console.log();
console.log(`  Not a hole in the restore, and not hidden here. Rooted at c this graph`);
console.log(`  has diameter 2, so Hops(2) and Hops(3) enumerate the same entries: the`);
console.log(`  tampered bytes are bytes this engine would have produced, and the`);
console.log(`  guarantee is that unproducible bytes are refused. What it costs is`);
console.log(`  agreement about which read this is — the restored id differs:`);
console.log(`    sender   = ${toHex(sent.projectionId())}`);
console.log(`    restored = ${toHex(accepted.projectionId())}`);
console.log(`  Same bytes, different question. Compare the id out of band.`);
console.log();

// ---- (4) the hand-off: a history another process wrote
const artifactPath = new URL("../handoff/history.json", import.meta.url);
let artifact: {
    spec: { root: string; direction: "upstream" | "downstream" | "undirected"; hops: number; signal: string };
    pairs: Array<[string, string]>;
    expected_projection_id: string;
    expected_canonical_bytes: string;
    tampered_canonical_bytes: string;
};
try {
    artifact = JSON.parse(readFileSync(artifactPath, "utf8"));
} catch {
    throw new Error(
        `no hand-off artifact at ${artifactPath.pathname}.\n` +
            `Run the producer first, from the repository root:\n` +
            `  cargo run --example 04_wire_format_roundtrip`,
    );
}
assert(artifact.spec.signal === "adjacency", `unexpected signal: ${artifact.spec.signal}`);

const engineD = new WasmEngine();
const theirHistory: Pair[] = artifact.pairs.map(([l, r]) => [idFromHex(l), idFromHex(r)]);
// Our own order, not theirs. The producer's order is in the artifact and is
// deliberately not used: reproducing bytes by replaying the sender's exact
// sequence would be determinism again.
const ourOrder = reorder(theirHistory, engineD.d0(), engineD.d1());
const reordered = ourOrder.filter(
    (p, i) => toHex(p[0]) + toHex(p[1]) !== toHex(theirHistory[i][0]) + toHex(theirHistory[i][1]),
).length;
assert(reordered > 0, "this process replayed the producer's order, which proves less");
for (const [l, r] of ourOrder) engineD.synthesize(l, r);

const theirRoot = idFromHex(artifact.spec.root);
const ours = engineD.projectAdjacency(theirRoot, artifact.spec.direction, artifact.spec.hops);
const theirBytes = fromHex(artifact.expected_canonical_bytes);
const theirId = fromHex(artifact.expected_projection_id);

assert(bytesEqual(ours.canonicalBytes(), theirBytes), "cross-process: canonical bytes diverged");
assert(bytesEqual(ours.projectionId(), theirId), "cross-process: projection id diverged");
assert(
    refuses(() => engineD.restoreProjectionAdjacency(fromHex(artifact.tampered_canonical_bytes))),
    "cross-process: the producer's tampered payload was accepted here",
);
assert(
    bytesEqual(engineD.restoreProjectionAdjacency(theirBytes).projectionId(), theirId),
    "cross-process: the producer's clean payload did not restore",
);

console.log("cross-process hand-off (Rust wrote, this Node process read):");
console.log(`  pairs read from artifact           = ${theirHistory.length}`);
console.log(`  replayed in a different order      ✓  (${reordered} pairs moved)`);
console.log(`  canonicalBytes match the producer  ✓  (${theirBytes.length} bytes)`);
console.log(`  projectionId matches the producer  ✓`);
console.log(`  producer's clean payload restores  ✓`);
console.log(`  producer's tampered payload        refused ✓`);
console.log();
console.log(`projection id (both processes) = ${artifact.expected_projection_id}`);
