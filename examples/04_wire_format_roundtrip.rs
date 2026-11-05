//! Tier 4 — wire_format_roundtrip. Content addressing at three scales,
//! and the hand-off that makes the convergence claim a cross-process one.
//!
//! 1. Distinction-level: `to_hex` → wire → `from_hex` → `verify` →
//!    identical bytes back inside the trust boundary.
//! 2. Projection-level (Laws 8 and 9, live): the **same parent pairs in a
//!    different order** produce byte-identical `canonical_bytes` and an
//!    identical `ProjectionId`. Replaying one fixed order on two engines
//!    would only show Axiom 1 (determinism); the reordering is what makes
//!    this the order-independence claim.
//! 3. Tamper rejection at the wire boundary — including the one tamper that
//!    is *accepted*, and what that costs.
//!
//! It then writes `examples/wasm-demos/handoff/history.json` so that
//! `npm run demo:4` can replay this history in a separate Node process and
//! reproduce these bytes. Both sides are this crate — one compiled native,
//! one to `wasm32` — so this is a cross-process witness of an artifact, not
//! an independent implementation. DEMO-5 is the item that would be.
//!
//! ```text
//! cargo run --example 04_wire_format_roundtrip
//! ```

use koru_lambda_core::projection::{Adjacency, Direction};
use koru_lambda_core::{Distinction, DistinctionEngine, RawDistinctionId};
use std::collections::HashSet;

/// One synthesis request: the two parents, in the order they were supplied.
type Pair = (Distinction, Distinction);

/// Deterministic index pairs, so the demo prints the same bytes every run.
///
/// A small LCG rather than `rand`: the corpus is the demonstration, and a
/// dependency whose stream could change under it would change the printed
/// output without any change here. Same generator as
/// `tests/order_independence.rs`.
fn seeded_pairs(seed: u64, count: usize) -> Vec<(usize, usize)> {
    let mut state = seed;
    let mut next = || {
        state =
            state.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    (0..count).map(|_| (next(), next())).collect()
}

/// Synthesize a branching history. Returns the parent pairs in the order they
/// were supplied, and the last child, which the projection below is rooted at.
fn build_history(engine: &DistinctionEngine, pairs: &[(usize, usize)]) -> (Vec<Pair>, Distinction) {
    let mut chain = vec![engine.d0(), engine.d1()];
    let mut history = Vec::with_capacity(pairs.len());
    for &(a, b) in pairs {
        let left = chain[a % chain.len()];
        let right = chain[b % chain.len()];
        history.push((left, right));
        chain.push(engine.synthesize(left, right));
    }
    let last = *chain.last().expect("chain always holds the primordials");
    (history, last)
}

/// Produce a different topological order of the same parent pairs.
///
/// `synthesize` rejects a parent the engine has not witnessed, so a literal
/// shuffle is not a legal input — the only reorderings that exist are other
/// topological orders of the same DAG. This takes the *last* eligible pair at
/// each step, which maximises divergence from the original sequence.
fn reorder(history: &[Pair], d0: Distinction, d1: Distinction) -> Vec<Pair> {
    let mut witnessed: HashSet<Distinction> = HashSet::from([d0, d1]);
    let mut pending: Vec<Option<Pair>> = history.iter().copied().map(Some).collect();
    let mut out = Vec::with_capacity(history.len());

    while out.len() < history.len() {
        let mut progressed = false;
        for slot in pending.iter_mut().rev() {
            let Some((left, right)) = *slot else { continue };
            if witnessed.contains(&left) && witnessed.contains(&right) {
                *slot = None;
                out.push((left, right));
                progressed = true;
                break;
            }
        }
        if !progressed {
            // The remaining pairs need children not yet produced. Replay the
            // consumed prefix through a scratch engine to learn the new
            // witnessed set, then continue.
            let scratch = DistinctionEngine::new();
            for &(left, right) in &out {
                witnessed.insert(scratch.synthesize(left, right));
            }
            assert!(
                pending
                    .iter()
                    .flatten()
                    .any(|(l, r)| witnessed.contains(l) && witnessed.contains(r)),
                "reorder: {} pairs remain and none are eligible — history is not a DAG",
                pending.iter().flatten().count()
            );
        }
    }
    out
}

/// The one spec both engines are queried with. A free function rather than a
/// closure: the returned `Projection` borrows the engine, and a closure
/// cannot name that lifetime.
fn project(
    engine: &DistinctionEngine,
    root: Distinction,
) -> koru_lambda_core::projection::Projection<'_, Adjacency> {
    engine.project(root).direction(Direction::Undirected).hops(2).signal(Adjacency).materialize()
}

