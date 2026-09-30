//! Primitives — [`Canonicalizable`] trait and [`ByteMapping`].
//!
//! How arbitrary types lift into the substrate: every consumer-side
//! value that wants to participate in a synthesis must produce a
//! canonical chain of distinctions through `engine.synthesize` calls.
//! [`Canonicalizable`] is the trait that captures this contract.
//!
//! The substrate ships one impl: [`u8`] via [`ByteMapping`]. Higher-level
//! consumer types (transactions, peer IDs, custom action data) implement
//! the trait themselves by composing byte-level folds.
//!
//! # Phantom-node fix (v2.0)
//!
//! The v1.2 [`ByteMapping`] cached byte→distinction IDs against a
//! throwaway engine and handed those IDs to the calling engine. The
//! returned IDs were "valid" hex strings but the corresponding
//! distinctions were never registered in the consumer's engine — hence
//! "phantom nodes" appearing in `parents_of` entries without
//! corresponding `all_distinctions` entries. Violated `r = 2d − 3`
//! semantically and broke engine-state-independence for byte folds.
//!
//! v2.0's [`ByteMapping::map_byte_to_distinction`] takes the calling
//! engine and folds each byte through it via `engine.synthesize` calls.
//! Every intermediate distinction in the fold is registered. No phantoms
//! possible by construction.

use crate::{Distinction, DistinctionEngine};

/// Lift a value into its canonical chain of distinctions in `engine`.
///
/// Every value type that participates in synthesis (action data,
/// transaction payloads, peer identities, etc.) implements this trait
/// to declare how it folds into the substrate. The returned
/// [`Distinction`] is the "tip" of the canonical chain — the
/// distinction that represents this value's complete causal structure.
///
/// # Determinism
///
/// Implementations MUST be deterministic: the same value folded through
/// the same engine state MUST produce the same tip distinction. This
/// follows from the four axioms — non-deterministic canonicalization
/// would break determinism, content addressing, and engine-state
/// independence simultaneously.
///
/// # Engine registration
///
/// Implementations MUST use `engine.synthesize` for every step of the
/// fold; constructing a [`Distinction`] from raw bytes (via
/// `Distinction::from_hex` and bypassing synthesize) is forbidden
/// because the resulting distinction would not be registered in
/// `engine.all_distinctions` and would trip the foreign-byte guard on
/// any subsequent `engine.synthesize` call that uses it as a parent.
///
/// The v2.0 [`ByteMapping`] implementation demonstrates the contract.
pub trait Canonicalizable {
    /// Fold `self` into a chain of synthesize calls on `engine`, returning
    /// the tip distinction.
    #[must_use]
    fn to_canonical_structure(self, engine: &DistinctionEngine) -> Distinction;
}

/// Byte → distinction folding (v2.0: engine-registered, no phantoms).
///
/// [`ByteMapping::map_byte_to_distinction`] folds an 8-bit byte through
/// 8 bit-steps in `engine`. At each bit-step, both primordials d₀ and
/// d₁ participate in a synthesis; the bit value determines the order,
/// which preserves bit information in the canonical chain via the
/// distinct intermediate distinctions.
///
/// # Fold Law contribution
///
/// Each bit-step performs two `synthesize` calls — one with `d₀` as
/// the "added" parent, one with `d₁`. Per byte fold *in isolation* that
/// would be 8 + 8 = 16 participations per primordial; but under
/// content-addressing saturation across the 256-byte exercise, only
/// the unique-bit-prefix accumulators contribute novel bumps.
///
/// **Saturation arithmetic:** at bit-step `i+1`, only `2^(i+1)` unique
/// accumulator values exist across all 256 bytes (one per bit-prefix
/// length `i+1`). Each unique acc produces one novel `synth(acc, d₀)`
/// and one novel `synth(intermediate, d₀)`-or-`d₁` (the second depends
/// on bit value but contributes exactly one novel bump for each unique
/// (acc, bit_value) combination). The cumulative novel bumps per
/// primordial:
///
/// ```text
///   2 + 4 + 8 + 16 + 32 + 64 + 128 + 256 = 510
/// ```
///
/// Plus 1 from the initial `synthesize(d₀, d₁)` setup (novel at byte 0,
/// saturated after — one bump each across all 256 bytes).
///
/// So `degree_counts[d₀] = degree_counts[d₁] = 511` after the exercise.
/// `degree(d₀) = degree(d₁) = 512` (adding the genesis-edge addend).
///
/// This is **the topological maximum** for this 8-bit fold shape — you
/// cannot exceed it without (a) more bit-steps, (b) a wider primordial
/// set, or (c) breaking the per-bit-prefix collapse by mixing byte
/// identity into the acc earlier. 512 vs typical-leaf degree 2 = 256×
/// dominance ratio, which is the structural origin of d₀/d₁ as
/// topological mega-hubs (Law 11, Fold Law).
pub struct ByteMapping;

