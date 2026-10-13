//! A batch lift over borrows must *compile*. TST-7's pass case.
//!
//! The trait defect this guards was expressible, not runtime. With
//! `to_canonical_structure(self, &DistinctionEngine)` — taking `self` by
//! value and building its own `ByteFold` — the loop below does not compile
//! at all: `for record in &batch` yields `&Record`, and a by-value method
//! cannot be called on a shared borrow without cloning. That is why the
//! only real consumer stopped using the trait and wrote its own
//! reference-taking helper, and why nothing noticed: a compile-fail probe
//! asserts the absence of a path, and no probe asserted the presence of
//! this one.
//!
//! So this is a `t.pass(...)` case. It fails by not compiling, which is
//! exactly how the defect presented.

use koru_lambda_core::primitives::{domain_anchor, ByteFold, Canonicalizable, DomainLabel};
use koru_lambda_core::{Distinction, DistinctionEngine};

struct Record {
    tag: Vec<u8>,
    payload: Vec<u8>,
}

impl Canonicalizable for Record {
    fn to_canonical_structure(&self, fold: &ByteFold, engine: &DistinctionEngine) -> Distinction {
        let anchor = domain_anchor(engine, fold, DomainLabel::new(0x20));
        let parts =
            [fold.fold(anchor, &self.tag, engine), fold.fold(anchor, &self.payload, engine)];
        fold.fold_distinctions(anchor, &parts, engine)
    }
}

fn main() {
    let engine = DistinctionEngine::new();
    let batch = vec![
        Record { tag: b"a".to_vec(), payload: b"one".to_vec() },
        Record { tag: b"b".to_vec(), payload: b"two".to_vec() },
    ];

    // One alphabet for the whole batch: the table is built once here, not
    // once per record. A signature without this parameter forces the second.
    let fold = ByteFold::new(&engine);

    // Borrowed, not consumed. A by-value signature rejects this loop.
    let roots: Vec<Distinction> =
        batch.iter().map(|record| record.to_canonical_structure(&fold, &engine)).collect();

    assert_eq!(roots.len(), 2);

    // And the trait is usable as a bound, which is what makes it a trait
    // rather than a convention.
    fn lift_all<C: Canonicalizable>(
        items: &[C],
        fold: &ByteFold,
        engine: &DistinctionEngine,
    ) -> Vec<Distinction> {
        items.iter().map(|i| i.to_canonical_structure(fold, engine)).collect()
    }
    assert_eq!(lift_all(&batch, &fold, &engine), roots);
}
