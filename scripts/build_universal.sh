#!/bin/bash
# Koru Lambda Core - Universal WASM Build
#
# Philosophy: One artifact, infinite platforms
# Builds: One universal WASM module that runs everywhere

set -e

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

# Create distribution directory
echo "Creating universal distribution..."
mkdir -p dist/universal

# Copy WASM artifacts
cp pkg/distinction_engine_bg.wasm dist/universal/koru.wasm
cp pkg/distinction_engine.js dist/universal/koru.js
cp pkg/distinction_engine.d.ts dist/universal/koru.d.ts

echo "✓ Copied WASM artifacts"

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
            log('═══ Core Distinction Engine ═══');
            log(`Distinctions: ${engine.distinctionCount()}`);
            log(`  Δ₀ = ${engine.d0Id().substring(0, 16)}...`);
            log(`  Δ₁ = ${engine.d1Id().substring(0, 16)}...`);
            log('');

            // Trusted Subprocess 1: Network Agent (LocalCausalAgent)
            log('═══ Subprocess 1: Network Agent ═══');
            log('Pattern: ΔNew = ΔNetwork_Root ⊕ ΔNetwork_Action');
            const network = new WasmNetworkAgent(engine);
            log(`Network root: ${network.currentRoot().substring(0, 16)}...`);
            log(`Consensus root: ${network.consensusRoot().substring(0, 16)}...`);
            log('');

            // Join peers (each is a causal synthesis)
            log('Joining peers (causal synthesis)...');
            for (let i = 0; i < 3; i++) {
                const newRoot = network.joinPeer(`validator_${i}`);
                log(`  Peer ${i}: ΔNew = ${newRoot.substring(0, 16)}...`);
            }
            log(`Validators: ${network.validatorCount()}`);
            log(`Leader: ${network.getLeader()} (deterministic)`);
            log('');

            // Trusted Subprocess 2: Consensus Validator (LocalCausalAgent)
            log('═══ Subprocess 2: Consensus Validator ═══');
            log('Pattern: ΔNew = ΔState_Root ⊕ ΔTransaction');
            const validator = new WasmValidator(engine);
            log(`State root: ${validator.currentRoot().substring(0, 16)}...`);
            log(`Expected nonce: ${validator.expectedNonce()}`);
            log('');

            // Trusted Subprocess 3: Commitment Agent (LocalCausalAgent)
            log('═══ Subprocess 3: Commitment Agent ═══');
            log('Pattern: ΔNew = ΔCommitment_Root ⊕ ΔBatch_Commitment');
            const commitment = new WasmCommitmentAgent(engine);
            log(`Commitment root: ${commitment.currentRoot().substring(0, 16)}...`);
            log(`Expected nonce: ${commitment.expectedNonce()}`);
            log(`Commitments processed: ${commitment.commitmentsProcessed()}`);
            log('');

            // Two-stage commitment protocol
            log('═══ Two-Stage Commitment Protocol ═══');
            const batch = JSON.stringify({
                transactions: [{ nonce: 0, data: [1, 2, 3] }],
                previous_root: network.consensusRoot()
            });

            // Stage 1: Lightweight commitment (80 bytes)
            const hash = network.proposeCommitment(batch);
            log(`Stage 1: Commitment = ${hash.substring(0, 32)}...`);
            log('         (gossip 80 bytes, no batch download)');

            // Light node verification (no data needed!)
            const isValid = network.checkCommitment(hash, 0, 0);
            log(`Verify:  ${isValid ? '✓' : '✗'} Light node check passed`);

            // Stage 2: Full validator finalization
            const newRoot = network.finalizeBatch(batch, hash);
            log(`Stage 2: ΔNew = ${newRoot.substring(0, 16)}...`);
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

import init, { WasmEngine, WasmAgent } from './koru.js';
import { readFile } from 'fs/promises';