/// An ordered sequence of bytes, folded into one distinction.
///
/// # Why this exists as a primitive
///
/// The operator is commutative and irreflexive, so a sequence cannot be
/// folded naively: `synthesize` cannot tell `(x, y)` from `(y, x)`, and
/// `synthesize(x, x)` returns `x`. An ordered encoding therefore has to
/// carry order in the *nesting* — each step folds the running
/// accumulator with the next element, so position is recorded by depth.
///
/// Two consumers wrote that fold by hand and both wrote it the same way
/// and wrong: they seeded the accumulator with the **first element's**
/// tip. That makes the first two positions an unordered pair, and a
/// leading run of equal bytes vanishes through irreflexivity. Nonce 1
/// and nonce 256 produced identical commitments. The encoding is one
/// primitive here so that it is specified once, tested once, and cannot
/// be reinvented differently.
///
/// # The rules that make it injective
///
/// - **The accumulator starts at an anchor, never at an element.** The
///   anchor is a parameter: a consumer with a perspective passes its own
///   root, so two consumers folding the same bytes from different
///   anchors get different results. Domain separation is an anchor
///   choice, which is why it does not need to enter the hash.
/// - **The anchor may not be an element tip.** If it were, the first
///   step could collapse through irreflexivity and silently drop an
///   element. This is checked, not assumed: the alphabet is 256 known
///   values, so membership is a lookup.
/// - **Length is folded in before the elements.** Without it, a fold is
///   only injective among sequences of equal length.
///
/// Beyond the first step, an accumulator is a hash-chain node while
/// elements come from a fixed 256-value alphabet, so a later collapse
/// would require landing on one of 256 specific 128-bit values — the
/// collision bound in the specification, not a structural hole.
pub struct ByteFold {
    /// The 256 element tips, one per byte value, for this engine.
    tips: [Distinction; 256],
    /// Tip bytes, for the anchor check.
    tip_bytes: std::collections::HashSet<[u8; 16]>,
}

impl ByteFold {
    /// Build the fold for `engine`, computing all 256 element tips once.
    ///
    /// Tips are engine-independent as *values* — identical histories give
    /// identical identifiers — but every tip must be **registered in the
    /// engine it will be folded against**, or it would reach `synthesize`
    /// as an unregistered parent. That is why this is per engine and not
    /// a global table.
    ///
    /// Cost is paid once: 256 byte folds, most of which saturate
    /// immediately after the first. Afterwards a tip is an array index
    /// rather than 17 synthesis calls.
    #[must_use]
    pub fn new(engine: &DistinctionEngine) -> Self {
        let tips: [Distinction; 256] =
            std::array::from_fn(|i| ByteMapping::map_byte_to_distinction(i as u8, engine));
        let tip_bytes = tips.iter().map(|d| *d.as_bytes()).collect();
        Self { tips, tip_bytes }
    }

    /// The tip distinction for one byte value.
    #[must_use]
    pub const fn tip(&self, byte: u8) -> Distinction {
        self.tips[byte as usize]
    }

    /// Whether `d` is one of the 256 element tips.
    ///
    /// Public because it is the precondition on [`Self::fold`]'s anchor,
    /// and a caller choosing an anchor should be able to check it.
    #[must_use]
    pub fn is_tip(&self, d: Distinction) -> bool {
        self.tip_bytes.contains(d.as_bytes())
    }

