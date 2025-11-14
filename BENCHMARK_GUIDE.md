# 🚀 Benchmark & Performance Testing Guide

## Quick Start

### Run All Performance Tests (Recommended)
```bash
cargo test --test performance_validation --release -- --nocapture
```

**Output includes:**
- ✅ Core synthesis: 174k ops/s (target: 10k+)
- ✅ Batch validation: 2.5k batches/s (target: 1k+)
- ✅ Leader election: 7.1μs latency (target: <10μs)
- ✅ Graph compaction: 12ms for 10k nodes (target: <100ms)
- ✅ Distributed consensus: 6.6k tx/s (target: 5k+)
- ✅ Byte canonicalization: 246k bytes/s (target: 100k+)

## Individual Benchmark Commands

### 1. Core Synthesis Performance
```bash
cargo test --release test_core_synthesis_performance -- --nocapture
```
**Measures:** Raw distinction synthesis throughput
**Result:** ~174,000 operations/second

### 2. Batch Validation Performance
```bash
cargo test --release test_batch_validation_performance -- --nocapture
```
**Measures:** Transaction batch processing
**Result:** ~2,500 batches/s, ~25,000 tx/s

### 3. Leader Election Performance
```bash
cargo test --release test_leader_election_performance -- --nocapture
```
**Measures:** Deterministic leader selection latency
**Result:** ~7 microseconds average

### 4. Graph Compaction Performance
```bash
cargo test --release test_compaction_performance -- --nocapture
```
**Measures:** Large graph compression speed
**Result:** ~12ms for 10,000 node graph

### 5. Distributed Consensus Throughput
```bash
cargo test --release test_distributed_consensus_throughput -- --nocapture
```
**Measures:** Multi-node coordination throughput
**Result:** ~6,600 tx/s across 5 nodes

### 6. Byte Canonicalization Performance
```bash
cargo test --release test_byte_canonicalization_performance -- --nocapture
```
**Measures:** Data mapping throughput
**Result:** ~246,000 bytes/s

## End-to-End System Tests

### Run All E2E Tests
```bash
cargo test --test end_to_end -- --nocapture --test-threads=1
```

### Individual E2E Tests

**Multi-Node Consensus (7 validators, 100 transactions):**
```bash
cargo test --test end_to_end test_e2e_multi_node_consensus -- --nocapture
```

**System Under Load + Compaction (5 nodes, 1000 transactions):**
```bash
cargo test --test end_to_end test_e2e_system_under_load_with_compaction -- --nocapture
```

**Byzantine Fault Tolerance (5 honest + 2 byzantine):**
```bash
cargo test --test end_to_end test_e2e_byzantine_fault_tolerance -- --nocapture
```

**Network Partition Recovery:**
```bash
cargo test --test end_to_end test_e2e_network_partition_recovery -- --nocapture
```

## Performance Comparison

### Debug vs Release
```bash
# Debug mode (slower execution, faster compile)
cargo test --test performance_validation -- --nocapture

# Release mode (optimized, accurate benchmarks)
cargo test --test performance_validation --release -- --nocapture
```

### With Timing
```bash
# Measure total execution time
time cargo test --test performance_validation --release -- --nocapture
```

## Complete Test Suite

### Everything (All 52 Tests)
```bash
cargo test --release
```

**Breakdown:**
- 26 unit tests (instant)
- 18 falsification tests (~70 seconds)
- 4 end-to-end tests (~2 seconds)
- 6 performance tests (~3 seconds)
- 1 doc test (instant)

### Fast Tests Only
```bash
# Just unit tests (~1 second)
cargo test --lib

# Unit + performance tests (~4 seconds)
cargo test --lib && cargo test --test performance_validation --release
```

## Profiling & Advanced Analysis

### CPU Profiling (macOS)
```bash
# Install cargo-instruments
cargo install cargo-instruments

# Profile core synthesis
cargo instruments -t time --release --test performance_validation -- test_core_synthesis_performance

# Profile distributed consensus
cargo instruments -t time --release --test performance_validation -- test_distributed_consensus_throughput
```

### Memory Profiling (macOS)
```bash
cargo instruments -t alloc --release --test performance_validation
```

