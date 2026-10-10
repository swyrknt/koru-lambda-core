//! `conformance/vectors.json` is the source; `src/vectors.rs` is the mirror.
//!
//! The published vectors used to live only inside the crate they pin, which
//! meant they certified internal consistency and nothing else: one commit could
//! change `synth_bytes` and regenerate the constants together, and every gate
//! stayed green. The API snapshot could not catch it either — it records
//! `pub const C_HEX: &str`, the *type*, not the value.
//!
//! So the vectors move out of tree, and this test is the tie. It asserts three
//! separate things, and the third is the one that matters:
//!
//! 1. Every constant in `src/vectors.rs` equals the corresponding value in the
//!    fixture. Two sources of truth for the same bytes is the drift content
//!    addressing exists to prevent.
//! 2. The fixture's `frozen_for_major` matches this crate's major version, so a
//!    fixture carried forward from a previous major cannot be mistaken for this
//!    one's.
//! 3. **The fixture's values are what a live engine actually produces.** A
//!    fixture that agreed with the mirror but not with the engine would be two
//!    files agreeing with each other about a lie.

use koru_lambda_core::projection::{
    Adjacency, CanonicalBytes, Degree, Direction, HopDistance, RestoreError, Signal,
};
use koru_lambda_core::{
    domain_anchor, vectors, ByteFold, Distinction, DistinctionEngine, DomainLabel, RawDistinctionId,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/conformance/vectors.json");
    let raw = std::fs::read_to_string(path).unwrap_or_else(|e| {
        panic!(
            "conformance/vectors.json is the source of the published vectors and must exist: {e}"
        )
    });
    serde_json::from_str(&raw).expect("conformance/vectors.json is valid JSON")
}

fn hex_at(v: &Value, pointer: &str) -> String {
    v.pointer(pointer)
        .unwrap_or_else(|| panic!("fixture has no value at {pointer}"))
        .as_str()
        .unwrap_or_else(|| panic!("fixture value at {pointer} is not a string"))
        .to_owned()
}

/// Find a named case inside an array, by its `name` field.
fn case<'a>(v: &'a Value, pointer: &str, name: &str) -> &'a Value {
    v.pointer(pointer)
        .and_then(Value::as_array)
        .unwrap_or_else(|| panic!("fixture has no array at {pointer}"))
        .iter()
        .find(|c| c.get("name").and_then(Value::as_str) == Some(name))
        .unwrap_or_else(|| panic!("fixture has no case named {name} at {pointer}"))
}

#[test]
fn the_mirror_matches_the_fixture() {
    let f = fixture();

    assert_eq!(case(&f, "/operator/cases", "c")["id"].as_str(), Some(vectors::C_HEX));
    assert_eq!(case(&f, "/operator/cases", "d")["id"].as_str(), Some(vectors::D_HEX));
    assert_eq!(case(&f, "/operator/cases", "e")["id"].as_str(), Some(vectors::E_HEX));

    let tips = f.pointer("/lift/tips").and_then(Value::as_array).expect("lift tips");
    let tip_of = |byte: u64| -> &str {
        tips.iter()
            .find(|t| t["byte"].as_u64() == Some(byte))
            .unwrap_or_else(|| panic!("fixture has no tip for byte {byte}"))["id"]
            .as_str()
            .expect("tip id is a string")
    };
    assert_eq!(tip_of(0x00), vectors::TIP_00_HEX);
    assert_eq!(tip_of(0x01), vectors::TIP_01_HEX);
    assert_eq!(tip_of(0x41), vectors::TIP_41_HEX);
    assert_eq!(tip_of(0xFF), vectors::TIP_FF_HEX);

    assert_eq!(
        case(&f, "/lift/folds", "fold_hi_anchored_at_d")["id"].as_str(),
        Some(vectors::FOLD_HI_ANCHORED_AT_D_HEX)
    );

    assert_eq!(
        case(&f, "/projections/cases", "adjacency_upstream_hops2")["canonical_bytes"].as_str(),
        Some(vectors::ADJACENCY_UPSTREAM_HOPS2_CANONICAL_HEX),
        "the mirror and the fixture disagree on the canonical wire form"
    );
}

#[test]
fn the_fixture_declares_this_major_version() {
    let f = fixture();
    let declared = f["frozen_for_major"].as_u64().expect("frozen_for_major is an integer");
    let major: u64 = env!("CARGO_PKG_VERSION_MAJOR").parse().expect("crate major version");
    assert_eq!(
        declared, major,
        "the fixture is frozen for major {declared} but this crate is {major}. Identity is \
         pinned per major version, so a fixture from another major must not be read as this \
         one's."
    );
}

