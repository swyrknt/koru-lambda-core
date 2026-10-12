//! Replay — persistence and reconstruction helpers (consumer-side, not substrate).
//!
//! The substrate is timeless: the engine has no log, no observation
//! channel, no order-bearing state. Persistence and reconstruction are
//! CONSUMER patterns built on top of the engine's canonical
//! [`parents_of`](crate::DistinctionEngine::parents_of) projection.
//!
//! This module provides two:
//!
//! - [`replay_topological`] — rebuild a fresh engine from a parentage
//!   snapshot, in any input order, via repeated `synthesize` calls.
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
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Reasons [`replay_topological`] can fail.
///
/// `#[non_exhaustive]`: future variants will not break consumer semver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ReplayError {
    /// No input entry can bootstrap from d₀/d₁ — the parentage never
    /// roots in the primordials. (Contrast
    /// [`Unreachable`](ReplayError::Unreachable): some progress was
    /// made, but a cycle or missing intermediate stalled completion.)
    #[error("no parentage entry roots in primordials: input cannot bootstrap from d₀/d₁")]
    MissingPrimordial,

    /// Progress stalled after partial reconstruction: some entries
    /// remain whose parents are never satisfied (cycle in the input,
    /// or a missing intermediate parentage entry).
    ///
    /// Carries the count of unresolved entries for diagnostics.
    #[error("corrupted parentage: {0} entries unresolved (cycle or missing intermediate)")]
    Unreachable(usize),

    /// Input claims `child = synthesize(a, b)` but replay produced
    /// different bytes. Parentage was tampered or corrupted in transit.
    /// Checked in release (not debug-only).
    #[error("content-address mismatch: {child:?} re-derived as {actual:?}")]
    Mismatch {
        /// The claimed child — what the snapshot said this pair produces.
        child: Distinction,
        /// What `synthesize(a, b)` actually produced.
        actual: Distinction,
    },
}

