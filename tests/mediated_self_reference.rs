//! Law 10: a structure can refer to itself without collapsing into itself.
//!
//! Irreflexivity makes direct self-reference degenerate — `synth(x, x)` is `x`,
//! so a thing synthesized with itself yields nothing new. That is the axiom
//! doing its job: it forecloses runaway self-generation. The open question is
//! whether self-reference is therefore impossible, or merely has to be
//! *mediated*.
//!
//! Law 10's answer is `synth(synth(x, obs), x)`: pass through an observation
//! first, then fold the original back in. The result genuinely references `x` —
//! `x` is one of its parents, recoverably — while being distinct from `x`.
//!
//! This matters beyond the law. If an observer can be inside the structure it
//! observes, without the act of observing collapsing the observer into the
//! observed, then "an agent is a vantage within the system" is coherent rather
//! than aspirational. If iterating it collapses or cycles, that architecture
//! has no foundation.
//!
//! The probe iterates to depth and checks three things: no fixed point, no
//! repeat before the birthday bound, and that the reference relation actually
//! holds at every step. Three observation regimes, because a result that held
//! only for a constant observation would be an artifact of the constant.

use koru_lambda_core::{ByteFold, Distinction, DistinctionEngine};
use std::collections::HashSet;

/// Depth. At 100_000 identifiers of 128 bits, the birthday bound puts the
/// probability of an accidental collision near 1.5e-29, so a repeat here is a
/// structural result rather than luck.
const DEPTH: usize = 100_000;

/// How the observation is chosen at each step. Each regime is a different way
/// the law could fail, so a pass under one is not a pass under all.
enum Observation {
    /// The same observation forever. The regime most likely to find a cycle,
    /// because the recurrence has no external variation at all.
    Constant,
    /// A small repeating set. Catches a period that a constant observation
    /// would hide inside its own fixed point.
    Cycling(usize),
    /// The observation is drawn from the chain's own history, so the structure
    /// observes itself observing. The regime closest to what an agent does.
    FromChain,
}

struct Outcome {
    seen: usize,
    min_degree_of_x: usize,
}

fn run(regime: Observation, engine: &DistinctionEngine, fold: &ByteFold) -> Outcome {
    let mut current = fold.tip(0x01);
    let mut seen: HashSet<Distinction> = HashSet::with_capacity(DEPTH);
    seen.insert(current);
    let mut chain: Vec<Distinction> = vec![current];
    let mut min_degree_of_x = usize::MAX;

    for step in 1..=DEPTH {
        let observation = match regime {
            Observation::Constant => fold.tip(0x80),
            // Offset into a byte range disjoint from the seed tip. Without the
            // offset, step 1 of a 17-cycle selects `tip(1)`, which *is* the
            // seed — the probe then reports a precondition failure that is an
            // artifact of its own indexing rather than anything about the law.
            Observation::Cycling(period) => fold.tip(0x80 + (step % period) as u8),
            // Reach back far enough that the observation is neither the current
            // element nor its immediate parent. Until the chain is that long
            // there is no earlier element to draw on, so fall back to a fixed
            // tip rather than wrapping around onto `current`.
            Observation::FromChain => {
                const LOOKBACK: usize = 8;
                if chain.len() > LOOKBACK {
                    chain[chain.len() - LOOKBACK]
                } else {
                    fold.tip(0x80)
                }
            },
        };

        // The observation must differ from the current element, or the inner
        // synthesis saturates to `current` and the outer one collapses by
        // irreflexivity. This is a precondition of the law, not a failure of
        // it, so it is asserted rather than silently skipped.
        assert_ne!(
            observation, current,
            "step {step}: observation equals the observed, so mediation degenerates — \
             the law's precondition does not hold and the probe would be testing nothing"
        );

        let mediated = engine.synthesize(current, observation);
        let next = engine.synthesize(mediated, current);

        // No fixed point.
        assert_ne!(
            next, current,
            "step {step}: mediated self-reference reached a fixed point — Law 10 is false"
        );

        // The reference relation must actually hold: `current` is recoverably a
        // parent of `next`. Without this the chain is merely novel, not
        // self-referential, and the law would be vacuous.
        let parents = engine.parents_of(next).unwrap_or_else(|| {
            panic!("step {step}: engine has no parentage for a value it just produced")
        });
        assert!(
            parents.0 == current || parents.1 == current,
            "step {step}: the new distinction does not reference the one it was derived from — \
             self-reference is not preserved, so the construction is not mediated \
             self-reference at all"
        );

        min_degree_of_x = min_degree_of_x.min(engine.degree(current));

        // No repeat.
        assert!(
            seen.insert(next),
            "step {step}: the chain revisited a distinction — mediated self-reference cycles, \
             which bounds how deep any self-observing structure can go"
        );

        chain.push(next);
        current = next;
    }

    Outcome { seen: seen.len(), min_degree_of_x }
}

#[test]
fn a_constant_observation_never_collapses_or_cycles() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let outcome = run(Observation::Constant, &engine, &fold);
    assert_eq!(outcome.seen, DEPTH + 1, "chain length mismatch");
    eprintln!(
        "constant observation: {} distinct to depth {DEPTH}, min degree of the observed = {}",
        outcome.seen, outcome.min_degree_of_x
    );
}

#[test]
fn a_cycling_observation_never_collapses_or_cycles() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let outcome = run(Observation::Cycling(17), &engine, &fold);
    assert_eq!(outcome.seen, DEPTH + 1, "chain length mismatch");
    eprintln!("cycling observation: {} distinct to depth {DEPTH}", outcome.seen);
}

#[test]
fn an_observation_drawn_from_the_chain_never_collapses_or_cycles() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let outcome = run(Observation::FromChain, &engine, &fold);
    assert_eq!(outcome.seen, DEPTH + 1, "chain length mismatch");
    eprintln!("observation from chain: {} distinct to depth {DEPTH}", outcome.seen);
}

/// The contrast that makes the law non-trivial: unmediated self-reference
/// really does collapse, so mediation is doing work rather than decorating a
/// construction that would have succeeded anyway.
#[test]
fn unmediated_self_reference_collapses_immediately() {
    let engine = DistinctionEngine::new();
    let fold = ByteFold::new(&engine);
    let x = fold.tip(0x01);
    assert_eq!(
        engine.synthesize(x, x),
        x,
        "irreflexivity broken — if direct self-synthesis produced something new, Law 10 \
         would be solving a problem that does not exist"
    );
}
