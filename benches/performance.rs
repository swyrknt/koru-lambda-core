use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use koru_lambda_core::{
    BatchSynthesizer, Canonicalizable, ConsensusValidator, Distinction, DistinctionEngine,
    NetworkAgent, PeerIdentity, StructuralCompactor, TransactionAction, TransactionBatch,
};
use std::sync::Arc;

/// Helper: Propose and finalize batch using two-stage commitment protocol
fn propose_and_finalize_batch(
    agent: &mut NetworkAgent,
    batch: TransactionBatch,
    engine: &Arc<DistinctionEngine>,
) -> Result<Distinction, String> {
    let commitment = agent.propose_commitment(batch.clone(), engine)?;
    agent.finalize_batch(batch, commitment.commitment_hash, engine)
}

/// Benchmark: Core synthesis operations
///
/// Measures raw distinction synthesis throughput.
/// Target: 100,000+ synthesize ops/second
fn bench_core_synthesis(c: &mut Criterion) {
    let mut group = c.benchmark_group("core_synthesis");

    for size in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let engine = Arc::new(DistinctionEngine::new());

            b.iter(|| {
                let d0 = engine.d0().clone();
                let d1 = engine.d1().clone();

                let mut current = engine.synthesize(&d0, &d1);

                for i in 0..size {
                    let byte = ((i % 256) as u8).to_canonical_structure(&engine);
                    current = engine.synthesize(&current, &byte);
                }

                black_box(current)
            });
        });
    }

    group.finish();
}

