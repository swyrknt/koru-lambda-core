//! Order is not in the operator. It falls out of composition.
//!
//! `synthesize` is commutative by axiom: `synth(a, b) == synth(b, a)`. The
//! operator therefore cannot represent an ordered pair — there is no field in
//! which "a came first" could be stored, and no amount of calling it supplies
//! one.
//!
//! Yet sequences of bytes get distinct identifiers, and the nonce ordering a
//! consensus consumer depends on survives. Something produces order from an
//! order-blind primitive, and this module shows exactly what: **nesting**.
//! Composing commutative operations is not commutative, because the
//! intermediate values differ. `synth(synth(X, a), b)` and
//! `synth(synth(X, b), a)` are syntheses of *different parents*, so content
//! addressing separates them even though each individual call ignores
//! argument order.
//!
//! This is the cleanest instance in the crate of structure that was never put
//! in: no position index is recorded anywhere, and order is recoverable
//! anyway. It is also a quantitative claim rather than a vibe, so the test
//! counts — see `the_flat_fold_loses_exactly_the_leading_transposition`.

use koru_lambda_core::{domain_anchor, ByteFold, Distinction, DistinctionEngine};
use std::collections::HashSet;

/// Every permutation of this many distinct bytes is enumerated, so keep it
/// small enough that `n!` stays cheap and large enough that `n!` is not a
/// coincidence. 5 gives 120 orderings.
const ELEMENTS: [u8; 5] = [0x41, 0x42, 0x43, 0x44, 0x45];

fn permutations(items: &[u8]) -> Vec<Vec<u8>> {
    if items.len() <= 1 {
        return vec![items.to_vec()];
    }
    let mut out = Vec::new();
    for (index, &head) in items.iter().enumerate() {
        let mut rest = items.to_vec();
        rest.remove(index);
        for mut tail in permutations(&rest) {
            tail.insert(0, head);
            out.push(tail);
        }
    }
    out
}

/// Left-associated application of the bare operator over the element tips.
/// This is the naive encoding — the one a reader reaches for before reading
/// the specification's section on why it fails.
fn flat_fold(engine: &DistinctionEngine, fold: &ByteFold, bytes: &[u8]) -> Distinction {
    let mut acc = fold.tip(bytes[0]);
    for &byte in &bytes[1..] {
        acc = engine.synthesize(acc, fold.tip(byte));
    }
    acc
}

/// The premise: the operator is blind to argument order.
#[test]
fn the_operator_cannot_represent_an_ordered_pair() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);

    for &a in &ELEMENTS {
        for &b in &ELEMENTS {
            let forward = engine.synthesize(fold.tip(a), fold.tip(b));
            let backward = engine.synthesize(fold.tip(b), fold.tip(a));
            assert_eq!(
                forward, backward,
                "commutativity broken for ({a:#04x}, {b:#04x}) — if this fails the rest of \
                 this module is moot, because the operator would already carry order"
            );
        }
    }
}

/// The mechanism, isolated: nesting breaks the symmetry that a single call has.
#[test]
fn nesting_is_what_creates_order() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);

    let context = engine.synthesize(fold.tip(0x01), fold.tip(0x02));
    let a = fold.tip(0x41);
    let b = fold.tip(0x42);

    // Each call ignores argument order...
    assert_eq!(engine.synthesize(context, a), engine.synthesize(a, context));

    // ...yet the two nestings differ, because their parents differ.
    let a_then_b = engine.synthesize(engine.synthesize(context, a), b);
    let b_then_a = engine.synthesize(engine.synthesize(context, b), a);
    assert_ne!(
        a_then_b, b_then_a,
        "two nestings of a commutative operator collapsed — order cannot emerge from \
         composition, and the anchored fold's sequence guarantee would be unfounded"
    );
}

/// The quantitative claim. The flat fold is not merely "weaker" — it loses a
/// specific, countable amount of order information, and the anchored fold
/// loses none.
///
/// Prediction, derived rather than observed: in `((((a1,a2),a3),a4),a5)` the
/// only transposition the operator's commutativity hides is `a1 <-> a2`,
/// because that is the one pair handed to a single call as bare arguments.
/// Every other position is separated by a nesting level. So permutations
/// should collide in pairs: `n!` orderings yield `n!/2` distinct identifiers.
#[test]
fn the_flat_fold_loses_exactly_the_leading_transposition() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let orderings = permutations(&ELEMENTS);
    assert_eq!(orderings.len(), 120, "expected 5! orderings");

    let distinct: HashSet<Distinction> =
        orderings.iter().map(|bytes| flat_fold(&engine, &fold, bytes)).collect();

    eprintln!(
        "flat fold: {} orderings collapse to {} identifiers",
        orderings.len(),
        distinct.len()
    );
    assert_eq!(
        distinct.len(),
        orderings.len() / 2,
        "the flat fold lost a different amount of order information than the operator's \
         commutativity accounts for — either the leading pair is no longer the only \
         unprotected position, or something else is collapsing"
    );
}

/// The emergence result: the anchored fold recovers every ordering, with no
/// position index stored anywhere.
#[test]
fn the_anchored_fold_recovers_every_ordering() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let anchor = domain_anchor(&engine, &fold, 0x01);
    assert!(
        !fold.is_tip(anchor),
        "the anchor must not be an element tip, or the first element would be handed to a \
         bare call and the leading transposition would be invisible here too"
    );

    let orderings = permutations(&ELEMENTS);
    let distinct: HashSet<Distinction> =
        orderings.iter().map(|bytes| fold.fold(anchor, bytes, &engine)).collect();

    eprintln!("anchored fold: {} orderings give {} identifiers", orderings.len(), distinct.len());
    assert_eq!(
        distinct.len(),
        orderings.len(),
        "the anchored fold collapsed {} orderings — sequence identity is not preserved, and \
         any consumer relying on byte order is unsound",
        orderings.len() - distinct.len()
    );
}

/// The anchor is what does the work. Folding from an element tip instead of a
/// proper anchor should reintroduce exactly the flat fold's blind spot —
/// which is why the fold rejects a tip as an anchor rather than merely
/// discouraging it.
#[test]
fn the_anchor_is_load_bearing_not_decorative() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let anchor = domain_anchor(&engine, &fold, 0x01);

    // Two sequences differing only in their leading pair.
    let forward = fold.fold(anchor, &[0x41, 0x42, 0x43], &engine);
    let swapped = fold.fold(anchor, &[0x42, 0x41, 0x43], &engine);
    assert_ne!(
        forward, swapped,
        "the anchored fold lost the leading transposition — the anchor is not breaking the \
         symmetry it exists to break"
    );

    // And length is carried, so a prefix is not a sequence.
    let prefix = fold.fold(anchor, &[0x41, 0x42], &engine);
    let longer = fold.fold(anchor, &[0x41, 0x42, 0x00], &engine);
    assert_ne!(
        prefix, longer,
        "a sequence and its extension collided — length is not folded in, so trailing zero \
         bytes would be absorbed"
    );
}
