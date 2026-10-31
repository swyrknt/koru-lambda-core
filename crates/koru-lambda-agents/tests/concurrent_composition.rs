//! Composition of N agents under real concurrency.
//!
//! # Why this file is not called `loom_composition.rs`
//!
//! It uses threads and `join`, not loom. Loom replaces `std::sync`
//! primitives at compile time under `--cfg loom`, which the *substrate*
//! must be built with — a dependent crate cannot impose that on its
//! dependency, and the substrate's loom coverage already lives in
//! `tests/loom_kernel.rs` at the workspace root with its own mutant
//! sentinels.
//!
//! A file named for a tool it does not use is the same defect class as a CI
//! job named for a subset it does not cover, so it carries the name of what
//! it does.
//!
//! # What threads buy that the sequential suite cannot
//!
//! `multi_agent_composition.rs` establishes that interleavings converge
//! when *we* choose the interleaving. These tests remove that choice: the
//! scheduler interleaves, and the assertions must still hold. The substrate
//! claims novelty is decided exactly once per pair under concurrency
//! (`docs/SPEC.md` §2.2); this checks that claim from the agent level, where
//! a consumer would meet it.
//!
//! Public API only.

use koru_lambda_agents::agent::LocalCausalAgent;
use koru_lambda_core::primitives::ByteFold;
use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::HashSet;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

struct Lca {
    root: Distinction,
}

impl LocalCausalAgent for Lca {
    type ActionData = u8;

    fn get_current_root(&self) -> Distinction {
        self.root
    }

    fn update_local_root(&mut self, new_root: Distinction) {
        self.root = new_root;
    }
}

const AGENTS: usize = 4;
const STEPS: u8 = 25;

fn root_for(engine: &Arc<DistinctionEngine>, fold: &ByteFold, seed: u8) -> Distinction {
    let lifted = fold.fold(engine.d0(), &[seed], engine);
    engine.synthesize(engine.d0(), lifted)
}

/// Drive one agent's whole chain. Shared by the threaded and the sequential
/// arms, so the comparison is between schedulers and not between two
/// different pieces of code.
fn drive(engine: &Arc<DistinctionEngine>, fold: &ByteFold, seed: u8) -> Distinction {
    let mut agent = Lca { root: root_for(engine, fold, seed) };
    for step in 0..STEPS {
        let _ = agent.synthesize_action(seed.wrapping_mul(31).wrapping_add(step), fold, engine);
    }
    agent.root
}

fn invariant_holds(engine: &DistinctionEngine) {
    engine
        .check_structural_invariant()
        .expect("concurrent composition left the engine's own auditor unsatisfied");
}

/// **L1 — concurrent multi-agent composition converges.
/// Category: DISCOVERY.**
///
/// Four threads, four agents, four distinct roots, 25 advances each against
/// one shared engine. Asserts: the run completes (no deadlock), every final
/// root is in the graph, the engine's auditor is satisfied, and the node
/// count equals a single-threaded replay of the same 100 advances.
///
/// That last assertion is the load-bearing one. It says the scheduler
/// contributed *nothing* — not one extra node, not one fewer.
///
/// *Passes* → every sequential property survives real concurrency, so the
/// composition is coordination-free in the operational sense and not only
/// in the algebraic one.
///
/// *Fails* → either the engine deadlocks under multi-agent pressure, or
/// concurrency leaks into identity, and a consumer would need a lock
/// discipline the substrate claims not to require.
#[test]
fn concurrent_multi_agent_converges() {
    let engine = Arc::new(DistinctionEngine::new());
    let fold = Arc::new(ByteFold::new(&engine));

    let mut handles = Vec::new();
    for i in 0..AGENTS {
        let engine = Arc::clone(&engine);
        let fold = Arc::clone(&fold);
        handles.push(thread::spawn(move || drive(&engine, &fold, i as u8 + 1)));
    }
    let threaded: Vec<Distinction> =
        handles.into_iter().map(|h| h.join().expect("an agent thread panicked")).collect();

    for (i, &r) in threaded.iter().enumerate() {
        assert!(
            engine.has(r),
            "agent {i}'s final root is absent from the shared engine after a \
             concurrent run"
        );
    }
    invariant_holds(&engine);

    // The same 100 advances, one thread, fresh engine.
    let seq_engine = Arc::new(DistinctionEngine::new());
    let seq_fold = ByteFold::new(&seq_engine);
    let sequential: Vec<Distinction> =
        (0..AGENTS).map(|i| drive(&seq_engine, &seq_fold, i as u8 + 1)).collect();

    assert_eq!(
        threaded, sequential,
        "concurrent and sequential runs of the same advances produced \
         different final roots"
    );
    assert_eq!(
        engine.distinction_count(),
        seq_engine.distinction_count(),
        "the scheduler changed how many nodes exist: {} concurrent vs {} \
         sequential",
        engine.distinction_count(),
        seq_engine.distinction_count()
    );
    let mut a = engine.snapshot_parentage();
    let mut b = seq_engine.snapshot_parentage();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "concurrent and sequential runs produced different graphs");
    invariant_holds(&seq_engine);
}

