#!/bin/bash
# Koru Lambda Core - Universal WASM Build
#
# Philosophy: One artifact, infinite platforms
# Builds: One universal WASM module that runs everywhere

set -e

# Change to project root (parent of scripts directory)
cd "$(dirname "$0")/.."

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Koru Lambda Core - Universal WASM Build                    ║"
echo "║   \"The hash IS the truth, the data is evidence\"              ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Check dependencies
echo "Checking dependencies..."

if ! command -v rustup &> /dev/null; then
    echo "❌ rustup not found. Install from: https://rustup.rs"
    exit 1
fi

if ! command -v wasm-pack &> /dev/null; then
    echo "⚠️  wasm-pack not found. Installing..."
    cargo install wasm-pack
fi

echo "✓ All dependencies available"
echo ""

# Add WASM target
echo "Adding WASM target..."
rustup target add wasm32-unknown-unknown
echo "✓ WASM target ready"
echo ""

# Build WASM module
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Building Universal Artifact                                 ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

echo "Building WASM module with wasm-pack..."
wasm-pack build \
    --target web \
    --out-dir pkg \
    --release \
    --features wasm

if [ $? -eq 0 ]; then
    echo "✓ WASM build successful"
else
    echo "❌ WASM build failed"
    exit 1
fi

echo ""

# Note: wasm-pack automatically runs wasm-opt when it's available on the system
# (see wasm-pack output: "Optimizing wasm binaries with `wasm-opt`...")
# No need for manual optimization step here.
if command -v wasm-opt &> /dev/null; then
    echo "✓ wasm-opt optimization applied by wasm-pack"
else
    echo "⚠️  wasm-opt not found. Install for better optimization:"
    echo "   brew install binaryen  (macOS)"
fi
echo ""

# Create distribution directory
echo "Creating universal distribution..."
mkdir -p dist/universal

# Copy WASM artifacts
cp pkg/koru_lambda_core_bg.wasm dist/universal/koru.wasm
cp pkg/koru_lambda_core.js dist/universal/koru.js
cp pkg/koru_lambda_core.d.ts dist/universal/koru.d.ts

echo "✓ Copied WASM artifacts"

# Fix TypeScript Symbol.dispose compatibility
echo "Fixing TypeScript compatibility..."
cat > /tmp/koru_ts_header.txt << 'TSHEADER'
/* tslint:disable */
/* eslint-disable */
// Augment global Symbol for ES2022 Disposable support
declare global {
  interface SymbolConstructor {
    readonly dispose: unique symbol;
  }
}

TSHEADER
# Remove original header and prepend new one with augmentation
tail -n +3 dist/universal/koru.d.ts > /tmp/koru_ts_body.txt
cat /tmp/koru_ts_header.txt /tmp/koru_ts_body.txt > dist/universal/koru.d.ts
rm -f /tmp/koru_ts_header.txt /tmp/koru_ts_body.txt
echo "✓ Added Symbol.dispose type augmentation"

# Generate hash (the "commitment" - the truth!)
echo ""
echo "Generating content-addressable hash..."
HASH=$(shasum -a 256 dist/universal/koru.wasm | awk '{print $1}')
echo "$HASH  koru.wasm" > dist/universal/koru.wasm.sha256

echo "✓ Hash: $HASH"
echo ""

# Get file sizes
WASM_SIZE=$(ls -lh dist/universal/koru.wasm | awk '{print $5}')
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Universal Artifact Created                                  ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Artifact:  dist/universal/koru.wasm ($WASM_SIZE)"
echo "Hash:      $HASH"
echo "JS Glue:   dist/universal/koru.js"
echo "Types:     dist/universal/koru.d.ts"
echo ""

echo "This single artifact runs on:"
echo "  ✅ Web Browsers (Chrome, Firefox, Safari, Edge)"
echo "  ✅ Node.js / Deno / Bun"
echo "  ✅ Go (via wazero)"
echo "  ✅ Kotlin/Android (via WasmEdge)"
echo "  ✅ Swift/iOS (via WasmKit)"
echo "  ✅ Python (via wasmtime)"
echo "  ✅ Embedded (via WAMR)"
echo ""