    /// Fold `bytes` into a single distinction, anchored at `anchor`.
    ///
    /// Distinct sequences give distinct results: order is carried by
    /// nesting depth, length is folded in first, and the accumulator
    /// never starts at an element.
    ///
    /// # Panics
    ///
    /// Panics if `anchor` is an element tip, which would let the first
    /// step collapse and drop an element, or if `anchor` is not
    /// registered in `engine`.
    #[must_use]
    pub fn fold(
        &self,
        anchor: Distinction,
        bytes: &[u8],
        engine: &DistinctionEngine,
    ) -> Distinction {
        assert!(
            !self.is_tip(anchor),
            "fold: the anchor is an element tip, so the first step could collapse through \
             irreflexivity and drop an element. Anchor on a synthesized root instead."
        );

        // Length first, so sequences of different lengths cannot coincide.
        let mut acc = anchor;
        for len_byte in (bytes.len() as u64).to_le_bytes() {
            acc = engine.synthesize(acc, self.tip(len_byte));
        }
        for &b in bytes {
            acc = engine.synthesize(acc, self.tip(b));
        }
        acc
    }
}

impl ByteMapping {
    /// Number of bit-steps per byte fold. Each step performs two
    /// `synthesize` calls (one with d₀, one with d₁); the bit value
    /// determines the order.
    ///
    /// This constant is the `8` in the Fold Law gate (DESIGN.md gate 14):
    /// `degree(d₀) ≥ 256 × Self::BITS_PER_BYTE` after the 256-byte
    /// exercise.
    pub const BITS_PER_BYTE: usize = 8;

    /// Fold an 8-bit byte through `engine` into a single tip
    /// [`Distinction`].
    ///
    /// All intermediate distinctions are registered in
    /// `engine.all_distinctions` via the synthesize calls — no
    /// phantoms.
    ///
    /// Per-byte cost: 1 initial + 16 inner = 17 synthesize calls. After
    /// the first byte, the initial call saturates (no new distinction);
    /// inner calls only saturate when two bytes produce identical
    /// intermediate accumulators.
    #[must_use]
    pub fn map_byte_to_distinction(byte: u8, engine: &DistinctionEngine) -> Distinction {
        let d0 = engine.d0();
        let d1 = engine.d1();
        // Seed the fold with the primordial child synth(d0, d1). After
        // the first byte across the entire exercise, this saturates —
        // the cost is one-time, not per-byte.
        let mut acc = engine.synthesize(d0, d1);

        for bit_index in 0..Self::BITS_PER_BYTE {
            let bit_value = (byte >> bit_index) & 1;
            // Each bit-step does two synthesize calls. The bit value
            // determines the order. Both primordials participate at
            // every step — that's what makes them topological mega-hubs.
            //
            // For bit=0: acc → acc⊗d0 → (acc⊗d0)⊗d1
            // For bit=1: acc → acc⊗d1 → (acc⊗d1)⊗d0
            //
            // Result differs by bit value because the intermediates
            // (acc⊗d0) and (acc⊗d1) are distinct by content addressing,
            // and a synthesis with different parents produces a different
            // child. So bit information is preserved end-to-end.
            if bit_value == 0 {
                acc = engine.synthesize(acc, d0);
                acc = engine.synthesize(acc, d1);
            } else {
                acc = engine.synthesize(acc, d1);
                acc = engine.synthesize(acc, d0);
            }
        }
        acc
    }
}

// ---------------------------------------------------------------------------
// Canonicalizable impls
// ---------------------------------------------------------------------------

