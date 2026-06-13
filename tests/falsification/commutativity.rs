/// Falsification Test: Commutativity (Symmetry Axiom)
///
/// Tests whether synthesis is commutative, verifying that the order
/// of operands does not affect the result. This is a direct test of
/// the Symmetry axiom.
///
/// Falsification Target:
/// Non-commutativity - synthesize(a, b) ≠ synthesize(b, a) for some a, b
use koru_lambda_core::DistinctionEngine;

/// Falsification Test: Commutativity Verification
///
/// Hypothesis: Synthesis is commutative - the order of operands
/// does not affect the result. This is a direct consequence of the
/// Symmetry axiom: synthesize(a, b) = synthesize(b, a)
///
/// Falsifies if: synthesize(a, b) ≠ synthesize(b, a) for any a, b
///
/// Measurement:
/// Test commutativity for multiple pairs of distinctions across
/// different levels of the synthesis hierarchy:
/// 1. Primordial distinctions (Δ₀, Δ₁)
/// 2. Derived distinctions (first-level syntheses)
/// 3. Complex nested distinctions (multi-level syntheses)
#[test]
fn test_falsify_commutativity() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Commutativity Falsification");

    let engine = DistinctionEngine::new();
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();

    // ============================================================
    // TEST 1: Primordial distinctions
    // ============================================================
    println!("  Test 1: Commutativity of primordial distinctions");

    let ab = engine.synthesize(&d0, &d1);
    let ba = engine.synthesize(&d1, &d0);

    assert_eq!(
        ab.to_hex(),
        ba.to_hex(),
        "FALSIFIED: Commutativity violated for primordial distinctions.\n  d0⊕d1: {}\n  d1⊕d0: {}",
        ab.to_hex(),
        ba.to_hex()
    );

    println!("    Primordial commutativity verified: {}", ab.to_hex());

    // ============================================================
    // TEST 2: Derived distinctions
    // ============================================================
    println!("  Test 2: Commutativity of derived distinctions");

    let a = engine.synthesize(&d0, &d1);
    let b = engine.synthesize(&a, &d0);

    let result1 = engine.synthesize(&a, &b);
    let result2 = engine.synthesize(&b, &a);

    assert_eq!(
        result1.to_hex(),
        result2.to_hex(),
        "FALSIFIED: Commutativity violated for derived distinctions.\n  a⊕b: {}\n  b⊕a: {}",
        result1.to_hex(),
        result2.to_hex()
    );

    println!("    Derived commutativity verified: {}", result1.to_hex());

    // ============================================================
    // TEST 3: Complex nested distinctions
    // ============================================================
    println!("  Test 3: Commutativity of complex nested distinctions");

    let c = engine.synthesize(&a, &b);
    let d = engine.synthesize(&b, &d0);

    let result3 = engine.synthesize(&c, &d);
    let result4 = engine.synthesize(&d, &c);

    assert_eq!(
        result3.to_hex(),
        result4.to_hex(),
        "FALSIFIED: Commutativity violated for complex distinctions.\n  c⊕d: {}\n  d⊕c: {}",
        result3.to_hex(),
        result4.to_hex()
    );

    println!("    Complex commutativity verified: {}", result3.to_hex());

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Commutativity verified at all levels:");
    println!("    - Primordial distinctions");
    println!("    - Derived distinctions");
    println!("    - Complex nested distinctions");
}