### CPU Profiling (Linux)
```bash
# Record performance data
perf record -g cargo test --release test_core_synthesis_performance

# View report
perf report

# Generate flamegraph
perf script | stackcollapse-perf.pl | flamegraph.pl > flamegraph.svg
```

### Memory Profiling (Linux)
```bash
valgrind --tool=massif cargo test --release test_core_synthesis_performance
ms_print massif.out.*
```

## Benchmark History & Regression Testing

### Save Baseline
```bash
# Run and save results
cargo test --test performance_validation --release -- --nocapture > baseline.txt

# Compare against baseline later
cargo test --test performance_validation --release -- --nocapture > current.txt
diff baseline.txt current.txt
```

### Watch Mode (Continuous Benchmarking)
```bash
# Install cargo-watch
cargo install cargo-watch

# Auto-run performance tests on file changes
cargo watch -x "test --test performance_validation --release -- --nocapture"
```

## Expected Performance Results

### Core Metrics (Release Mode)

| Component | Metric | Target | Typical Result | Status |
|-----------|--------|--------|----------------|--------|
| **Core Synthesis** | ops/s | 10,000+ | 174,000 | ✅ 17x |
| **Batch Validation** | batches/s | 1,000+ | 2,500 | ✅ 2.5x |
| **Transaction Processing** | tx/s | 10,000+ | 25,000 | ✅ 2.5x |
| **Leader Election** | latency | <10μs | 7μs | ✅ 30% faster |
| **Graph Compaction** | time (10k nodes) | <100ms | 12ms | ✅ 8x faster |
| **Distributed Consensus** | tx/s (5 nodes) | 5,000+ | 6,600 | ✅ 32% faster |
| **Byte Canonicalization** | bytes/s | 100,000+ | 246,000 | ✅ 2.5x |

### System Tests (Release Mode)

| Test | Scenario | Duration | Result |
|------|----------|----------|--------|
| **Multi-Node Consensus** | 7 validators, 100 tx | ~0.5s | ✅ All converged |
| **System Under Load** | 5 nodes, 1000 tx | ~1.7s | ✅ 582 tx/s |
| **Byzantine Tolerance** | 5 honest + 2 byzantine | ~0.3s | ✅ Rejected malicious |
| **Partition Recovery** | 3 active + 2 partitioned | ~0.5s | ✅ Recovered & synced |

## Troubleshooting

### Benchmarks Too Slow?
```bash
# Ensure using release mode
cargo test --test performance_validation --release

# Check if running in debug mode accidentally
cargo test --test performance_validation  # This is debug mode!
```

### Inconsistent Results?
```bash
# Run single-threaded for consistency
cargo test --test performance_validation --release -- --test-threads=1 --nocapture

# Disable CPU frequency scaling (Linux)
sudo cpupower frequency-set --governor performance
```

### Out of Memory?
```bash
# Run tests one at a time
cargo test --release test_core_synthesis_performance -- --nocapture
cargo test --release test_batch_validation_performance -- --nocapture
# ... etc
```

## CI/CD Integration

### GitHub Actions Example
```yaml
- name: Run Performance Tests
  run: |
    cargo test --test performance_validation --release -- --nocapture
    cargo test --test end_to_end --release -- --nocapture
```

### Performance Regression Detection
```yaml
- name: Check Performance Regression
  run: |
    cargo test --test performance_validation --release -- --nocapture | tee current.txt
    if grep -q "FAILED" current.txt; then
      echo "Performance regression detected!"
      exit 1
    fi
```

## Quick Reference Card

```bash
# ⚡ Fast performance check (3 seconds)
cargo test --test performance_validation --release --quiet

# 📊 Detailed performance with metrics (3 seconds)
cargo test --test performance_validation --release -- --nocapture

# 🌐 Distributed system validation (2 seconds)
cargo test --test end_to_end --release -- --nocapture

# 🔍 Everything with timing (~75 seconds)
time cargo test --release

# 🎯 Single benchmark
cargo test --release test_core_synthesis_performance -- --nocapture
```

---

**Pro Tips:**
- Always use `--release` for accurate performance measurements
- Use `--nocapture` to see detailed metrics
- Run `--test-threads=1` for deterministic results
- Performance varies 10-20% based on system load