/// The assertion that makes the other two mean something: the fixture agrees
/// with a live engine, not merely with the mirror.
#[test]
fn a_live_engine_reproduces_the_fixture() {
    let f = fixture();
    let e = DistinctionEngine::new();

    assert_eq!(hex::encode(e.d0().as_bytes()), hex_at(&f, "/primordials/d0"));
    assert_eq!(hex::encode(e.d1().as_bytes()), hex_at(&f, "/primordials/d1"));

    // Operator cases, derived in the order the fixture declares them.
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let e_child = e.synthesize(d, e.d1());
    for (name, value) in [("c", c), ("d", d), ("e", e_child)] {
        assert_eq!(
            hex::encode(value.as_bytes()),
            case(&f, "/operator/cases", name)["id"].as_str().expect("id"),
            "a live engine disagrees with the fixture on operator case {name}"
        );
    }

    // The lift: tips and the anchored fold.
    let fold = ByteFold::new(&e);
    for tip in f.pointer("/lift/tips").and_then(Value::as_array).expect("tips") {
        let byte = u8::try_from(tip["byte"].as_u64().expect("byte")).expect("byte fits u8");
        assert_eq!(
            hex::encode(fold.tip(byte).as_bytes()),
            tip["id"].as_str().expect("id"),
            "a live engine disagrees with the fixture on the tip for byte {byte:#04x}"
        );
    }
    let fold_case = case(&f, "/lift/folds", "fold_hi_anchored_at_d");
    let anchor_hex = fold_case["anchor"].as_str().expect("anchor");
    // The fixture is a file: its bytes arrive from outside, so they cross
    // the trust boundary like anything else. `verify` also checks the engine
    // actually witnessed this anchor, which a bare parse would not.
    let anchor = e
        .verify(RawDistinctionId::from_hex(anchor_hex).expect("fixture anchor is valid hex"))
        .expect("the engine witnessed the fixture's anchor");
    let bytes: Vec<u8> = fold_case["bytes"]
        .as_array()
        .expect("fold bytes")
        .iter()
        .map(|b| u8::try_from(b.as_u64().expect("byte")).expect("byte fits u8"))
        .collect();
    assert_eq!(
        hex::encode(fold.fold(anchor, &bytes, &e).as_bytes()),
        fold_case["id"].as_str().expect("id"),
        "a live engine disagrees with the fixture on the anchored fold"
    );

    // Domain anchors. Public, identity-bearing, and unpinned until now: two
    // implementations deriving these differently separate domains
    // differently, and then agree on nothing a consumer folds.
    for anchor_case in
        f.pointer("/lift/domain_anchors/cases").and_then(Value::as_array).expect("anchors")
    {
        let label =
            u8::try_from(anchor_case["label"].as_u64().expect("label")).expect("label fits u8");
        assert_eq!(
            hex::encode(domain_anchor(&e, &fold, DomainLabel::new(label)).as_bytes()),
            anchor_case["id"].as_str().expect("id"),
            "a live engine disagrees with the fixture on the anchor for domain {label:#04x}"
        );
    }

    // Every projection wire form, and every projection identity: all
    // eighteen, three signals by three directions by two boundary kinds.
    //
    // On a FRESH engine carrying exactly the fixture's declared history.
    // Fourteen of the eighteen are not invariant under appending — Degree
    // grows as children appear, and the Downstream and Undirected node sets
    // grow — so they are reproducible only against a stated history. Reusing
    // the engine above, which has 256 byte tips in it from the lift cases,
    // disagrees with the fixture on every Degree form while being entirely
    // correct. That is how this requirement was discovered.
    let pe = DistinctionEngine::new();
    let pc = pe.synthesize(pe.d0(), pe.d1());
    let pd = pe.synthesize(pc, pe.d0());
    let _ = pe.synthesize(pd, pe.d1());

    let cases =
        f.pointer("/projections/cases").and_then(Value::as_array).expect("projection cases");
    assert_eq!(cases.len(), 18, "the fixture should pin the full cross product");
    for proj_case in cases {
        let name = proj_case["name"].as_str().expect("name");
        let root = e
            .verify(
                RawDistinctionId::from_hex(proj_case["root"].as_str().expect("root"))
                    .expect("fixture root is valid hex"),
            )
            .expect("the engine witnessed the fixture's projection root");
        let direction = match proj_case["direction"].as_str() {
            Some("Upstream") => Direction::Upstream,
            Some("Downstream") => Direction::Downstream,
            Some("Undirected") => Direction::Undirected,
            other => panic!("{name}: unknown direction {other:?}"),
        };
        let saturated =
            proj_case.pointer("/boundary/kind").and_then(Value::as_str) == Some("Saturated");
        let hops = usize::try_from(
            proj_case.pointer("/boundary/value").and_then(Value::as_u64).unwrap_or(2),
        )
        .expect("hops fits usize");

        // The signal axis cannot be a runtime value: `Signal` is a trait and
        // its implementors are distinct types, so each arm monomorphises.
        let got = match proj_case["signal"].as_str() {
            Some("Adjacency") => materialize(&pe, root, direction, saturated, hops, Adjacency),
            Some("Degree") => materialize(&pe, root, direction, saturated, hops, Degree),
            Some("HopDistance") => materialize(&pe, root, direction, saturated, hops, HopDistance),
            other => panic!("{name}: unknown signal {other:?}"),
        };

        assert_eq!(
            got.0,
            proj_case["projection_id"].as_str().expect("projection_id"),
            "{name}: a live engine disagrees with the fixture on the projection identity"
        );
        assert_eq!(
            got.1,
            proj_case["canonical_bytes"].as_str().expect("canonical_bytes"),
            "{name}: a live engine disagrees with the fixture on the canonical wire form"
        );
    }
}

