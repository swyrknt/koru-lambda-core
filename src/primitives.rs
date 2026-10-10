//! The lift — getting a value that is not a distinction into the graph.
//!
//! This is the substrate's only free parameter. Everything downstream of
//! the primordials is forced by the axioms; the lift is the one place a
//! choice enters, which is why `docs/SPEC.md` §4 specifies it and why two
//! implementations that lift differently agree on nothing.
//!
//! It is **one rule at three levels**, and in this module that is literal:
//! all three call the same private fold.
//!
//! | Level | Alphabet | Elements | Length folded |
//! |---|---|---|---|
//! | bits | `{d₀, d₁}` | the byte's 8 bits | no, width is fixed |
//! | bytes | the 256 bit-tips | the sequence's bytes | yes |
//! | composites | the parts' own tips | a value's parts, in order | yes |
//!
//! [`ByteMapping::map_byte_to_distinction`] is the first,
//! [`ByteFold::fold`] the second, [`ByteFold::fold_distinctions`] the
//! third. Nesting needs no fourth: a composite's fold is a distinction,
//! so it is an element one level up.
//!
//! [`Canonicalizable`] names the contract a liftable type satisfies. The
//! substrate ships [`u8`]; a consumer lifting its own type decides how
//! that type decomposes into parts and folds them.
//!
//! # Every step goes through the engine
//!
//! A lift must use `engine.synthesize` for every step. The v1.2 byte
//! mapping cached tips from a throwaway engine and handed them to the
//! caller's, producing identifiers whose distinctions that engine had
//! never witnessed — present in `parents_of`, absent from
//! `all_distinctions`, and breaking `r = 2d − 3` semantically. That is
//! why these take `&DistinctionEngine` rather than returning free-standing
//! values, and why the trust boundary has no third path.

use crate::{Distinction, DistinctionEngine};

/// `docs/SPEC.md` §4.2's anchored fold: continue the chain at `acc` by
/// folding each element's tip into it, in order.
///
/// This function is the whole of the lift. Everything the substrate lifts is
/// it at one of three levels — bits over the primordials, bytes over the bit
/// tips, composites over their field tips — and the levels differ only in
/// which alphabet `tip` reads from. The documentation claimed "one lift rule,
/// applied recursively" while the code wrote the loop out twice, once per
/// level, which is two implementations of one rule and two places for it to
/// drift.
///
/// It takes an accumulator rather than an anchor so that the stages compose:
/// a length prefix folded over the byte alphabet can be continued by elements
/// folded over a different one, which is what §4.2 requires and what a
/// composite fold needs.
///
/// # Panics
///
/// Panics if an element's tip equals the accumulator it is folded into.
/// `synthesize(a, a)` returns `a` by Axiom 3, so that step would silently
/// drop the element — the exact failure the anchor rule exists to prevent,
/// arriving one step later. For the two byte-level alphabets this cannot
/// happen (an accumulator past the first step is a hash-chain node while
/// elements come from a fixed finite alphabet, so it is the collision bound
/// of §11), and the assertion says so rather than assuming it. For
/// caller-supplied element distinctions it is reachable, which is why the
/// check is not a `debug_assert!`.
fn fold_into<S>(
    engine: &DistinctionEngine,
    acc: Distinction,
    elements: impl IntoIterator<Item = S>,
    tip: impl Fn(S) -> Distinction,
) -> Distinction {
    let mut acc = acc;
    for element in elements {
        let t = tip(element);
        assert!(
            t != acc,
            "fold: an element's tip equals the accumulator it folds into, so Axiom 3 would \
             return the accumulator and drop the element. Anchor elsewhere, or do not fold a \
             distinction that is already in the chain."
        );
        // Nesting carries position; the tip carries the value.
        acc = engine.synthesize(acc, t);
    }
    acc
}

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
/// bypassing synthesize) is forbidden
/// because the resulting distinction would not be registered in
/// `engine.all_distinctions` and would trip the foreign-byte guard on
/// any subsequent `engine.synthesize` call that uses it as a parent.
///
/// # A type brings its own domain
///
/// There is no anchor parameter, and that is the design rather than an
/// omission. A liftable type chooses its own [`domain_anchor`], because the
/// separation has to hold between *types*: a commitment and a transaction
/// carrying identical bytes must not fold alike. A caller-supplied anchor
/// would move that decision to the call site, where one forgetful consumer
/// collapses the separation for everyone sharing the engine.
///
/// The consequence is that the substrate ships exactly one impl — [`u8`],
/// whose anchor §4.3 forces — and no impls for `&[u8]`, `u64` or `&str`.
/// Those types have no domain of their own, and a substrate-chosen
/// `DOMAIN_BYTES` would put every consumer's sequences in one domain, which
/// is the collision the anchor exists to prevent. A consumer lifts its own
/// named type instead, and names the domain there.
pub trait Canonicalizable {
    /// Fold `self` into a chain of synthesize calls on `engine`, returning
    /// the tip distinction.
    ///
    /// Takes `&self` and a prebuilt `fold`. Both were learned from the two
    /// real implementors, which bypassed this method for exactly those two
    /// reasons: a batch commit folds `for tx in &transactions` and cannot
    /// consume, and building the 256-tip table per call dominates a
    /// thousand-transaction batch.
    #[must_use]
    fn to_canonical_structure(&self, fold: &ByteFold, engine: &DistinctionEngine) -> Distinction;
}

