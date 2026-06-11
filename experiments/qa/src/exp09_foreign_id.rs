// Experiment 9: Foreign-ID poisoning.
//
// HYPOTHESIS
// ----------
// `Distinction::new(String)` is public (primitives.rs / engine.rs:13).
// Any caller can fabricate a Distinction with an arbitrary ID string,
// including:
//   - ID of a node from a different engine ("cross-engine" reuse)
//   - A never-synthesized but well-formed looking ID ("foreign" ID)
//   - Garbage strings like empty, very long, or non-ASCII
//
// When such a foreign Distinction is passed into `engine.synthesize`, the
// engine has no way to verify provenance: it hashes the canonical pair and
// creates a new child, with relationships pointing to the foreign ID as a
// parent. The child IS registered. But the foreign parent is NEVER
// registered in `all_distinctions`, creating a "phantom parent" indistinguishable
// from the byte-mapping phantom bug.
//
// METHOD
// ------
// 1. Construct Distinction::new("deadbeef".repeat(8)).
// 2. Call engine.synthesize(d0, &foreign). Observe: panics, succeeds, etc.
// 3. Inspect relationships for foreign id presence.
// 4. Inspect all_distinctions for foreign id presence (expect: absent).
// 5. Demonstrate cross-engine poisoning: node from engine A used in engine B.
// 6. Edge cases: empty ID, huge ID.

use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};

fn relationship_participants(engine: &DistinctionEngine) -> HashSet<String> {
    let mut set = HashSet::new();
    for (a, b) in engine.get_relationships_snapshot() {
        set.insert(a);
        set.insert(b);
    }
    set
}

