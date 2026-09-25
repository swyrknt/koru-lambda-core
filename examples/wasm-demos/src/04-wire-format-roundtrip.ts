// Tier 4 — wire_format_roundtrip. Content addressing at two scales:
//
//   1. Distinction: `idToHex` → wire → `idFromHex` → `verify`.
//   2. Projection: two engines / same history →
//      byte-identical `canonicalBytes` (Cond D, live).
//
//   npm run demo:4
//
// Mirrors examples/04_wire_format_roundtrip.rs.

import { WasmEngine, idToHex, idFromHex } from "koru-lambda-core";

// `idToHex` is distinction-only (16 bytes); projection ids are 32
// bytes, so the twin-engine section below hex-encodes locally.
function toHex(bytes: Uint8Array): string {
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
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

function runHistory(engine: WasmEngine): Uint8Array {
    const d0 = engine.d0();
    const d1 = engine.d1();
    const c = engine.synthesize(d0, d1);
    const d = engine.synthesize(c, d0);
    engine.synthesize(d, c);
    return c;
}

// ---- (1) distinction wire round-trip through the trust boundary
const engine = new WasmEngine();
const child = runHistory(engine);
const wire = idToHex(child);
const raw = idFromHex(wire);
const verified = engine.verify(raw);
assert(bytesEqual(verified, child), "distinction round-trip failed");

console.log("distinction wire round-trip:");
console.log(`  child  = ${wire}`);
console.log(`  verify(idFromHex(wire)) == child  ✓`);
console.log();

// ---- (2) projection Cond D — twin engines
const engineA = new WasmEngine();
const engineB = new WasmEngine();
const cA = runHistory(engineA);
const cB = runHistory(engineB);
assert(bytesEqual(cA, cB), "content addressing broken");

const projA = engineA.projectAdjacency(cA, "upstream", 2);
const projB = engineB.projectAdjacency(cB, "upstream", 2);

const bytesA = projA.canonicalBytes();
const bytesB = projB.canonicalBytes();
const idA = projA.projectionId();
const idB = projB.projectionId();

assert(bytesEqual(bytesA, bytesB), "Cond D violated: canonical bytes diverged");
assert(bytesEqual(idA, idB), "Cond D violated: projection id diverged");

console.log("projection Cond D (two engines, same history):");
console.log(`  engine A canonicalBytes.length = ${bytesA.length}`);
console.log(`  engine B canonicalBytes.length = ${bytesB.length}`);
console.log(`  bytewise A == B                     ✓`);
console.log(`  projectionId A == B                 ✓`);
console.log();
console.log(`projection id (both engines) = ${toHex(idA)}`);