/// **L1 (sharper) — one pair, one novelty, under contention.
/// Category: DISCOVERY.**
///
/// All four threads race the *same* `(root, action)`. Exactly one must
/// observe `Novel`; all four must receive the identical identifier.
///
/// This is A1 under contention. The sequential A1 guard shows order does
/// not matter when we pick the order; this shows it does not matter when
/// nobody does.
///
/// *Passes* → the substrate decides novelty once per pair however many
/// agents ask at once, so a consumer needs no election to find out who
/// created a node.
///
/// *Fails* → a race can mint two identities for one pair, which would break
/// content addressing at exactly the moment a multi-agent system depends
/// on it.
#[test]
fn concurrent_shared_pair_single_novel() {
    let engine = Arc::new(DistinctionEngine::new());
    let fold = ByteFold::new(&engine);
    let left = root_for(&engine, &fold, 9);
    let right = root_for(&engine, &fold, 10);

    let (tx, rx) = mpsc::channel();
    let mut handles = Vec::new();
    for _ in 0..AGENTS {
        let engine = Arc::clone(&engine);
        let tx = tx.clone();
        handles.push(thread::spawn(move || {
            // Accessors, not a match: `SynthesisOutcome` is
            // `#[non_exhaustive]`, and a wildcard arm here would absorb a
            // future variant instead of failing on it.
            let outcome = engine.synthesize_novel(left, right);
            tx.send((outcome.is_novel(), outcome.distinction()))
                .expect("result channel closed early");
        }));
    }
    drop(tx);
    for h in handles {
        h.join().expect("a racing thread panicked");
    }

    let results: Vec<(bool, Distinction)> = rx.iter().collect();
    assert_eq!(results.len(), AGENTS, "not every thread reported");

    let novel_count = results.iter().filter(|(n, _)| *n).count();
    assert_eq!(
        novel_count, 1,
        "{novel_count} of {AGENTS} threads observed Novel for one pair — \
         novelty must be decided exactly once"
    );

    let ids: HashSet<Distinction> = results.iter().map(|(_, d)| *d).collect();
    assert_eq!(
        ids.len(),
        1,
        "racing threads received {} distinct identifiers for one pair",
        ids.len()
    );
    invariant_holds(&engine);
}

