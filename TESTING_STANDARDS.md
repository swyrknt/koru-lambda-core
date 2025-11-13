# Testing Standards - Forma Core (Rust)

## Philosophy

The Distinction Engine test suite employs **falsification methodology**. Tests are designed to attack hypotheses, not validate them. A hypothesis is considered sustained only when rigorous attempts to falsify it fail.

**Core principle**: Truth is established through failure to prove wrong, not through attempts to prove right.

## Falsification Approach

### Structure

Every research test must contain three components:

1. **Hypothesis**: Clear statement of the expected emergent property or behavior
2. **Falsification Target**: Specific condition that would prove the hypothesis wrong
3. **Measurement**: Precise methodology for detecting the falsification condition

### Example

```rust
/// Path Independence Test Suite
///
/// Tests whether synthesis results are independent of construction order,
/// verifying that the substrate exhibits true timeless consistency.
///
/// Falsification Target:
/// Path dependence - different construction sequences yield different
/// final distinctions, proving order-dependent (temporal) dynamics.
```

---

## Writing Tests in Rust

### Test Module Structure

All falsification tests live in dedicated test modules:

```rust
#[cfg(test)]
mod falsification_tests {
    use super::*;

    /// Helper utilities for falsification tests
    mod helpers {
        // Test-specific helper functions
    }

    // Individual falsification tests
}
```

### Test Function Structure

```rust
#[test]
fn test_falsify_hypothesis_name() {
    // ============================================================
    // HYPOTHESIS
    // ============================================================
    // Clear statement of expected emergent behavior

    // ============================================================
    // FALSIFICATION TARGET
    // ============================================================
    // Specific threshold or condition that indicates failure

    // ============================================================
    // MEASUREMENT
    // ============================================================

    // 1. Setup
    let mut engine = DistinctionEngine::new();

    // 2. Execute test protocol
    // ... test code ...

    // 3. Assert falsification condition
    assert_eq!(
        result, expected,
        "FALSIFIED: Description of what failed (metric: {:?})",
        metric_value
    );

    // 4. Report sustained hypothesis
    println!("\nHypothesis sustained.");
    println!("  Metric value: {}", metric_value);
}
```

### Assertion Format

Assertions must explicitly state the falsification condition:

```rust
// Good: Clear falsification message
assert_eq!(
    final_1.id(),
    final_2.id(),
    "FALSIFIED: Path Dependence detected. Final ID depends on assembly order."
);

// Good: Threshold-based falsification
assert!(
    ratio > 0.95,
    "FALSIFIED: Structural fragmentation detected (largest component: {:.2%})",
    ratio
);

// Good: Monotonicity check
assert!(
    current >= previous,
    "FALSIFIED: Causal radius decreased from {} to {}",
    previous,
    current
);
```

The assertion message must:
- Start with "FALSIFIED:"
- Explain what condition was violated
- Include relevant metric values using Rust formatting

### Output Format

Test output must be professional, concise, and scientific:

**Good:**
```rust
println!("\nTest: Structural Fragmentation Falsification");
println!("  Executing {} synthesis operations...", iterations);
println!("  Largest component ratio: {:.2%}", ratio);
println!("\nHypothesis sustained.");
```

**Bad:**
```rust
println!("🧪 ATTACKING THE THEORY!");
println!("⚠️ Running the 'Big Test'...");
println!("✨ Theory VALIDATED! ✨");
```

**Avoid:**
- Emojis or special characters
- Informal language ("attacking", "validated", scare quotes)
- Dramatic emphasis
- ALL CAPS (except "FALSIFIED:" in assertions)

---

## Measurement Methodology

### Standard Test Helpers

Create reusable helper functions in a `helpers` submodule:

```rust
mod helpers {
    use super::*;

    /// Creates a fresh engine and evolves it for N steps using random synthesis
    pub fn evolve_random(steps: usize) -> DistinctionEngine {
        let mut engine = DistinctionEngine::new();
        let mut rng = rand::thread_rng();

        for _ in 0..steps {
            let distinctions: Vec<_> = engine.get_state_snapshot().0
                .iter()
                .cloned()
                .cloned()
                .collect();

            if distinctions.len() >= 2 {
                let a = distinctions.choose(&mut rng).unwrap();
                let b = distinctions.choose(&mut rng).unwrap();
                engine.synthesize(a, b);
            }
        }

        engine
    }

    /// Computes the degree distribution of the relationship graph
    pub fn degree_distribution(engine: &DistinctionEngine) -> HashMap<String, usize> {
        let snapshot = engine.get_state_snapshot();
        let mut degrees: HashMap<String, usize> = HashMap::new();

        for (a, b) in snapshot.1 {
            *degrees.entry(a.clone()).or_insert(0) += 1;
            *degrees.entry(b.clone()).or_insert(0) += 1;
        }

        degrees
    }
}
```

