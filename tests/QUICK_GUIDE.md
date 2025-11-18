Running Tests - Quick Reference Guide

  Basic Commands

  Run All Tests

  # Run all tests (debug mode - faster compile, slower execution)
  cargo test

  # Run all tests in release mode (slower compile, faster execution - recommended 
  for long tests)
  cargo test --release

  # Show test output even for passing tests
  cargo test -- --nocapture

  Run Specific Tests

  # Run a specific test by name
  cargo test test_falsify_uniform_vulnerability

  # Run all tests matching a pattern
  cargo test robustness          # Runs all tests with "robustness" in the name
  cargo test falsify             # Runs all falsification tests

  # Run tests in a specific module
  cargo test commutativity::     # All tests in commutativity module

  Run Test Files

  # Run only library tests (src/ files)
  cargo test --lib

  # Run only integration tests (tests/ files)
  cargo test --test integration_tests

  # Run specific falsification test
  cargo test --test integration_tests
  robustness::test_falsify_uniform_vulnerability

  Useful Flags

  Parallel Control

  # Run tests single-threaded (useful for debugging)
  cargo test -- --test-threads=1

  # Run with 4 threads
  cargo test -- --test-threads=4

  Output Control

  # Show println! output for passing tests
  cargo test -- --nocapture

  # Show only failed test output
  cargo test -- --quiet

  # List all tests without running
  cargo test -- --list

  Filter by Result

  # Run only ignored tests
  cargo test -- --ignored

  # Run all tests including ignored
  cargo test -- --include-ignored

  Your Specific Tests

  Falsification Tests

  # Run all falsification tests
  cargo test --test integration_tests

  # Run individual falsification tests
  cargo test commutativity
  cargo test determinism
  cargo test non_associativity
  cargo test robustness
  cargo test information_dynamics
  cargo test structural_coherence

  # Run specific test with output
  cargo test --release test_falsify_uniform_vulnerability -- --nocapture

  Subsystem Tests

  # Run all validator tests
  cargo test validator

  # Run specific validator test
  cargo test test_single_transaction_batch

  Performance Tips

  For Quick Iteration

  # Fast compile, good for development
  cargo test --lib

  # Only run tests that failed last time
  cargo test -- --failed

  For Accurate Timing

  # Use release mode for accurate performance measurement
  cargo test --release -- --nocapture

  # Time a specific test
  time cargo test --release test_falsify_uniform_vulnerability -- --nocapture

  Advanced Usage

  Run with Specific Features

  # If you had features defined
  cargo test --features "feature_name"
  cargo test --all-features
  cargo test --no-default-features

  Documentation Tests

  # Run only doc tests
  cargo test --doc

  # Run specific doc test
  cargo test --doc module_name

## 🚀 Performance & Benchmarks

### Quick Performance Tests

The easiest way to measure performance is using our optimized performance validation tests:

```bash
# Run all performance tests (recommended)
cargo test --test performance_validation --release -- --nocapture

# Run specific performance test
cargo test --test performance_validation --release test_core_synthesis_performance -- --nocapture

# All performance tests with timing
time cargo test --test performance_validation --release -- --nocapture
```

### Available Performance Tests

1. **Core Synthesis** - Raw distinction synthesis throughput
   ```bash
   cargo test --release test_core_synthesis_performance -- --nocapture
   ```
   Target: 10,000+ ops/s | Achieved: ~213,000 ops/s

2. **Batch Validation** - Transaction batch processing
   ```bash
   cargo test --release test_batch_validation_performance -- --nocapture
   ```
   Target: 1,000+ batches/s | Achieved: ~2,600 batches/s

3. **Leader Election** - Deterministic leader selection speed
   ```bash
   cargo test --release test_leader_election_performance -- --nocapture
   ```
   Target: <10μs latency | Achieved: ~6.5μs

4. **Graph Compaction** - Large graph compression performance
   ```bash
   cargo test --release test_compaction_performance -- --nocapture
   ```
   Target: <100ms for 10k nodes | Achieved: ~9ms

5. **Distributed Consensus** - Multi-node coordination throughput
   ```bash
   cargo test --release test_distributed_consensus_throughput -- --nocapture
   ```
   Target: 5,000+ tx/s | Achieved: ~7,000 tx/s

6. **Byte Canonicalization** - Data mapping throughput
   ```bash
   cargo test --release test_byte_canonicalization_performance -- --nocapture
   ```
   Target: 100,000+ bytes/s | Achieved: ~278,000 bytes/s

### End-to-End System Tests

These tests validate complete distributed system behavior:

```bash
# Run all end-to-end tests
cargo test --test end_to_end -- --nocapture --test-threads=1

# Individual E2E tests
cargo test --test end_to_end test_e2e_multi_node_consensus -- --nocapture
cargo test --test end_to_end test_e2e_system_under_load_with_compaction -- --nocapture
cargo test --test end_to_end test_e2e_byzantine_fault_tolerance -- --nocapture
cargo test --test end_to_end test_e2e_network_partition_recovery -- --nocapture
```

