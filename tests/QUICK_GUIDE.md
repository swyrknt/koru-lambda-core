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
  cargo test conscious_dynamics

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

  Benchmark Mode

  # Run benchmarks (if you have #[bench] tests)
  cargo bench

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

  Your test suite has 22 tests total:
  - 11 library tests (instant)
  - 10 integration tests (60-70 seconds)
  - 1 doc test (instant)

  The information_dynamics test is the slowest (~60 seconds) due to 4000 evolution
   steps + 1000 observation steps.