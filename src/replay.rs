//! Replay — persistence and reconstruction helpers (consumer-side, not substrate).
//!
//! The substrate is timeless: the engine has no log, no observation
//! channel, no order-bearing state. Persistence and reconstruction are
//! CONSUMER patterns built on top of the engine's canonical
//! [`parents_of`](crate::DistinctionEngine::parents_of) projection.
//!
//! This module provides three helpers:
//!
//! - [`replay_topological`] — rebuild a fresh engine from a parentage
//!   snapshot, in any input order, via repeated `synthesize` calls.
//! - [`build_children_index`] — materialize the inverse-of-`parents_of`
//!   map for consumers that need children iteration.
//! - [`ReplayError`] — diagnostic error type for replay failures.
//!
//! These functions are pure (no engine state in this module). Consumers
//! who don't need persistence pay nothing.
//!
//! # Why replay is theory-load-bearing
//!
//! The combination of [`DistinctionEngine::snapshot_parentage`] +
//! [`replay_topological`] verifies two structural laws:
//!
//! - **Law 8 (engine independence):** two engines processing the same
//!   operations produce byte-identical state.
//! - **Law 9 (order-independent reconstruction):** replay in any input
//!   order converges to byte-identical state.
//!
//! Both laws are exercised by the round-trip test in this module:
//! snapshot → replay → byte-identical engine.

use crate::{Distinction, DistinctionEngine, ParentPair};
use std::collections::HashMap;
use std::sync::Arc;

/// Reasons [`replay_topological`] can fail.
///
/// `#[non_exhaustive]`: future variants will not break consumer semver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReplayError {
    /// The input parentage cannot bootstrap from d₀/d₁ — no entry has
    /// both parents already registered in a fresh engine (which starts
    /// with only the primordials). Distinct from
    /// [`Unreachable`](ReplayError::Unreachable) because it indicates
    /// the input never had a root edge into the primordials, whereas
    /// `Unreachable` means some progress was made but a downstream
    /// cycle or missing intermediate prevented completion.
    #[error("no parentage entry roots in primordials: input cannot bootstrap from d₀/d₁")]
    MissingPrimordial,

    /// Progress stalled after partial reconstruction: some entries
    /// remain whose parents are never satisfied (cycle in the input,
    /// or a missing intermediate parentage entry).
    ///
    /// Carries the count of unresolved entries for diagnostics.
    #[error("corrupted parentage: {0} entries unresolved (cycle or missing intermediate)")]
    Unreachable(usize),

    /// Content-address mismatch: an input entry claims `child` has
    /// parents `(a, b)`, but `synthesize(a, b)` on a fresh engine
    /// produced different bytes. Indicates the input parentage has
    /// been tampered with or corrupted in transit.
    ///
    /// Release-safe — does NOT depend on `debug_assert`.
    #[error("content-address mismatch: expected {expected:?}, got {actual:?}")]
    Mismatch {
        /// The child distinction the input claims this synthesis produced.
        child: Distinction,
        /// Same as `child` — what the input claimed.
        expected: Distinction,
        /// What `synthesize(a, b)` actually produced.
        actual: Distinction,
    },
}