### Criterion Benchmarks (Advanced)

For detailed statistical benchmarking:

```bash
# Run all criterion benchmarks
cargo bench

# Run specific benchmark suite
cargo bench core_synthesis
cargo bench transaction_validation
cargo bench leader_election

# View benchmark results
ls -lh target/criterion/

# Open HTML report in browser (after running benchmarks)
open target/criterion/report/index.html
```

### Performance Comparison

```bash
# Quick performance check (just run the tests)
cargo test --release --test performance_validation --quiet

# Detailed performance with all output
cargo test --release --test performance_validation -- --nocapture

# Compare debug vs release performance
cargo test --test performance_validation -- --nocapture  # Debug
cargo test --test performance_validation --release -- --nocapture  # Release
```

### Profiling & Analysis

```bash
# Profile with perf (Linux)
perf record -g cargo test --release test_core_synthesis_performance
perf report

# Profile with instruments (macOS)
cargo instruments -t time --release --test performance_validation

# Memory profiling with valgrind
valgrind --tool=massif cargo test --release test_core_synthesis_performance
```

  Your Project-Specific Examples

  Quick Smoke Test

  # Run core axiom tests only (very fast)
  cargo test --lib test_axiom

  Full Validation Suite

  # Run everything in release mode with output
  cargo test --release -- --nocapture

  Debug Single Test

  # Run one test with full output and single-threaded
  cargo test test_falsify_uniform_vulnerability -- --nocapture --test-threads=1

  Check What Tests Exist

  # List all tests
  cargo test -- --list

  # Count total tests
  cargo test -- --list | grep -c "test "

  Watch Mode (Continuous Testing)

  Install cargo-watch:
  cargo install cargo-watch

  Then run tests automatically on file changes:
  # Run all tests on save
  cargo watch -x test

  # Run specific test on save
  cargo watch -x "test test_falsify_uniform_vulnerability"

  # Run with release mode
  cargo watch -x "test --release"

  Common Patterns for Your Codebase

  # Quick development cycle (just core tests)
  cargo test --lib

  # Full test suite before commit
  cargo test --release

  # Deep dive into robustness behavior
  cargo test --release robustness -- --nocapture

  # Debug failing test with full info
  RUST_BACKTRACE=1 cargo test test_name -- --nocapture --test-threads=1

  # Run all tests except slow ones (if you mark them #[ignore])
  cargo test -- --skip slow

  Environment Variables

  # Show full backtraces on panic
  RUST_BACKTRACE=1 cargo test

  # Show even more detailed backtraces
  RUST_BACKTRACE=full cargo test

  # Set log level (if using env_logger)
  RUST_LOG=debug cargo test

  Examples You Can Run Right Now

  # 1. Quick sanity check (11 tests, ~1 second)
  cargo test --lib

  # 2. Run robustness tests with output (shows topology metrics)
  cargo test --release robustness -- --nocapture

  # 3. See information dynamics correlation
  cargo test --release information_dynamics -- --nocapture

  # 4. Run concurrency example
  cargo run --example concurrent_synthesis --release

  # 5. Full suite with timing
  time cargo test --release

## 📊 Test Suite Summary

Your test suite has **52 tests total**:

### By Category
- **26 library tests** (unit tests) - instant
- **18 integration tests** (falsification suite) - 60-70 seconds
- **4 end-to-end tests** (distributed system) - ~2 seconds
- **6 performance tests** (benchmarks) - ~3 seconds in release mode
- **1 doc test** - instant

### By Speed
- **Fast** (< 1s): Library tests, doc tests
- **Medium** (~2-3s): E2E tests, performance tests
- **Slow** (~60s): Information dynamics test (4000 evolution steps + 1000 observations)

### Quick Reference Commands

```bash
# Everything (all 52 tests)
cargo test --release

# Just the fast ones (27 tests, <1 second)
cargo test --lib

# Performance validation only (6 tests, ~3 seconds)
cargo test --test performance_validation --release -- --nocapture

# End-to-end distributed system (4 tests, ~2 seconds)
cargo test --test end_to_end -- --nocapture

# Full falsification suite (18 tests, ~70 seconds)
cargo test --test integration_tests
```

### Performance Targets vs Achieved

| Metric | Target | Achieved | Status |
|--------|--------|----------|--------|
| Core synthesis | 10k ops/s | 213k ops/s | ✅ 21x |
| Batch validation | 1k batches/s | 2.6k batches/s | ✅ 2.7x |
| Transaction throughput | 10k tx/s | 26k tx/s | ✅ 2.6x |
| Leader election | <10μs | 6.5μs | ✅ 35% faster |
| Graph compaction | <100ms | 9ms | ✅ 11x |
| Distributed consensus | 5k tx/s | 7k tx/s | ✅ 40% faster |