/// Byte → distinction folding (v2.0: engine-registered, no phantoms).
///
/// [`ByteMapping::map_byte_to_distinction`] folds an 8-bit byte through
/// 8 bit-steps in `engine`. Each step folds the accumulator with the one
/// primordial that bit selects, so `synthesize(acc, d₀)` and
/// `synthesize(acc, d₁)` diverge by content addressing and every bit
/// survives into the chain.
///
/// # Alphabet-construction cost
///
/// Constructing the alphabet routes every byte through `d₀` and `d₁`, so each
/// reaches degree 257 against a maximum of 4 for any other node — a ratio of
/// about 64×.
///
/// **That ratio measures the setup and nothing else.** It was described here
/// as what the Fold Law names, and a probe that decoupled the anchor from the
/// alphabet showed otherwise: the primordials gain **zero** degree from
/// folding data, so the 257 is a one-time construction cost that does not
/// grow with the corpus. At 64 KiB of folded data `d₀` is rank 42, and the
/// ratio is 0.02×. Law 11 lost its independent status on that measurement —
/// see `THEORY.md § Law 11` and
/// `experiments/runner/src/anchor_hub_probe.rs`.
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
/// forces. And the alphabet is what decides where degree concentrates, which
/// the probe confirmed by varying the two factors independently: the
/// top-degree node changed in 0 of 90 alphabet-varying pairs and 90 of 90
/// anchor-varying pairs. An anchor touched only at a fold's first step is
/// ceilinged at the alphabet size however much is folded.
///
pub struct ByteMapping;

