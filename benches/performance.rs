use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId, Throughput};
use distinction_engine::{
    DistinctionEngine,
    NetworkAgent,
    PeerIdentity,
    TransactionAction,
    TransactionBatch,
    StructuralCompactor,
    ParallelBatchProcessor,
    ParallelSynthesizer,
    ParallelAction,
    ProcessingStrategy,
    Canonicalizable,
    LocalCausalAgent,
};
use std::sync::Arc;

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
                    let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
                    agent.join_peer(peer, &engine);
                }

                b.iter(|| {
                    let transactions: Vec<TransactionAction> = (0..batch_size)
                        .map(|i| TransactionAction {
                            nonce: i as u64,
                            data: vec![i as u8],
                        })
                        .collect();

                    let batch = TransactionBatch {
                        transactions,
                        previous_root: agent.consensus_state_root().to_string(),
                    };

                    let result = agent.propose_batch(batch, &engine);
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
                    let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
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
                    let mut compactor = StructuralCompactor::new(&engine);
                    compactor.set_hot_threshold(8);
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
                let mut nodes: Vec<NetworkAgent> = (0..num_nodes)
                    .map(|_| NetworkAgent::new(&engine))
                    .collect();

                // Bootstrap validators
                let validators: Vec<PeerIdentity> = (0..num_nodes)
                    .map(|i| PeerIdentity::new(format!("node_{}", i), &engine))
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
                            let _ = node.propose_batch(batch.clone(), &engine);
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
            let peer = PeerIdentity::new("test_peer".to_string(), &engine);
            agent.join_peer(peer, &engine);
            black_box(&agent);
        });
    });

    group.bench_function("epoch_advance", |b| {
        let engine = Arc::new(DistinctionEngine::new());
        let mut agent = NetworkAgent::new(&engine);

        // Add some validators
        for i in 0..5 {
            let peer = PeerIdentity::new(format!("validator_{}", i), &engine);
            agent.join_peer(peer, &engine);
        }

        b.iter(|| {
            agent.advance_epoch(&engine);
            black_box(&agent);
        });
    });

    group.finish();
}