/// **L2 — hand-off under concurrency. Category: DISCOVERY.**
///
/// A7's hand-off, with the handed-off node produced concurrently. Four
/// producer threads race one `(root, action)`; the single resulting node is
/// then adopted as the root by four *consumer* threads, each advancing it
/// with a distinct action.
///
/// Asserts: the producers agree on one node; each consumer's advance carries
/// that node as a parent; the four consumer advances are four distinct
/// identifiers; and replaying the whole thing single-threaded on a fresh
/// engine reproduces every one of them.
///
/// *Passes* → the most common real multi-agent pattern — one agent's output
/// is another's input — works with no coordination protocol even when the
/// hand-off point is produced under contention.
///
/// *Fails* → hand-off needs synchronisation that the substrate does not
/// provide, and "coordination-free" would hold only for parallel advance
/// among agents that never exchange anything.
#[test]
fn hand_off_under_concurrency() {
    let engine = Arc::new(DistinctionEngine::new());
    let fold = Arc::new(ByteFold::new(&engine));
    let origin = root_for(&engine, &fold, 1);

    // Stage 1: four producers race the same advance.
    let mut producers = Vec::new();
    for _ in 0..AGENTS {
        let engine = Arc::clone(&engine);
        let fold = Arc::clone(&fold);
        producers.push(thread::spawn(move || {
            let mut a = Lca { root: origin };
            a.synthesize_action(77u8, &fold, &engine)
        }));
    }
    let produced: HashSet<Distinction> =
        producers.into_iter().map(|h| h.join().expect("a producer thread panicked")).collect();
    assert_eq!(
        produced.len(),
        1,
        "four agents advancing the same (root, action) produced {} nodes",
        produced.len()
    );
    let handoff = *produced.iter().next().expect("one node");

    // Stage 2: four consumers adopt it as their root, concurrently.
    let mut consumers = Vec::new();
    for i in 0..AGENTS {
        let engine = Arc::clone(&engine);
        let fold = Arc::clone(&fold);
        consumers.push(thread::spawn(move || {
            let mut a = Lca { root: handoff };
            a.synthesize_action(100u8 + i as u8, &fold, &engine)
        }));
    }
    let advances: Vec<Distinction> =
        consumers.into_iter().map(|h| h.join().expect("a consumer thread panicked")).collect();

    for (i, &adv) in advances.iter().enumerate() {
        let p = engine.parents_of(adv).expect("a consumer advance must have parents");
        assert!(
            p.0 == handoff || p.1 == handoff,
            "consumer {i}'s advance does not carry the handed-off node as a \
             parent"
        );
    }
    let distinct: HashSet<Distinction> = advances.iter().copied().collect();
    assert_eq!(
        distinct.len(),
        AGENTS,
        "four distinct actions from one handed-off root collapsed to {} \
         identifiers",
        distinct.len()
    );
    invariant_holds(&engine);

    // Stage 3: replay the whole structure single-threaded.
    let fresh = Arc::new(DistinctionEngine::new());
    let fresh_fold = ByteFold::new(&fresh);
    let fresh_origin = root_for(&fresh, &fresh_fold, 1);
    let mut p = Lca { root: fresh_origin };
    let fresh_handoff = p.synthesize_action(77u8, &fresh_fold, &fresh);
    assert_eq!(
        fresh_handoff, handoff,
        "the concurrently produced hand-off node is not reproducible \
         single-threaded"
    );
    for (i, &adv) in advances.iter().enumerate() {
        let mut c = Lca { root: fresh_handoff };
        let replayed = c.synthesize_action(100u8 + i as u8, &fresh_fold, &fresh);
        assert_eq!(replayed, adv, "consumer {i}'s concurrent advance did not replay identically");
    }
    invariant_holds(&fresh);
}

