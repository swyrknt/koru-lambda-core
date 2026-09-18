import {
    WasmEngine,
    idToHex,
    idFromHex,
} from "koru-lambda-core";
import type {
    SynthesisOutcome,
    AdjacencyEntry,
} from "koru-lambda-core";

// `wasm-pack --target bundler` auto-initializes on import (see the
// pkg/koru_lambda_core.js side-effect `wasm.__wbindgen_start()`). No
// explicit `init()` call — vite-plugin-wasm + vite-plugin-top-level-await
// handle the async import at bundle time.
async function main(): Promise<void> {
    const engine = new WasmEngine();

    // Primordials — 16-byte Uint8Arrays.
    const d0 = engine.d0();
    const d1 = engine.d1();

    // Synthesize d0 x d1 -> child.
    const child = engine.synthesize(d0, d1);

    // synthesizeNovel — discriminated-union outcome. Because we already
    // synthesized (d0, d1) above, this call must yield `existing`.
    // TypeScript narrows on `kind` via the custom-section type.
    const outcome = engine.synthesizeNovel(d0, d1) as SynthesisOutcome;

    // Trust boundary — round-trip through hex and verify.
    const raw = idFromHex(idToHex(child));
    const verified = engine.verify(raw);
    const verifiedMatchesChild = verified.every(
        (b: number, i: number) => b === child[i],
    );

    // Projection — 2-hop upstream cone anchored at the child.
    const projection = engine.projectAdjacency(child, "upstream", 2);
    const entries = projection.entries() as AdjacencyEntry[];
    const canonicalBytes = projection.canonicalBytes();
    const projectionId = projection.projectionId();

    const summary = {
        d0Hex: idToHex(d0),
        d1Hex: idToHex(d1),
        childHex: idToHex(child),
        outcomeKind: outcome.kind,
        outcomeDistinctionHex: idToHex(outcome.distinction),
        verifiedMatchesChild,
        distinctionCount: Number(engine.distinctionCount()),
        relationshipCount: Number(engine.relationshipCount()),
        projectionSize: entries.length,
        canonicalBytesLen: canonicalBytes.length,
        projectionIdHex: idToHex(projectionId),
    };

    const root = document.getElementById("root");
    if (!root) return;
    root.innerHTML = `
        <h1>koru-lambda-core wasm quickstart</h1>
        <p>See <code>src/main.ts</code>. Every JS input crosses the
        Axiom-4 trust boundary via <code>engine.verify()</code> before
        reaching a hot path.</p>
        <pre>${JSON.stringify(summary, null, 2)}</pre>
    `;
}

main().catch((err: unknown) => {
    console.error(err);
    const msg = err instanceof Error ? err.message : String(err);
    document.body.textContent = `Error: ${msg}`;
});