/// Rebuild a fresh engine from a parentage snapshot.
///
/// Iterates `parentage` repeatedly, applying any entry whose parents
/// are already registered in the rebuild engine. Continues until either
/// all entries are applied (success), no progress was made on an
/// iteration (failure: cycle, missing parent, or input never roots in
/// primordials), or a content-address mismatch is detected (failure:
/// tampered parentage).
///
/// # Order independence (Law 9)
///
/// The result is byte-identical regardless of input order — the
/// topological-sort loop reaches the same set of synthesize calls in
/// the same canonical order. Verified by the shuffled-replay test in
/// this module.
///
/// # Engine independence (Law 8)
///
/// The result is byte-identical regardless of which machine runs the
/// replay — content addressing guarantees `synthesize(a, b)` produces
/// the same child bytes everywhere. Combined with order independence,
/// any subset/permutation of a parentage snapshot reconstructs the
/// same engine.
///
/// # Complexity
///
/// O(N) typical for well-shaped DAGs (each entry visited a constant
/// number of times). O(N²) worst case on pathologically linear chains
/// where each iteration applies exactly one entry — a probe in Step 4
/// quantifies the worst-case overhead.
///
/// # Errors
///
/// - [`ReplayError::MissingPrimordial`] if no input entry can apply on
///   the first iteration (input never roots in d₀/d₁).
/// - [`ReplayError::Unreachable`] if progress stalls partway (cycle or
///   missing intermediate).
/// - [`ReplayError::Mismatch`] if any input claim contradicts the
///   content-addressed result (tampered parentage).
///
/// All three error checks happen in release builds — not gated on
/// `debug_assert!`.
#[must_use = "replay_topological returns a Result; handle the corrupted-parentage case"]
pub fn replay_topological(
    parentage: impl IntoIterator<Item = (Distinction, ParentPair)>,
) -> Result<Arc<DistinctionEngine>, ReplayError> {
    let engine = Arc::new(DistinctionEngine::new());
    let mut pending: Vec<(Distinction, ParentPair)> = parentage.into_iter().collect();
    let initial_len = pending.len();

    while !pending.is_empty() {
        let before = pending.len();
        let mut err: Option<ReplayError> = None;

        pending.retain(|(child, (a, b))| {
            if engine.has(*a) && engine.has(*b) {
                let result = engine.synthesize(*a, *b);
                if result != *child {
                    err = Some(ReplayError::Mismatch {
                        child: *child,
                        expected: *child,
                        actual: result,
                    });
                }
                // Drop this entry whether it succeeded or mismatched —
                // err is checked after retain completes.
                false
            } else {
                true
            }
        });

        if let Some(e) = err {
            return Err(e);
        }

        if pending.len() == before {
            // No progress made on this iteration.
            // - If we made no progress on the FIRST iteration (no entries
            //   removed since start), the input has no entry that can
            //   bootstrap from d₀/d₁ alone.
            // - Otherwise, partial progress + stall means a downstream
            //   cycle or a missing intermediate parentage entry.
            return Err(if pending.len() == initial_len {
                ReplayError::MissingPrimordial
            } else {
                ReplayError::Unreachable(pending.len())
            });
        }
    }

    Ok(engine)
}