/// Materialize one spec and return `(projection_id, canonical_bytes)` as hex.
fn materialize<S: Signal>(
    e: &DistinctionEngine,
    root: Distinction,
    direction: Direction,
    saturated: bool,
    hops: usize,
    signal: S,
) -> (String, String) {
    let b = e.project(root).direction(direction);
    let bounded = if saturated { b.saturated() } else { b.hops(hops) };
    let p = bounded.signal(signal).materialize();
    (hex::encode(p.projection_id().as_bytes()), hex::encode(p.canonical_bytes()))
}

/// The decode direction, which nothing covered.
///
/// Every other projection test encodes and decodes with the same build, so a
/// symmetric loosening of the codec — one that writes and reads the same wrong
/// thing — is invisible. This parses the published hex and asserts the
/// recovered projection re-materialises to it, which is what the
/// specification means by "restoring is verification, not deserialisation".
#[test]
fn the_published_wire_form_decodes() {
    let f = fixture();
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let _ = e.synthesize(d, e.d1());

    let proj_case = case(&f, "/projections/cases", "adjacency_upstream_hops2");
    let wire = hex::decode(proj_case["canonical_bytes"].as_str().expect("canonical_bytes"))
        .expect("fixture wire form is valid hex");

    let restored = e
        .restore_projection(&wire, Adjacency)
        .expect("the engine restores its own published wire form");
    assert_eq!(
        hex::encode(restored.canonical_bytes()),
        proj_case["canonical_bytes"].as_str().expect("canonical_bytes"),
        "restored projection does not re-serialise to the bytes it came from"
    );
    assert_eq!(
        hex::encode(restored.projection_id().as_bytes()),
        proj_case["projection_id"].as_str().expect("projection_id"),
        "restored projection recovered a different spec than the one encoded"
    );
}

/// The trust boundary, as rejection.
///
/// Every other case in the fixture is a value to reproduce, and a fixture of
/// values cannot express a boundary: an implementation that accepts a
/// well-formed forgery reproduces all of them perfectly. These are the valid
/// wire form with one thing wrong each, so a failure localises to the check
/// that is missing rather than to "decoding broke".
#[test]
fn every_rejection_case_is_rejected() {
    let f = fixture();
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let _ = e.synthesize(d, e.d1());

    let cases = f.pointer("/reject/cases").and_then(Value::as_array).expect("reject cases");
    assert!(cases.len() >= 8, "the rejection set should cover every documented offset");

    for rc in cases {
        let name = rc["name"].as_str().expect("name");
        let wire = hex::decode(rc["bytes"].as_str().expect("bytes")).expect("case bytes are hex");
        let want = rc["expect"]["kind"].as_str().expect("expect.kind");

        let err = match e.restore_projection(&wire, Adjacency) {
            Ok(_) => panic!(
                "{name}: ACCEPTED. {}",
                rc["why"].as_str().unwrap_or("this input must be refused")
            ),
            Err(err) => err,
        };

        match want {
            "any_rejection" => {},
            "ForeignEngine" => assert!(
                matches!(err, RestoreError::ForeignEngine),
                "{name}: expected ForeignEngine, got {err}"
            ),
            "Malformed" => {
                let RestoreError::Malformed { offset, reason } = err else {
                    panic!("{name}: expected Malformed, got {err}");
                };
                // Offset and reason are checked only where the fixture fixes
                // them: an implementation may detect a truncation at a
                // different offset and still be correct.
                if let Some(want_offset) = rc["expect"]["offset"].as_u64() {
                    assert_eq!(offset as u64, want_offset, "{name}: rejected at the wrong offset");
                }
                if let Some(want_reason) = rc["expect"]["reason"].as_str() {
                    assert_eq!(reason, want_reason, "{name}: rejected for the wrong reason");
                }
            },
            other => panic!("{name}: fixture asks for unknown error kind {other}"),
        }
    }
}