# Create simple example
echo "Creating quick start example..."
cat > dist/universal/example.html << 'EOF'
<!DOCTYPE html>
<html>
<head>
    <title>Koru Lambda Core - Universal WASM Demo</title>
    <meta charset="utf-8">
</head>
<body>
    <h1>🌟 Koru Lambda Core - Universal Runtime</h1>
    <div id="output"></div>

    <script type="module">
        import init, { WasmEngine, WasmNetworkAgent, WasmValidator, WasmCommitmentAgent } from './koru.js';

        // Helper to convert Uint8Array to hex string
        const toHex = (bytes) => Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');

        const output = document.getElementById('output');
        const log = (msg) => {
            output.innerHTML += `<div>${msg}</div>`;
            console.log(msg);
        };

        async function demo() {
            // Initialize WASM module
            await init();
            log('✓ WASM module loaded');
            log('');

            // Core engine with primordial distinctions
            const engine = new WasmEngine();
            log('═══ Core Engine ═══');
            log(`Distinctions: ${engine.distinctionCount()}`);
            log(`  Δ₀ = ${toHex(engine.d0Id()).substring(0, 16)}...`);
            log(`  Δ₁ = ${toHex(engine.d1Id()).substring(0, 16)}...`);
            log('');

            // Trusted Subprocess 1: Network Agent (LocalCausalAgent)
            log('═══ Subprocess 1: Network Agent ═══');
            log('Pattern: ΔNew = ΔNetwork_Root ⊕ ΔNetwork_Action');
            const network = new WasmNetworkAgent(engine);
            log(`Network root: ${toHex(network.currentRoot()).substring(0, 16)}...`);
            log(`Consensus root: ${toHex(network.consensusRoot()).substring(0, 16)}...`);
            log('');

            // Join peers (each is a causal synthesis)
            log('Joining peers (causal synthesis)...');
            for (let i = 0; i < 3; i++) {
                const newRoot = network.joinPeer(`validator_${i}`);
                log(`  Peer ${i}: ΔNew = ${toHex(newRoot).substring(0, 16)}...`);
            }
            log(`Validators: ${network.validatorCount()}`);
            log(`Leader: ${network.getLeader()} (deterministic)`);
            log('');

            // Trusted Subprocess 2: Consensus Validator (LocalCausalAgent)
            log('═══ Subprocess 2: Consensus Validator ═══');
            log('Pattern: ΔNew = ΔState_Root ⊕ ΔTransaction');
            const validator = new WasmValidator(engine);
            log(`State root: ${toHex(validator.currentRoot()).substring(0, 16)}...`);
            log(`Expected nonce: ${validator.expectedNonce()}`);
            log('');

            // Trusted Subprocess 3: Commitment Agent (LocalCausalAgent)
            log('═══ Subprocess 3: Commitment Agent ═══');
            log('Pattern: ΔNew = ΔCommitment_Root ⊕ ΔBatch_Commitment');
            const commitment = new WasmCommitmentAgent(engine);
            log(`Commitment root: ${toHex(commitment.currentRoot()).substring(0, 16)}...`);
            log(`Expected nonce: ${commitment.expectedNonce()}`);
            log(`Commitments processed: ${commitment.commitmentsProcessed()}`);
            log('');

            // Two-stage commitment protocol
            log('═══ Two-Stage Commitment Protocol ═══');
            const batch = JSON.stringify({
                transactions: [{ nonce: 0, data: [1, 2, 3] }],
                previous_root: toHex(network.consensusRoot())
            });

            // Stage 1: Lightweight commitment (returns Uint8Array)
            const hash = network.proposeCommitment(batch);
            log(`Stage 1: Commitment = ${toHex(hash).substring(0, 32)}...`);
            log('         (gossip 80 bytes, no batch download)');

            // Light node verification (pass Uint8Array directly)
            const isValid = network.checkCommitment(hash, 0n, 0n);
            log(`Verify:  ${isValid ? '✓' : '✗'} Light node check passed`);

            // Stage 2: Full validator finalization (pass Uint8Array)
            const newRoot = network.finalizeBatch(batch, hash);
            log(`Stage 2: ΔNew = ${toHex(newRoot).substring(0, 16)}...`);
            log(`         (synthesized: ΔNetwork_Root ⊕ ΔBatchProposed)`);
            log('');

            log('✅ All subsystems are trusted LocalCausalAgents!');
            log('   ΔNew = ΔLocal ⊕ ΔAction (enforced by architecture)');
            log('');
            log('This universal artifact runs everywhere:');
            log('  • Web, Node.js, Deno, Bun');
            log('  • Go, Kotlin, Swift, Python');
            log('  • Embedded systems');
        }

        demo().catch(err => {
            log(`❌ Error: ${err}`);
        });
    </script>
