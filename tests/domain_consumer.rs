//! A domain type that lifts itself, and the collision the composite rule closes.
//!
//! TST-7 and CONF-10. Two separate gaps with one cause: nothing in this
//! repository lifts a *domain type*. The substrate's only `Canonicalizable`
//! impl is `u8`, its own machinery, and the two real impls live in
//! `koru-lambda-agents` — the crate that leaves. The three tests that touch
//! `ByteFold` are substrate self-tests, folding bytes to check the fold's
//! properties, not consumers choosing how their own data decomposes.
//!
//! TST-7 is specific about why an example is not enough: the trait defect
//! was found because a real consumer folds *N* values in a loop and the
//! per-call table cost became visible enough to route around. A forty-line
//! example folds once and the cost stays invisible. So this is an
//! integration test with a batch loop.
//!
//! # The shape that discriminates
//!
//! `Record` carries **two** variable-length fields, and that is deliberate.
//! With a single variable-length field at the end, byte concatenation is
//! already injective — the fixed-width prefix makes the boundary
//! unambiguous, which is the reasoning the agents crate relied on for
//! `nonce ‖ data`. Two of them is where naive concatenation fails:
//!
//! ```text
//!   { tag: [01],    payload: [02, 03] }  ->  01 02 03
//!   { tag: [01, 02], payload: [03]    }  ->  01 02 03
//! ```
//!
//! Same bytes, different values. That is the collision class `ByteFold`
//! closed for sequences, relocated one level up — and `docs/SPEC.md` §4.4 is
//! what closes it again, by folding each part's own tip with the part count
//! committed first.

use koru_lambda_core::primitives::{domain_anchor, ByteFold, Canonicalizable, DomainLabel};
use koru_lambda_core::{Distinction, DistinctionEngine};

/// This consumer's domain. Chosen by the type, not by its caller — §4.2's
/// separation holds between *types*, so a record and a bare byte sequence
/// carrying the same bytes must not fold alike.
const DOMAIN_RECORD: DomainLabel = DomainLabel::new(0x20);

/// A record with one fixed-width field and two variable-length ones.
#[derive(Clone, PartialEq, Eq, Debug)]
struct Record {
    seq: u64,
    tag: Vec<u8>,
    payload: Vec<u8>,
}

impl Record {
    fn new(seq: u64, tag: &[u8], payload: &[u8]) -> Self {
        Self { seq, tag: tag.to_vec(), payload: payload.to_vec() }
    }

    /// What a naive implementation would fold: the fields run together.
    fn concatenated(&self) -> Vec<u8> {
        let mut v = self.seq.to_le_bytes().to_vec();
        v.extend_from_slice(&self.tag);
        v.extend_from_slice(&self.payload);
        v
    }
}

impl Canonicalizable for Record {
    /// Three parts, folded in declared order per §4.4.
    ///
    /// Each part is lifted on its own — so each carries its own length — and
    /// the composite commits the part count before any of them. Nesting is
    /// free: a part that were itself a record would fold to a distinction
    /// and become an element here with no further rule.
    fn to_canonical_structure(&self, fold: &ByteFold, engine: &DistinctionEngine) -> Distinction {
        let anchor = domain_anchor(engine, fold, DOMAIN_RECORD);
        let parts = [
            fold.fold(anchor, &self.seq.to_le_bytes(), engine),
            fold.fold(anchor, &self.tag, engine),
            fold.fold(anchor, &self.payload, engine),
        ];
        fold.fold_distinctions(anchor, &parts, engine)
    }
}

/// Records whose bytes run together identically lift to different values.
///
/// The case the composite rule exists for. Both records concatenate to the
/// same nine bytes; a consumer that byte-folded `concatenated()` would give
/// them one identity.
#[test]
fn two_records_with_identical_bytes_do_not_collide() {
    let e = DistinctionEngine::new();
    let f = ByteFold::new(&e);

    let a = Record::new(1, &[0x01], &[0x02, 0x03]);
    let b = Record::new(1, &[0x01, 0x02], &[0x03]);
    assert_eq!(a.concatenated(), b.concatenated(), "the premise: the bytes are identical");
    assert_ne!(a, b, "and the values are not");

    assert_ne!(
        a.to_canonical_structure(&f, &e),
        b.to_canonical_structure(&f, &e),
        "the composite rule must separate what concatenation merges"
    );
}

