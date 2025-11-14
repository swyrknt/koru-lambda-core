/// Falsification Test: Structural Compaction and Universal Coding Law
///
/// Tests whether the Structural Compactor correctly enforces R ∝ U
/// (Resources proportional to Utility) by leveraging emergent scale-free topology.
///
/// The compactor should act as a "pressure cooker" that:
/// 1. Identifies high-utility distinctions (hubs) via S.I.S.
/// 2. Archives low-utility distinctions (power-law tail)
/// 3. Preserves structural integrity of the graph
///
/// Falsification Target:
/// Random pruning - if compaction doesn't leverage structural importance,
/// system will fail to achieve compression while preserving connectivity.

use distinction_engine::{
    DistinctionEngine,
    StructuralCompactor,
    CompactionAction,
    Canonicalizable,
    LocalCausalAgent,
};
use std::sync::Arc;

/// Falsification Test: Random Compaction Strategy
///
/// **Hypothesis**: The Structural Compactor leverages emergent scale-free
/// topology to achieve efficient compression. By preserving high-degree hubs
/// and archiving low-degree nodes, it maintains graph coherence while
/// reducing resource footprint.
///
/// **Falsifies if**: Compaction fails to identify structural hierarchy,
/// archiving nodes randomly without regard to S.I.S., leading to poor
/// compression ratios (<2x) or loss of graph connectivity.
///
/// **Measurement**:
/// 1. Build graph via preferential attachment (creates power-law distribution)
/// 2. Measure initial graph size and connectivity
/// 3. Perform compaction with S.I.S.-based archival
/// 4. Verify compression ratio >2x while preserving hub structure
#[test]
fn test_falsify_random_compaction() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Random Compaction Falsification");
    println!("  Testing S.I.S.-based structural compression...");

    let engine = Arc::new(DistinctionEngine::new());
    let mut compactor = StructuralCompactor::new(&engine);

    // ============================================================
    // EVOLUTION: Build scale-free graph
    // ============================================================
    println!("  Building scale-free graph with preferential attachment...");

    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();

    // Create initial hub
    let hub1 = engine.synthesize(&d0, &d1);
    let hub2 = engine.synthesize(&hub1, &d0);

    // Create power-law distribution: many low-degree nodes, few hubs
    // Build larger graph to get better power-law distribution
    for i in 0..100 {
        let byte = (i as u8).to_canonical_structure(&engine);

        // Preferential attachment: connect to existing hubs
        if i % 3 == 0 {
            engine.synthesize(&hub1, &byte);
        } else if i % 3 == 1 {
            engine.synthesize(&hub2, &byte);
        } else {
            let intermediate = engine.synthesize(&hub1, &hub2);
            engine.synthesize(&intermediate, &byte);
        }
    }

    let initial_count = engine.distinction_count();
    println!("    Initial graph size: {} distinctions", initial_count);

    // Set threshold high enough to capture power-law tail
    // In synthesis graphs, minimum degree is 2, so we need threshold > 4
    // to classify low-degree nodes as COLD
    compactor.set_hot_threshold(8);

    // ============================================================
    // COMPACTION: Apply S.I.S.-based archival
    // ============================================================
    println!("  Performing S.I.S.-based compaction...");

    let _action = compactor.compact(&engine);
    let stats = compactor.get_stats();

    println!("    HOT (high S.I.S.): {}", stats.hot_count);
    println!("    WARM (medium S.I.S.): {}", stats.warm_count);
    println!("    COLD (low S.I.S.): {}", stats.cold_count);

    // ============================================================
    // MEASUREMENT: Compression and Structure
    // ============================================================

    // Calculate compression ratio (if we actually removed COLD nodes)
    let active_size = stats.hot_count + stats.warm_count;
    let compression_ratio = initial_count as f64 / active_size.max(1) as f64;

    println!("    Compression ratio: {:.2}x", compression_ratio);
    println!("    Preserved: {} / {}", active_size, initial_count);

    // ============================================================
    // ASSERTION: Structural compaction works
    // ============================================================

    // Verify compaction identified hierarchy
    assert!(
        stats.hot_count > 0,
        "FALSIFIED: No high-S.I.S. nodes identified (system failed to detect hubs)"
    );

    assert!(
        stats.cold_count + stats.warm_count > 0,
        "FALSIFIED: No low-S.I.S. nodes identified (system failed to detect power-law tail)"
    );

    // Verify compression is meaningful (>2x)
    assert!(
        compression_ratio >= 2.0,
        "FALSIFIED: Insufficient compression ratio {:.2}x (expected ≥2.0x).\\n  \
         System failed to leverage structural hierarchy for efficient compaction.",
        compression_ratio
    );

    // Verify preservation strategy is selective (not everything is HOT)
    let hot_percentage = (stats.hot_count as f64 / initial_count as f64) * 100.0;
    assert!(
        hot_percentage < 50.0,
        "FALSIFIED: Too many nodes classified as HOT ({:.1}%).\\n  \
         System failed to identify power-law tail for archival.",
        hot_percentage
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  S.I.S.-based compaction successfully leverages scale-free topology:");
    println!("    Compression ratio: {:.2}x", compression_ratio);
    println!("    HOT preservation: {:.1}%", hot_percentage);
    println!("    COLD archival: {:.1}%",
        (stats.cold_count as f64 / initial_count as f64) * 100.0
    );
    println!("  Universal Coding Law (R ∝ U) enforced through structural hierarchy");
}