### Evolution Strategies

Document the selection bias for any evolution function:

```rust
/// Evolves the substrate using degree-weighted selection
///
/// Selection Bias: Distinctions with higher degree (more relationships)
/// have proportionally higher probability of being selected for synthesis.
/// This tests whether preferential attachment emerges.
fn evolve_preferential(engine: &mut DistinctionEngine, steps: usize) {
    // Implementation...
}
```

### Metric Selection

Choose metrics that directly measure the falsification target:

| Metric | Use Case | Rust Implementation |
|--------|----------|---------------------|
| **Path Independence** | Verify timeless consistency | Compare final IDs from different paths |
| **Determinism** | Same inputs → same outputs | Multiple engines, compare results |
| **Irreflexivity** | `synthesize(a, a) = a` | Check ID unchanged |
| **Symmetry** | `synthesize(a, b) = synthesize(b, a)` | Compare IDs in both orders |
| **Connectivity** | Graph doesn't fragment | Count connected components |
| **Degree Distribution** | Test for emergent structure | Histogram of node degrees |

---

## Rigor Requirements

### Statistical Validity

```rust
// Good: Sufficient sample size
const SYNTHESIS_ITERATIONS: usize = 1000;
const MIN_GRAPH_SIZE: usize = 100;

// Good: Multiple trials for probabilistic tests
const NUM_TRIALS: usize = 100;
```

### Isolation

Each test must be independent:

```rust
#[test]
fn test_falsify_property_a() {
    let mut engine = DistinctionEngine::new();  // Fresh engine
    // Test A logic...
}

#[test]
fn test_falsify_property_b() {
    let mut engine = DistinctionEngine::new();  // Fresh engine
    // Test B logic - no shared state with Test A
}
```

### Reproducibility

For deterministic tests, avoid randomness:

```rust
// Good: Fully deterministic
#[test]
fn test_falsify_path_independence() {
    // No randomness - same result every run
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let a = engine.synthesize(&d0, &d1);
    // ...
}
```

For probabilistic tests, use seeded RNG:

```rust
// Good: Reproducible randomness
use rand::{SeedableRng, rngs::StdRng};

#[test]
fn test_falsify_fragmentation() {
    let mut rng = StdRng::seed_from_u64(42);  // Fixed seed
    // Use rng for random selection...
}
```

---

## Failure Reporting

When a test fails, the output must clearly indicate:

1. Which hypothesis was falsified
2. What metric violated the threshold
3. The actual measured value

Example:

```
---- falsification_tests::test_falsify_arrow_of_time stdout ----
Test: Arrow of Time Falsification
  Executing 1000 synthesis operations...
  Epoch 0 radius: 5
  Epoch 1 radius: 4

thread 'falsification_tests::test_falsify_arrow_of_time' panicked at 'FALSIFIED: Causal radius decreased from 5 to 4', tests/falsification.rs:89:5
```

---

## Success Reporting

When a test passes, report sustained hypothesis with measured values:

```rust
println!("\nHypothesis sustained.");
println!("  Path independence verified across {} construction sequences", num_paths);
println!("  All final distinctions identical: {}", final_id);
```

---

## Test Organization

### File Structure

```
tests/
├── falsification/
│   ├── mod.rs              # Shared helpers and utilities
│   ├── path_independence.rs
│   ├── determinism.rs
│   ├── structural.rs
│   └── temporal.rs
└── integration_tests.rs    # Legacy integration tests
```

### Module Organization

```rust
// tests/falsification/mod.rs
pub mod helpers;

// Re-export common types
pub use distinction_engine::{Distinction, DistinctionEngine};
```

```rust
// tests/falsification/path_independence.rs
use super::*;

#[test]
fn test_falsify_path_independence() {
    // Test implementation
}

#[test]
fn test_falsify_associativity() {
    // Related test
}
```

---

## Documentation Requirements

### Test-Level Documentation

Every test must have a doc comment:

```rust
/// Falsification Test: Path Independence
///
/// **Hypothesis**: Synthesis results are independent of construction order.
/// The final distinction ID depends only on the set of inputs, not the
/// sequence of operations used to construct it.
///
/// **Falsifies if**: Different construction sequences (paths) that should
/// yield identical results produce distinctions with different IDs.
///
/// **Measurement**:
/// 1. Construct distinction X via path A: (a + b) + c
/// 2. Construct distinction Y via path B: (a + c) + b
/// 3. Compare X.id() == Y.id()
#[test]
fn test_falsify_path_independence() {
    // ...
}
```

### Inline Comments for Complex Logic