/// Field order is part of the value.
#[test]
fn swapping_two_fields_changes_the_identity() {
    let e = DistinctionEngine::new();
    let f = ByteFold::new(&e);

    let a = Record::new(7, &[0xAA], &[0xBB]);
    let b = Record::new(7, &[0xBB], &[0xAA]);

    assert_ne!(a.to_canonical_structure(&f, &e), b.to_canonical_structure(&f, &e));
}

/// An empty variable-length field is a value, not an absence.
#[test]
fn an_empty_field_is_distinguishable() {
    let e = DistinctionEngine::new();
    let f = ByteFold::new(&e);

    let empty_tag = Record::new(1, &[], &[0x09]);
    let empty_payload = Record::new(1, &[0x09], &[]);
    let both_empty = Record::new(1, &[], &[]);

    let lift = |r: &Record| r.to_canonical_structure(&f, &e);
    assert_ne!(lift(&empty_tag), lift(&empty_payload));
    assert_ne!(lift(&empty_tag), lift(&both_empty));
    assert_ne!(lift(&empty_payload), lift(&both_empty));
}

/// The same record lifts to the same distinction on a different engine.
///
/// Law 8 at the consumer level: a record's identity is a function of the
/// record, not of who witnessed it.
#[test]
fn a_record_lifts_the_same_on_any_engine() {
    let r = Record::new(0xDEAD_BEEF, b"metric", b"a longer variable payload");

    let e1 = DistinctionEngine::new();
    let f1 = ByteFold::new(&e1);
    let e2 = DistinctionEngine::new();
    let f2 = ByteFold::new(&e2);

    assert_eq!(r.to_canonical_structure(&f1, &e1), r.to_canonical_structure(&f2, &e2));
}

/// A batch loop over borrows, sharing one `ByteFold`.
///
/// This is the pressure TST-7 asks for and the reason the trait signature is
/// what it is. The impl takes `&self`, so the loop folds without cloning;
/// and it takes the prebuilt `fold`, so the 256-tip table is paid for once
/// across the batch instead of once per record. Building it per call is what
/// drove the only real consumer to route around the trait entirely.
#[test]
fn a_batch_shares_one_alphabet_and_borrows_its_records() {
    let e = DistinctionEngine::new();

    let batch: Vec<Record> = (0..64u64)
        .map(|i| Record::new(i, &[(i % 251) as u8], &vec![0xC0u8; (i % 7) as usize]))
        .collect();

    // Paid once, for the whole batch.
    let fold = ByteFold::new(&e);
    let after_alphabet = e.distinction_count();

    // `&Record`, not `Record` — the loop never clones.
    let mut roots: Vec<Distinction> = Vec::with_capacity(batch.len());
    for record in &batch {
        roots.push(record.to_canonical_structure(&fold, &e));
    }

    assert_eq!(roots.len(), 64);
    assert!(e.distinction_count() > after_alphabet, "folding records grows the engine");
    e.check_structural_invariant().expect("the batch leaves a well-formed graph (invariant)");

    // Distinct records, distinct roots — 64 of them, no collisions.
    let unique: std::collections::BTreeSet<Distinction> = roots.iter().copied().collect();
    assert_eq!(unique.len(), roots.len(), "every distinct record has its own identity");

    // And the batch is reproducible: a second pass over the same engine is
    // all saturating, so it adds nothing.
    let count_before = e.distinction_count();
    let second: Vec<Distinction> =
        batch.iter().map(|r| r.to_canonical_structure(&fold, &e)).collect();
    assert_eq!(second, roots, "lifting is deterministic");
    assert_eq!(e.distinction_count(), count_before, "and a repeat witnesses nothing new");
}

/// A record's domain separates it from the same bytes folded plainly.
#[test]
fn a_records_domain_separates_it_from_a_bare_sequence() {
    let e = DistinctionEngine::new();
    let f = ByteFold::new(&e);

    let r = Record::new(0, &[], &[0x11, 0x22]);
    let bare_anchor = domain_anchor(&e, &f, DomainLabel::new(0x21));

    assert_ne!(
        r.to_canonical_structure(&f, &e),
        f.fold(bare_anchor, &r.concatenated(), &e),
        "a record and a byte sequence must not share an identity"
    );
}