/// The three byte encodings the substrate publishes for signal outputs,
/// asserted against the live impls.
///
/// These are part of the wire format rather than of any one signal, and
/// `bool` proves why that distinction matters: no shipped signal has
/// `Output = bool`, so the projection cases could never reach its encoding,
/// and an implementation that wrote `true` as `0xFF` would have passed every
/// other case in the fixture. The encoding is still published — a consumer
/// signal with a boolean output serialises through it and a second
/// implementation has to agree.
#[test]
fn the_published_output_encodings_are_what_the_impls_produce() {
    let f = fixture();
    let cases =
        f.pointer("/lift/canonical_bytes/cases").and_then(Value::as_array).expect("encoding cases");

    let e = DistinctionEngine::new();
    let mut checked = 0usize;
    for c in cases {
        let ty = c["type"].as_str().expect("type");
        let want = c["bytes"].as_str().expect("bytes");
        let got = match ty {
            "bool" => c["value"].as_bool().expect("bool value").canonical_bytes(),
            "usize" => {
                let n = usize::try_from(c["value"].as_u64().expect("usize value"))
                    .expect("fixture usize fits this target");
                n.canonical_bytes()
            },
            "Option<ParentPair>" => match c["value"].as_array() {
                None => None.canonical_bytes(),
                Some(pair) => {
                    let mut ds = pair.iter().map(|h| {
                        let raw = RawDistinctionId::from_hex(h.as_str().expect("hex"))
                            .expect("fixture pair is hex");
                        e.verify(raw).expect("fixture pair names a primordial this engine has")
                    });
                    let (min, max) = (ds.next().expect("min"), ds.next().expect("max"));
                    Some((min, max)).canonical_bytes()
                },
            },
            other => panic!("fixture names an output type with no impl to check: {other}"),
        };
        assert_eq!(hex::encode(&got), want, "{ty} {} encodes differently", c["value"]);
        checked += 1;
    }
    assert_eq!(checked, cases.len(), "every encoding case must be checked, not skipped");

    // Cond D width: the usize encoding must not follow the target pointer.
    assert_eq!(usize::MAX.canonical_bytes().len(), 8, "usize encodes as 8 bytes on every target");
}

/// The composite rule, asserted against a live engine.
///
/// Every element in these cases is derived rather than listed — a byte whose
/// tip it is, or an earlier case in the same block — so the fixture states no
/// value twice and this test has to resolve the references to check anything.
/// That is deliberate: the one thing a composite fixture must not do is carry
/// a hand-copied identifier, because then it certifies a transcription.
#[test]
fn the_composite_fold_is_what_a_live_engine_produces() {
    let f = fixture();
    let e = DistinctionEngine::new();
    let c = e.synthesize(e.d0(), e.d1());
    let d = e.synthesize(c, e.d0());
    let _ = e.synthesize(d, e.d1());
    let fold = ByteFold::new(&e);

    let cases =
        f.pointer("/lift/composites/cases").and_then(Value::as_array).expect("composite cases");
    assert!(cases.len() >= 4, "the composite block needs the flat/nested pair and a domain case");

    let mut by_name: std::collections::BTreeMap<String, Distinction> = Default::default();
    for case in cases {
        let name = case["name"].as_str().expect("name").to_owned();

        let anchor = &case["anchor"];
        let anchor = if let Some(h) = anchor.get("case_history").and_then(Value::as_str) {
            assert_eq!(h, "d", "{name}: only `d` is declared as a composite anchor");
            d
        } else if let Some(label) = anchor.get("domain").and_then(Value::as_u64) {
            let label = u8::try_from(label).expect("a domain label is one byte");
            domain_anchor(&e, &fold, DomainLabel::new(label))
        } else {
            panic!("{name}: anchor names neither a history node nor a domain");
        };

        let elements: Vec<Distinction> = case["elements"]
            .as_array()
            .unwrap_or_else(|| panic!("{name}: elements"))
            .iter()
            .map(|el| {
                if let Some(b) = el.get("tip_of_byte").and_then(Value::as_u64) {
                    fold.tip(u8::try_from(b).expect("a byte"))
                } else if let Some(prior) = el.get("case").and_then(Value::as_str) {
                    *by_name.get(prior).unwrap_or_else(|| {
                        panic!("{name}: references case `{prior}`, which is not listed before it")
                    })
                } else {
                    panic!("{name}: an element is neither a byte tip nor an earlier case")
                }
            })
            .collect();

        let got = fold.fold_distinctions(anchor, &elements, &e);
        assert_eq!(
            got.to_hex(),
            case["id"].as_str().expect("id"),
            "{name}: the composite fold disagrees with the fixture"
        );
        by_name.insert(name, got);
    }

    // The point of the pair: same leaves, same order, different grouping.
    let flat = by_name["flat_1_2_3"];
    let nested = by_name["nested_1_2_then_3"];
    assert_ne!(flat, nested, "grouping must be part of the value, or nesting carries nothing");
}