impl Canonicalizable for u8 {
    fn to_canonical_structure(self, engine: &DistinctionEngine) -> Distinction {
        ByteMapping::map_byte_to_distinction(self, engine)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ----- Determinism --------------------------------------------------

    #[test]
    fn same_byte_same_distinction_on_same_engine() {
        let e = DistinctionEngine::new();
        let a = ByteMapping::map_byte_to_distinction(42, &e);
        let b = ByteMapping::map_byte_to_distinction(42, &e);
        assert_eq!(a, b);
    }

    #[test]
    fn different_bytes_different_distinctions() {
        let e = DistinctionEngine::new();
        let mut seen = std::collections::HashSet::with_capacity(256);
        for byte in 0u8..=255 {
            let d = ByteMapping::map_byte_to_distinction(byte, &e);
            assert!(
                seen.insert(d),
                "byte {byte} produced a duplicate distinction (collides with an earlier byte)"
            );
        }
        // 256 distinct distinctions produced.
        assert_eq!(seen.len(), 256);
    }

    #[test]
    fn same_byte_same_distinction_across_engines() {
        // Content addressing — different engines, same byte → same bytes.
        let e1 = DistinctionEngine::new();
        let e2 = DistinctionEngine::new();
        let a = ByteMapping::map_byte_to_distinction(0xAB, &e1);
        let b = ByteMapping::map_byte_to_distinction(0xAB, &e2);
        assert_eq!(a.as_bytes(), b.as_bytes());
    }

    // ----- Phantom-node fix --------------------------------------------

    #[test]
    fn no_phantom_nodes_after_byte_fold() {
        // The historical phantom-node bug: v1.2's ByteMapping cached
        // distinction IDs against a throwaway engine, so returned IDs
        // were not registered in the calling engine. v2.0 folds through
        // the calling engine, so every intermediate is registered.
        //
        // Structural test: the tip distinction must be in
        // engine.all_distinctions (i.e., engine.degree(tip) returns a
        // non-zero value, since registered non-primordials get the +2
        // parent-edge addend).
        let e = DistinctionEngine::new();
        let tip = ByteMapping::map_byte_to_distinction(0x42, &e);
        // tip is not a primordial — it was synthesized.
        assert_ne!(tip, e.d0());
        assert_ne!(tip, e.d1());
        // tip is registered: degree returns >= 2 (the addend).
        assert!(e.degree(tip) >= 2);
        // tip has parents (structural law 5).
        assert!(e.parents_of(tip).is_some());
    }

    #[test]
    fn distinction_count_grows_with_byte_folds() {
        // Folding more bytes registers more intermediate distinctions.
        let e = DistinctionEngine::new();
        let count_initial = e.distinction_count();
        let _ = ByteMapping::map_byte_to_distinction(0x00, &e);
        let count_after_one = e.distinction_count();
        assert!(count_after_one > count_initial);

        let _ = ByteMapping::map_byte_to_distinction(0xFF, &e);
        let count_after_two = e.distinction_count();
        // 0xFF takes the bit=1 path at every step, producing different
        // intermediates than 0x00 (which takes bit=0 everywhere).
        assert!(count_after_two > count_after_one);
    }

    // ----- Fold Law byte coverage --------------------------------------

    #[test]
    fn fold_law_byte_coverage_exact_bound() {
        // After the 256-byte exercise, d0 and d1 must each have
        // exactly 512 degree — the topological maximum for the 8-bit
        // fold shape (see ByteMapping doc-comment for the derivation).
        //
        // Tightened from `>= 510` to `== 512`
        // because the value is provably exact under content-addressing
        // saturation. A `>=` gate would mask regressions where someone
        // accidentally adds an extra synth call (degree silently grows
        // to 513+, hiding a Fold redesign).
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        assert_eq!(e.degree(e.d0()), 512, "Fold Law byte coverage: d0 degree");
        assert_eq!(e.degree(e.d1()), 512, "Fold Law byte coverage: d1 degree");
    }

    #[test]
    fn fold_law_subset_predicted_degree() {
        // Falsification check on the per-bit-prefix collapse model:
        // fold only the first half of the byte space (0..=127). Bytes
        // 0-127 vary bits 0-6 but always have bit 7 = 0.
        //
        // Predicted novel bumps per primordial:
        //   Steps 0-6 (bits 0-6 vary normally): 2+4+8+16+32+64+128 = 254
        //   Bit 7 (always 0): 128 unique starting accs from
        //     step 6, ONE bit-value processed, so 128 combinations
        //     each contributing one d0 and one d1 bump.
        //   Subtotal: 254 + 128 = 382 novel bumps.
        //   Plus 1 from initial synth(d0, d1) saturated after byte 0.
        //   Plus 1 genesis addend in degree().
        //   Total: 384.
        //
        // The full 256-byte exercise gets one extra contribution at
        // step 7 (the bit_7=1 paths) giving +128 more → 512.
        let e = DistinctionEngine::new();
        for byte in 0u8..=127 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        assert_eq!(e.degree(e.d0()), 384, "subset fold: d0 degree");
        assert_eq!(e.degree(e.d1()), 384, "subset fold: d1 degree");
    }

    #[test]
    fn fold_law_d0_d1_are_top_two_by_degree() {
        // The Fold Law's claim is that d0 and d1
        // are the topological mega-hubs — meaning they have the highest
        // degrees in the engine, not merely "above some threshold."
        // After the 256-byte exercise, iterate every distinction in
        // engine.all_distinctions and confirm no non-primordial degree
        // matches or exceeds d0/d1's degree.
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        let d0_degree = e.degree(e.d0());
        let d1_degree = e.degree(e.d1());

        // Iterate every distinction in the engine via the snapshot API,
        // exclude the primordials, and find the maximum non-primordial
        // degree. Must be strictly less than the primordial degrees.
        let max_non_primordial_degree: usize = e
            .snapshot_distinctions()
            .into_iter()
            .filter(|d| *d != e.d0() && *d != e.d1())
            .map(|d| e.degree(d))
            .max()
            .unwrap_or(0);

        assert!(
            d0_degree > max_non_primordial_degree,
            "d0 degree {d0_degree} not strictly greater than max non-primordial {max_non_primordial_degree} — Fold Law dominance failed"
        );
        assert!(
            d1_degree > max_non_primordial_degree,
            "d1 degree {d1_degree} not strictly greater than max non-primordial {max_non_primordial_degree} — Fold Law dominance failed"
        );
    }

    #[test]
    fn fold_law_d0_d1_hub_ratio_clears_100x_gate() {
        // DESIGN.md gate 14: d0/d1 hub ratio
        // ≥ 100× the maximum non-primordial degree after the full
        // 256-byte exercise. This is the Fold Law's quantitative
        // teeth: d0 and d1 aren't just slightly dominant (review
        // round-2 test asserts strict-greater), they are TOPOLOGICAL
        // MEGA-HUBS — orders of magnitude above any other node.
        //
        // Floor gate: ≥ 50× (DESIGN.md gate 14 hard-cap).
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        let d0_degree = e.degree(e.d0());
        let d1_degree = e.degree(e.d1());
        let primordial_min = d0_degree.min(d1_degree);

        let max_nonprim_degree: usize = e
            .snapshot_distinctions()
            .into_iter()
            .filter(|d| *d != e.d0() && *d != e.d1())
            .map(|d| e.degree(d))
            .max()
            .unwrap_or(0);

        // Guard against div-by-zero in degenerate engine.
        assert!(max_nonprim_degree > 0, "non-primordials exist after 256-byte fold (sanity)");

        let ratio = primordial_min as f64 / max_nonprim_degree as f64;
        // Gate target ≥ 100×, floor ≥ 50×.
        assert!(
            ratio >= 100.0,
            "Fold Law hub ratio {ratio:.1}× misses 100× target \
             (d0={d0_degree}, d1={d1_degree}, max_nonprim={max_nonprim_degree})"
        );
    }

    #[test]
    fn fold_law_structural_invariant_holds_after_exercise() {
        // Phantom-node regression guard: after the full 256-byte
        // exercise, the engine's r = 2d − 3 invariant must hold. If
        // ByteMapping ever reintroduces the throwaway-engine pattern
        // (cached IDs not registered in caller's all_distinctions),
        // this fails because parents_of entries would reference
        // unregistered distinctions.
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        e.check_structural_invariant().expect("r = 2d − 3 holds after byte fold (invariant)");
    }

    // ----- Canonicalizable trait ---------------------------------------

    #[test]
    fn canonicalizable_u8_matches_byte_mapping() {
        let e = DistinctionEngine::new();
        let via_trait = 0x42_u8.to_canonical_structure(&e);
        let via_direct = ByteMapping::map_byte_to_distinction(0x42, &e);
        assert_eq!(via_trait, via_direct);
    }

    // ----- The anchored fold -------------------------------------------

    /// An anchor that is not an element tip: a synthesized root, which is
    /// what a consumer with a perspective actually holds.
    fn anchor_for(e: &DistinctionEngine) -> Distinction {
        let c = e.synthesize(e.d0(), e.d1());
        e.synthesize(c, e.d0())
    }

    #[test]
    fn all_256_element_tips_are_distinct() {
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let mut seen = std::collections::HashSet::new();
        for b in 0..=255u8 {
            assert!(seen.insert(*fold.tip(b).as_bytes()), "two byte values share a tip: {b}");
        }
        assert_eq!(seen.len(), 256);
    }

    #[test]
    fn the_cases_that_collided_before() {
        // The reported failures, as regression vectors. Each pair folded
        // to the same distinction under the hand-rolled fold.
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let anchor = anchor_for(&e);

        // Byte order in the first two positions was unordered.
        let nonce_1 = 1u64.to_le_bytes();
        let nonce_256 = 256u64.to_le_bytes();
        assert_ne!(
            fold.fold(anchor, &nonce_1, &e),
            fold.fold(anchor, &nonce_256, &e),
            "nonce 1 and nonce 256 differ only by byte order and must not fold alike"
        );

        let nonce_2 = 2u64.to_le_bytes();
        let nonce_512 = 512u64.to_le_bytes();
        assert_ne!(fold.fold(anchor, &nonce_2, &e), fold.fold(anchor, &nonce_512, &e));

        // A leading run of equal bytes vanished through irreflexivity.
        assert_ne!(
            fold.fold(anchor, &[0x00], &e),
            fold.fold(anchor, &[0x00, 0x00], &e),
            "a repeated leading byte must not be absorbed"
        );
    }

    #[test]
    fn order_length_and_repetition_all_change_the_result() {
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let anchor = anchor_for(&e);
        let cases: [&[u8]; 8] = [
            &[],
            &[0x01],
            &[0x02],
            &[0x01, 0x02],
            &[0x02, 0x01],
            &[0x01, 0x01],
            &[0x01, 0x02, 0x03],
            &[0x03, 0x02, 0x01],
        ];
        let mut seen = std::collections::HashMap::new();
        for case in cases {
            let d = fold.fold(anchor, case, &e);
            if let Some(prev) = seen.insert(*d.as_bytes(), case) {
                panic!("{case:?} folds the same as {prev:?}");
            }
        }
    }

    #[test]
    fn prefixes_do_not_fold_alike() {
        // Length is folded in first precisely so that a sequence and its
        // prefix cannot coincide.
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let anchor = anchor_for(&e);
        let full = [0xAA, 0xBB, 0xCC, 0xDD];
        let mut seen = std::collections::HashSet::new();
        for n in 0..=full.len() {
            assert!(
                seen.insert(*fold.fold(anchor, &full[..n], &e).as_bytes()),
                "prefix of length {n} collides with a shorter one"
            );
        }
    }

    #[test]
    fn the_anchor_changes_the_result() {
        // Domain separation without touching identity: the same bytes
        // under two anchors are two different distinctions.
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let a1 = anchor_for(&e);
        let a2 = e.synthesize(a1, e.d1());
        assert_ne!(fold.fold(a1, b"same bytes", &e), fold.fold(a2, b"same bytes", &e));
    }

    #[test]
    #[should_panic(expected = "the anchor is an element tip")]
    fn an_anchor_that_is_an_element_tip_is_rejected() {
        // The degenerate case, constructed directly. A property test would
        // reach it with probability 2^-128, so it has to be built by hand:
        // an anchor drawn from the element alphabet lets the first step
        // collapse through irreflexivity and drop an element.
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let _ = fold.fold(fold.tip(0x41), &[0x41, 0x42], &e);
    }

    #[test]
    fn folding_registers_every_intermediate() {
        // A fold whose output is not registered would be unusable as a
        // parent — the reason a shared, engine-independent tip table
        // would be wrong.
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        let anchor = anchor_for(&e);
        let out = fold.fold(anchor, b"registered?", &e);
        assert!(e.has(out));
        assert!(e.check_structural_invariant().is_ok());
        // And it can be used as a parent without tripping the guard.
        let _ = e.synthesize(out, e.d0());
    }

    #[test]
    fn tips_are_engine_independent_but_folds_need_their_own_engine() {
        let e1 = DistinctionEngine::new();
        let e2 = DistinctionEngine::new();
        let f1 = ByteFold::new(&e1);
        let f2 = ByteFold::new(&e2);
        for b in [0u8, 1, 0x7F, 0xFF] {
            assert_eq!(
                f1.tip(b).as_bytes(),
                f2.tip(b).as_bytes(),
                "tips must be engine-independent"
            );
        }
        // Same anchor history, same bytes, two engines: same result.
        let a1 = anchor_for(&e1);
        let a2 = anchor_for(&e2);
        assert_eq!(a1.as_bytes(), a2.as_bytes());
        assert_eq!(f1.fold(a1, b"abc", &e1).as_bytes(), f2.fold(a2, b"abc", &e2).as_bytes());
    }
}
