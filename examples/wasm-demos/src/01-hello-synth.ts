// Tier 1 — hello_synth. Predict a child id with a plain SHA-256 call,
// *then* ask the engine, and confirm they agree.
//
//   npm run demo:1
//
// The order is the point: the prediction cannot be a readback, because
// nothing here touches the engine before the hash is already computed. If
// the two sixteen-byte values agree, the rule is the rule — a distinction's
// identity is `SHA-256(min ‖ max)` truncated to the leading sixteen bytes,
// with no registry, counter, clock or node identity consulted. The last
// block prints a command you can paste into a shell to check it without
// this package at all.
//
// `createHash` here is Node's, not the wasm module's. That matters: the
// prediction and the engine do not share an implementation of SHA-256.
//
// Mirrors examples/01_hello_synth.rs.

import { createHash } from "node:crypto";
import { WasmEngine, idToHex } from "koru-lambda-core";

function assert(cond: boolean, msg: string): void {
    if (!cond) throw new Error(msg);
}

function toHex(bytes: Uint8Array): string {
    return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

/** Bytewise lexicographic compare, which is what `a.0 <= b.0` is in Rust. */
function lessOrEqual(a: Uint8Array, b: Uint8Array): boolean {
    for (let i = 0; i < a.length; i++) {
        if (a[i] !== b[i]) return a[i] < b[i];
    }
    return true;
}

/**
 * The content-addressing rule, written out independently of the package:
 * SHA-256 over the canonically ordered pair, leading sixteen bytes. The pair
 * is unordered, so the operator has to pick one arrangement and commit.
 */
function predict(a: Uint8Array, b: Uint8Array): Uint8Array {
    const [first, second] = lessOrEqual(a, b) ? [a, b] : [b, a];
    const h = createHash("sha256");
    h.update(first);
    h.update(second);
    return new Uint8Array(h.digest()).slice(0, 16);
}

// ---- predict, before any engine exists
const d0Bytes = new Uint8Array(16);
const d1Bytes = new Uint8Array(16);
d1Bytes[0] = 1;

const predictedChild = predict(d0Bytes, d1Bytes);
const predictedGrandchild = predict(predictedChild, d0Bytes);

console.log("predicted, with a plain SHA-256 call and no engine:");
console.log(`  child      = ${toHex(predictedChild)}`);
console.log(`  grandchild = ${toHex(predictedGrandchild)}`);
console.log();

// ---- now ask the engine
const engine = new WasmEngine();
const d0 = engine.d0();
const d1 = engine.d1();
const child = engine.synthesize(d0, d1);
const grandchild = engine.synthesize(child, d0);

console.log(`d0    = ${idToHex(d0)}`);
console.log(`d1    = ${idToHex(d1)}`);
console.log(`child = ${idToHex(child)}`);
console.log();
console.log(`distinctions  = ${engine.distinctionCount()}`);
console.log(`relationships = ${engine.relationshipCount()}`);
console.log();

// ---- confirm
assert(toHex(d0) === toHex(d0Bytes), "d0 is not [0x00; 16]");
assert(toHex(d1) === toHex(d1Bytes), "d1 is not [0x01, 0x00 * 15]");
assert(toHex(child) === toHex(predictedChild), "the prediction missed the child");
assert(
    toHex(grandchild) === toHex(predictedGrandchild),
    "the prediction missed the grandchild — the rule does not compose",
);

console.log("confirmed:");
console.log("  engine child      == predicted  ✓");
console.log("  engine grandchild == predicted  ✓  (the rule composes: the child's");
console.log("                                      own bytes are the next input)");
console.log();

// ---- check it by hand
const [lo, hi] = lessOrEqual(d0Bytes, d1Bytes) ? [d0Bytes, d1Bytes] : [d1Bytes, d0Bytes];
const escaped = Array.from([...lo, ...hi], (b) => `\\x${b.toString(16).padStart(2, "0")}`).join("");
console.log("check it yourself — no Rust, no npm:");
console.log(`  printf '${escaped}' | shasum -a 256 | cut -c1-32`);
console.log(`  => ${idToHex(child)}`);