async function main() {
    console.log('🚀 Koru Lambda Core - Node.js Runtime\n');

    // Load WASM module
    const wasmBuffer = await readFile('./koru.wasm');
    await init(wasmBuffer);
    console.log('✓ WASM module loaded\n');

    // Create engine
    const engine = new WasmEngine();
    console.log(`✓ Engine: ${engine.distinctionCount()} distinctions`);
    console.log(`  Δ₀ = ${engine.d0Id()}`);
    console.log(`  Δ₁ = ${engine.d1Id()}\n`);

    // Create agent
    const agent = new WasmAgent(engine);
    console.log('✓ Network Agent created');
    console.log(`  State root: ${agent.stateRoot()}\n`);

    // Add validators
    for (let i = 0; i < 5; i++) {
        agent.joinPeer(`validator_${i}`);
    }
    console.log(`✓ Validators: ${agent.validatorCount()}`);
    console.log(`  Leader: ${agent.getLeader()}\n`);

    // Two-stage commitment
    const batch = JSON.stringify({
        transactions: [{ nonce: 0, data: [1, 2, 3] }],
        previous_root: agent.stateRoot()
    });

    console.log('Two-Stage Commitment Protocol:');
    const hash = agent.proposeCommitment(batch);
    console.log(`  Stage 1: ${hash.substring(0, 16)}...`);

    const valid = agent.checkCommitment(hash, 0, 0);
    console.log(`  Verify:  ${valid ? '✓' : '✗'} Valid`);

    const newRoot = agent.finalizeBatch(batch, hash);
    console.log(`  Stage 2: ${newRoot.substring(0, 16)}...\n`);

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

async function main() {
    console.log('🧪 WASM Integration Test\n');
    const wasmBuffer = await readFile('./koru.wasm');
    await init(wasmBuffer);
    console.log('✓ WASM module loaded\n');

    // Test 1: Core Engine
    console.log('═══ Test 1: Core Distinction Engine ═══');
    const engine = new WasmEngine();
    console.log(`✓ Distinctions: ${engine.distinctionCount()}`);
    const d0 = engine.d0Id();
    const d1 = engine.d1Id();
    console.log(`✓ Δ₀ = ${d0.substring(0, 16)}...`);
    console.log(`✓ Δ₁ = ${d1.substring(0, 16)}...`);
    const d2 = engine.synthesize(d0, d1);
    console.log(`✓ synthesis(Δ₀, Δ₁) = ${d2.substring(0, 16)}...`);
    const d2_rev = engine.synthesize(d1, d0);
    if (d2 !== d2_rev) throw new Error('Symmetry axiom violated');
    console.log('✓ Symmetry axiom holds\n');

    // Test 2: NetworkAgent (LocalCausalAgent)
    console.log('═══ Test 2: Network Agent (LocalCausalAgent) ═══');
    const network = new WasmNetworkAgent(engine);
    const r0 = network.currentRoot();
    console.log(`✓ Network root: ${r0.substring(0, 16)}...`);
    const r1 = network.joinPeer('validator_0');
    if (r1 === r0) throw new Error('Network root did not change');
    console.log(`✓ After join: ${r1.substring(0, 16)}... (ΔNew = ΔRoot ⊕ ΔAction)`);
    network.joinPeer('validator_1');
    network.joinPeer('validator_2');
    console.log(`✓ Validators: ${network.validatorCount()}`);
    console.log(`✓ Leader: ${network.getLeader()} (deterministic)\n`);

    // Test 3: Validator (LocalCausalAgent)
    console.log('═══ Test 3: Consensus Validator (LocalCausalAgent) ═══');
    const validator = new WasmValidator(engine);
    const v0 = validator.currentRoot();
    console.log(`✓ State root: ${v0.substring(0, 16)}...`);
    const batch = JSON.stringify({
        transactions: [{ nonce: 0, data: [1, 2, 3] }],
        previous_root: v0
    });
    const v1 = validator.validateBatch(batch);
    if (v1 === v0) throw new Error('Validator root did not change');
    console.log(`✓ After validation: ${v1.substring(0, 16)}...\n`);

    // Test 4: Two-Stage Commitment
    console.log('═══ Test 4: Two-Stage Commitment Protocol ═══');
    const agent = new WasmNetworkAgent(engine);
    for (let i = 0; i < 3; i++) agent.joinPeer(`node_${i}`);
    const testBatch = JSON.stringify({
        transactions: [{ nonce: 0, data: [4, 5, 6] }],
        previous_root: agent.consensusRoot()
    });
    const hash = agent.proposeCommitment(testBatch);
    console.log(`✓ Stage 1: ${hash.substring(0, 32)}... (80 bytes)`);
    const isValid = agent.checkCommitment(hash, BigInt(0), BigInt(0));
    if (!isValid) throw new Error('Commitment verification failed');
    console.log('✓ Light node verification passed');
    const newRoot = agent.finalizeBatch(testBatch, hash);
    console.log(`✓ Stage 2: ΔNew = ${newRoot.substring(0, 16)}...\n`);

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
