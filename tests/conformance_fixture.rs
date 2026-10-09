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

use koru_lambda_core::projection::{Adjacency, Direction};
use koru_lambda_core::{vectors, ByteFold, DistinctionEngine, RawDistinctionId};
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

    // The projection wire form.
    let proj_case = case(&f, "/projections/cases", "adjacency_upstream_hops2");
    let root = e
        .verify(
            RawDistinctionId::from_hex(proj_case["root"].as_str().expect("root"))
                .expect("fixture root is valid hex"),
        )
        .expect("the engine witnessed the fixture's projection root");
    let hops = usize::try_from(
        proj_case.pointer("/boundary/value").and_then(Value::as_u64).expect("boundary value"),
    )
    .expect("hops fits usize");
    let materialized =
        e.project(root).direction(Direction::Upstream).hops(hops).signal(Adjacency).materialize();
    assert_eq!(
        hex::encode(materialized.canonical_bytes()),
        proj_case["canonical_bytes"].as_str().expect("canonical_bytes"),
        "a live engine disagrees with the fixture on the canonical wire form"
    );
}
