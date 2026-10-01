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
/// 8 bit-steps in `engine`. Each step folds the accumulator with the one
/// primordial that bit selects, so `synthesize(acc, d₀)` and
/// `synthesize(acc, d₁)` diverge by content addressing and every bit
/// survives into the chain.
///
/// # Fold Law contribution
///
/// Every byte routes through `d₀` and `d₁`, so they accumulate degree far
/// faster than anything else: after folding all 256 byte values each
/// reaches 257, against a maximum of 4 for any other node — a ratio of
/// about 64×. That dominance is what the Fold Law names.
///
/// The arithmetic: at bit-step `i` there are `2^i` distinct accumulators
/// across all 256 bytes, each folded with the one primordial its bit
/// selects, so each primordial takes `2^i` novel bumps. Summed over the
/// eight steps that is `2^8 - 1 = 255`, plus one from the anchoring
/// `synthesize(d₀, d₁)` (novel at the first byte, saturated after), plus
/// the genesis addend `degree()` adds for a primordial.
///
/// **The magnitude is a property of this encoding, not of the axioms.**
/// An earlier version folded *both* primordials at every step, reaching
/// 512 and 1023 nodes. Measurement showed the extra calls bought no
/// injectivity — both shapes produce 256 distinct tips — so the larger
/// number described the encoding's shape rather than anything the theory
/// forces. Choosing a different alphabet moves it again: fold sequences
/// over the 256 byte tips and the tips become the hubs.
///
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
    /// rather than a fold per lookup.
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

/// Domain label for a consumer's anchored folds.
///
/// Two folds of the same bytes under different domains give different
/// distinctions. This is the separation mechanism the substrate offers,
/// and it is deliberately *not* part of identity: mixing a domain tag
/// into the hash would be a naming layer above content addressing, and
/// would end the property that two engines performing the same
/// operations agree. Anchoring keeps separation in the structure that is
/// synthesized instead.
pub type DomainLabel = u8;

/// A domain anchor to fold against.
///
/// Returns a shallow synthesized node — never an element tip, so it is a
/// legal anchor for [`ByteFold::fold`], which checks that anyway.
/// Consumers that carry a perspective should anchor on their own root
/// instead; this is for folds with no natural anchor of their own.
#[must_use]
pub fn domain_anchor(
    engine: &DistinctionEngine,
    fold: &ByteFold,
    domain: DomainLabel,
) -> Distinction {
    let base = engine.synthesize(engine.d0(), engine.d1());
    engine.synthesize(base, fold.tip(domain))
}

impl ByteMapping {
    /// Number of bit-steps per byte fold, one `synthesize` call each.
    ///
    /// One synthesize call per bit, so this is also the per-byte cost of
    /// the lift. After folding all 256 byte values each primordial
    /// reaches `2^BITS_PER_BYTE + 1 = 257`: `2^i` novel bumps at step
    /// `i`, plus the anchoring synthesis, plus the genesis addend.
    pub const BITS_PER_BYTE: usize = 8;