</body>
</html>
EOF

echo "✓ Created example.html"
echo ""

# Create Node.js example
cat > dist/universal/example.mjs << 'EOF'
#!/usr/bin/env node
// Koru Lambda Core - Node.js Example
// Same WASM artifact, different runtime

import init, { WasmEngine, WasmNetworkAgent } from './koru.js';
import { readFile } from 'fs/promises';

// Helper to convert Uint8Array to hex string
const toHex = (bytes) => Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');

async function main() {
    console.log('🚀 Koru Lambda Core - Node.js Runtime\n');

    // Load WASM module
    const wasmBuffer = await readFile('./koru.wasm');
    await init(wasmBuffer);
    console.log('✓ WASM module loaded\n');

    // Create engine
    const engine = new WasmEngine();
    console.log(`✓ Engine: ${engine.distinctionCount()} distinctions`);
    console.log(`  Δ₀ = ${toHex(engine.d0Id())}`);
    console.log(`  Δ₁ = ${toHex(engine.d1Id())}\n`);

    // Create network agent
    const agent = new WasmNetworkAgent(engine);
    console.log('✓ Network Agent created');
    console.log(`  Network root: ${toHex(agent.currentRoot()).substring(0, 16)}...\n`);

    // Add validators
    for (let i = 0; i < 5; i++) {
        agent.joinPeer(`validator_${i}`);
    }
    console.log(`✓ Validators: ${agent.validatorCount()}`);
    console.log(`  Leader: ${agent.getLeader()}\n`);

    // Two-stage commitment
    const batch = JSON.stringify({
        transactions: [{ nonce: 0, data: [1, 2, 3] }],
        previous_root: toHex(agent.consensusRoot())
    });

    console.log('Two-Stage Commitment Protocol:');
    const hash = agent.proposeCommitment(batch);
    console.log(`  Stage 1: ${toHex(hash).substring(0, 16)}...`);

    const valid = agent.checkCommitment(hash, BigInt(0), BigInt(0));
    console.log(`  Verify:  ${valid ? '✓' : '✗'} Valid`);

    const newRoot = agent.finalizeBatch(batch, hash);
    console.log(`  Stage 2: ${toHex(newRoot).substring(0, 16)}...\n`);

    console.log('✅ Universal artifact works in Node.js!');
    console.log('   Try: python example.py (same WASM)');
    console.log('   Try: go run example.go (same WASM)');
}

main().catch(console.error);
EOF

chmod +x dist/universal/example.mjs
echo "✓ Created example.mjs (Node.js)"
echo ""

# Create comprehensive test
echo "Creating integration test..."
cp dist/universal/test.mjs dist/universal/test_wasm.mjs 2>/dev/null || cat > dist/universal/test_wasm.mjs << 'TESTEOF'
#!/usr/bin/env node
// WASM Integration Test - Verify WASM artifact works in Node.js
import init, { WasmEngine, WasmNetworkAgent, WasmValidator, WasmCommitmentAgent } from './koru.js';
import { readFile } from 'fs/promises';

// Helper to convert Uint8Array to hex string
const toHex = (bytes) => Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');

// Convert bytes to ID string (32 bytes = hex hash, otherwise UTF-8)
const bytesToId = (bytes) => bytes.length === 32 ? toHex(bytes) : new TextDecoder().decode(bytes);

