// Tier 3 — projection_composition. All three substrate signal
// projections (Adjacency, Degree, HopDistance) on one anchor.
//
//   npm run demo:3
//
// Mirrors examples/03_projection_composition.rs.

import { WasmEngine, idToHex } from "koru-lambda-core";

// `idToHex` is distinction-only (16 bytes); projection ids are 32
// bytes, so we hex-encode locally.
function toHex(bytes: Uint8Array): string {
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

function hexPrefix(bytes: Uint8Array): string {
    return `${toHex(bytes).slice(0, 12)}…`;
}

const engine = new WasmEngine();
const d0 = engine.d0();
const d1 = engine.d1();

const c = engine.synthesize(d0, d1);
const d = engine.synthesize(c, d0);
engine.synthesize(d, c); // grows the cone; projections anchor at c

const adjacency = engine.projectAdjacency(c, "upstream", 2);
const degree = engine.projectDegree(c, "upstream", 2);
const hopDistance = engine.projectHopDistance(c, "upstream", 2);

console.log(`anchor: c = ${idToHex(c)}`);
console.log(`axis:   direction=upstream, boundary=hops(2)`);
console.log();

const rows: Array<[string, number, string, number]> = [
    ["Adjacency", (adjacency.entries() as unknown[]).length, hexPrefix(adjacency.projectionId()), adjacency.canonicalBytes().length],
    ["Degree", (degree.entries() as unknown[]).length, hexPrefix(degree.projectionId()), degree.canonicalBytes().length],
    ["HopDistance", (hopDistance.entries() as unknown[]).length, hexPrefix(hopDistance.projectionId()), hopDistance.canonicalBytes().length],
];

for (const [name, len, idPrefix, cbLen] of rows) {
    console.log(
        `${name.padEnd(12)} len=${String(len).padStart(2)}  id=${idPrefix}  canonical=${cbLen}B`,
    );
}
