/// Falsification Test: Determinism and Timeless Consistency
///
/// Tests whether synthesis results are deterministic across different
/// engines and contexts, verifying that the substrate exhibits true
/// timeless consistency.
///
/// Falsification Target:
/// Temporal or contextual dependence - synthesis results vary based on
/// when, where, or how they are computed.

use distinction_engine::DistinctionEngine;

/// Falsification Test: Determinism Across Engines
///
/// **Hypothesis**: Synthesizing identical distinctions in completely
/// separate engines always yields identical results. The synthesis
/// operation is deterministic and context-independent.
///
/// **Falsifies if**: The same synthesis operation performed in different
/// engines produces distinctions with different IDs.
///
/// **Measurement**:
/// 1. Perform synthesis sequence in Engine A
/// 2. Perform identical synthesis sequence in Engine B
/// 3. Verify all intermediate and final distinctions have identical IDs
#[test]
fn test_falsify_determinism_across_engines() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Determinism Across Engines Falsification");
    println!("  Testing synthesis determinism...");

    // ============================================================
    // ENGINE 1: First synthesis sequence
    // ============================================================
    let mut engine1 = DistinctionEngine::new();
    let d0_1 = engine1.d0().clone();
    let d1_1 = engine1.d1().clone();

    let a1 = engine1.synthesize(&d0_1, &d1_1);
    let b1 = engine1.synthesize(&a1, &d0_1);
    let c1 = engine1.synthesize(&a1, &d1_1);
    let final1 = engine1.synthesize(&b1, &c1);

    println!("  Engine 1 final ID: {}", final1.id());

    // ============================================================
    // ENGINE 2: Identical synthesis sequence
    // ============================================================
    let mut engine2 = DistinctionEngine::new();
    let d0_2 = engine2.d0().clone();
    let d1_2 = engine2.d1().clone();

    let a2 = engine2.synthesize(&d0_2, &d1_2);
    let b2 = engine2.synthesize(&a2, &d0_2);
    let c2 = engine2.synthesize(&a2, &d1_2);
    let final2 = engine2.synthesize(&b2, &c2);

    println!("  Engine 2 final ID: {}", final2.id());

    // ============================================================
    // ASSERTION: All distinctions must be identical
    // ============================================================

    // Verify primordial distinctions
    assert_eq!(
        d0_1.id(),
        d0_2.id(),
        "FALSIFIED: Primordial distinction d0 differs across engines"
    );

    assert_eq!(
        d1_1.id(),
        d1_2.id(),
        "FALSIFIED: Primordial distinction d1 differs across engines"
    );

    // Verify intermediate distinctions
    assert_eq!(
        a1.id(),
        a2.id(),
        "FALSIFIED: Intermediate distinction 'a' differs across engines.\n  Engine 1: {}\n  Engine 2: {}",
        a1.id(),
        a2.id()
    );

    assert_eq!(
        b1.id(),
        b2.id(),
        "FALSIFIED: Intermediate distinction 'b' differs across engines.\n  Engine 1: {}\n  Engine 2: {}",
        b1.id(),
        b2.id()
    );

    assert_eq!(
        c1.id(),
        c2.id(),
        "FALSIFIED: Intermediate distinction 'c' differs across engines.\n  Engine 1: {}\n  Engine 2: {}",
        c1.id(),
        c2.id()
    );

    // Verify final distinction
    assert_eq!(
        final1.id(),
        final2.id(),
        "FALSIFIED: Final distinction differs across engines.\n  Engine 1: {}\n  Engine 2: {}",
        final1.id(),
        final2.id()
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Determinism verified across independent engines");
    println!("  All distinctions identical: primordial, intermediate, and final");
}
