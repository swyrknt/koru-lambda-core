# Koru Lambda Core

[![Crates.io](https://img.shields.io/crates/v/koru-lambda-core.svg)](https://crates.io/crates/koru-lambda-core)
[![Docs.rs](https://docs.rs/koru-lambda-core/badge.svg)](https://docs.rs/koru-lambda-core)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![Build Status](https://github.com/swyrknt/koru-lambda-core/actions/workflows/ci.yml/badge.svg)](https://github.com/swyrknt/koru-lambda-core/actions)

A minimal axiomatic system for computation based on distinction calculus. This engine implements a timeless, self-consistent computational substrate where complex distributed system properties arise from simple synthesis operations.

## 🌟 Key Features

- **Axiomatic Foundation**: Built on five core axioms (Identity, Nontriviality, Synthesis, Symmetry, Irreflexivity)
- **Complex Behavior**: Distributed consensus, deterministic state transitions, and mathematical structures arise naturally
- **Deterministic & Timeless**: Content-addressable structure ensures reproducibility
- **High Performance**: Optimized Rust implementation with batch operations
- **Comprehensive Testing**: Extensive test suite with falsification targets

## 🚀 Quick Start

### Installation

```toml
[dependencies]
koru-lambda-core = "0.1.0"
```

### Basic Usage (Rust)

```rust
use koru_lambda_core::{DistinctionEngine, Distinction};

fn main() {
    let mut engine = DistinctionEngine::new();

    // Synthesize the primordial distinctions
    let existence = engine.synthesize(engine.d0(), engine.d1());
    println!("Created distinction: {}", existence.id());

    // Build complex structures
    let order = engine.synthesize(&existence, engine.d0());
    let chaos = engine.synthesize(&existence, engine.d1());
    let nature = engine.synthesize(&order, &chaos);

    println!("Nature distinction: {}", nature.id());
}
```

### Basic Usage (JavaScript/WASM)

```javascript
import { Engine, NetworkAgent } from './koru-wrapper.js';

const engine = new Engine();

// Synthesize distinctions
const d2 = engine.synthesize(engine.d0Id(), engine.d1Id());
console.log(`Created distinction: ${d2}`);

// Network consensus
const agent = new NetworkAgent(engine);
agent.joinPeer('validator_0');
agent.joinPeer('validator_1');
console.log(`Leader: ${agent.getLeader()}`);
```

Build the universal WASM artifact:

```bash
./scripts/build_universal.sh
```

This produces a single artifact that runs on browsers, Node.js, Deno, Bun, Go, Kotlin, Swift, Python, and embedded systems.

## 🧠 Core Concepts

### The Five Axioms

1. **Identity**: A distinction is defined solely by its unique identifier
2. **Nontriviality**: The system initializes with two primordial distinctions (Δ₀, Δ₁)
3. **Synthesis**: Two distinctions combine deterministically to create a third
4. **Symmetry**: Relationships are bidirectional and order-independent  
5. **Irreflexivity**: A distinction synthesized with itself yields itself

### System Properties

The engine exhibits:

- **Structural Coherence**: Graph topology correlates with causal evolution patterns
- **Mathematical Invariants**: Mathematical patterns arise as deterministic structural relationships
- **Distributed Consensus**: BFT consensus, persistence, and fault tolerance without explicit coordination protocols
- **High-Coherence Structures**: Tightly integrated subgraphs with measurable clustering coefficients

## 📚 Documentation

- **[Documentation Hub](docs/)** - All documentation
- [Design Documentation](docs/DESIGN_DOC.md) - Theoretical foundation and SPoC protocol
- [Testing Standards](docs/development/TESTING_STANDARDS.md) - Test philosophy and approach
- [API Reference](https://docs.rs/koru-lambda-core) - Auto-generated API docs

## 🏗️ Architecture

```
koru-lambda-core/
├── src/
│   ├── engine.rs           # Core synthesis (265 lines)
│   ├── primitives.rs       # Data canonicalization
│   ├── wasm.rs             # WASM bindings with binary marshalling
│   ├── lib.rs              # Public API
│   └── subsystems/
│       ├── validator.rs    # Consensus validation (SPoC)
│       ├── compactor.rs    # Structural compaction (R ∝ U)
│       ├── network.rs      # Forkless P2P consensus
│       ├── runtime.rs      # Async P2P networking (libp2p)
│       └── parallel.rs     # Multi-core processing
├── scripts/
│   └── build_universal.sh  # Universal WASM artifact builder
├── tests/                  # Comprehensive test suite
│   ├── end_to_end.rs       # Distributed system tests
│   ├── runtime_integration.rs # Async runtime validation
│   ├── integration_tests.rs # Falsification suite
│   ├── parallel_integration.rs # Concurrency tests
│   └── throughput_verification.rs # Performance benchmarks
└── benches/
    └── performance.rs      # Criterion benchmarks
```

## 🔬 Research & Testing

The project includes a comprehensive falsification test suite:

```rust
#[test]
fn test_structural_coherence() {
    // Tests whether graph topology correlates with causal evolution
    // Falsifies if: Spatially adjacent nodes exhibit large causal age differences
}

#[test]
fn test_mathematical_invariants() {
    // Tests whether mathematical structures arise as deterministic patterns
    // Falsifies if: Mathematical truths depend on construction method
}

#[test]
fn test_structural_feedback() {
    // Tests for high-coherence structural feedback
    // Falsifies if: No high-coherence structures arise
}
```

Run the test suite:

```bash
cargo test
cargo test --release  # For optimized builds
```

## 📊 Performance

Benchmark the engine:

```bash
cargo bench
```

### Performance Targets

**Core Operations:**
- **198,000 ops/s** - Core synthesis throughput (19.8x target)
- **14,000,000 ops/s** - Parallel synthesis with Rayon (140x target)
- **35,000 tx/s** - Sustained batch validation throughput

**Concurrency:**
- **Multi-core scaling** - Auto-detects CPU cores for parallelism
- **Thread-safe** - DashMap enables lock-free concurrent operations
- **Deterministic** - Same inputs → identical outputs across all threads
- **Zero data races** - Validated with 100 concurrent threads

**Distributed Consensus:**
- **6,500 tx/s** - Across 5 nodes (BFT configuration)
- **7μs leader election** - Sub-10μs deterministic leader selection
- **Instant finality** - No probabilistic confirmation needed

**Storage Efficiency:**
- **3.85x compression** - Via structural compaction
- **O(log n) growth** - Logarithmic storage with compaction
- **14ms compaction** - For 10,000 node graphs

**WASM (Universal Artifact):**
- **1,500,000 ops/s** - Core synthesis throughput
- **880,000 ops/s** - Leader election (7 validators)
- **47,000 tx/s** - Batch validation throughput
- **Binary marshalling** - Zero-copy Uint8Array returns eliminate FFI overhead
- **Structural batching** - Bulk operations run entirely inside WASM

## 🎯 Use Cases

### Research & Academia
- Study complex systems and computational foundations
- Explore axiomatic approaches to distributed consensus
- Test formal theories of deterministic computation

### Distributed Systems
- Build fault-tolerant distributed databases
- Implement novel consensus mechanisms
- Create self-organizing network protocols

### AI & Machine Learning
- Develop structurally-aware neural networks
- Explore topological learning algorithms
- Build explainable AI systems

## 🔧 Advanced Usage

### Parallel Batch Processing

```rust
use koru_lambda_core::{
    DistinctionEngine, ParallelBatchProcessor, ParallelAction,
    ProcessingStrategy, TransactionBatch, TransactionAction, LocalCausalAgent,
};
use std::sync::Arc;

let engine = Arc::new(DistinctionEngine::new());
let mut processor = ParallelBatchProcessor::new(&engine);

// Create transaction batches
let batch = TransactionBatch {
    transactions: vec![
        TransactionAction { nonce: 0, data: vec![1, 2, 3] },
        TransactionAction { nonce: 1, data: vec![4, 5, 6] },
    ],
    previous_root: processor.get_current_root().id().to_string(),
};

// Process via LocalCausalAgent trait
let action = ParallelAction {
    batches: vec![batch],
    strategy: ProcessingStrategy::Sequential,
};

let new_root = processor.synthesize_action(action, &engine);
println!("Processed {} batches", processor.batches_processed());
```

### Parallel Synthesis Operations

```rust
use koru_lambda_core::{DistinctionEngine, ParallelSynthesizer};
use std::sync::Arc;

let engine = Arc::new(DistinctionEngine::new());
let synthesizer = ParallelSynthesizer::new(engine.clone());

// Parallelize byte canonicalization using Rayon
let data: Vec<u8> = (0..100_000).map(|i| (i % 256) as u8).collect();
let results = synthesizer.canonicalize_bytes_parallel(data);

println!("Canonicalized {} bytes in parallel", results.len());
```

### Multi-Threaded Usage

```rust
use koru_lambda_core::{DistinctionEngine, Canonicalizable};
use std::sync::Arc;
use std::thread;

let engine = Arc::new(DistinctionEngine::new());
let mut handles = vec![];

// Spawn multiple threads for concurrent synthesis
for thread_id in 0..10 {
    let engine_clone = Arc::clone(&engine);

    let handle = thread::spawn(move || {
        let byte = (thread_id % 256) as u8;
        byte.to_canonical_structure(&engine_clone)
    });

    handles.push(handle);
}

// Collect results - all synthesis is thread-safe via DashMap
for handle in handles {
    let result = handle.join().unwrap();
    println!("Result: {}", result.id());
}
```

### Custom Data Mapping

```rust
use koru_lambda_core::{DistinctionEngine, ByteMapping};

let mut engine = DistinctionEngine::new();
let data = "Hello, World!".as_bytes();

// Map arbitrary data to distinction structures
for &byte in data {
    let distinction = ByteMapping::map_byte_to_distinction(byte, &mut engine);
    // Use distinction for storage or computation
}
```

## 🤝 Contributing

We welcome contributions! Please see our [Contributing Guide](CONTRIBUTING.md) for details.

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

## 📜 License

This project is licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🙏 Acknowledgments

- Based on distinction calculus and computational foundations research
- Inspired by work in mathematical foundations and distributed systems theory
- Built with the Rust programming language ecosystem

## 🔗 Links

- [Issue Tracker](https://github.com/swyrknt/koru-lambda-core/issues)
- [Discussion Forum](https://github.com/swyrknt/koru-lambda-core/discussions)
- [Changelog](CHANGELOG.md)

---

**Note**: This is research software implementing novel distributed consensus mechanisms. While production-ready from an engineering perspective, the axiomatic approach is experimental.