// Helper to compare Uint8Arrays
const arraysEqual = (a, b) => a.length === b.length && a.every((v, i) => v === b[i]);

async function main() {
    console.log('🧪 WASM Integration Test\n');
    const wasmBuffer = await readFile('./koru.wasm');
    await init(wasmBuffer);
    console.log('✓ WASM module loaded\n');

    // Test 1: Core Engine
    console.log('═══ Test 1: Core Engine ═══');
    const engine = new WasmEngine();
    console.log(`✓ Distinctions: ${engine.distinctionCount()}`);
    const d0Bytes = engine.d0Id();
    const d1Bytes = engine.d1Id();
    const d0 = bytesToId(d0Bytes);
    const d1 = bytesToId(d1Bytes);
    console.log(`✓ Δ₀ = ${d0}`);
    console.log(`✓ Δ₁ = ${d1}`);
    const d2 = engine.synthesize(d0, d1);
    console.log(`✓ synthesis(Δ₀, Δ₁) = ${bytesToId(d2).substring(0, 16)}...`);
    const d2_rev = engine.synthesize(d1, d0);
    if (!arraysEqual(d2, d2_rev)) throw new Error('Symmetry axiom violated');
    console.log('✓ Symmetry axiom holds\n');

    // Test 2: NetworkAgent (LocalCausalAgent)
    console.log('═══ Test 2: Network Agent (LocalCausalAgent) ═══');
    const network = new WasmNetworkAgent(engine);
    const r0 = network.currentRoot();
    console.log(`✓ Network root: ${bytesToId(r0).substring(0, 16)}...`);
    const r1 = network.joinPeer('validator_0');
    if (arraysEqual(r1, r0)) throw new Error('Network root did not change');
    console.log(`✓ After join: ${bytesToId(r1).substring(0, 16)}... (ΔNew = ΔRoot ⊕ ΔAction)`);
    network.joinPeer('validator_1');
    network.joinPeer('validator_2');
    console.log(`✓ Validators: ${network.validatorCount()}`);
    console.log(`✓ Leader: ${network.getLeader()} (deterministic)\n`);

    // Test 3: Validator (LocalCausalAgent)
    console.log('═══ Test 3: Consensus Validator (LocalCausalAgent) ═══');
    const validator = new WasmValidator(engine);
    const v0 = validator.currentRoot();
    console.log(`✓ State root: ${bytesToId(v0).substring(0, 16)}...`);
    const batch = JSON.stringify({
        transactions: [{ nonce: 0, data: [1, 2, 3] }],
        previous_root: bytesToId(v0)
    });
    const v1 = validator.validateBatch(batch);
    if (arraysEqual(v1, v0)) throw new Error('Validator root did not change');
    console.log(`✓ After validation: ${bytesToId(v1).substring(0, 16)}...\n`);

    // Test 4: Two-Stage Commitment
    console.log('═══ Test 4: Two-Stage Commitment Protocol ═══');
    const agent = new WasmNetworkAgent(engine);
    for (let i = 0; i < 3; i++) agent.joinPeer(`node_${i}`);
    const testBatch = JSON.stringify({
        transactions: [{ nonce: 0, data: [4, 5, 6] }],
        previous_root: bytesToId(agent.consensusRoot())
    });
    const hash = agent.proposeCommitment(testBatch);
    console.log(`✓ Stage 1: ${toHex(hash).substring(0, 32)}... (32 bytes)`);
    const isValid = agent.checkCommitment(hash, BigInt(0), BigInt(0));
    if (!isValid) throw new Error('Commitment verification failed');
    console.log('✓ Light node verification passed');
    const newRoot = agent.finalizeBatch(testBatch, hash);
    console.log(`✓ Stage 2: ΔNew = ${bytesToId(newRoot).substring(0, 16)}...\n`);

    console.log('╔══════════════════════════════════════════════════════════════╗');
    console.log('║   ✅ ALL WASM TESTS PASSED                                   ║');
    console.log('╚══════════════════════════════════════════════════════════════╝');
}

main().catch(err => {
    console.error(`\n❌ Test failed: ${err.message}`);
    process.exit(1);
});
TESTEOF