/// An ordered sequence of bytes, folded into one distinction.
///
/// # Why this exists as a primitive
///
/// The operator is commutative and idempotent, so a sequence cannot be
/// folded naively: `synthesize` cannot tell `(x, y)` from `(y, x)`, and
/// `synthesize(x, x)` returns `x`. An ordered encoding therefore has to
/// carry order in the *nesting* — each step folds the running
/// accumulator with the next element, so position is recorded by depth.
///
/// Two consumers wrote that fold by hand and both wrote it the same way
/// and wrong: they seeded the accumulator with the **first element's**
/// tip. That makes the first two positions an unordered pair, and a
/// leading run of equal bytes vanishes through idempotence. Nonce 1
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
///   step could collapse through idempotence and silently drop an
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
             idempotence and drop an element. Anchor on a synthesized root instead."
        );
        let acc = self.fold_length(anchor, bytes.len(), engine);
        fold_into(engine, acc, bytes.iter().copied(), |b| self.tip(b))
    }

    /// Fold an ordered sequence of distinctions, anchored at `anchor`.
    ///
    /// This is the composite rule, and it is the same rule as
    /// [`Self::fold`] one level up: where that folds byte tips, this folds
    /// whatever the caller already lifted. A struct is lifted by lifting each
    /// field to a distinction and folding those in declared field order.
    ///
    /// # Why this rather than a byte-layout rule
    ///
    /// `docs/SPEC.md` §4.2 specifies the fold over an arbitrary alphabet, but
    /// only the byte case shipped, so a consumer holding a struct with a
    /// variable-length field had to invent its own framing — which is the
    /// collision class [`Self::fold`] closed, reappearing one level up. Two
    /// implementations could pass every published vector and disagree on the
    /// first composite either one lifted.
    ///
    /// The alternative was a documented byte-concatenation rule: serialise
    /// the fields with length prefixes, then byte-fold the result. It was
    /// rejected because a composite containing a composite would then need
    /// the rule to specify nesting *within bytes*, which is a serialisation
    /// format growing inside the substrate's specification. Here nesting is
    /// free: an inner composite's fold is a distinction, and a distinction is
    /// an element at the next level. The specification stays one rule.
    ///
    /// # Panics
    ///
    /// Panics if `anchor` is a byte tip (the length prefix folds over the
    /// byte alphabet, so the first step could collapse), or if any element
    /// equals the accumulator it folds into, which Axiom 3 would answer by
    /// returning the accumulator and dropping that element.
    #[must_use]
    pub fn fold_distinctions(
        &self,
        anchor: Distinction,
        elements: &[Distinction],
        engine: &DistinctionEngine,
    ) -> Distinction {
        assert!(
            !self.is_tip(anchor),
            "fold_distinctions: the anchor is a byte tip, and the length prefix folds over the \
             byte alphabet, so the first step could collapse through idempotence. Anchor on a \
             synthesized root instead."
        );
        let acc = self.fold_length(anchor, elements.len(), engine);
        fold_into(engine, acc, elements.iter().copied(), |d| d)
    }

    /// Fold a length prefix over the byte alphabet, per `docs/SPEC.md` §4.2.
    ///
    /// Shared by both sequence folds so the length discipline is written
    /// once: `u64` little-endian, folded before the elements, so a sequence
    /// and a prefix of it cannot coincide. The width is `u64` and not the
    /// element count's native width so that a 32-bit and a 64-bit producer
    /// agree.
    fn fold_length(
        &self,
        anchor: Distinction,
        len: usize,
        engine: &DistinctionEngine,
    ) -> Distinction {
        fold_into(engine, anchor, (len as u64).to_le_bytes(), |b| self.tip(b))
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
///
/// # What this type does, and what it cannot
///
/// It makes the choice explicit and typed. It was a `pub type … = u8`, so
/// any byte in scope satisfied the parameter and a call read
/// `domain_anchor(&e, &f, DomainLabel::new(0x01))` — an integer literal pointing at no
/// documentation and distinguishable from a nonce byte only by position.
///
/// What it does **not** do is prevent a collision. Nothing here reserves a
/// label: two crates can each choose `0x01`, and in a shared engine their
/// folds of the same bytes land on the same distinction. The substrate
/// cannot fix that, and should not pretend to — a registry would have to be
/// a naming authority, which is the thing content addressing replaces. Two
/// consumers sharing an engine have to agree on labels out of band, the way
/// they agree on anything else outside the axioms.
///
/// No range is reserved either. `conformance/vectors.json` pins the anchors
/// for labels 1, 2, 127 and 255, but those are *derivation* vectors — they
/// check the anchor formula across the byte range — and claiming them as
/// reserved would be inventing policy the substrate does not enforce.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DomainLabel(u8);

impl DomainLabel {
    /// Name a domain by its label byte.
    #[must_use]
    pub const fn new(label: u8) -> Self {
        Self(label)
    }

    /// The label byte, for a consumer that needs to serialise it.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }
}

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
    engine.synthesize(base, fold.tip(domain.get()))
}