/// Replay parent pairs into an engine in the order given, returning the
/// children produced. The caller compares the set against the original
/// engine's: a reorder that silently dropped a pair would otherwise show up
/// only as a byte difference, which reads as a failure of the law rather
/// than of the test.
fn replay(engine: &DistinctionEngine, history: &[Pair]) -> HashSet<Distinction> {
    history.iter().map(|&(left, right)| engine.synthesize(left, right)).collect()
}

/// `docs/SPEC.md` §10.1's history. Used for the tamper section because the
/// offsets and outcomes below are the ones pinned in
/// `tests/projection_wire_tamper.rs` against exactly this cone.
fn chain_history(engine: &DistinctionEngine) -> Distinction {
    let c = engine.synthesize(engine.d0(), engine.d1());
    let d = engine.synthesize(c, engine.d0());
    // Third node. Named, not discarded: it is downstream of `c` and so adds
    // no member to an Upstream cone, but it is what gives the graph the
    // diameter the boundary tamper below turns on.
    let _e = engine.synthesize(d, engine.d1());
    c
}

fn main() {
    // ---- (1) distinction wire round-trip ------------------------------
    let engine = DistinctionEngine::new();
    let child = chain_history(&engine);

    let wire = child.to_hex();
    let raw = RawDistinctionId::from_hex(&wire).expect("hex parses");
    let verified = engine.verify(raw).expect("engine has these bytes");

    assert_eq!(verified.as_bytes(), child.as_bytes());
    println!("distinction wire round-trip:");
    println!("  child  = {wire}");
    println!("  verify(from_hex(wire)) == child  ✓");
    println!();

    // ---- (2) order independence — same pairs, different order ---------
    let engine_a = DistinctionEngine::new();
    let (history, root) = build_history(&engine_a, &seeded_pairs(20_260_101, 24));

    let shuffled = reorder(&history, engine_a.d0(), engine_a.d1());
    assert_ne!(history, shuffled, "the reordering degenerated to the original order");
    let moved = history.iter().zip(&shuffled).filter(|(a, b)| a != b).count();

    let engine_b = DistinctionEngine::new();
    let children_b = replay(&engine_b, &shuffled);
    let children_a: HashSet<Distinction> =
        history.iter().map(|&(l, r)| engine_a.synthesize(l, r)).collect();
    assert_eq!(children_a, children_b, "the reorder did not replay the same pairs");

    let proj_a = project(&engine_a, root);
    let proj_b = project(&engine_b, root);

    let bytes_a = proj_a.canonical_bytes();
    let bytes_b = proj_b.canonical_bytes();
    let id_a = proj_a.projection_id();

    assert_eq!(bytes_a, bytes_b, "Laws 8/9 violated: canonical bytes diverged under reorder");
    assert_eq!(
        id_a.as_bytes(),
        proj_b.projection_id().as_bytes(),
        "projection id diverged, which would mean the two engines asked different questions"
    );

    println!("order independence ({} pairs, {moved} of them in a new position):", history.len());
    println!("  root  = {}", root.to_hex());
    println!("  axis  = direction=Undirected, boundary=Hops(2), signal=Adjacency");
    println!("  entries                            = {}", proj_a.output().len());
    println!("  canonical_bytes.len                = {}", bytes_a.len());
    println!("  bytewise original == reordered     ✓");
    println!("  projection_id matches              ✓  (necessary, not sufficient — it");
    println!("                                         hashes the axes, not the output)");
    println!();

    // ---- (3) tamper at the wire boundary ------------------------------
    //
    // The spec history, so these are the offsets and outcomes pinned in
    // `tests/projection_wire_tamper.rs`. The 55-byte header puts the first
    // entry's identifier at 55..71 and its u32 output length at 71..75.
    let engine_c = DistinctionEngine::new();
    let c = chain_history(&engine_c);
    let sent =
        engine_c.project(c).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
    let clean = sent.canonical_bytes();

    let restored = engine_c.restore_projection(&clean, Adjacency).expect("clean bytes restore");
    assert_eq!(restored.projection_id().as_bytes(), sent.projection_id().as_bytes());

    let mut bad_id = clean.clone();
    bad_id[63] = bad_id[63].wrapping_add(1);
    let mut bad_len = clean.clone();
    bad_len[71] = bad_len[71].wrapping_add(1);
    let mut bad_boundary = clean.clone();
    bad_boundary[22] = 0x03; // Hops(2) -> Hops(3)

    assert!(engine_c.restore_projection(&bad_id, Adjacency).is_err());
    assert!(engine_c.restore_projection(&bad_len, Adjacency).is_err());
    let accepted = engine_c
        .restore_projection(&bad_boundary, Adjacency)
        .expect("a boundary that does not change the cone re-materializes identically");

    println!("tamper rejection (restore_projection):");
    println!("  clean bytes                        restored ✓");
    println!("  entry identifier, byte 63          refused  ✓");
    println!("  declared output length, byte 71    refused  ✓");
    println!("  boundary payload, byte 22          ACCEPTED");
    println!();
    println!("  The last one is not a hole in the restore, and it is not hidden here.");
    println!("  Rooted at c this graph has diameter 2, so Hops(2) and Hops(3) enumerate");
    println!("  the same entries: the tampered bytes are bytes this engine would have");
    println!("  produced, and the guarantee is exactly that unproducible bytes are");
    println!("  refused. What it costs is agreement about which read this is —");
    println!("  the restored projection's id differs from the sender's:");
    println!("    sender   = {}", hex::encode(sent.projection_id().as_bytes()));
    println!("    restored = {}", hex::encode(accepted.projection_id().as_bytes()));
    println!("  Same bytes, different question. Compare the id out of band.");
    println!();

    // ---- (4) hand-off to a separate process ---------------------------
    //
    // The artifact tells another process that one of these payloads must be
    // refused. Assert it here first: an artifact carrying an unverified
    // "this must fail" is a claim the reader cannot check without trusting it.
    let mut handoff_tampered = bytes_a.clone();
    handoff_tampered[63] = handoff_tampered[63].wrapping_add(1);
    assert!(
        engine_a.restore_projection(&bytes_a, Adjacency).is_ok(),
        "the bytes handed off must restore in the engine that produced them"
    );
    assert!(
        engine_a.restore_projection(&handoff_tampered, Adjacency).is_err(),
        "the tampered bytes handed off must be refused here before Node is asked to refuse them"
    );

    let path = write_handoff(&history, root, &bytes_a, id_a.as_bytes(), &handoff_tampered);
    println!("hand-off artifact written:");
    println!("  {path}");
    println!("  `cd examples/wasm-demos && npm run demo:4` replays this history in a");
    println!("  separate Node process, in an order it computes itself, and reproduces");
    println!("  the projection id above.");
    println!();
    println!("projection id (section 2) = {}", hex::encode(id_a.as_bytes()));
}

