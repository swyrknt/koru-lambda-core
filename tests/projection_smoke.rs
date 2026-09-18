//! v2.0.0 projection API smoke tests — construction, serialize/deserialize
//! round-trip, and coerce-site checks.
//!
//! Broader Cond A-F falsifiers ship elsewhere in the test tree. These
//! are the bring-up-time verifications (coerce-site test and basic
//! wire-format round-trip).

use std::sync::Arc;

use koru_lambda_core::projection::{Direction, ReadOnlyEngine, RestoreError};
use koru_lambda_core::{Adjacency, Degree, DistinctionEngine};

#[test]
fn basic_projection_construction() {
    let engine = DistinctionEngine::new();
    let child = engine.synthesize(engine.d0(), engine.d1());
    let proj = engine
        .project(child)
        .direction(Direction::Upstream)
        .hops(2)
        .signal(Adjacency)
        .materialize();

    assert!(proj.contains(&child));
    assert!(proj.contains(&engine.d0()));
    assert!(proj.contains(&engine.d1()));
}

#[test]
fn projection_id_stable_across_engines() {
    let e1 = DistinctionEngine::new();
    let e2 = DistinctionEngine::new();
    let c1 = e1.synthesize(e1.d0(), e1.d1());
    let c2 = e2.synthesize(e2.d0(), e2.d1());

    let p1 = e1.project(c1).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
    let p2 = e2.project(c2).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();

    assert_eq!(p1.projection_id(), p2.projection_id());
}

#[test]
fn canonical_bytes_stable_across_engines() {
    let e1 = DistinctionEngine::new();
    let e2 = DistinctionEngine::new();
    let c1 = e1.synthesize(e1.d0(), e1.d1());
    let c2 = e2.synthesize(e2.d0(), e2.d1());

    let p1 = e1.project(c1).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
    let p2 = e2.project(c2).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();

    assert_eq!(p1.canonical_bytes(), p2.canonical_bytes());
}

#[test]
fn wire_format_round_trip_adjacency() {
    let engine = DistinctionEngine::new();
    let c = engine.synthesize(engine.d0(), engine.d1());
    let proj =
        engine.project(c).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();

    let bytes = proj.canonical_bytes();
    let restored = engine
        .restore_projection::<Adjacency>(&bytes, Adjacency)
        .expect("restore succeeds against same-history engine");

    assert_eq!(restored.projection_id(), proj.projection_id());
    assert_eq!(restored.canonical_bytes(), bytes);
}

#[test]
fn wire_format_round_trip_degree() {
    let engine = DistinctionEngine::new();
    let c = engine.synthesize(engine.d0(), engine.d1());
    let proj =
        engine.project(c).direction(Direction::Undirected).saturated().signal(Degree).materialize();

    let bytes = proj.canonical_bytes();
    let restored = engine
        .restore_projection::<Degree>(&bytes, Degree)
        .expect("restore succeeds against same-history engine");

    assert_eq!(restored.canonical_bytes(), bytes);
}

#[test]
fn restore_wrong_signal_returns_signal_mismatch() {
    let engine = DistinctionEngine::new();
    let c = engine.synthesize(engine.d0(), engine.d1());
    let proj =
        engine.project(c).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();

    let bytes = proj.canonical_bytes();
    let err = engine
        .restore_projection::<Degree>(&bytes, Degree)
        .expect_err("restore under wrong signal must fail");
    assert!(matches!(err, RestoreError::SignalMismatch { .. }));
}

#[test]
fn project_arc_coerce_site() {
    // PLAN_V2 item 17: verify that `Projection<'a, Adjacency>` is
    // constructible from `&'a Arc<DistinctionEngine>` via the
    // `project_arc` entry point. Doubly-fat `&(dyn ReadOnlyEngine +
    // Send + Sync)` must coerce cleanly.
    let engine = Arc::new(DistinctionEngine::new());
    let child = engine.synthesize(engine.d0(), engine.d1());
    let proj = engine
        .project_arc(child)
        .direction(Direction::Upstream)
        .hops(1)
        .signal(Adjacency)
        .materialize();

    assert!(proj.contains(&child));
}

#[test]
fn dyn_readonly_engine_dispatches() {
    let engine = DistinctionEngine::new();
    let dyn_engine: &(dyn ReadOnlyEngine + Send + Sync) = &engine;
    assert_eq!(dyn_engine.distinction_count(), 2);
    assert!(dyn_engine.has(engine.d0()));
    assert!(dyn_engine.parents_of(engine.d0()).is_none());
    assert_eq!(dyn_engine.degree(engine.d0()), 1);
}
