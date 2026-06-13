// Experiment 5: Phantom node fix validation.
//
// HYPOTHESIS
// ----------
// `ByteMapping::map_byte_to_distinction` returns Distinction objects whose
// IDs were computed against a throwaway engine (see primitives.rs:28).
// These IDs, and any IDs downstream of them, appear in the CALLING engine's
// relationships DashMap (because synthesize creates them) but NOT in its
// all_distinctions DashMap when the byte mapping is fetched directly from
// the cache WITHOUT a prior synthesize call.
//
// Actually, reading more carefully: `map_byte_to_distinction` looks up the
// ID in the cache and returns a fresh Distinction. It does NOT register it
// in the calling engine. So if a caller:
//
//     let byte_d = ByteMapping::map_byte_to_distinction(0x41, &engine);
//
// and then uses `byte_d` as an input to `engine.synthesize(&other, &byte_d)`,
// synthesize will:
//   1. See byte_d.id != other.id → proceed
//   2. Compute new_id = sha256 of concatenation
//   3. Insert new_id into all_distinctions
//   4. Call add_relationship(new_id, other.id) and add_relationship(new_id, byte_d.id)
//
// Result: `byte_d.id` appears in TWO relationship tuples as a parent, but
// is never registered in `all_distinctions`. It is a "phantom parent."
//
// Additionally: byte_d itself is the terminus of an 8-step derivation chain
// (bit_7, bit_6, ..., bit_0). All 8 intermediate IDs are phantom — they
// exist in the cache's throwaway engine but never touched this engine.
//
// METHOD
// ------
// 1. Build a fresh engine. Fetch byte 0x41 via ByteMapping.
// 2. Count: distinction_count vs unique IDs in relationships.
// 3. Show that get_distinction_by_id(byte_id) returns None.
// 4. Scale: fetch all 256 bytes, count phantoms.
// 5. Exercise actual usage: synthesize each byte with d0 (simulating what a
//    consumer does), and measure how many phantom IDs end up referenced but
//    unregistered in the parent chain.

use koru_lambda_core::{ByteMapping, DistinctionEngine};
use std::collections::HashSet;

fn ids_referenced_in_relationships(engine: &DistinctionEngine) -> HashSet<String> {
    let mut ids = HashSet::new();
    for (a, b) in engine.get_relationships_snapshot() {
        ids.insert(a);
        ids.insert(b);
    }
    ids
}

