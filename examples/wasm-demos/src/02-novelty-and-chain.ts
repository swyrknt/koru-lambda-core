// Tier 2 — novelty_and_chain. 3-generation lattice + `SynthesisOutcome`
// discriminated-union narrowing on the operator return.
//
//   npm run demo:2
//
// Mirrors examples/02_novelty_and_chain.rs.

import { WasmEngine, idToHex } from "koru-lambda-core";
import type { SynthesisOutcome } from "koru-lambda-core";

const engine = new WasmEngine();
const d0 = engine.d0();
const d1 = engine.d1();

// Three generations.
const c = engine.synthesize(d0, d1);
const d = engine.synthesize(c, d0);
const e = engine.synthesize(d, c);

console.log("lattice:");
console.log(`  c = synth(d0, d1) = ${idToHex(c)}`);
console.log(`  d = synth(c,  d0) = ${idToHex(d)}`);
console.log(`  e = synth(d,  c ) = ${idToHex(e)}`);
console.log();

// (d0, d1) is already in the graph — expect Existing.
// TS narrows on `kind` via the typescript_custom_section type.
const existingOutcome = engine.synthesizeNovel(d0, d1) as SynthesisOutcome;
if (existingOutcome.kind !== "existing") {
    throw new Error(`Law-7 broken: (d0, d1) should saturate, got ${existingOutcome.kind}`);
}
console.log(`re-synth(d0, d1) -> Existing(${idToHex(existingOutcome.distinction)})`);

// (d0, d) is a fresh pair — expect Novel.
const novelOutcome = engine.synthesizeNovel(d0, d) as SynthesisOutcome;
if (novelOutcome.kind !== "novel") {
    throw new Error(`(d0, d) was not previously synthesized, got ${novelOutcome.kind}`);
}
console.log(`  synth(d0, d ) -> Novel(   ${idToHex(novelOutcome.distinction)})`);

console.log();
console.log(`distinctions  = ${engine.distinctionCount()}`);
console.log(`relationships = ${engine.relationshipCount()}`);