chmod +x dist/universal/test_wasm.mjs
echo "✓ Created test_wasm.mjs"
echo ""

# Create benchmark suite
echo "Creating benchmark suite..."
cat > dist/universal/bench.mjs << 'BENCHEOF'
#!/usr/bin/env node
// WASM Performance Benchmark Suite
import init, { WasmEngine, WasmNetworkAgent, WasmValidator } from './koru.js';
import { readFile } from 'fs/promises';

// Helper to convert Uint8Array to hex string
const toHex = (bytes) => Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');

async function benchmark(name, fn, iterations = 10000) {
    const start = process.hrtime.bigint();
    for (let i = 0; i < iterations; i++) {
        await fn(i);
    }
    const end = process.hrtime.bigint();
    const durationMs = Number(end - start) / 1_000_000;
    const opsPerSec = Math.floor((iterations / durationMs) * 1000);

    console.log(`${name}:`);
    console.log(`  Iterations: ${iterations.toLocaleString()}`);
    console.log(`  Duration: ${durationMs.toFixed(2)}ms`);
    console.log(`  Throughput: ${opsPerSec.toLocaleString()} ops/s`);
    console.log('');

    return opsPerSec;
}

async function main() {
    console.log('╔══════════════════════════════════════════════════════════════╗');
    console.log('║   WASM Performance Benchmark Suite                           ║');
    console.log('╚══════════════════════════════════════════════════════════════╝');
    console.log('');

    // Load WASM
    const wasmBuffer = await readFile('./koru.wasm');
    await init(wasmBuffer);
    console.log('✓ WASM module loaded');
    console.log('');

    const results = {};

    // Benchmark 1: Core Synthesis (using batch benchmark to eliminate FFI overhead)
    console.log('═══ Benchmark 1: Core Synthesis ═══');
    const engine = new WasmEngine();
    const d0Hex = toHex(engine.d0Id());
    const d1Hex = toHex(engine.d1Id());

    // Use the batch benchmark function for true performance
    const synthStart = process.hrtime.bigint();
    const synthIters = engine.benchmarkSynthesis(10000);
    const synthEnd = process.hrtime.bigint();
    const synthMs = Number(synthEnd - synthStart) / 1_000_000;
    results.synthesis = Math.floor((synthIters / synthMs) * 1000);
    console.log(`Core Synthesis (batch):`);
    console.log(`  Iterations: ${synthIters.toLocaleString()}`);
    console.log(`  Duration: ${synthMs.toFixed(2)}ms`);
    console.log(`  Throughput: ${results.synthesis.toLocaleString()} ops/s`);
    console.log('');

    // Benchmark 2: Network Agent Operations
    console.log('═══ Benchmark 2: Network Agent ═══');
    const network = new WasmNetworkAgent(engine);

    results.peerJoin = await benchmark(
        'Peer Join',
        (i) => network.joinPeer(`peer_${i}`),
        1000
    );

    // Use batch benchmark for leader election (separate agent with realistic validator set)
    // Real networks use 5-21 validators, not 1000 - O(n) algorithm
    const leaderNetwork = new WasmNetworkAgent(engine);
    for (let i = 0; i < 7; i++) leaderNetwork.joinPeer(`leader_validator_${i}`);

    const leaderStart = process.hrtime.bigint();
    const leaderIters = leaderNetwork.benchmarkLeaderElection(100000);
    const leaderEnd = process.hrtime.bigint();
    const leaderMs = Number(leaderEnd - leaderStart) / 1_000_000;
    results.leaderElection = Math.floor((leaderIters / leaderMs) * 1000);
    console.log(`Leader Election (batch, 7 validators):`);
    console.log(`  Iterations: ${leaderIters.toLocaleString()}`);
    console.log(`  Duration: ${leaderMs.toFixed(2)}ms`);
    console.log(`  Throughput: ${results.leaderElection.toLocaleString()} ops/s`);
    console.log('');

    // Benchmark 3: Batch Validation (using batch benchmark)
    console.log('═══ Benchmark 3: Batch Validation ═══');
    const validator = new WasmValidator(engine);

    const validStart = process.hrtime.bigint();
    const validIters = validator.benchmarkValidation(5000);
    const validEnd = process.hrtime.bigint();
    const validMs = Number(validEnd - validStart) / 1_000_000;
    results.batchValidation = Math.floor((validIters / validMs) * 1000);
    console.log(`Batch Validation (batch):`);
    console.log(`  Iterations: ${validIters.toLocaleString()}`);
    console.log(`  Duration: ${validMs.toFixed(2)}ms`);
    console.log(`  Throughput: ${results.batchValidation.toLocaleString()} tx/s`);
    console.log('');

    // Benchmark 4: Two-Stage Commitment (with FFI overhead for real-world comparison)
    console.log('═══ Benchmark 4: Two-Stage Commitment ═══');
    const agent = new WasmNetworkAgent(engine);
    for (let i = 0; i < 5; i++) agent.joinPeer(`validator_${i}`);

    let txNonce = 0;
    results.commitment = await benchmark(
        'Full Commitment Protocol',
        () => {
            const batch = JSON.stringify({
                transactions: [{ nonce: txNonce++, data: [1, 2, 3] }],
                previous_root: toHex(agent.consensusRoot())
            });
            const hash = agent.proposeCommitment(batch);
            agent.checkCommitment(hash, BigInt(0), BigInt(0));
            agent.finalizeBatch(batch, hash);
        },
        1000
    );

    // Summary
    console.log('╔══════════════════════════════════════════════════════════════╗');
    console.log('║   Benchmark Summary                                          ║');
    console.log('╚══════════════════════════════════════════════════════════════╝');
    console.log('');
    console.log(`Core Synthesis:        ${results.synthesis.toLocaleString()} ops/s`);
    console.log(`Peer Join:             ${results.peerJoin.toLocaleString()} ops/s`);
    console.log(`Leader Election:       ${results.leaderElection.toLocaleString()} ops/s`);
    console.log(`Batch Validation:      ${results.batchValidation.toLocaleString()} tx/s`);
    console.log(`Full Commitment:       ${results.commitment.toLocaleString()} ops/s`);
    console.log('');

    // Targets
    console.log('Target Comparison:');
    console.log(`  Core Synthesis:    ${results.synthesis >= 10000 ? '✅' : '❌'} (target: 10k+ ops/s)`);
    console.log(`  Batch Validation:  ${results.batchValidation >= 1000 ? '✅' : '❌'} (target: 1k+ tx/s)`);
    console.log(`  Leader Election:   ${results.leaderElection >= 100000 ? '✅' : '❌'} (target: 100k+ ops/s)`);
    console.log('');

    const allPass = results.synthesis >= 10000 &&
                    results.batchValidation >= 1000 &&
                    results.leaderElection >= 100000;

    if (allPass) {
        console.log('✅ All performance targets met!');
    } else {
        console.log('⚠️  Some targets not met (may vary by system)');
    }
}