fn main() {
    println!("=== Experiment 5: Phantom Node Validation ===\n");

    // --- Phase 1: Bare byte lookup (no synthesize call made by us) ---
    println!("-- Phase 1: Bare byte lookup --");
    {
        let engine = DistinctionEngine::new();
        let before_distinctions = engine.distinction_count();
        let before_rels = engine.relationship_count();

        let byte_d = ByteMapping::map_byte_to_distinction(0x41, &engine);

        let after_distinctions = engine.distinction_count();
        let after_rels = engine.relationship_count();

        println!("  byte 0x41 -> id = {}", byte_d.to_hex());
        println!("  engine.distinction_count: before={} after={}", before_distinctions, after_distinctions);
        println!("  engine.relationship_count: before={} after={}", before_rels, after_rels);

        let lookup = engine.get_distinction_by_id(&byte_d.to_hex());
        println!("  engine.get_distinction_by_id(byte_id) = {:?}", lookup.as_ref().map(|d| d.to_hex()));

        if lookup.is_none() && after_distinctions == 2 {
            println!("  VERDICT: CONFIRMED -- byte lookup produces an ID unknown to the engine.");
        } else {
            println!("  VERDICT: REFUTED or unexpected state");
        }
    }

    // --- Phase 2: After using the byte in synthesize (consumer flow) ---
    println!("\n-- Phase 2: Consumer calls synthesize(d0, byte_d) --");
    {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let byte_d = ByteMapping::map_byte_to_distinction(0x41, &engine);

        let child = engine.synthesize(&d0, &byte_d);

        let registered_ids: HashSet<String> = engine
            .get_distinctions_snapshot()
            .iter()
            .map(|d| d.to_hex())
            .collect();
        let referenced_ids = ids_referenced_in_relationships(&engine);

        let phantoms: HashSet<_> = referenced_ids.difference(&registered_ids).cloned().collect();

        println!("  child.id = {}", child.to_hex());
        println!("  child is registered?  {}", registered_ids.contains(&child.to_hex()));
        println!("  byte_d.id is registered? {}", registered_ids.contains(&byte_d.to_hex()));
        println!("  distinctions registered: {}", registered_ids.len());
        println!("  distinct IDs in relationships: {}", referenced_ids.len());
        println!("  phantom IDs (referenced but NOT registered): {}", phantoms.len());
        for p in &phantoms {
            println!("    phantom: {}", p);
        }

        // One phantom expected: byte_d.id itself, since we never synthesized
        // the 8-step chain in THIS engine.
        assert!(phantoms.contains(&byte_d.to_hex()));
        println!("  ASSERT ok: byte_d.id is phantom");
    }

    // --- Phase 3: to_canonical_structure + synthesize for all 256 bytes ---
    println!("\n-- Phase 3: All 256 bytes consumed via synthesize --");
    {
        use koru_lambda_core::Canonicalizable;
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();

        // Same flow a consumer would take: fetch canonical for each byte,
        // then use it (here we synthesize each with d0).
        let mut byte_ids = Vec::new();
        for b in 0u8..=255 {
            let d = b.to_canonical_structure(&engine);
            byte_ids.push(d.to_hex());
            let _ = engine.synthesize(&d0, &d);
        }

        let registered_ids: HashSet<String> = engine
            .get_distinctions_snapshot()
            .iter()
            .map(|d| d.to_hex())
            .collect();
        let referenced_ids = ids_referenced_in_relationships(&engine);
        let phantoms: HashSet<_> = referenced_ids.difference(&registered_ids).cloned().collect();

        // Every byte id ought to be phantom (never synthesized in this engine).
        let byte_id_set: HashSet<String> = byte_ids.iter().cloned().collect();
        let byte_ids_phantom = byte_id_set.intersection(&phantoms).count();
        let unique_bytes = byte_id_set.len();

        println!("  unique byte IDs: {}", unique_bytes);
        println!("  registered distinctions: {}", registered_ids.len());
        println!("  referenced in relationships: {}", referenced_ids.len());
        println!("  total phantoms: {}", phantoms.len());
        println!("  of which byte IDs: {}", byte_ids_phantom);

        // Expected: every unique byte id is a phantom.
        if byte_ids_phantom == unique_bytes {
            println!("  VERDICT: CONFIRMED -- all 256 byte IDs are phantom");
        } else {
            println!("  VERDICT: PARTIAL -- {}/{} byte IDs phantom", byte_ids_phantom, unique_bytes);
        }
    }

    // --- Phase 4: Measure phantom fan-out in an 8-step chain ---
    // The byte derivation is 8 synthesize calls in the throwaway engine. Only
    // the terminal id is surfaced. The 7 intermediate ids are completely
    // invisible to the consumer but they WOULD show up in the relationship
    // graph if the fix built the chain against the consumer engine.
    println!("\n-- Phase 4: Counting intermediate derivations hidden by cache --");
    {
        let engine = DistinctionEngine::new();

        // Simulate the fix: build byte 0x41 against the real engine step by step.
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let byte: u8 = 0x41;
        let mut current = d0.clone();
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1;
            let bit_d = if bit == 1 { d1.clone() } else { d0.clone() };
            current = engine.synthesize(&current, &bit_d);
        }

        println!("  After synthesizing the 8-step chain for byte 0x41 in the real engine:");
        println!("    distinction_count = {}", engine.distinction_count());
        println!("    relationship_count = {}", engine.relationship_count());
        println!("    terminal id = {}", current.to_hex());
        let cache_id = ByteMapping::map_byte_to_distinction(byte, &engine);
        println!("    cache-reported id = {}", cache_id.to_hex());
        assert_eq!(current.to_hex(), cache_id.to_hex(), "chain terminal must equal cache id (determinism)");
        println!("  ASSERT ok: chain terminal matches cache id (proves fix is equivalent)");

        // How many *new* nodes would a fix add per byte?
        // Up to 8 intermediate synthesis results per byte. With bit alternation
        // and shared prefixes across bytes the marginal cost decreases.
    }

    // --- Phase 5: Per-byte incremental cost of the fix (256 bytes) ---
    println!("\n-- Phase 5: Incremental cost of fix (all 256 bytes) --");
    {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();

        for byte in 0u8..=255 {
            let mut current = d0.clone();
            for i in (0..8).rev() {
                let bit = (byte >> i) & 1;
                let bit_d = if bit == 1 { d1.clone() } else { d0.clone() };
                current = engine.synthesize(&current, &bit_d);
            }
        }

        println!("  With fix: engine.distinction_count = {}", engine.distinction_count());
        println!("  With fix: engine.relationship_count = {}", engine.relationship_count());

        // Invariant check: r = 2d - 3 (see CLAUDE.md)
        let d = engine.distinction_count();
        let r = engine.relationship_count();
        let expected_r = 2 * (d as i64) - 3;
        println!("  Invariant r=2d-3: r={} expected={} match={}", r, expected_r, (r as i64) == expected_r);
    }

    // --- Phase 6: Audit existing tests that assert distinction_count ---
    println!("\n-- Phase 6: Tests sensitive to a phantom-fix --");
    println!("  The following assertions in /src/lib.rs would need updating under a fix:");
    println!("    test_axiom_irreflexivity: distinction_count == 2 (unaffected, no byte use)");
    println!("    test_axiom_synthesis: distinction_count == 3 (unaffected)");
    println!("    test_axiom_idempotency: distinction_count unchanged (unaffected)");
    println!("    test_byte_mapping: no count assertion (unaffected)");
    println!("    test_canonicalizable_trait: no count assertion (unaffected)");
    println!("  Conclusion: in-tree lib.rs tests are tolerant. Consumer tests that");
    println!("  cache distinction_count snapshots across ByteMapping calls would");
    println!("  observe an increase after a fix (currently frozen at 2).");

    println!("\n=== Done ===");
}