impl ByteMapping {
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
    /// 513) and the primordials' degree (512 against 257).
    ///
    /// The reasoning was wrong twice over. The magnitude was an artifact of
    /// the encoding — and so was the hub. A later probe found the primordials
    /// gain no degree at all from folded data, so neither call count made
    /// them hubs of anything beyond their own construction.
    #[must_use]
    pub fn map_byte_to_distinction(byte: u8, engine: &DistinctionEngine) -> Distinction {
        let d0 = engine.d0();
        let d1 = engine.d1();
        // Anchor: the primordial child. Not an element of this alphabet,
        // which is the same rule `ByteFold::fold` enforces for its own —
        // an anchor drawn from the alphabet could collapse the first step
        // through idempotence.
        let anchor = engine.synthesize(d0, d1);

        // Width is fixed, so no length is folded in: every byte produces a
        // chain of the same depth and only the bit values distinguish them.
        fold_into(engine, anchor, 0..u8::BITS, |bit| if (byte >> bit) & 1 == 0 { d0 } else { d1 })
    }
}

// ---------------------------------------------------------------------------
// Canonicalizable impls
// ---------------------------------------------------------------------------

impl Canonicalizable for u8 {
    /// A table lookup, not a fold: the caller already holds the 256 tips,
    /// and a byte's tip is one of them. Identical in value to
    /// [`ByteMapping::map_byte_to_distinction`] — `ByteFold::new` is what
    /// computed the table — and eight synthesize calls cheaper. Pinned by
    /// `the_byte_impl_is_the_tip_table`.
    fn to_canonical_structure(&self, fold: &ByteFold, _engine: &DistinctionEngine) -> Distinction {
        fold.tip(*self)
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
        // Scoped claim: immediately after constructing the alphabet, and
        // before folding any data, d0 and d1 hold the top two degrees. That
        // is a fact about the construction and not about the substrate — a
        // probe that decoupled anchor from alphabet found the primordials
        // gain zero degree from folded data and fall to rank 42 by 64 KiB.
        // What this pins is that building the alphabet routes through them.
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
    fn alphabet_construction_is_the_primordials_entire_degree() {
        // This asserted a 50x "hub ratio" floor, and it folds no corpus at
        // all — only the 256 alphabet-construction calls. So the ratio
        // measured the setup, while the surrounding docs generalised it into
        // a claim about where the lift concentrates degree. A probe that
        // decoupled the anchor from the alphabet showed the generalisation
        // was false: the primordials gain ZERO degree from folded data, and
        // by 64 KiB d0 is rank 42 against a ratio of 0.02x.
        //
        // So the floor is replaced by the two things actually true here, and
        // the second is the one with content.
        let e = DistinctionEngine::new();
        for byte in 0u8..=255 {
            let _ = ByteMapping::map_byte_to_distinction(byte, &e);
        }

        // 1. The construction cost is exact, not a floor: 2^u8::BITS
        //    novel bumps minus one, plus the anchoring synthesize(d0, d1),
        //    plus the genesis addend `degree()` adds for a primordial.
        let expected = (1usize << u8::BITS) + 1;
        assert_eq!(e.degree(e.d0()), expected, "d0 construction cost");
        assert_eq!(e.degree(e.d1()), expected, "d1 construction cost");

        let after_setup = e.degree(e.d0());
        let max_nonprim: usize = e
            .snapshot_distinctions()
            .into_iter()
            .filter(|d| *d != e.d0() && *d != e.d1())
            .map(|d| e.degree(d))
            .max()
            .unwrap_or(0);
        assert_eq!(max_nonprim, 4, "interior chain nodes have two children plus two parents");

        // 2. Folding DATA adds nothing to either primordial. This is the
        //    assertion that makes the demotion of Law 11 structural rather
        //    than a workload observation: an anchor touched only at a fold's
        //    first step is ceilinged at the alphabet size, so the ratio above
        //    decays as 1/corpus and is not a property of the substrate.
        let fold = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &fold, DomainLabel::new(0x7F));
        for chunk in 0u8..64 {
            let payload = [chunk, chunk.wrapping_add(1), chunk.wrapping_add(2)];
            let _ = fold.fold(anchor, &payload, &e);
        }
        assert_eq!(
            e.degree(e.d0()),
            after_setup,
            "d0 gained degree from folded data; the alphabet-construction cost is supposed \
             to be the whole of it, and Law 11's demotion rests on that"
        );
        assert_eq!(e.degree(e.d1()), after_setup, "d1 gained degree from folded data");
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

    /// The `u8` impl reads the tip table; `map_byte_to_distinction` folds.
    /// They must agree on every byte, not just one, because the table is
    /// what the fold produced and a divergence would mean the lift has two
    /// values for one byte.
    #[test]
    fn the_byte_impl_is_the_tip_table() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        for byte in 0u8..=255 {
            assert_eq!(
                byte.to_canonical_structure(&f, &e),
                ByteMapping::map_byte_to_distinction(byte, &e),
                "byte {byte:#04x} lifts differently through the trait than through the fold"
            );
        }
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

        // A leading run of equal bytes vanished through idempotence.
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
        // collapse through idempotence and drop an element.
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

    // -------------------------------------------------------------------
    // The composite rule — `fold_distinctions`
    // -------------------------------------------------------------------

    /// Order survives, which is the whole reason the fold nests.
    #[test]
    fn composite_fold_distinguishes_field_order() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let (a, b) = (f.tip(0xAA), f.tip(0xBB));

        assert_ne!(
            f.fold_distinctions(anchor, &[a, b], &e),
            f.fold_distinctions(anchor, &[b, a], &e),
            "a struct's fields are ordered; swapping two must change the composite"
        );
    }