/// Benchmark: Parallel batch processing
///
/// Measures ParallelBatchProcessor throughput.
/// Target: 100,000+ tx/s with batched operations
fn bench_parallel_batch_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_batch_processing");

    for num_batches in [10, 100, 1_000].iter() {
        group.throughput(Throughput::Elements(*num_batches as u64));
        group.bench_with_input(
            BenchmarkId::from_parameter(num_batches),
            num_batches,
            |b, &num_batches| {
                let engine = Arc::new(DistinctionEngine::new());
                let mut processor = ParallelBatchProcessor::new(&engine);

                b.iter(|| {
                    let mut current_root = processor.get_current_root().id().to_string();

                    for i in 0..num_batches {
                        let batch = TransactionBatch {
                            transactions: vec![TransactionAction {
                                nonce: i as u64,
                                data: vec![i as u8],
                            }],
                            previous_root: current_root.clone(),
                        };

                        let action = ParallelAction {
                            batches: vec![batch],
                            strategy: ProcessingStrategy::Sequential,
                        };

                        let new_root = processor.synthesize_action(action, &engine);
                        current_root = new_root.id().to_string();
                    }

                    black_box(processor.batches_processed())
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Parallel byte canonicalization
///
/// Compares single-threaded vs parallel byte canonicalization.
/// Target: 1M+ bytes/s with parallelism
fn bench_parallel_byte_canonicalization(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_byte_canon");

    for size in [1_000, 10_000, 100_000].iter() {
        group.throughput(Throughput::Bytes(*size as u64));

        // Single-threaded baseline
        group.bench_with_input(
            BenchmarkId::new("sequential", size),
            size,
            |b, &size| {
                let engine = Arc::new(DistinctionEngine::new());
                let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

                b.iter(|| {
                    let results: Vec<_> = data
                        .iter()
                        .map(|&byte| {
                            let d = byte.to_canonical_structure(&engine);
                            d.id().to_string()
                        })
                        .collect();

                    black_box(results)
                });
            },
        );

        // Parallel with Rayon
        group.bench_with_input(
            BenchmarkId::new("parallel", size),
            size,
            |b, &size| {
                let engine = Arc::new(DistinctionEngine::new());
                let synthesizer = ParallelSynthesizer::new(engine.clone());
                let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();

                b.iter(|| {
                    let results = synthesizer.canonicalize_bytes_parallel(data.clone());
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Parallel synthesis operations
///
/// Measures parallel synthesis throughput using Rayon.
/// Target: Multi-core speedup over sequential operations
fn bench_parallel_synthesis(c: &mut Criterion) {
    let mut group = c.benchmark_group("parallel_synthesis");

    for num_ops in [100, 1_000, 10_000].iter() {
        group.throughput(Throughput::Elements(*num_ops as u64));

        group.bench_with_input(
            BenchmarkId::from_parameter(num_ops),
            num_ops,
            |b, &num_ops| {
                let engine = Arc::new(DistinctionEngine::new());
                let synthesizer = ParallelSynthesizer::new(engine.clone());

                // Create distinction ID pairs
                let d0_id = engine.d0().id().to_string();
                let d1_id = engine.d1().id().to_string();
                let pairs: Vec<(String, String)> = (0..num_ops)
                    .map(|_| (d0_id.clone(), d1_id.clone()))
                    .collect();

                b.iter(|| {
                    let results = synthesizer.synthesize_parallel(pairs.clone());
                    black_box(results)
                });
            },
        );
    }

    group.finish();
}

/// Benchmark: Multi-batch parallel processing
///
/// Measures throughput when processing multiple batches per action.
/// Target: 100,000+ tx/s with optimal batching
fn bench_multi_batch_parallel(c: &mut Criterion) {
    let mut group = c.benchmark_group("multi_batch_parallel");
    group.sample_size(10);

    for batches_per_action in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*batches_per_action as u64 * 10)); // 10 tx per batch

        group.bench_with_input(
            BenchmarkId::from_parameter(batches_per_action),
            batches_per_action,
            |b, &batches_per_action| {
                let engine = Arc::new(DistinctionEngine::new());
                let mut processor = ParallelBatchProcessor::new(&engine);

                b.iter(|| {
                    let initial_root = processor.get_current_root().id().to_string();
                    let current_root = initial_root.clone();

                    // Create batches
                    let batches: Vec<TransactionBatch> = (0..batches_per_action)
                        .map(|batch_idx| {
                            let transactions: Vec<TransactionAction> = (0..10)
                                .map(|tx_idx| TransactionAction {
                                    nonce: (batch_idx * 10 + tx_idx) as u64,
                                    data: vec![batch_idx as u8, tx_idx as u8],
                                })
                                .collect();

                            TransactionBatch {
                                transactions,
                                previous_root: if batch_idx == 0 {
                                    current_root.clone()
                                } else {
                                    String::new() // Will be updated
                                },
                            }
                        })
                        .collect();

                    let action = ParallelAction {
                        batches,
                        strategy: ProcessingStrategy::Sequential,
                    };

                    let _new_root = processor.synthesize_action(action, &engine);

                    black_box(processor.batches_processed())
                });
            },
        );
    }

    group.finish();
}

fn bench_runtime_event_synthesis(c: &mut Criterion) {
    use distinction_engine::{Canonicalizable, DistinctionEngine, RuntimeAction};
    use std::sync::Arc;

    let engine = Arc::new(DistinctionEngine::new());

    let mut group = c.benchmark_group("runtime_events");

    // Benchmark peer discovery canonicalization
    group.bench_function("peer_discovered_canonical", |b| {
        let action = RuntimeAction::PeerDiscovered {
            peer_id: "12D3KooWTest123456789".to_string(),
        };
        b.iter(|| black_box(action.to_canonical_structure(&engine)));
    });

    // Benchmark batch receipt canonicalization
    group.bench_function("batch_received_canonical", |b| {
        let action = RuntimeAction::BatchReceived {
            epoch: 42,
            leader_id: "node_1".to_string(),
        };
        b.iter(|| black_box(action.to_canonical_structure(&engine)));
    });

    // Benchmark epoch advancement canonicalization
    group.bench_function("epoch_advanced_canonical", |b| {
        let action = RuntimeAction::EpochAdvanced { new_epoch: 100 };
        b.iter(|| black_box(action.to_canonical_structure(&engine)));
    });

    group.finish();
}

fn bench_network_message_serialization(c: &mut Criterion) {
    use distinction_engine::{NetworkMessage, TransactionAction, TransactionBatch};

    let batch = TransactionBatch {
        transactions: vec![
            TransactionAction {
                nonce: 0,
                data: vec![1, 2, 3, 4, 5],
            },
            TransactionAction {
                nonce: 1,
                data: vec![6, 7, 8, 9, 10],
            },
        ],
        previous_root: "root_abc123".to_string(),
    };

    let msg = NetworkMessage::BatchProposal {
        epoch: 42,
        batch: batch.clone(),
        leader_id: "node_1".to_string(),
    };

    let mut group = c.benchmark_group("network_messages");

    group.bench_function("serialize_batch_proposal", |b| {
        b.iter(|| black_box(serde_json::to_vec(&msg).unwrap()));
    });

    let json = serde_json::to_vec(&msg).unwrap();
    group.bench_function("deserialize_batch_proposal", |b| {
        b.iter(|| {
            black_box(serde_json::from_slice::<NetworkMessage>(&json).unwrap())
        });
    });

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
    bench_parallel_batch_processing,
    bench_parallel_byte_canonicalization,
    bench_parallel_synthesis,
    bench_multi_batch_parallel,
    bench_runtime_event_synthesis,
    bench_network_message_serialization,
);

criterion_main!(benches);