    /// Fold an 8-bit byte through `engine` into a single tip
    /// [`Distinction`].
    ///
    /// All intermediate distinctions are registered in
    /// `engine.all_distinctions` via the synthesize calls — no
    /// phantoms.
    ///
    /// This is the anchored fold of [`ByteFold`], instantiated one level
    /// down: the alphabet is `{d₀, d₁}` and the elements are the byte's
    /// eight bits. The substrate therefore has **one** lift rule, applied
    /// recursively — bytes fold over the primordials, sequences fold over
    /// the byte tips — rather than a sequence fold plus a special-cased
    /// byte encoding.
    ///
    /// Width is fixed at eight, so unlike [`ByteFold::fold`] no length
    /// needs folding in: every byte produces a chain of the same depth
    /// and only the bit values distinguish them.
    ///
    /// Per-byte cost: 8 synthesize calls, one per bit.
    ///
    /// # Why not two calls per bit
    ///
    /// This previously ran both primordials at every bit-step — 17 calls
    /// per byte — on the reasoning that it made d₀ and d₁ topological
    /// hubs. Measurement says the extra nine calls buy nothing: both
    /// shapes yield 256 distinct tips, so injectivity is unchanged, while
    /// the two-call form exactly doubles the graph (1023 nodes against
    /// 513) and the primordials' degree (512 against 257). The hub
    /// magnitude was an artifact of the encoding, not a consequence of
    /// the axioms — which is what the Fold Law now says.
    #[must_use]
    pub fn map_byte_to_distinction(byte: u8, engine: &DistinctionEngine) -> Distinction {
        let d0 = engine.d0();
        let d1 = engine.d1();
        // Anchor: the primordial child. Not an element of this alphabet,
        // which is the same rule `ByteFold::fold` enforces for its own —
        // an anchor drawn from the alphabet could collapse the first step
        // through irreflexivity.
        let mut acc = engine.synthesize(d0, d1);

        for bit_index in 0..Self::BITS_PER_BYTE {
            // Nesting carries position; the element carries the value.
            // `synthesize(acc, d0)` and `synthesize(acc, d1)` differ by
            // content addressing, so each bit survives into the chain.
            acc = engine.synthesize(acc, if (byte >> bit_index) & 1 == 0 { d0 } else { d1 });
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
        // Derivation for the 8-step fold. At bit-step i there are 2^i
        // distinct accumulators across all 256 bytes, each folded with one
        // primordial chosen by the bit, so each primordial receives 2^i
        // novel bumps. Summed over i = 0..7 that is 2^8 - 1 = 255, plus one
        // from the anchoring synth(d0, d1) which saturates after the first
        // byte, plus the genesis addend that `degree()` adds. 257.
        //
        // It was 512 under the previous encoding, which folded *both*
        // primordials at every step. That doubled the graph and bought no
        // injectivity — both shapes produce 256 distinct tips — so the
        // number characterises the encoding, not the axioms.
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        assert_eq!(e.degree(e.d0()), 257, "Fold Law byte coverage: d0 degree");
        assert_eq!(e.degree(e.d1()), 257, "Fold Law byte coverage: d1 degree");
        assert_eq!(e.distinction_count(), 513, "one chain node per bit-prefix, plus primordials");
    }

    #[test]
    fn fold_law_subset_predicted_degree() {
        // Bytes 0..=127 never set bit 7, and the asymmetry that produces is
        // the point of the test. Steps 0-6 give each primordial 2^i bumps
        // (127 each). At step 7 all 128 accumulators fold with d0, so d0
        // takes 128 more and d1 takes none. With the anchor and the genesis
        // addend: d0 = 257, d1 = 129.
        //
        // So a bit that is never set starves one primordial. Degree tracks
        // the data folded, not the alphabet's mere existence.
        let e = DistinctionEngine::new();
        for byte in 0u8..=127 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        assert_eq!(e.degree(e.d0()), 257, "subset fold: d0 degree");
        assert_eq!(e.degree(e.d1()), 129, "subset fold: d1 degree (bit 7 never set)");
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
    fn fold_law_hub_ratio_characterises_this_encoding() {
        // Renamed from a "clears 100x gate" test. The 100x target was
        // calibrated against the previous 17-step encoding, which inflated
        // primordial degree to 512 by folding both primordials at every
        // step. Under the derived 8-step fold the same exercise measures
        // 64.2x, and the drop is not a regression: it is the artifact
        // leaving.
        //
        // What survives as a claim is the shape, not the magnitude — the
        // primordials dominate every other node by a wide margin because
        // the lift routes every byte through them. The magnitude is a
        // property of the encoding, which is why this asserts a floor and
        // records the measured value rather than gating on a number the
        // axioms never implied.
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }
        let primordial_min = e.degree(e.d0()).min(e.degree(e.d1()));
        let max_nonprim: usize = e
            .snapshot_distinctions()
            .into_iter()
            .filter(|d| *d != e.d0() && *d != e.d1())
            .map(|d| e.degree(d))
            .max()
            .unwrap_or(0);
        assert!(max_nonprim > 0, "non-primordials exist after the 256-byte fold (sanity)");

        let ratio = primordial_min as f64 / max_nonprim as f64;
        assert!(
            ratio >= 50.0,
            "hub ratio {ratio:.1}x fell below the floor \
             (d0={}, d1={}, max_nonprim={max_nonprim})",
            e.degree(e.d0()),
            e.degree(e.d1())
        );
        // Pinned so a change in the encoding shows up as a test change.
        assert_eq!(max_nonprim, 4, "interior chain nodes have two children plus two parents");
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
    fn lift_matches_the_published_vectors() {
        // The lift is the substrate's only free parameter, so these are the
        // vectors that matter most for agreement between implementations:
        // everything else can match while every folded value differs.
        use crate::vectors;
        let e = DistinctionEngine::new();
        let fold = ByteFold::new(&e);
        for (byte, expected) in [
            (0x00u8, vectors::TIP_00_HEX),
            (0x01, vectors::TIP_01_HEX),
            (0x41, vectors::TIP_41_HEX),
            (0xFF, vectors::TIP_FF_HEX),
        ] {
            assert_eq!(fold.tip(byte).to_hex(), expected, "byte tip {byte:#04x} drifted");
        }

        // The anchor is vector D: a synthesized node, not a byte tip,
        // which is what the fold requires of an anchor.
        let c = e.synthesize(e.d0(), e.d1());
        let d = e.synthesize(c, e.d0());
        assert_eq!(d.to_hex(), vectors::D_HEX, "anchor is not vector D");
        assert!(!fold.is_tip(d), "the anchor must not be an element tip");

        assert_eq!(
            fold.fold(d, b"hi", &e).to_hex(),
            vectors::FOLD_HI_ANCHORED_AT_D_HEX,
            "sequence fold drifted"
        );
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