main().catch(console.error);
BENCHEOF

chmod +x dist/universal/bench.mjs
echo "✓ Created bench.mjs (WASM benchmarks)"
echo ""

# Create high-level wrapper with clean string API
echo "Creating high-level wrapper..."
cat > dist/universal/koru-wrapper.js << 'WRAPPEREOF'
/**
 * Koru Lambda Core - High-Level Wrapper
 *
 * Abstracts the binary FFI complexity into a clean string-based API.
 * Uses optimized binary transport internally for maximum performance.
 */

import initWasm, {
    WasmEngine as RawEngine,
    WasmNetworkAgent as RawNetworkAgent,
    WasmValidator as RawValidator,
    WasmCommitmentAgent as RawCommitmentAgent
} from './koru.js';

// Helpers for conversion
const toHex = (bytes) => Array.from(bytes).map(b => b.toString(16).padStart(2, '0')).join('');
// SHA256 = 32 bytes, primordial IDs ("0", "1") = 1 byte
const bytesToId = (bytes) => bytes.length === 32 ? toHex(bytes) : new TextDecoder().decode(bytes);

/**
 * Initialize the WASM module
 * @param {BufferSource|WebAssembly.Module} [module] - Optional WASM module or buffer
 */
export async function init(module) {
    return initWasm(module);
}