/// Benchmark: Transaction validation throughput
///
/// Measures batch validation performance.
/// Target: 10,000+ batches/second (100k+ tx/s with 10 tx/batch)
fn bench_transaction_validation(c: &mut Criterion) {
    let mut group = c.benchmark_group("transaction_validation");

    for batch_size in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            batch_size,
            |b, &batch_size| {
                let engine = Arc::new(DistinctionEngine::new());
                let mut agent = NetworkAgent::new(&engine);

                // Bootstrap with validators
                for i in 0..5 {
                    let peer = PeerIdentity::new(format!("validator_{}", i), &engine).unwrap();
                    agent.join_peer(peer, &engine);
                }

                b.iter(|| {
                    let transactions: Vec<TransactionAction> = (0..batch_size)
                        .map(|i| TransactionAction { nonce: i as u64, data: vec![i as u8] })
                        .collect();

                    let batch = TransactionBatch {
                        transactions,
                        previous_root: agent.consensus_state_root().to_string(),
                    };

                    let result = propose_and_finalize_batch(&mut agent, batch, &engine);
                    black_box(result)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Leader election performance
///
/// Measures deterministic leader election speed.
/// Should be sub-microsecond (pure hash computation)
fn bench_leader_election(c: &mut Criterion) {
    let mut group = c.benchmark_group("leader_election");

    for num_validators in [5, 10, 20, 50].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_validators),
            num_validators,
            |b, &num_validators| {
                let engine = Arc::new(DistinctionEngine::new());
                let mut agent = NetworkAgent::new(&engine);

                // Add validators
                for i in 0..num_validators {
                    let peer = PeerIdentity::new(format!("validator_{}", i), &engine).unwrap();
                    agent.join_peer(peer, &engine);
                }

                b.iter(|| {
                    let leader = agent.get_current_leader();
                    black_box(leader)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Structural compaction performance
///
/// Measures compaction throughput on large graphs.
/// Target: <100ms for 10,000 node graph
fn bench_compaction(c: &mut Criterion) {
    let mut group = c.benchmark_group("compaction");
    group.sample_size(10); // Fewer samples for expensive operation

    for graph_size in [1_000, 5_000, 10_000].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(graph_size),
            graph_size,
            |b, &graph_size| {
                // Build graph once
                let engine = Arc::new(DistinctionEngine::new());
                let d0 = engine.d0().clone();
                let d1 = engine.d1().clone();

                let mut current = engine.synthesize(&d0, &d1);

                // Create large graph
                for i in 0..graph_size {
                    let byte = ((i % 256) as u8).to_canonical_structure(&engine);
                    current = engine.synthesize(&current, &byte);
                }

                b.iter(|| {
                    let mut compactor = StructuralCompactor::new(&engine, 8, 4);
                    let action = compactor.compact(&engine);
                    black_box(action)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Multi-node consensus throughput
///
/// Measures end-to-end distributed consensus performance.
/// Target: 50,000+ tx/s across 5 nodes
fn bench_distributed_consensus(c: &mut Criterion) {
    let mut group = c.benchmark_group("distributed_consensus");
    group.sample_size(10);

    for num_nodes in [3, 5, 7].iter() {
        group.bench_with_input(
            BenchmarkId::from_parameter(num_nodes),
            num_nodes,
            |b, &num_nodes| {
                let engine = Arc::new(DistinctionEngine::new());

                // Setup nodes
                let mut nodes: Vec<NetworkAgent> =
                    (0..num_nodes).map(|_| NetworkAgent::new(&engine)).collect();

                // Bootstrap validators
                let validators: Vec<PeerIdentity> = (0..num_nodes)
                    .map(|i| PeerIdentity::new(format!("node_{}", i), &engine).unwrap())
                    .collect();

                for node in nodes.iter_mut() {
                    for validator in validators.iter() {
                        node.join_peer(validator.clone(), &engine);
                    }
                }

                b.iter(|| {
                    // Process 10 batches of 10 tx each = 100 tx
                    let mut tx_count = 0;

                    for batch_idx in 0..10 {
                        let transactions: Vec<TransactionAction> = (0..10)
                            .map(|i| TransactionAction {
                                nonce: (batch_idx * 10 + i) as u64,
                                data: vec![batch_idx as u8, i as u8],
                            })
                            .collect();

                        let batch = TransactionBatch {
                            transactions: transactions.clone(),
                            previous_root: nodes[0].consensus_state_root().to_string(),
                        };

                        // All nodes process
                        for node in nodes.iter_mut() {
                            let _ = propose_and_finalize_batch(node, batch.clone(), &engine);
                        }

                        tx_count += transactions.len();

                        // Advance epoch
                        for node in nodes.iter_mut() {
                            node.advance_epoch(&engine);
                        }
                    }

                    black_box(tx_count)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Byte canonicalization performance
///
/// Measures primitive data mapping throughput.
/// Target: 1M+ bytes/second
fn bench_byte_canonicalization(c: &mut Criterion) {
    let mut group = c.benchmark_group("byte_canonicalization");

    for size in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), size, |b, &size| {
            let engine = Arc::new(DistinctionEngine::new());
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

            b.iter(|| {
                for &byte in &data {
                    let d = byte.to_canonical_structure(&engine);
                    black_box(d);
                }
            });
        });
    }

    group.finish();
}

/// Benchmark: Network event synthesis
///
/// Measures network action processing speed.
/// Target: 100,000+ events/second
fn bench_network_events(c: &mut Criterion) {
    let mut group = c.benchmark_group("network_events");

    group.bench_function("peer_join", |b| {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        b.iter(|| {
            let peer = PeerIdentity::new("test_peer".to_string(), &engine).unwrap();
            agent.join_peer(peer, &engine);
            black_box(&agent);
        });
    });

    group.bench_function("epoch_advance", |b| {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        // Add some validators
        for i in 0..5 {
            let peer = PeerIdentity::new(format!("validator_{}", i), &engine).unwrap();
            agent.join_peer(peer, &engine);
        }

        b.iter(|| {
            agent.advance_epoch(&engine);
            black_box(&agent);
        });
    });

    group.finish();
}

/// Benchmark: sequential batch processing via ConsensusValidator.
///
/// v2.0 history: replaces the v1.2.0 `bench_parallel_batch_processing`
/// against the deleted `ParallelBatchProcessor` (which had a Sequential
/// body anyway per the Phase 1 parallel-audit).
fn bench_sequential_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("sequential_batch_processing");

    for num_batches in [10, 100, 1_000].iter() {
        group.throughput(Throughput::Elements(*num_batches as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_batches),
            num_batches,
            |b, &num_batches| {
                let engine = Arc::new(DistinctionEngine::new());
                let mut validator = ConsensusValidator::new(&engine);

                b.iter(|| {
                    let mut current_root = validator.state_root_id();

                    for i in 0..num_batches {
                        let batch = TransactionBatch {
                            transactions: vec![TransactionAction {
                                nonce: i as u64,
                                data: vec![i as u8],
                            }],
                            previous_root: current_root.clone(),
                        };

                        if let koru_lambda_core::BatchValidationResult::Valid(new_root) =
                            validator.validate_batch(batch, &engine)
                        {
                            current_root = new_root.to_hex();
                        }
                    }

                    black_box(validator.expected_nonce())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: byte canonicalization, sequential vs Rayon-parallel via
/// `BatchSynthesizer`.
fn bench_parallel_byte_canonicalization(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_byte_canon");

    for size in [1_000, 10_000, 100_000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        // Single-threaded baseline.
        group.bench_with_input(BenchmarkId::new("sequential", size), size, |b, &size| {
            let engine = Arc::new(DistinctionEngine::new());
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

            b.iter(|| {
                let results: Vec<_> =
                    data.iter().map(|&byte| byte.to_canonical_structure(&engine)).collect();
                black_box(results)
            });
        });

        // Parallel with Rayon (via BatchSynthesizer).
        group.bench_with_input(BenchmarkId::new("parallel", size), size, |b, &size| {
            let engine = Arc::new(DistinctionEngine::new());
            let synthesizer = BatchSynthesizer::new(engine.clone());
            let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

            b.iter(|| {
                let results = synthesizer.canonicalize_bytes_batch(data.clone());
                black_box(results)
            });
        });
    }

    group.finish();
}

/// Benchmark: parallel synthesis throughput via BatchSynthesizer.
fn bench_parallel_synthesis(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_synthesis");

    for num_ops in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*num_ops as u64));

        group.bench_with_input(BenchmarkId::from_parameter(num_ops), num_ops, |b, &num_ops| {
            let engine = Arc::new(DistinctionEngine::new());
            let synthesizer = BatchSynthesizer::new(engine.clone());

            let d0_id = engine.d0().to_hex();
            let d1_id = engine.d1().to_hex();
            let pairs: Vec<(String, String)> =
                (0..num_ops).map(|_| (d0_id.clone(), d1_id.clone())).collect();

            b.iter(|| {
                let results = synthesizer.synthesize_batch(pairs.clone());
                black_box(results)
            });
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    bench_core_synthesis,
    bench_transaction_validation,
    bench_leader_election,
    bench_compaction,
    bench_distributed_consensus,
    bench_byte_canonicalization,
    bench_network_events,
    bench_sequential_batch_processing,
    bench_parallel_byte_canonicalization,
    bench_parallel_synthesis,
);

criterion_main!(benches);