/// Materialize the inverse of `parents_of`: for each distinction, the
/// set of distinctions that have it as a parent.
///
/// # Snapshot-in-time semantics
///
/// The returned index reflects only the parentage entries passed in.
/// Concurrent syntheses against the live engine after the snapshot was
/// taken do NOT appear in this index. Build it from a fresh
/// `engine.snapshot_parentage()` call at a quiescent moment, or accept
/// that the view is consistent with the snapshot, not with the engine's
/// current state.
///
/// # Why this isn't in the engine
///
/// The engine itself doesn't carry this index — `degree_counts` is the
/// canonical O(1) projection of the Coding Law primitive (degree =
/// total participations). This helper materializes the dual enumeration
/// on demand for consumers that need to iterate children (e.g.,
/// diagnostic dumps, custom graph algorithms). Build it once at a
/// quiescent moment; query in O(1) thereafter.
///
/// O(N) build, where N is `parentage.len()`. Each parent gets entries
/// proportional to its child count.
#[must_use]
pub fn build_children_index(
    parentage: &[(Distinction, ParentPair)],
) -> HashMap<Distinction, Vec<Distinction>> {
    let mut index: HashMap<Distinction, Vec<Distinction>> = HashMap::new();
    for (child, (a, b)) in parentage {
        index.entry(*a).or_default().push(*child);
        index.entry(*b).or_default().push(*child);
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::primitives::ByteMapping;

    /// Convenience alias for test fixtures.
    type SourceFixture = (DistinctionEngine, Vec<(Distinction, ParentPair)>);

    /// Build a small engine with a non-trivial parentage graph for replay
    /// tests. Returns the engine and its parentage snapshot.
    fn build_source() -> SourceFixture {
        let e = DistinctionEngine::new();
        let _ = e.synthesize(e.d0(), e.d1()); // pool[2]
                                              // Fold a few bytes — exercises both shallow and deeper graph paths.
        let _ = ByteMapping::map_byte_to_distinction(0x42, &e);
        let _ = ByteMapping::map_byte_to_distinction(0xAB, &e);
        let snap = e.snapshot_parentage();
        (e, snap)
    }

    // ----- Round-trip (Laws 8 + 9) --------------------------------------

    #[test]
    fn replay_natural_order_byte_identical() {
        let (source, snap) = build_source();
        let replayed = replay_topological(snap).expect("natural-order replay succeeds (invariant)");

        // Same counts.
        assert_eq!(replayed.distinction_count(), source.distinction_count());
        assert_eq!(replayed.relationship_count(), source.relationship_count());

        // Every parentage entry in source exists in replayed with the
        // same canonical parents.
        for (child, expected_parents) in source.snapshot_parentage() {
            let replayed_parents =
                replayed.parents_of(child).expect("child registered in replay (invariant)");
            assert_eq!(replayed_parents, expected_parents);
        }
    }

    #[test]
    fn replay_shuffled_order_byte_identical() {
        // Law 9 falsifier: shuffled input → byte-identical state.
        // Deterministic shuffle via a fixed seed (no rand dep needed
        // for this test).
        let (source, mut snap) = build_source();
        snap.reverse(); // simplest "non-natural" order
        let replayed =
            replay_topological(snap).expect("shuffled-order replay succeeds (invariant)");

        assert_eq!(replayed.distinction_count(), source.distinction_count());
        assert_eq!(replayed.relationship_count(), source.relationship_count());
        replayed.check_structural_invariant().expect("invariant holds after replay (invariant)");

        for (child, expected_parents) in source.snapshot_parentage() {
            let replayed_parents =
                replayed.parents_of(child).expect("child registered in replay (invariant)");
            assert_eq!(replayed_parents, expected_parents);
        }
    }

    #[test]
    fn replay_arbitrary_permutation_byte_identical() {
        // Stronger Law 9 falsifier: a deterministic permutation
        // (not just reverse). Builds confidence that replay is truly
        // order-independent, not just "works for two specific orders."
        let (source, mut snap) = build_source();
        // Rotate by 1/3 then reverse first half — a non-trivial perm
        // that depends on no randomness library.
        let third = snap.len() / 3;
        snap.rotate_left(third);
        snap[..third].reverse();

        let replayed =
            replay_topological(snap).expect("permuted-order replay succeeds (invariant)");

        assert_eq!(replayed.distinction_count(), source.distinction_count());
        for (child, expected_parents) in source.snapshot_parentage() {
            let replayed_parents =
                replayed.parents_of(child).expect("child registered in replay (invariant)");
            assert_eq!(replayed_parents, expected_parents);
        }
    }

    #[test]
    fn replay_empty_input_yields_empty_engine() {
        // Edge case: empty input → engine with only primordials.
        let replayed = replay_topological(Vec::<(Distinction, ParentPair)>::new())
            .expect("empty replay succeeds (invariant)");
        assert_eq!(replayed.distinction_count(), 2);
        assert_eq!(replayed.relationship_count(), 1);
    }

    // ----- Log-replay invariant (theory-guardian round-2 promotion) ----

    #[test]
    fn log_replay_invariant_byte_identical_state() {
        // Theory gate 10 in DESIGN.md Part 10: rebuilding the engine
        // from parents_of must produce byte-identical state. Catches
        // the case where a future contributor adds a hidden state
        // field that isn't a pure function of the synthesis log.
        let (source, snap) = build_source();
        let replayed = replay_topological(snap).expect("log-replay succeeds (invariant)");

        // Same distinction set (compare via sorted bytes).
        let mut source_ids: Vec<[u8; 16]> =
            source.snapshot_distinctions().iter().map(|d| *d.as_bytes()).collect();
        let mut replayed_ids: Vec<[u8; 16]> =
            replayed.snapshot_distinctions().iter().map(|d| *d.as_bytes()).collect();
        source_ids.sort();
        replayed_ids.sort();
        assert_eq!(source_ids, replayed_ids);

        // Same parentage set.
        let mut source_par = source.snapshot_parentage();
        let mut replayed_par = replayed.snapshot_parentage();
        source_par.sort_by_key(|(c, _)| *c.as_bytes());
        replayed_par.sort_by_key(|(c, _)| *c.as_bytes());
        assert_eq!(source_par, replayed_par);

        // Same degree for every distinction.
        for d in source.snapshot_distinctions() {
            assert_eq!(source.degree(d), replayed.degree(d), "degree mismatch for {d:?}");
        }
    }

    // ----- Error variants ----------------------------------------------

    #[test]
    fn replay_returns_mismatch_on_tampered_parentage() {
        // Find an entry with both parents primordial so Mismatch fires
        // deterministically on iteration 1 (independent of DashMap's
        // unstable iteration order). build_source guarantees at least
        // one such entry: the initial synth(d0, d1) seed.
        let (e, snap) = build_source();
        let target_parents = snap
            .iter()
            .find_map(|(_, (a, b))| {
                if (*a == e.d0() || *a == e.d1()) && (*b == e.d0() || *b == e.d1()) {
                    Some((*a, *b))
                } else {
                    None
                }
            })
            .expect("source must contain a primordial-rooted entry (invariant)");

        // Replace that entry with one that claims d0 is the child.
        // synth(target_parents) ≠ d0, so Mismatch fires.
        let mut tampered: Vec<(Distinction, ParentPair)> =
            snap.into_iter().filter(|(_, parents)| *parents != target_parents).collect();
        tampered.push((e.d0(), target_parents));

        let err =
            replay_topological(tampered).expect_err("tampered parentage must reject (invariant)");
        assert!(matches!(err, ReplayError::Mismatch { .. }));
    }

    #[test]
    fn replay_returns_unreachable_on_partial_cycle() {
        // Build a manual depth-3 chain so we can orphan an intermediate:
        //   c1 = synth(d0, d1)        — depth 2, primordial-rooted
        //   c2 = synth(c1, d0)        — depth 3, depends on c1
        //   c3 = synth(c2, e.d1())    — depth 4, depends on c2
        // Drop c2 from the parentage. c1 still applies (its parents are
        // d0 and d1, both registered in the fresh replay engine). But
        // c3 can never apply because c2 is missing — orphan, no progress
        // after first iteration ⇒ Unreachable.
        let e = DistinctionEngine::new();
        let c1 = e.synthesize(e.d0(), e.d1());
        let c2 = e.synthesize(c1, e.d0());
        let c3 = e.synthesize(c2, e.d1());
        let snap = e.snapshot_parentage();
        assert_eq!(snap.len(), 3);
        assert!(snap.iter().any(|(child, _)| *child == c2));

        // Prune the c2 entry.
        let pruned: Vec<(Distinction, ParentPair)> =
            snap.into_iter().filter(|(child, _)| *child != c2).collect();
        assert_eq!(pruned.len(), 2);

        let err = replay_topological(pruned)
            .expect_err("orphan-parent parentage must reject (invariant)");
        match err {
            ReplayError::Unreachable(count) => {
                // c3 is unreachable because c2 is missing. c1 succeeded
                // on iteration 1 (its parents are primordials).
                assert_eq!(count, 1, "exactly one entry (c3) is unreachable");
            },
            other => panic!("expected Unreachable, got {other:?}"),
        }
        let _ = c3; // suppress unused warning
    }

    #[test]
    fn replay_returns_missing_primordial_when_no_root_in_d0_d1() {
        // Input parentage references distinctions never registered in
        // the fresh engine (which starts with only d0/d1). NO entry
        // can bootstrap. MissingPrimordial fires.
        //
        // Construct by hand: fabricate two non-primordial distinctions
        // and claim one synthesizes from them.
        let fake_a = Distinction::from_bytes_unchecked([0xAA; 16]);
        let fake_b = Distinction::from_bytes_unchecked([0xBB; 16]);
        let fake_child = Distinction::from_bytes_unchecked([0xCC; 16]);
        let parentage = vec![(fake_child, (fake_a, fake_b))];

        let err =
            replay_topological(parentage).expect_err("rootless parentage must reject (invariant)");
        assert_eq!(err, ReplayError::MissingPrimordial);
    }

    // ----- build_children_index ----------------------------------------

    #[test]
    fn build_children_index_inverts_parents_of() {
        let (e, snap) = build_source();
        let index = build_children_index(&snap);

        // Every parentage entry (child, (a, b)) implies a is in
        // index[child]'s reverse — i.e., child appears in index[a] and
        // index[b].
        for (child, (a, b)) in &snap {
            assert!(
                index.get(a).is_some_and(|children| children.contains(child)),
                "child {child:?} missing from index[{a:?}]"
            );
            assert!(
                index.get(b).is_some_and(|children| children.contains(child)),
                "child {child:?} missing from index[{b:?}]"
            );
        }

        // d0 has lots of children (it's a Fold Law mega-hub after byte folds).
        let d0_children = index.get(&e.d0()).expect("d0 has children (invariant)");
        assert!(!d0_children.is_empty());
    }

    #[test]
    fn build_children_index_empty_input_empty_output() {
        let index = build_children_index(&[]);
        assert!(index.is_empty());
    }
}