/// Write the history, the spec and the expected bytes where the Node demo
/// reads them. Returns the path for printing.
fn write_handoff(
    history: &[Pair],
    root: Distinction,
    canonical_bytes: &[u8],
    projection_id: &[u8],
    tampered: &[u8],
) -> String {
    use std::path::PathBuf;

    let pairs: Vec<[String; 2]> = history.iter().map(|(l, r)| [l.to_hex(), r.to_hex()]).collect();

    let doc = serde_json::json!({
        "note": concat!(
            "Written by `cargo run --example 04_wire_format_roundtrip`. ",
            "Both producer and consumer are this crate — one compiled native, one to ",
            "wasm32 — so reproducing these bytes is a cross-process witness of this ",
            "artifact, not an independent implementation of docs/SPEC.md."
        ),
        "spec": {
            "root": root.to_hex(),
            "direction": "undirected",
            "hops": 2,
            "signal": "adjacency",
        },
        "pairs": pairs,
        "expected_projection_id": hex::encode(projection_id),
        "expected_canonical_bytes": hex::encode(canonical_bytes),
        "tampered_canonical_bytes": hex::encode(tampered),
    });

    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/wasm-demos/handoff");
    std::fs::create_dir_all(&dir).expect("create handoff directory");
    let path = dir.join("history.json");
    std::fs::write(&path, format!("{}\n", serde_json::to_string_pretty(&doc).expect("serialize")))
        .expect("write handoff artifact");
    path.display().to_string()
}