fn main() {
    println!("=== Experiment 9: Foreign-ID Poisoning ===\n");

    // --- Case 1: canonical-looking foreign ID ---
    println!("-- Case 1: canonical-looking foreign ID (hex-like) --");
    {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let foreign = Distinction::new("deadbeef".repeat(8));

        let result = panic::catch_unwind(AssertUnwindSafe(|| engine.synthesize(&d0, &foreign)));
        match result {
            Ok(child) => {
                println!("  synthesize(d0, foreign) SUCCEEDED: child.id={}", child.id());
                let registered = engine.get_distinction_by_id(child.id()).is_some();
                let foreign_registered = engine.get_distinction_by_id(foreign.id()).is_some();
                let participants = relationship_participants(&engine);
                let foreign_referenced = participants.contains(foreign.id());

                println!("  Child registered in engine: {}", registered);
                println!("  Foreign registered in engine: {}", foreign_registered);
                println!("  Foreign referenced in relationships: {}", foreign_referenced);
                println!("  Engine distinction_count: {}", engine.distinction_count());
                println!("  Engine relationship_count: {}", engine.relationship_count());
                // Invariant r = 2d - 3 still holds?
                let d = engine.distinction_count();
                let r = engine.relationship_count();
                let expected_r = 2i64 * d as i64 - 3;
                println!("  r={}  expected=2d-3={}  invariant_holds={}", r, expected_r, r as i64 == expected_r);

                if foreign_referenced && !foreign_registered {
                    println!("  VERDICT: CONFIRMED -- foreign ID appears as a phantom parent.");
                    println!("  SEVERITY: HIGH -- any external code can inject unauthenticated");
                    println!("  parent IDs into the relationship graph without panic or error.");
                }
            }
            Err(_) => println!("  synthesize PANICKED on foreign input (would be safer)"),
        }
    }

    // --- Case 2: same-engine cross-engine: use a Distinction from engine A in engine B ---
    println!("\n-- Case 2: Distinction from engine A used in engine B --");
    {
        let engine_a = DistinctionEngine::new();
        let engine_b = DistinctionEngine::new();
        // Create a real distinction in A via a chain
        let a_d0 = engine_a.d0().clone();
        let a_d1 = engine_a.d1().clone();
        let a_child = engine_a.synthesize(&a_d0, &a_d1);
        let deep_a = {
            let mut cur = a_child.clone();
            for _ in 0..5 {
                cur = engine_a.synthesize(&cur, &a_d1);
            }
            cur
        };

        // Now pass it to engine_b.synthesize
        let b_d0 = engine_b.d0().clone();
        let before_b = engine_b.distinction_count();
        let child_in_b = engine_b.synthesize(&b_d0, &deep_a);
        let after_b = engine_b.distinction_count();

        println!("  engine_a distinction_count (before): {}", engine_a.distinction_count());
        println!("  Moved id={} from A into B.synthesize", deep_a.id());
        println!("  engine_b distinction_count: {} -> {}", before_b, after_b);
        println!("  Child registered in B: {}", engine_b.get_distinction_by_id(child_in_b.id()).is_some());
        println!("  Foreign (from A) registered in B: {}", engine_b.get_distinction_by_id(deep_a.id()).is_some());
        let participants_b = relationship_participants(&engine_b);
        println!("  Foreign (from A) referenced in B.relationships: {}", participants_b.contains(deep_a.id()));

        // Determinism property: does engine_b produce the same child id as a hypothetical
        // engine_a.synthesize(d0_a, deep_a)?
        let parallel = engine_a.synthesize(engine_a.d0(), &deep_a);
        println!("  child_in_b.id == engine_a.synthesize(d0_a, deep_a).id = {}",
            child_in_b.id() == parallel.id());
        println!("  (This is content addressing working as designed. But engine_b now has");
        println!("   a child whose parent is a ghost to it.)");
        if participants_b.contains(deep_a.id())
            && engine_b.get_distinction_by_id(deep_a.id()).is_none()
        {
            println!("  VERDICT: CONFIRMED -- cross-engine poisoning succeeds silently.");
        }
    }

    // --- Case 3: empty-string ID ---
    println!("\n-- Case 3: empty-string ID --");
    {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let empty = Distinction::new(String::new());
        let result = panic::catch_unwind(AssertUnwindSafe(|| engine.synthesize(&d0, &empty)));
        match result {
            Ok(child) => {
                println!("  synthesize(d0, empty) succeeded: child.id={}", child.id());
                let rels = engine.get_relationships_snapshot();
                let mentions_empty = rels.iter().any(|(a, b)| a.is_empty() || b.is_empty());
                println!("  relationships contain empty string ID: {}", mentions_empty);
                // Canonical ordering: "" < "0" < "1". So empty will be FIRST id.
                // That produces a parent id that sorts before d0. Pathological.
                println!("  NOTE: empty < \"0\" in lex order, so empty becomes canonical 'first'");
                println!("        position in synth hash. This creates arbitrary tamper surface.");
            }
            Err(_) => println!("  PANIC on empty id"),
        }
    }

    // --- Case 4: extremely long ID ---
    println!("\n-- Case 4: 1MB ID --");
    {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let huge = Distinction::new("z".repeat(1_000_000));
        let t = std::time::Instant::now();
        let result = panic::catch_unwind(AssertUnwindSafe(|| engine.synthesize(&d0, &huge)));
        let elapsed = t.elapsed();
        match result {
            Ok(child) => {
                println!("  synthesize(d0, 1MB id) succeeded in {:?}: child.id={}", elapsed, &child.id()[..16]);
                println!("  SEVERITY: a malicious caller can force arbitrarily large hash inputs,");
                println!("  slowing synthesize by proportional factor. DoS vector.");
            }
            Err(_) => println!("  PANIC on huge id"),
        }
    }

    // --- Case 5: ID containing the canonical separator ':' ---
    println!("\n-- Case 5: ID containing ':' (the canonical separator) --");
    {
        let engine = DistinctionEngine::new();
        // Observation: synthesize format!("{}:{}", first, second). If one "foreign" ID
        // contains ':', we can fabricate a distinction whose id collides with a real
        // pre-image. Example:
        //   Real synth: SHA256("0:1") = child_01
        //   Forged foreign: Distinction::new("fake_id"). synth(d0, fake):
        //     new_id = SHA256("0:fake_id")
        //   Try harder: Distinction::new("1") already exists (d1). What about
        //   a foreign id equal to "1:something"?
        //
        // Engine sort is lexicographic, so "0" < "1:x" because "0" < "1". Then
        //   hash input = "0:1:x".
        // And an attacker could also construct a REAL chain producing input
        // "0:SHA256(...)" — if that ever equaled "0:1:x" we'd have a collision.
        // In practice SHA256 hex output is 64 hex chars, so no real id would
        // ever literally be "1:x". But the attack surface exists if IDs were
        // allowed shorter forms (empty, "0", "1", "1:x", etc.).
        let forged = Distinction::new("1:evil".to_string());
        let d0 = engine.d0().clone();
        let _ = engine.synthesize(&d0, &forged);
        println!("  synthesize(d0, Distinction::new(\"1:evil\")) succeeded");
        println!("  Hash input was literally \"0:1:evil\" -- a colon-separated forgery.");
        println!("  SEVERITY: low in practice (no collision with real 64-hex IDs), but");
        println!("  a newtype wrapper with invariant-preserving constructor would close this.");
    }

    // --- Case 6: Does foreign usage break r = 2d - 3? ---
    println!("\n-- Case 6: r = 2d - 3 invariant under foreign poisoning --");
    {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        // 10 foreign synths
        for i in 0..10 {
            let f = Distinction::new(format!("FOREIGN_{}_{}", i, "x".repeat(60)));
            let _ = engine.synthesize(&d0, &f);
        }
        let d = engine.distinction_count();
        let r = engine.relationship_count();
        let expected_r = 2i64 * d as i64 - 3;
        println!("  distinctions={} relationships={} expected=2d-3={} match={}",
            d, r, expected_r, r as i64 == expected_r);
        if r as i64 != expected_r {
            println!("  VERDICT: INVARIANT BROKEN -- foreign synths perturb r=2d-3");
        } else {
            println!("  VERDICT: invariant holds numerically (because each synth still adds");
            println!("  exactly 1 distinction + 2 relationships). BUT the relationships point");
            println!("  to phantom parents. Numerical invariant is misleadingly preserved.");
        }
    }

    println!("\n=== Done ===");
    println!();
    println!("SUMMARY OF SAFETY POSTURE:");
    println!("  * Distinction::new is fully public.");
    println!("  * synthesize does NOT validate that input ids exist in all_distinctions.");
    println!("  * synthesize does NOT validate input shape (length, charset).");
    println!("  * Foreign IDs are accepted silently as parents.");
    println!("  * The phantom-parent bug from ByteMapping is a SPECIFIC INSTANCE of this");
    println!("    more general safety hole.");
    println!("  * Recommendation for item #6 (pub(crate) field): necessary, but not");
    println!("    sufficient. Also need: a constructor-only-within-this-crate policy,");
    println!("    plus parent-existence check in synthesize (debug-assert minimum).");
}