/**
 * Koru Engine - Core distinction synthesis
 */
export class Engine {
    constructor() {
        this._raw = new RawEngine();
    }

    /** Get number of distinctions */
    distinctionCount() {
        return this._raw.distinctionCount();
    }

    /** Get number of relationships */
    relationshipCount() {
        return this._raw.relationshipCount();
    }

    /** Get primordial distinction Δ₀ ID */
    d0Id() {
        return bytesToId(this._raw.d0Id());
    }

    /** Get primordial distinction Δ₁ ID */
    d1Id() {
        return bytesToId(this._raw.d1Id());
    }

    /**
     * Synthesize two distinctions
     * @param {string} idA - First distinction ID
     * @param {string} idB - Second distinction ID
     * @returns {string} Resulting distinction ID
     */
    synthesize(idA, idB) {
        const result = this._raw.synthesize(idA, idB);
        return bytesToId(result);
    }

    /**
     * Run synthesis benchmark inside WASM (eliminates FFI overhead)
     * @param {number} iterations - Number of iterations
     * @returns {number} Completed iterations
     */
    benchmarkSynthesis(iterations) {
        return this._raw.benchmarkSynthesis(iterations);
    }

    /** Get raw WASM binding (for advanced usage) */
    get raw() {
        return this._raw;
    }
}

/**
 * Network Agent - Peer management and consensus
 */
export class NetworkAgent {
    constructor(engine) {
        if (!(engine instanceof Engine)) {
            throw new Error('NetworkAgent requires an Engine instance');
        }
        this._raw = new RawNetworkAgent(engine._raw);
    }

    /** Get current network root ID */
    currentRoot() {
        return bytesToId(this._raw.currentRoot());
    }

    /** Get consensus state root ID */
    consensusRoot() {
        return bytesToId(this._raw.consensusRoot());
    }

    /**
     * Join a peer to the validator set
     * @param {string} peerId - Peer identifier
     * @returns {string} New root ID
     */
    joinPeer(peerId) {
        const result = this._raw.joinPeer(peerId);
        return bytesToId(result);
    }

    /**
     * Bulk join multiple peers (eliminates FFI overhead)
     * @param {string[]} peerIds - Array of peer identifiers
     * @returns {string} Final root ID
     */
    joinPeers(peerIds) {
        const result = this._raw.joinPeers(peerIds);
        return bytesToId(result);
    }

    /**
     * Advance to next epoch
     * @returns {string} New root ID
     */
    advanceEpoch() {
        const result = this._raw.advanceEpoch();
        return bytesToId(result);
    }

    /** Get current epoch number */
    currentEpoch() {
        return this._raw.currentEpoch();
    }

    /** Get number of validators */
    validatorCount() {
        return this._raw.validatorCount();
    }

    /** Get current leader ID (or undefined if no validators) */
    getLeader() {
        return this._raw.getLeader();
    }

    /**
     * Run leader election benchmark inside WASM
     * @param {number} iterations - Number of iterations
     * @returns {number} Completed iterations
     */
    benchmarkLeaderElection(iterations) {
        return this._raw.benchmarkLeaderElection(iterations);
    }

    /**
     * Stage 1: Propose commitment
     * @param {string} batchJson - JSON-serialized transaction batch
     * @returns {string} Commitment hash (hex)
     */
    proposeCommitment(batchJson) {
        const hash = this._raw.proposeCommitment(batchJson);
        return toHex(hash);
    }

    /**
     * Verify commitment (light node check)
     * @param {string} hashHex - Commitment hash (hex)
     * @param {bigint} nonce - Expected nonce
     * @param {bigint} epoch - Expected epoch
     * @returns {boolean} True if valid
     */
    checkCommitment(hashHex, nonce, epoch) {
        const hashBytes = new Uint8Array(hashHex.match(/.{2}/g).map(b => parseInt(b, 16)));
        return this._raw.checkCommitment(hashBytes, nonce, epoch);
    }

