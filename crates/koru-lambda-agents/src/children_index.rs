//! The children index — the inverse of `parents_of`, materialized.
//!
//! Consumer pattern, **not substrate**. It determines no identifier: delete it
//! and two conforming implementations still agree on every byte. The engine
//! carries `degree` as the canonical O(1) read; this builds the dual
//! (children-iteration) on demand for consumers that need to walk downstream
//! repeatedly.
//!
//! Build once at a quiescent moment; query O(1) thereafter.

use koru_lambda_core::engine::ParentPair;
use koru_lambda_core::Distinction;
use std::collections::HashMap;

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
/// The engine doesn't carry this index — `degree` is the canonical
/// O(1) Coding Law projection. This helper materializes the dual
/// (children-iteration) on demand. Build once at a quiescent moment;
/// query O(1) thereafter.
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
    use koru_lambda_core::{ByteMapping, DistinctionEngine};

    /// Engine plus its parentage snapshot — the pair every test here needs.
    type SourceFixture = (DistinctionEngine, Vec<(Distinction, ParentPair)>);

    /// A small engine with a non-trivial parentage graph. Folding two bytes
    /// gives the primordials many children, which is what makes the inverse
    /// index worth building in the first place.
    fn build_source() -> SourceFixture {
        let e = DistinctionEngine::new();
        let _ = e.synthesize(e.d0(), e.d1());
        let _ = ByteMapping::map_byte_to_distinction(0x42, &e);
        let _ = ByteMapping::map_byte_to_distinction(0xAB, &e);
        let snap = e.snapshot_parentage();
        (e, snap)
    }

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