    /// A prefix is not the sequence, which is what the length prefix buys.
    #[test]
    fn composite_fold_distinguishes_a_prefix_from_the_whole() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let (a, b) = (f.tip(0xAA), f.tip(0xBB));

        assert_ne!(
            f.fold_distinctions(anchor, &[a], &e),
            f.fold_distinctions(anchor, &[a, b], &e),
            "length is folded in first, so a prefix cannot coincide with the whole"
        );
    }

    /// A repeated field survives rather than collapsing through Axiom 3.
    ///
    /// This is the failure the anchor rule exists to prevent, at the
    /// composite level: a naive fold seeded on the first element loses a
    /// leading run of equal elements entirely.
    #[test]
    fn composite_fold_keeps_a_repeated_element() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let a = f.tip(0xAA);

        let once = f.fold_distinctions(anchor, &[a], &e);
        let twice = f.fold_distinctions(anchor, &[a, a], &e);
        let thrice = f.fold_distinctions(anchor, &[a, a, a], &e);
        assert_ne!(once, twice);
        assert_ne!(twice, thrice);
        assert_ne!(once, thrice);
    }

    /// Nesting is free: an inner composite is an element of the outer one.
    ///
    /// This is why the composite rule folds distinctions rather than
    /// specifying a byte layout — a byte layout would have to say how a
    /// nested composite is framed *within bytes*, which is a serialisation
    /// format inside the specification.
    #[test]
    fn composite_fold_nests_without_a_framing_rule() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let (a, b, c) = (f.tip(0x01), f.tip(0x02), f.tip(0x03));

        // `{ inner: { a, b }, c }` against `{ a, b, c }` — a flat fold of
        // three fields must not equal a nested fold of two.
        let inner = f.fold_distinctions(anchor, &[a, b], &e);
        let nested = f.fold_distinctions(anchor, &[inner, c], &e);
        let flat = f.fold_distinctions(anchor, &[a, b, c], &e);
        assert_ne!(nested, flat, "structure is part of the value, not just the leaves");
    }

    /// The domain anchor separates composites, as it does byte folds.
    #[test]
    fn composite_fold_separates_by_domain() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let a = f.tip(0xAA);

        assert_ne!(
            f.fold_distinctions(domain_anchor(&e, &f, DomainLabel::new(0x01)), &[a], &e),
            f.fold_distinctions(domain_anchor(&e, &f, DomainLabel::new(0x02)), &[a], &e),
            "two domains folding the same field must not collide"
        );
    }

    /// Every intermediate is registered, so the result is foldable again.
    #[test]
    fn composite_fold_registers_its_chain() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let r = f.fold_distinctions(anchor, &[f.tip(0x01), f.tip(0x02)], &e);

        // If any step had produced an unregistered distinction, using the
        // result as a parent would trip the foreign-byte guard.
        let _ = e.synthesize(r, e.d0());
        assert!(e.snapshot_distinctions().contains(&r));
    }

    /// A byte tip is refused as a composite anchor.
    ///
    /// The length prefix folds over the byte alphabet, so the first step of
    /// a composite fold has the same collapse exposure as a byte fold's.
    #[test]
    #[should_panic(expected = "the anchor is a byte tip")]
    fn composite_fold_refuses_a_byte_tip_anchor() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let _ = f.fold_distinctions(f.tip(0x00), &[f.tip(0xAA)], &e);
    }

    /// An element equal to the accumulator it folds into is refused.
    ///
    /// Reachable only by a caller who computes the accumulator, which this
    /// test does from public API alone — `tip` and `synthesize`. That it can
    /// be reconstructed this way is also the check that the documented
    /// length discipline is the one implemented: eight `u64` little-endian
    /// length bytes over the byte alphabet, before any element.
    #[test]
    #[should_panic(expected = "equals the accumulator it folds into")]
    fn composite_fold_refuses_an_element_already_in_the_chain() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let a = f.tip(0xAA);

        // Rebuild what `fold_distinctions(anchor, &[a, _], &e)` accumulates
        // after its first element: the length prefix for two elements, then
        // `a`.
        let mut acc = anchor;
        for len_byte in 2u64.to_le_bytes() {
            acc = e.synthesize(acc, f.tip(len_byte));
        }
        let after_a = e.synthesize(acc, a);

        let _ = f.fold_distinctions(anchor, &[a, after_a], &e);
    }

    /// The reconstruction above is the implementation, not a coincidence.
    #[test]
    fn the_documented_length_discipline_is_the_implemented_one() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x01));
        let a = f.tip(0xAA);

        let mut acc = anchor;
        for len_byte in 1u64.to_le_bytes() {
            acc = e.synthesize(acc, f.tip(len_byte));
        }
        let by_hand = e.synthesize(acc, a);

        assert_eq!(
            f.fold_distinctions(anchor, &[a], &e),
            by_hand,
            "u64 little-endian length over the byte alphabet, then the elements"
        );
    }

    /// Both shipped byte-level instances are the same rule as the composite
    /// one, so a change to the core cannot silently move only one of them.
    #[test]
    fn the_byte_fold_is_the_composite_fold_over_byte_tips() {
        let e = DistinctionEngine::new();
        let f = ByteFold::new(&e);
        let anchor = domain_anchor(&e, &f, DomainLabel::new(0x07));
        let bytes: &[u8] = b"hi";

        let as_elements: Vec<Distinction> = bytes.iter().map(|&b| f.tip(b)).collect();
        assert_eq!(
            f.fold(anchor, bytes, &e),
            f.fold_distinctions(anchor, &as_elements, &e),
            "folding bytes and folding their tips are one rule, not two"
        );
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