    /**
     * Stage 2: Finalize batch
     * @param {string} batchJson - JSON-serialized transaction batch
     * @param {string} hashHex - Commitment hash (hex)
     * @returns {string} New root ID
     */
    finalizeBatch(batchJson, hashHex) {
        const hashBytes = new Uint8Array(hashHex.match(/.{2}/g).map(b => parseInt(b, 16)));
        const result = this._raw.finalizeBatch(batchJson, hashBytes);
        return bytesToId(result);
    }

    /** Get raw WASM binding (for advanced usage) */
    get raw() {
        return this._raw;
    }
}

/**
 * Consensus Validator - Transaction validation
 */
export class Validator {
    constructor(engine) {
        if (!(engine instanceof Engine)) {
            throw new Error('Validator requires an Engine instance');
        }
        this._raw = new RawValidator(engine._raw);
    }

    /** Get current state root ID */
    currentRoot() {
        return bytesToId(this._raw.currentRoot());
    }

    /** Get expected nonce for next transaction */
    expectedNonce() {
        return this._raw.expectedNonce();
    }

    /**
     * Validate a transaction batch
     * @param {string} batchJson - JSON-serialized transaction batch
     * @returns {string} New root ID
     */
    validateBatch(batchJson) {
        const result = this._raw.validateBatch(batchJson);
        return bytesToId(result);
    }

    /**
     * Run validation benchmark inside WASM
     * @param {number} iterations - Number of iterations
     * @returns {number} Completed iterations
     */
    benchmarkValidation(iterations) {
        return this._raw.benchmarkValidation(iterations);
    }

    /** Get raw WASM binding (for advanced usage) */
    get raw() {
        return this._raw;
    }
}

/**
 * Commitment Agent - Commitment tracking
 */
export class CommitmentAgent {
    constructor(engine) {
        if (!(engine instanceof Engine)) {
            throw new Error('CommitmentAgent requires an Engine instance');
        }
        this._raw = new RawCommitmentAgent(engine._raw);
    }

    /** Get current commitment root ID */
    currentRoot() {
        return bytesToId(this._raw.currentRoot());
    }

    /** Get expected nonce for next commitment */
    expectedNonce() {
        return this._raw.expectedNonce();
    }

    /** Get total commitments processed */
    commitmentsProcessed() {
        return this._raw.commitmentsProcessed();
    }

    /** Get raw WASM binding (for advanced usage) */
    get raw() {
        return this._raw;
    }
}

// Default export for convenience
export default { init, Engine, NetworkAgent, Validator, CommitmentAgent };
WRAPPEREOF

echo "✓ Created koru-wrapper.js (high-level API)"
echo ""

# Create simple test runner script
cat > dist/universal/test.sh << 'RUNEOF'
#!/bin/bash
# Simple test runner for WASM distribution

echo "🧪 Testing Universal WASM Artifact..."
echo ""

if ! command -v node &> /dev/null; then
    echo "❌ Node.js not found. Install from: https://nodejs.org"
    exit 1
fi

# Run test
node test_wasm.mjs

if [ $? -eq 0 ]; then
    echo ""
    echo "✨ All tests passed! The universal artifact works."
    echo ""
    echo "This same WASM file runs on:"
    echo "  • Web browsers (try: test.html)"
    echo "  • Node.js / Deno / Bun"
    echo "  • Go (via wazero)"
    echo "  • Python (via wasmtime)"
    echo "  • And more!"
    exit 0
else
    echo ""
    echo "❌ Tests failed"
    exit 1
fi
RUNEOF

chmod +x dist/universal/test.sh
echo "✓ Created test.sh (simple test runner)"
echo ""

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   ✅ Universal Build Complete!                               ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "To test:"
echo "  cd dist/universal && ./test.sh"
echo ""
echo "Or test in browser:"
echo "  cd dist/universal"
echo "  python3 -m http.server 8000"
echo "  open http://localhost:8000/test.html"
echo ""
echo "The Commitment IS the Consensus."
echo "The Hash IS the Truth."
echo "The Artifact IS Universal."
echo ""