/// Rebuild a fresh engine from a parentage snapshot.
///
/// Kahn's algorithm over the parentage DAG: an entry becomes ready when
/// the engine holds both its parents, and resolving it wakes exactly the
/// entries that were waiting on the child it produces. Each entry is
/// synthesized once and each waiting edge traversed once.
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
/// O(N) in the number of entries, for every input shape and order.
///
/// This was a repeated-sweep loop — re-scan everything pending, apply
/// whatever had become ready — which is O(N²) when each sweep applies one
/// entry, and its documentation called that shape "pathologically linear".
/// It is the opposite of pathological here: an agent advancing its local
/// root produces a chain where every entry depends on the one before it,
/// so the worst case was the common case, and reverse-ordered input hit it
/// exactly.
///
/// Measured on a reverse-ordered chain, debug profile, before and after:
///
/// ```text
///   entries    sweep     Kahn
///     2,001    261 ms    17 ms
///     8,001   4.03 s     43 ms
///    20,001   25.9 s    108 ms
/// ```
///
/// The ratios are the evidence, not the absolute times: four times the
/// entries cost the sweep about sixteen times the work and cost this about
/// four. `replay_is_linear_in_the_number_of_entries` asserts that
/// separation rather than any wall-clock figure, so it does not depend on
/// the machine it runs on.
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
    let entries: Vec<(Distinction, ParentPair)> = parentage.into_iter().collect();
    if entries.is_empty() {
        return Ok(engine);
    }

    // `blocked_on[i]` is how many *distinct* parents of entry `i` the engine
    // does not hold yet; `waiting[p]` lists the entries that unblock when `p`
    // arrives. The pair is deduplicated because `synthesize(a, a)` is legal
    // and counting one missing parent twice would leave an entry blocked
    // forever after its single wake-up.
    let mut blocked_on: Vec<usize> = Vec::with_capacity(entries.len());
    let mut waiting: HashMap<Distinction, Vec<usize>> = HashMap::new();
    let mut ready: VecDeque<usize> = VecDeque::new();

    for (i, (_, (a, b))) in entries.iter().enumerate() {
        let (pair, len) = if a == b { ([*a, *a], 1) } else { ([*a, *b], 2) };
        let mut missing = 0;
        for parent in &pair[..len] {
            if !engine.has(*parent) {
                missing += 1;
                waiting.entry(*parent).or_default().push(i);
            }
        }
        blocked_on.push(missing);
        if missing == 0 {
            ready.push_back(i);
        }
    }

    if ready.is_empty() {
        // Nothing can bootstrap: no entry has both parents in d₀/d₁.
        return Err(ReplayError::MissingPrimordial);
    }

    let mut resolved = 0usize;
    while let Some(i) = ready.pop_front() {
        let (child, (a, b)) = entries[i];
        let actual = engine.synthesize(a, b);
        if actual != child {
            return Err(ReplayError::Mismatch { child, actual });
        }
        resolved += 1;

        // `child` is registered now, so anything waiting on it advances.
        // Duplicated entries claiming the same child find the list already
        // taken, which is correct: the wake happened once.
        if let Some(woken) = waiting.remove(&child) {
            for w in woken {
                blocked_on[w] -= 1;
                if blocked_on[w] == 0 {
                    ready.push_back(w);
                }
            }
        }
    }

    if resolved < entries.len() {
        // Some entries never became ready: a cycle, or a parent that no
        // entry produces and the engine never had.
        return Err(ReplayError::Unreachable(entries.len() - resolved));
    }

    Ok(engine)
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

    // ----- Log-replay invariant (promoted from review) ----------------

    #[test]
    fn log_replay_invariant_byte_identical_state() {
        // Law 8 and Law 9, per `DESIGN.md`: rebuilding the engine
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

    /// Replay is linear in the number of entries, on the input shape that
    /// used to be quadratic.
    ///
    /// The ratio is asserted, not a wall-clock budget, so the test does not
    /// depend on how fast the machine is: quadrupling the entries costs a
    /// linear algorithm about four times the work and a quadratic one about
    /// sixteen. The bound sits at nine, which neither class can reach from
    /// the wrong side under any plausible noise.
    ///
    /// The shape matters as much as the size. A chain presented in exactly
    /// reverse dependency order is the worst case for a repeated sweep —
    /// each pass applies one entry — and it is also what an agent that
    /// advances its local root produces, snapshotted and replayed backwards.
    /// The algorithm this replaced called that shape pathological.
    #[test]
    fn replay_is_linear_in_the_number_of_entries() {
        /// A chain of `n` syntheses, in reverse dependency order.
        fn reverse_chain(n: usize) -> Vec<(Distinction, ParentPair)> {
            let e = DistinctionEngine::new();
            let mut out = Vec::with_capacity(n);
            let mut prev = e.synthesize(e.d0(), e.d1());
            out.push((prev, (e.d0(), e.d1())));
            for _ in 1..n {
                let next = e.synthesize(prev, e.d1());
                out.push((next, (prev, e.d1())));
                prev = next;
            }
            out.reverse();
            out
        }

        fn time(entries: Vec<(Distinction, ParentPair)>) -> std::time::Duration {
            let start = std::time::Instant::now();
            let engine = replay_topological(entries).expect("a well-formed chain replays");
            let elapsed = start.elapsed();
            // Keep the result observable so the work cannot be optimised out.
            assert!(engine.distinction_count() > 0);
            elapsed
        }

        const N: usize = 10_000;
        let small = time(reverse_chain(N));
        let large = time(reverse_chain(N * 4));

        let ratio = large.as_secs_f64() / small.as_secs_f64();
        assert!(
            ratio < 9.0,
            "quadrupling the entries multiplied the work by {ratio:.1}x ({small:?} -> \
             {large:?}). Linear is about 4x and quadratic about 16x, so this is a \
             complexity regression in replay, not slow hardware."
        );
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
}