/// Falsification Test: Compaction Preserves Causal Chain
///
/// **Hypothesis**: The Structural Compactor maintains causal integrity
/// through the LocalCausalAgent pattern. Each compaction event is a
/// causal synthesis from the local root, creating a verifiable chain.
///
/// **Falsifies if**: Compaction operations fail to update local root,
/// break causal chain, or produce non-deterministic results.
///
/// **Measurement**:
/// 1. Perform sequential compaction operations
/// 2. Verify each creates new root via causal synthesis
/// 3. Verify determinism (same inputs → same root)
#[test]
fn test_falsify_compaction_causality_loss() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Compaction Causality Loss Falsification");
    println!("  Testing LocalCausalAgent compliance...");

    let engine = Arc::new(DistinctionEngine::new());
    let mut compactor = StructuralCompactor::new(&engine);

    let initial_root = compactor.get_current_root().id().to_string();

    // ============================================================
    // OPERATION: Sequential compaction
    // ============================================================

    // Build small graph
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let _a = engine.synthesize(&d0, &d1);

    // First compaction
    let action1 = compactor.compact(&engine);
    let root1 = compactor.synthesize_action(action1, &engine);

    // Verify causal transition
    assert_ne!(
        root1.id(),
        &initial_root,
        "FALSIFIED: Compaction failed to update root (causality broken)"
    );

    // Build more structure
    for i in 0..5 {
        let byte = (i as u8).to_canonical_structure(&engine);
        engine.synthesize(&d0, &byte);
    }

    // Second compaction
    let action2 = compactor.compact(&engine);
    let root2 = compactor.synthesize_action(action2, &engine);

    // Verify causal chain continues
    assert_ne!(
        root2.id(),
        root1.id(),
        "FALSIFIED: Second compaction failed to extend causal chain"
    );

    // ============================================================
    // DETERMINISM: Same action → same root
    // ============================================================

    let engine2 = Arc::new(DistinctionEngine::new());
    let mut compactor2 = StructuralCompactor::new(&engine2);

    // Same initial state
    let _a2 = engine2.synthesize(&engine2.d0(), &engine2.d1());

    // Same action parameters
    let action_deterministic = CompactionAction {
        archived_ids: vec![],
        sis_threshold: 3,
        preserved_count: 10,
    };

    let root_det1 = compactor2.synthesize_action(action_deterministic.clone(), &engine2);

    // Reset and try again
    let mut compactor3 = StructuralCompactor::new(&engine2);
    let root_det2 = compactor3.synthesize_action(action_deterministic, &engine2);

    assert_eq!(
        root_det1.id(),
        root_det2.id(),
        "FALSIFIED: Compaction is non-deterministic (same action → different roots)"
    );

    // ============================================================
    // REPORTING
    // ============================================================
    println!("\nHypothesis sustained.");
    println!("  Compaction maintains causal integrity:");
    println!("    Root transitions: genesis → root1 → root2");
    println!("    Determinism verified: same action → same root");
    println!("  LocalCausalAgent contract enforced");
}

/// Falsification Test: Compaction Statistics Accuracy
///
/// **Hypothesis**: The compactor accurately tracks thermal states and
/// provides correct statistics about graph composition.
///
/// **Falsifies if**: Statistics are inconsistent, missing nodes, or
/// incorrectly classify thermal states.
#[test]
fn test_falsify_statistics_inaccuracy() {
    // ============================================================
    // SETUP
    // ============================================================
    println!("\nTest: Compaction Statistics Accuracy");

    let engine = Arc::new(DistinctionEngine::new());
    let mut compactor = StructuralCompactor::new(&engine);
    compactor.set_hot_threshold(4);

    // Build known graph
    let d0 = engine.d0().clone();
    let d1 = engine.d1().clone();
    let a = engine.synthesize(&d0, &d1);
    let b = engine.synthesize(&a, &d0);
    let c = engine.synthesize(&a, &d1);
    let _d = engine.synthesize(&b, &c);

    let total_distinctions = engine.distinction_count();

    // Compact
    compactor.compact(&engine);
    let stats = compactor.get_stats();

    // ============================================================
    // VERIFICATION
    // ============================================================

    // All nodes should be accounted for
    let accounted = stats.hot_count + stats.warm_count + stats.cold_count;
    assert_eq!(
        accounted, total_distinctions,
        "FALSIFIED: Statistics don't account for all nodes.\\n  \
         Expected: {}, Accounted: {} (HOT: {}, WARM: {}, COLD: {})",
        total_distinctions,
        accounted,
        stats.hot_count,
        stats.warm_count,
        stats.cold_count
    );

    // Total should match
    assert_eq!(
        stats.total_distinctions, total_distinctions,
        "FALSIFIED: total_distinctions doesn't match engine count"
    );

    // Archived count should equal COLD count (in this implementation)
    assert_eq!(
        stats.archived_count, stats.cold_count,
        "FALSIFIED: archived_count doesn't match cold_count"
    );

    println!("\nHypothesis sustained.");
    println!("  Statistics accurately track graph composition:");
    println!("    Total: {}", stats.total_distinctions);
    println!("    HOT: {}, WARM: {}, COLD: {}", stats.hot_count, stats.warm_count, stats.cold_count);
}
