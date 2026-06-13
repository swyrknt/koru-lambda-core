/// Falsification Test: Non-Associativity (Documented Behavior)
///
/// This test DOCUMENTS that synthesis is intentionally non-associative.
/// The synthesis operation creates unique distinctions for each specific
/// synthesis event, meaning the construction history matters.
///
/// This is NOT a bug - it's a fundamental property that allows the system
/// to track and preserve construction history.
///
/// Falsification Target:
/// Unintended associativity - if (a⊕b)⊕c = (a⊕c)⊕b, this would indicate
/// that construction history is being lost, which would violate the
/// system's design intent.
use koru_lambda_core::DistinctionEngine;

/// Falsification Test: Non-Associativity Verification
///
/// Hypothesis: Synthesis is NON-associative - different construction
/// sequences create different distinctions, preserving construction history.
/// Specifically: (a⊕b)⊕c ≠ (a⊕c)⊕b
///
/// Falsifies if: (a⊕b)⊕c = (a⊕c)⊕b, indicating that construction
/// history is lost and the system is incorrectly associative.
///
/// Measurement:
/// 1. Construct distinction X via: (a⊕b)⊕c
/// 2. Construct distinction Y via: (a⊕c)⊕b
/// 3. Verify X.to_hex() ≠ Y.to_hex() (non-associativity preserved)
///
/// NOTE: This test PASSES when the two constructions yield DIFFERENT
/// IDs, confirming that construction history is preserved.
#[test]
fn test_falsify_unintended_associativity() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Non-Associativity Verification (Documented Behavior)");
    println!("  Testing that construction history is preserved...");

    let engine1 = DistinctionEngine::new();
    let engine2 = DistinctionEngine::new();

    let d0 = engine1.d0().clone();
    let d1 = engine1.d1().clone();

    // ============================================================
    // PATH 1: (a⊕b)⊕c
    // ============================================================
    println!("  Path 1: (a⊕b)⊕c");

    // Build base distinctions
    let a1 = engine1.synthesize(&d0, &d1);
    let b1 = engine1.synthesize(&a1, &d0);
    let c1 = engine1.synthesize(&a1, &d1);

    // Construct via path 1
    let ab = engine1.synthesize(&a1, &b1);
    let result1 = engine1.synthesize(&ab, &c1);

    println!("    Result 1 ID: {}", result1.to_hex());

    // ============================================================
    // PATH 2: (a⊕c)⊕b
    // ============================================================
    println!("  Path 2: (a⊕c)⊕b");

    // Build identical base distinctions in separate engine
    let a2 = engine2.synthesize(&d0, &d1);
    let b2 = engine2.synthesize(&a2, &d0);
    let c2 = engine2.synthesize(&a2, &d1);

    // Construct via path 2
    let ac = engine2.synthesize(&a2, &c2);
    let result2 = engine2.synthesize(&ac, &b2);

    println!("    Result 2 ID: {}", result2.to_hex());

    // ============================================================
    // ASSERTION: Results should be DIFFERENT
    // ============================================================
    assert_ne!(
        result1.to_hex(),
        result2.to_hex(),
        "FALSIFIED: Unintended associativity detected! Construction history is lost.\n  \
         This indicates (a⊕b)⊕c = (a⊕c)⊕b, which violates the design intent.\n  \
         Both paths yielded: {}",
        result1.to_hex()
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Non-associativity verified:");
    println!("    (a⊕b)⊕c ≠ (a⊕c)⊕b");
    println!("  Construction history is preserved");
    println!("  Path 1 result: {}", result1.to_hex());
    println!("  Path 2 result: {}", result2.to_hex());
}

/// Falsification Test: Construction History Preservation
///
/// Hypothesis: The system preserves complete construction history
/// by creating unique IDs for each distinct synthesis sequence.
///
/// Falsifies if: Multiple distinct construction sequences produce
/// identical final distinctions, indicating history loss.
///
/// Measurement:
/// Test multiple construction sequences with the same "ingredients"
/// but different orders, verifying each produces a unique distinction.
#[test]
fn test_falsify_construction_history_loss() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Construction History Preservation");
    println!("  Testing multiple construction sequences...");

    let d0 = {
        let engine = DistinctionEngine::new();
        engine.d0().clone()
    };
    let d1 = {
        let engine = DistinctionEngine::new();
        engine.d1().clone()
    };

    // ============================================================
    // Three different construction sequences using same base elements
    // ============================================================

    // Sequence 1: ((d0⊕d1)⊕d0)⊕d1
    let engine1 = DistinctionEngine::new();
    let step1_a = engine1.synthesize(&d0, &d1);
    let step2_a = engine1.synthesize(&step1_a, &d0);
    let result_a = engine1.synthesize(&step2_a, &d1);

    // Sequence 2: ((d0⊕d1)⊕d1)⊕d0
    let engine2 = DistinctionEngine::new();
    let step1_b = engine2.synthesize(&d0, &d1);
    let step2_b = engine2.synthesize(&step1_b, &d1);
    let result_b = engine2.synthesize(&step2_b, &d0);

    // Sequence 3: (d0⊕d1)⊕(d0⊕d1) - uses same base twice
    let engine3 = DistinctionEngine::new();
    let step1_c = engine3.synthesize(&d0, &d1);
    let result_c = engine3.synthesize(&step1_c, &step1_c);

    println!("  Sequence 1 ID: {}", result_a.to_hex());
    println!("  Sequence 2 ID: {}", result_b.to_hex());
    println!("  Sequence 3 ID: {}", result_c.to_hex());

    // ============================================================
    // ASSERTION: All three should be DIFFERENT
    // ============================================================

    assert_ne!(
        result_a.to_hex(),
        result_b.to_hex(),
        "FALSIFIED: Sequences 1 and 2 produced identical results (history loss)"
    );

    assert_ne!(
        result_a.to_hex(),
        result_c.to_hex(),
        "FALSIFIED: Sequences 1 and 3 produced identical results (history loss)"
    );

    assert_ne!(
        result_b.to_hex(),
        result_c.to_hex(),
        "FALSIFIED: Sequences 2 and 3 produced identical results (history loss)"
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Construction history fully preserved");
    println!("  All three distinct sequences produced unique distinctions");
}