```rust
// Construct base distinctions
let a = engine.synthesize(&d0, &d1);
let b = engine.synthesize(&a, &d0);

// Path 1: Left-first assembly
// ((a + b) + c)
let left_branch = engine_1.synthesize(&a, &b);
let final_1 = engine_1.synthesize(&left_branch, &c);

// Path 2: Right-first assembly
// ((a + c) + b)
let right_branch = engine_2.synthesize(&a, &c);
let final_2 = engine_2.synthesize(&right_branch, &b);
```

---

## Rust-Specific Best Practices

### Cloning for Independence

Tests often need independent copies:

```rust
// Good: Clone when you need independence
let d0 = engine.d0().clone();
let d1 = engine.d1().clone();
let result = engine.synthesize(&d0, &d1);
```

### Collecting Snapshots

```rust
// Good: Collect all distinctions for analysis
let distinctions: Vec<Distinction> = engine.get_state_snapshot().0
    .iter()
    .cloned()
    .cloned()
    .collect();
```

### Avoiding Borrow Checker Issues

```rust
// Bad: Holding reference while mutating
let d0 = engine.d0();  // Immutable borrow
let result = engine.synthesize(d0, d0);  // Can't mutate!

// Good: Clone before mutation
let d0 = engine.d0().clone();
let result = engine.synthesize(&d0, &d0);
```

### Iteration Patterns

```rust
// Good: Collect first, then iterate
let distinctions: Vec<_> = snapshot.0.iter().cloned().cloned().collect();
for d in &distinctions {
    // Can safely call engine methods here
}
```

---

## Naming Conventions

### Test Names

Format: `test_falsify_<property_name>`

Examples:
- `test_falsify_path_independence`
- `test_falsify_structural_fragmentation`
- `test_falsify_arrow_of_time`
- `test_falsify_determinism`

### Helper Functions

Format: `<action>_<strategy>`

Examples:
- `evolve_random()`
- `evolve_preferential()`
- `measure_connectivity()`
- `compute_degree_distribution()`

### Constants

```rust
const SYNTHESIS_ITERATIONS: usize = 1000;
const MIN_GRAPH_SIZE: usize = 100;
const CONNECTIVITY_THRESHOLD: f64 = 0.95;
```

---

## Template

```rust
/// Falsification Test: [Property Name]
///
/// **Hypothesis**: [Clear statement of expected behavior]
///
/// **Falsifies if**: [Specific condition that proves hypothesis wrong]
///
/// **Measurement**:
/// 1. [Step 1]
/// 2. [Step 2]
/// 3. [Comparison/assertion]
#[test]
fn test_falsify_property_name() {
    // ============================================================
    // SETUP
    // ============================================================
    let mut engine = DistinctionEngine::new();

    // ============================================================
    // EXECUTION
    // ============================================================
    println!("\nTest: [Property Name] Falsification");
    println!("  Executing test protocol...");

    // ... test logic ...

    // ============================================================
    // ASSERTION
    // ============================================================
    assert!(
        condition,
        "FALSIFIED: [Description] (metric: {})",
        metric
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Metric: {}", metric);
}
```

---

## Key Principles

1. **Attack, don't validate**: Design tests to find failure modes
2. **Be specific**: Define precise falsification conditions
3. **Be honest**: Let the data determine outcomes
4. **Be clear**: Use professional, scientific language
5. **Be rigorous**: Generate sufficient data for statistical validity
6. **Be Rusty**: Follow Rust idioms (ownership, borrowing, iterators)
7. **Be isolated**: Each test is independent and reproducible

---

## Running Tests

```bash
# Run all tests
cargo test

# Run only falsification tests
cargo test falsify

# Run specific test with output
cargo test test_falsify_path_independence -- --nocapture

# Run tests in release mode (faster for large iterations)
cargo test --release

# Run with detailed output
cargo test -- --nocapture --test-threads=1
```

---

## Review Checklist

Before submitting a falsification test, verify:

- [ ] Test has clear hypothesis in doc comment
- [ ] Falsification target is precisely defined
- [ ] Measurement methodology is documented
- [ ] Assertion message starts with "FALSIFIED:"
- [ ] Success message reports "Hypothesis sustained"
- [ ] Output is professional and scientific
- [ ] No emojis, dramatic language, or informal tone
- [ ] Test uses fresh engine (no shared state)
- [ ] Sufficient sample size for statistical validity
- [ ] Test is reproducible (deterministic or seeded)
- [ ] Helper functions are in `helpers` module
- [ ] Constants are named with SCREAMING_SNAKE_CASE
- [ ] Test passes `cargo clippy` and `cargo fmt`

---

**Truth emerges from surviving rigorous attack, not from gentle confirmation.**