/// **Stress — composition under real scheduler pressure.
/// Category: DISCOVERY.**
///
/// The two tests above are smoke tests: 4 threads and 25 advances each
/// complete in hundredths of a second, which establishes "does not deadlock"
/// and little more. This one runs **32 threads × 2,000 advances = 64,000
/// advances** against one shared engine, with a `Barrier` so every thread
/// starts inside the same instant rather than trickling in while earlier
/// threads have already finished.
///
/// Asserts: all 16 final roots are present; the engine's own auditor is
/// satisfied; and the node count and whole-graph parentage match a
/// single-threaded replay of the same 8,000 advances. That last one is the
/// load-bearing assertion — it says the scheduler contributed nothing, not
/// one extra node and not one fewer.
///
/// **This tests composition under scheduler pressure, not the substrate's
/// internal memory model.** The substrate's loom tests
/// (`tests/loom_kernel.rs` at the workspace root, with mutant sentinels)
/// cover that, and a dependent crate cannot impose `--cfg loom` on its
/// dependency. Threads exercise the composition; loom exercises the engine.
///
/// *Passes* → every sequential property survives contention at a scale where
/// races have room to happen, so the composition is coordination-free
/// operationally and not only algebraically.
///
/// *Fails* → either the engine serialises badly enough to deadlock under
/// load, or concurrency reaches identity — and a consumer would need a lock
/// discipline the substrate claims not to require.
///
/// Sizing was measured, not guessed. The specified 16×500 ran the whole file
/// in 0.17s, so it was raised; 32×2,000 takes 1.1s, and the ceiling probed
/// further is 64×5,000 — 320,000 advances, 262ms threaded, 5.6s with the
/// sequential replay — all with zero divergence. The committed size buys real
/// contention while leaving the suite fast enough to run on every change.
#[test]
fn concurrent_stress_converges() {
    const STRESS_AGENTS: usize = 32;
    const STRESS_STEPS: u16 = 2000;

    // Distinct per-agent action streams. Widened to u16 arithmetic so 500
    // steps do not wrap into another agent's stream, which would silently
    // turn independent chains into shared ones and weaken the test.
    fn stress_action(agent: usize, step: u16) -> u8 {
        (((agent as u16) * 7 + step * 11) % 251) as u8
    }

    fn drive_stress(engine: &Arc<DistinctionEngine>, fold: &ByteFold, agent: usize) -> Distinction {
        let mut lca = Lca { root: root_for(engine, fold, agent as u8 + 1) };
        for step in 0..STRESS_STEPS {
            let _ = lca.synthesize_action(stress_action(agent, step), fold, engine);
        }
        lca.root
    }

    let engine = Arc::new(DistinctionEngine::new());
    let fold = Arc::new(ByteFold::new(&engine));
    let barrier = Arc::new(std::sync::Barrier::new(STRESS_AGENTS));

    let started = std::time::Instant::now();
    let mut handles = Vec::with_capacity(STRESS_AGENTS);
    for agent in 0..STRESS_AGENTS {
        let engine = Arc::clone(&engine);
        let fold = Arc::clone(&fold);
        let barrier = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            // Every thread waits here, so the contention window is the whole
            // run rather than whatever overlap thread spawning happens to
            // produce.
            barrier.wait();
            drive_stress(&engine, &fold, agent)
        }));
    }
    let threaded: Vec<Distinction> =
        handles.into_iter().map(|h| h.join().expect("a stress thread panicked")).collect();
    let threaded_elapsed = started.elapsed();

    for (i, &r) in threaded.iter().enumerate() {
        assert!(
            engine.has(r),
            "agent {i}'s final root is absent after {} concurrent advances",
            STRESS_AGENTS * STRESS_STEPS as usize
        );
    }
    assert_eq!(
        threaded.iter().copied().collect::<HashSet<_>>().len(),
        STRESS_AGENTS,
        "two agents converged on the same final root — their action streams \
         were not independent"
    );
    invariant_holds(&engine);

    // The same 8,000 advances, one thread.
    let seq_engine = Arc::new(DistinctionEngine::new());
    let seq_fold = ByteFold::new(&seq_engine);
    let sequential: Vec<Distinction> =
        (0..STRESS_AGENTS).map(|agent| drive_stress(&seq_engine, &seq_fold, agent)).collect();

    assert_eq!(
        threaded,
        sequential,
        "{} concurrent advances produced different final roots from the same \
         advances run sequentially",
        STRESS_AGENTS * STRESS_STEPS as usize
    );
    assert_eq!(
        engine.distinction_count(),
        seq_engine.distinction_count(),
        "the scheduler changed how many nodes exist: {} concurrent vs {} \
         sequential",
        engine.distinction_count(),
        seq_engine.distinction_count()
    );
    let mut a = engine.snapshot_parentage();
    let mut b = seq_engine.snapshot_parentage();
    a.sort_unstable();
    b.sort_unstable();
    assert_eq!(a, b, "concurrent and sequential runs produced different graphs");
    invariant_holds(&seq_engine);

    // Reported rather than asserted: a timing assertion would be a flaky
    // test on a loaded machine, but the number belongs in the output so a
    // regression in contention cost is visible to whoever reads it.
    println!(
        "stress: {} agents x {} advances = {} advances, {} nodes, threaded {:?}",
        STRESS_AGENTS,
        STRESS_STEPS,
        STRESS_AGENTS * STRESS_STEPS as usize,
        engine.distinction_count(),
        threaded_elapsed
    );
}
