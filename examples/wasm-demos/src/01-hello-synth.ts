// Tier 1 — hello_synth. Primordials + a single synthesis.
//
//   npm run demo:1
//
// Mirrors examples/01_hello_synth.rs.

import { WasmEngine, idToHex } from "koru-lambda-core";

const engine = new WasmEngine();

const d0 = engine.d0();
const d1 = engine.d1();
const child = engine.synthesize(d0, d1);

console.log(`d0    = ${idToHex(d0)}`);
console.log(`d1    = ${idToHex(d1)}`);
console.log(`child = ${idToHex(child)}`);
console.log();
console.log(`distinctions  = ${engine.distinctionCount()}`);
console.log(`relationships = ${engine.relationshipCount()}`);
