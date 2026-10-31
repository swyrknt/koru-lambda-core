//! Composition of N local causal agents sharing one engine.
//!
//! Every test here asserts a property of the *composition*, not of a single
//! agent. The substrate's own suite covers one engine; `agent.rs`'s inline
//! tests cover one agent. Nothing covered what happens when several agents
//! advance against the same witness, which is the claim the agent layer
//! actually makes.
//!
//! # Categories
//!
//! Each test states its category, because the two kinds carry different
//! evidential weight and conflating them is how a suite comes to look
//! stronger than it is.
//!
//! - **GUARD** — cannot fail without a substrate defect. Determinism and
//!   content addressing are axioms (`THEORY.md` — the four axioms), so a
//!   guard does not discover anything; it fails loudly if the substrate
//!   stops doing its job *as observed from the agent level*. That is the
//!   point: the property is asserted where a consumer would notice it.
//! - **DISCOVERY** — genuinely tests a claim that could come apart. These
//!   are the tests whose outcome was not known before writing them.
//! - **DOMAIN-SEPARATION** — verifies that different inputs *do* separate.
//!   The dual of a guard: it fails if the substrate stops distinguishing.
//!
//! # What the suite tests, stated once
//!
//! A composition of N agents on one engine is **not** an LCA — multi-root
//! consumers are legal (`THEORY.md#using-the-substrate`) — but it stays in
//! the same family: content-addressed, deterministic, replayable, in-graph.
//! `docs/SPEC.md` §2a defines the advance each agent performs, and
//! `THEORY.md:416` reads agent-ness as a property of the graph rather than
//! of a type, which is why composition needs no new identity rule.
//!
//! Public API only. No internal access, no test-only constructors.

use koru_lambda_agents::agent::{synthesize_causal_action, LocalCausalAgent};
use koru_lambda_core::engine::ParentPair;
use koru_lambda_core::primitives::{domain_anchor, ByteFold, DomainLabel};
use koru_lambda_core::{Distinction, DistinctionEngine};
use std::collections::{HashSet, VecDeque};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

/// Minimal LCA. Carries one root and advances it; the trait's default
/// `synthesize_action` supplies the discipline, so this adds no behaviour
/// of its own. That matters for the suite's conclusions: anything these
/// tests observe is a property of the substrate plus the default advance,
/// not of a bespoke agent.
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

/// Shared setup. Built from a seed so a fresh engine can reconstruct the
/// identical roots — which is what makes replay testable at all: the
/// "history" of an agent is the operations that produced its root plus the
/// actions it applied, and nothing else.
struct Fixture {
    engine: Arc<DistinctionEngine>,
    fold: ByteFold,
    roots: Vec<Distinction>,
}

/// The three agent seeds. Distinct bytes, so the three roots are distinct
/// nodes rather than the same node reached three ways.
const SEEDS: [u8; 3] = [1, 2, 3];

/// Per-agent action sequences. Disjoint ranges, so a node appearing in two
/// agents' chains is evidence of sharing rather than of coincidence.
const ACTIONS: [[u8; 3]; 3] = [[10, 11, 12], [20, 21, 22], [30, 31, 32]];

fn setup() -> Fixture {
    let engine = Arc::new(DistinctionEngine::new());
    let fold = ByteFold::new(&engine);
    let roots = SEEDS.iter().map(|&s| root_for(&engine, &fold, s)).collect();
    Fixture { engine, fold, roots }
}

/// An agent's root, derived from its seed by operations only. Reproducible
/// on any engine, which is the whole basis of the replay tests.
fn root_for(engine: &Arc<DistinctionEngine>, fold: &ByteFold, seed: u8) -> Distinction {
    let lifted = fold.fold(engine.d0(), &[seed], engine);
    engine.synthesize(engine.d0(), lifted)
}

/// Run each agent's actions in the order the interleaving dictates.
/// `plan` is a list of agent indices; each entry advances that agent by one
/// action from its own sequence.
fn run_plan(fx: &Fixture, plan: &[usize]) -> Vec<Distinction> {
    let mut agents: Vec<Lca> = fx.roots.iter().map(|&r| Lca { root: r }).collect();
    let mut next = [0usize; 3];
    for &i in plan {
        let step = next[i];
        assert!(step < ACTIONS[i].len(), "plan overruns agent {i}'s actions");
        let _ = agents[i].synthesize_action(ACTIONS[i][step], &fx.fold, &fx.engine);
        next[i] = step + 1;
    }
    agents.iter().map(|a| a.root).collect()
}

/// Every index, three times, agent-major: 0 0 0 1 1 1 2 2 2.
fn plan_serial() -> Vec<usize> {
    (0..3).flat_map(|i| std::iter::repeat_n(i, 3)).collect()
}

/// Round-robin: 0 1 2 0 1 2 0 1 2.
fn plan_round_robin() -> Vec<usize> {
    (0..3).flat_map(|_| 0..3).collect()
}

/// Agent-major, reversed: 2 2 2 1 1 1 0 0 0.
fn plan_reverse_serial() -> Vec<usize> {
    (0..3).rev().flat_map(|i| std::iter::repeat_n(i, 3)).collect()
}

/// A whole graph, as the engine reports it: child paired with its parents.
type Parentage = Vec<(Distinction, ParentPair)>;

/// Sorted parentage, for comparing two engines' whole graphs as sets.
fn sorted_parentage(engine: &DistinctionEngine) -> Parentage {
    let mut p = engine.snapshot_parentage();
    p.sort_unstable();
    p
}

/// Walk `parents_of` upward and report whether every path terminates at a
/// node with no parents. A primordial is the only parentless node an engine
/// holds, so termination *is* reachability to a primordial.
fn reaches_primordial(engine: &DistinctionEngine, from: Distinction) -> bool {
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([from]);
    let mut found_parentless = false;
    while let Some(d) = queue.pop_front() {
        if !seen.insert(d) {
            continue;
        }
        match engine.parents_of(d) {
            Some((a, b)) => {
                queue.push_back(a);
                queue.push_back(b);
            },
            None => found_parentless = true,
        }
    }
    found_parentless
}

fn invariant_holds(engine: &DistinctionEngine) {
    engine
        .check_structural_invariant()
        .expect("composition left the engine's own auditor unsatisfied");
}

// ---------------------------------------------------------------------------
// A1 — GUARD
// ---------------------------------------------------------------------------

/// **A1 — cross-agent determinism. Category: GUARD.**
///
/// Three agents synthesize the same three pairs in three different orders,
/// on three independent engines. Every agent's identifier for a given pair
/// must be equal, and the engines must end with the same node count.
///
/// *Passes* → identity is independent of who asked and of when, so the four
/// axioms are doing the coordination and there is nothing left for a
/// separate protocol to do.
///
/// *Fails* → a substrate defect. Determinism and content addressing are
/// axioms, so this cannot fail because of anything a consumer chose. It is
/// a regression guard placed where a consumer would actually notice.
///
/// Not falsifiable from outside the substrate: see the module docs on
/// categories, and `distinct_anchors_separate_same_payload` for the
/// separate question of whether *different* inputs separate.
#[test]
fn cross_agent_determinism_over_shared_pairs() {
    let orders: [[usize; 3]; 3] = [[0, 1, 2], [2, 1, 0], [1, 2, 0]];
    let mut per_order: Vec<Vec<Distinction>> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();

    for order in &orders {
        let engine = Arc::new(DistinctionEngine::new());
        let fold = ByteFold::new(&engine);
        // The three shared pairs, named by operations so every engine can
        // build the identical ones.
        let pairs: Vec<(Distinction, Distinction)> = SEEDS
            .iter()
            .map(|&s| (root_for(&engine, &fold, s), fold.fold(engine.d0(), &[s, s], &engine)))
            .collect();

        // Seeded with d0 rather than a default, because `Distinction` has no
        // public constructor at all — which is ENG-4's trust boundary doing
        // its job, and the reason this suite cannot fabricate a node.
        let mut ids = vec![engine.d0(); 3];
        for &idx in order {
            let (a, b) = pairs[idx];
            ids[idx] = engine.synthesize(a, b);
        }
        per_order.push(ids);
        counts.push(engine.distinction_count());
        invariant_holds(&engine);
    }

    for (pair, &baseline) in per_order[0].iter().enumerate() {
        assert_eq!(
            baseline, per_order[1][pair],
            "pair {pair}: order {:?} and {:?} produced different identifiers — \
             identity depended on synthesis order",
            orders[0], orders[1]
        );
        assert_eq!(
            baseline, per_order[2][pair],
            "pair {pair}: order {:?} and {:?} produced different identifiers",
            orders[0], orders[2]
        );
    }
    assert_eq!(
        counts[0], counts[1],
        "same operations in a different order produced a different node count"
    );
    assert_eq!(counts[0], counts[2], "node count depended on order");
}

// ---------------------------------------------------------------------------
// A2 — GUARD
// ---------------------------------------------------------------------------

/// **A2 — agent-level replay. Category: GUARD.**
///
/// Each agent's `(root, actions)` history is replayed on a *fresh* engine
/// with a fresh fold, and must reproduce the same final root byte for byte.
///
/// *Passes* → time is derived rather than primitive. The history is
/// sufficient state, which is what makes "the graph is the log" true at the
/// agent level and not only at the substrate's.
///
/// *Fails* → an agent carries state the graph does not hold, and replay is
/// not free. That would contradict `docs/SPEC.md` §2a, under which an
/// advance is recoverable from parentage.
///
/// Not falsifiable from outside the substrate.
#[test]
fn agent_history_replays_byte_identically() {
    let fx = setup();
    let originals = run_plan(&fx, &plan_serial());

    // Replay: a new engine that has never seen any of this, driven only by
    // the seed and the action list.
    let replay_engine = Arc::new(DistinctionEngine::new());
    let replay_fold = ByteFold::new(&replay_engine);
    for (i, &seed) in SEEDS.iter().enumerate() {
        let mut agent = Lca { root: root_for(&replay_engine, &replay_fold, seed) };
        for &action in &ACTIONS[i] {
            let _ = agent.synthesize_action(action, &replay_fold, &replay_engine);
        }
        assert_eq!(
            agent.root, originals[i],
            "agent {i} replayed to a different final root: history was not \
             sufficient state"
        );
    }
    invariant_holds(&replay_engine);
    assert_eq!(
        sorted_parentage(&fx.engine),
        sorted_parentage(&replay_engine),
        "replay produced a different graph from the same operations"
    );
}

// ---------------------------------------------------------------------------
// A3 — GUARD
// ---------------------------------------------------------------------------

/// **A3 — locality is anchoring, not confinement. Category: GUARD.**
///
/// After every single advance, exactly one agent's root has changed, and it
/// changed to a node whose parentage contains its previous value.
///
/// *Passes* → an agent advancing does not disturb a peer's perspective.
/// Locality constrains where an agent reads from, not what it may see.
///
/// *Fails* → advancing mutates a peer, and locality would be confinement —
/// which would make the composition a coordination problem.
///
/// Not falsifiable from outside the substrate. (The nearest negative
/// control is reachable, though: `update_local_root` is unconditional by
/// design — `agent.rs` documents the convention as not trait-enforced — so
/// an agent *can* adopt a peer's root. That is a consumer error, not a
/// substrate one, and this test would catch it.)
#[test]
fn advance_is_local_not_confining() {
    let fx = setup();
    let mut agents: Vec<Lca> = fx.roots.iter().map(|&r| Lca { root: r }).collect();
    let mut next = [0usize; 3];

    for &acting in &plan_round_robin() {
        let before: Vec<Distinction> = agents.iter().map(|a| a.root).collect();
        let step = next[acting];
        let _ = agents[acting].synthesize_action(ACTIONS[acting][step], &fx.fold, &fx.engine);
        next[acting] = step + 1;
        let after: Vec<Distinction> = agents.iter().map(|a| a.root).collect();

        for i in 0..agents.len() {
            if i == acting {
                assert_ne!(before[i], after[i], "agent {i} acted but its own root did not move");
                let parents =
                    fx.engine.parents_of(after[i]).expect("an advanced root must have parents");
                assert!(
                    parents.0 == before[i] || parents.1 == before[i],
                    "agent {i}'s new root does not carry its previous root as a \
                     parent — the chain is not monotonically forward"
                );
            } else {
                assert_eq!(
                    before[i], after[i],
                    "agent {acting} advanced and changed agent {i}'s root — \
                     locality would be confinement, not anchoring"
                );
            }
        }
    }
    invariant_holds(&fx.engine);
}

// ---------------------------------------------------------------------------
// A4 — DISCOVERY
// ---------------------------------------------------------------------------

/// **A4 — every interleaving converges. Category: DISCOVERY.**
///
/// The same per-agent histories are run under three interleavings —
/// agent-serial, round-robin, and reverse-serial — on three fresh engines.
/// Final roots, whole-graph parentage and node counts must all agree.
///
/// *Passes* → composition is replayable regardless of scheduling, so an
/// interleaving is not part of the state.
///
/// *Fails* → composition holds order-dependent state that the substrate
/// disclaims, and a scheduler would become part of the trusted base.
#[test]
fn interleavings_converge_to_one_state() {
    let plans = [plan_serial(), plan_round_robin(), plan_reverse_serial()];
    let mut finals: Vec<Vec<Distinction>> = Vec::new();
    let mut graphs: Vec<Parentage> = Vec::new();

    for plan in &plans {
        let fx = setup();
        finals.push(run_plan(&fx, plan));
        graphs.push(sorted_parentage(&fx.engine));
        invariant_holds(&fx.engine);
    }

    for k in 1..plans.len() {
        assert_eq!(
            finals[0], finals[k],
            "interleaving {k} produced different final roots — scheduling \
             leaked into agent state"
        );
        assert_eq!(
            graphs[0], graphs[k],
            "interleaving {k} produced a different graph from the same \
             per-agent histories"
        );
    }
}

// ---------------------------------------------------------------------------
// A5′ — DISCOVERY, two halves
// ---------------------------------------------------------------------------

/// **A5′ (first half) — sharing inside one witness needs no protocol.
/// Category: DISCOVERY.**
///
/// Agent 1 advances. Agent 2 then reads that node by identifier alone —
/// `has`, `parents_of`, `degree` — with no method on either agent used to
/// move it, and no transfer API of any kind.
///
/// *Passes* → content crosses agents inside a witness for free. The engine
/// is the shared medium and the identifier is the whole reference.
///
/// *Fails* → sharing requires a transfer step, and the composition needs a
/// protocol after all.
#[test]
fn shared_engine_needs_no_transfer_api() {
    let fx = setup();
    let mut a1 = Lca { root: fx.roots[0] };
    let a2 = Lca { root: fx.roots[1] };

    let produced = a1.synthesize_action(ACTIONS[0][0], &fx.fold, &fx.engine);

    // Agent 2 holds only the identifier. Nothing was handed over.
    assert!(
        fx.engine.has(produced),
        "agent 2 cannot see a node agent 1 just introduced into the shared \
         witness"
    );
    let parents = fx
        .engine
        .parents_of(produced)
        .expect("agent 2 cannot read the parentage of agent 1's node");
    assert!(
        parents.0 == fx.roots[0] || parents.1 == fx.roots[0],
        "agent 1's prior root is not a parent of what it produced"
    );
    // A read that depends on nothing agent 2 was told, asserted rather than
    // discarded — a read whose result is thrown away does not demonstrate
    // that the read works.
    //
    // `docs/SPEC.md` §6.1 defines degree as the number of incident edges,
    // with two edges per non-primordial, one to each parent. A node that is
    // still a leaf therefore has degree exactly 2, like `e` in §6.1's worked
    // table. (My first draft asserted 0, reading degree as "syntheses this
    // node has participated in as a parent". That is a different rule which
    // §6.1 explicitly displaced, and it is the rule the engine does not
    // implement.)
    assert_eq!(
        fx.engine.degree(produced),
        2,
        "a leaf node's degree is not its two parent edges (SPEC §6.1)"
    );
    assert_eq!(
        a1.get_current_root(),
        produced,
        "agent 1's root did not advance to what it produced"
    );
    assert_ne!(
        a2.get_current_root(),
        produced,
        "agent 2's own perspective must be untouched by the read"
    );
    invariant_holds(&fx.engine);
}

/// **A5′ (second half) — across witnesses, agreement replaces transport.
/// Category: DISCOVERY.**
///
/// Two independent engines. The second does **not** have the first's node —
/// `docs/SPEC.md` §3 says an engine witnesses only what it has seen — yet
/// performing the same operations yields a byte-identical identifier.
///
/// Both halves are asserted in one test on purpose: either alone misleads.
/// "Content crosses agents with no protocol" is true inside a witness and
/// false between witnesses, and the thing that replaces transport is
/// reproduction.
///
/// *Passes* → agreement without coordination *and* an intact witness
/// boundary. These are the two claims that together make the substrate a
/// rule rather than a service.
///
/// *Fails* → either engines leak into each other, or content addressing
/// does not survive across them.
#[test]
fn separate_engines_agree_without_reading() {
    let e1 = Arc::new(DistinctionEngine::new());
    let f1 = ByteFold::new(&e1);
    let e2 = Arc::new(DistinctionEngine::new());
    let f2 = ByteFold::new(&e2);

    let mut a1 = Lca { root: root_for(&e1, &f1, SEEDS[0]) };
    let produced = a1.synthesize_action(ACTIONS[0][0], &f1, &e1);

    assert!(
        !e2.has(produced),
        "a second engine reported witnessing a node it never produced — the \
         witness boundary leaked"
    );

    let mut a2 = Lca { root: root_for(&e2, &f2, SEEDS[0]) };
    let reproduced = a2.synthesize_action(ACTIONS[0][0], &f2, &e2);

    assert_eq!(
        produced, reproduced,
        "two engines performing the same operations disagreed on the \
         identifier — agreement without coordination would be false"
    );
    assert!(e2.has(reproduced), "the second engine does not witness what it just produced");
    invariant_holds(&e1);
    invariant_holds(&e2);
}

// ---------------------------------------------------------------------------
// A6 — DISCOVERY
// ---------------------------------------------------------------------------

/// **A6 — composition is associative. Category: DISCOVERY.**
///
/// Three bracketings of the same three agents: `((A,B),C)` interleaves A and
/// B then runs C; `(A,(B,C))` runs A then interleaves B and C; the flat
/// `(A,B,C)` round-robins all three. Shared nodes and whole-graph parentage
/// must be identical across all three.
///
/// *Passes* → grouping is not observable, which is what makes composition
/// an algebraic structure rather than three properties that happen to hold
/// together. Commutativity alone would not give this.
///
/// *Fails* → composition is not associative, and "a composition of LCAs is
/// in the same family" would need restating — the family would not be
/// closed under grouping.
#[test]
fn composition_is_associative() {
    // ((A,B),C): A and B interleaved, then C.
    let left: Vec<usize> = [0, 1, 0, 1, 0, 1].into_iter().chain([2, 2, 2]).collect();
    // (A,(B,C)): A alone, then B and C interleaved.
    let right: Vec<usize> = [0, 0, 0].into_iter().chain([1, 2, 1, 2, 1, 2]).collect();
    // Flat.
    let flat = plan_round_robin();

    let mut graphs = Vec::new();
    let mut finals = Vec::new();
    for plan in [&left, &right, &flat] {
        let fx = setup();
        finals.push(run_plan(&fx, plan));
        graphs.push(sorted_parentage(&fx.engine));
        invariant_holds(&fx.engine);
    }

    assert_eq!(
        graphs[0], graphs[1],
        "((A,B),C) and (A,(B,C)) produced different graphs — composition is \
         not associative"
    );
    assert_eq!(graphs[0], graphs[2], "a bracketed composition differs from the flat one");
    assert_eq!(finals[0], finals[1], "bracketing changed the agents' final roots");
    assert_eq!(finals[0], finals[2], "bracketing changed the final roots");
}

// ---------------------------------------------------------------------------
// A7 — DISCOVERY
// ---------------------------------------------------------------------------

/// **A7 — one agent's output is another's input, with no protocol.
/// Category: DISCOVERY.**
///
/// Not a parallel advance: a hand-off. Agent 1 advances `root_A` to
/// `child_A`; Agent 2 *starts* from `child_A` and advances it further.
///
/// Asserts four things: Agent 2 advances from `child_A` correctly;
/// `child_A` is a parent of Agent 2's advance; replaying Agent 2's history
/// from `child_A` on a fresh engine reproduces its final root byte for
/// byte; and `child_A` is a parent of *both* Agent 1's next advance and
/// Agent 2's — a shared node with two futures.
///
/// *Passes* → hand-off between agents needs no coordination protocol, which
/// is the most common real multi-agent pattern. Without this test,
/// "coordination-free" is asserted for parallel advance only.
///
/// *Fails* → a consumer taking another's output as its root is doing
/// something the substrate does not support, and the composition claim
/// covers far less than it appears to.
#[test]
fn agent_handoff_needs_no_protocol() {
    let fx = setup();
    let mut a1 = Lca { root: fx.roots[0] };
    let child_a = a1.synthesize_action(ACTIONS[0][0], &fx.fold, &fx.engine);

    // The hand-off is a value assignment. No API mediates it.
    let mut a2 = Lca { root: child_a };
    let a2_advance = a2.synthesize_action(ACTIONS[1][0], &fx.fold, &fx.engine);

    let p2 = fx.engine.parents_of(a2_advance).expect("agent 2's advance must have parents");
    assert!(
        p2.0 == child_a || p2.1 == child_a,
        "the handed-off node is not a parent of agent 2's advance"
    );

    // Agent 1 keeps going from the same node it handed off.
    let a1_next = a1.synthesize_action(ACTIONS[0][1], &fx.fold, &fx.engine);
    let p1 = fx.engine.parents_of(a1_next).expect("agent 1's next advance must have parents");
    assert!(
        p1.0 == child_a || p1.1 == child_a,
        "agent 1's next advance does not descend from the node it handed off"
    );
    assert_ne!(
        a1_next, a2_advance,
        "two different actions from one shared node collapsed to one identifier"
    );

    // Agent 2's history replays from the handed-off node alone.
    let fresh = Arc::new(DistinctionEngine::new());
    let fresh_fold = ByteFold::new(&fresh);
    let rebuilt_root = root_for(&fresh, &fresh_fold, SEEDS[0]);
    let mut rebuilt_a1 = Lca { root: rebuilt_root };
    let rebuilt_child = rebuilt_a1.synthesize_action(ACTIONS[0][0], &fresh_fold, &fresh);
    assert_eq!(rebuilt_child, child_a, "the handed-off node is not reproducible from operations");
    let mut rebuilt_a2 = Lca { root: rebuilt_child };
    let rebuilt_advance = rebuilt_a2.synthesize_action(ACTIONS[1][0], &fresh_fold, &fresh);
    assert_eq!(
        rebuilt_advance, a2_advance,
        "agent 2's history did not replay byte-identically from the \
         handed-off root"
    );

    invariant_holds(&fx.engine);
    invariant_holds(&fresh);
}

// ---------------------------------------------------------------------------
// F1′ — GUARD
// ---------------------------------------------------------------------------

/// **F1′ — the chain is forward; idempotence is keyed on `(root, action)`.
/// Category: GUARD.**
///
/// Three sub-claims. (a) The same `(root, action)` applied twice yields the
/// *same* child. (b) The same action applied twice *in sequence* yields
/// different children, and the first is a parent of the second. (c) No node
/// in a chain is its own ancestor.
///
/// This replaces the original F1, which claimed composition breaks
/// idempotence. Measurement says otherwise: idempotence is **re-keyed**,
/// not lost. Axiom 3 holds on `(root, action)`; what is non-idempotent is a
/// *sequence*, because the root is part of the key and the root advances.
///
/// *Passes* → the agent layer adds no new identity rule. The axioms are
/// closed under composition, which is a stronger statement than their
/// merely holding at the substrate level.
///
/// *Fails* → either an agent mints identity the substrate did not
/// (contradicting content addressing), or a chain can revisit a node
/// (contradicting `docs/SPEC.md` §2a's monotonic reading).
///
/// Not falsifiable from outside the substrate.
#[test]
fn chain_is_forward_and_idempotence_is_keyed() {
    let fx = setup();
    let root = fx.roots[0];

    // (a) Idempotent on (root, action).
    let first = synthesize_causal_action(root, 7u8, &fx.fold, &fx.engine);
    let again = synthesize_causal_action(root, 7u8, &fx.fold, &fx.engine);
    assert_eq!(
        first, again,
        "the same (root, action) produced two identifiers — the agent layer \
         invented identity the substrate did not"
    );

    // Distinct actions from one root must not collapse.
    let other = synthesize_causal_action(root, 8u8, &fx.fold, &fx.engine);
    assert_ne!(first, other, "two different actions from one root collapsed to one identifier");

    // (b) Non-idempotent in sequence, and strictly forward.
    let mut agent = Lca { root };
    let s1 = agent.synthesize_action(7u8, &fx.fold, &fx.engine);
    let s2 = agent.synthesize_action(7u8, &fx.fold, &fx.engine);
    assert_eq!(s1, first, "the agent's first advance left the keyed result");
    assert_ne!(
        s1, s2,
        "the same action twice in sequence produced one node — the root did \
         not advance"
    );
    let parents = fx.engine.parents_of(s2).expect("s2 must have parents");
    assert!(
        parents.0 == s1 || parents.1 == s1,
        "the second advance does not carry the first as a parent"
    );

    // (c) Acyclic: no ancestor of s2 is s2.
    let mut seen = HashSet::new();
    let mut queue = VecDeque::from([s2]);
    let mut steps = 0usize;
    while let Some(d) = queue.pop_front() {
        steps += 1;
        assert!(steps < 10_000, "ancestor walk did not terminate — a cycle");
        if let Some((a, b)) = fx.engine.parents_of(d) {
            for p in [a, b] {
                assert_ne!(p, s2, "a node is its own ancestor");
                if seen.insert(p) {
                    queue.push_back(p);
                }
            }
        }
    }
    invariant_holds(&fx.engine);
}

// ---------------------------------------------------------------------------
// F2 — GUARD
// ---------------------------------------------------------------------------

/// **F2 — composition is deterministic. Category: GUARD.**
///
/// The same interleaving run twice, on two fresh engines, must give
/// identical final roots and identical whole-graph parentage.
///
/// *Passes* → nothing outside the operations — allocation, hashing order,
/// iteration order — reaches identity.
///
/// *Fails* → a substrate defect: something non-deterministic leaked into
/// the graph.
///
/// Not falsifiable from outside the substrate.
#[test]
fn composition_is_deterministic() {
    let plan = plan_round_robin();
    let fx_a = setup();
    let finals_a = run_plan(&fx_a, &plan);
    let fx_b = setup();
    let finals_b = run_plan(&fx_b, &plan);

    assert_eq!(
        finals_a, finals_b,
        "the same interleaving produced different final roots on two runs"
    );
    assert_eq!(
        sorted_parentage(&fx_a.engine),
        sorted_parentage(&fx_b.engine),
        "the same interleaving produced two different graphs"
    );
    assert_eq!(
        fx_a.engine.distinction_count(),
        fx_b.engine.distinction_count(),
        "node counts diverged between identical runs"
    );
    invariant_holds(&fx_a.engine);
    invariant_holds(&fx_b.engine);
}

// ---------------------------------------------------------------------------
// F3 — GUARD
// ---------------------------------------------------------------------------

/// **F3 — every agent root is a graph node. Category: GUARD.**
///
/// After every advance, each agent's root is present in the engine and
/// reaches a parentless node by following `parents_of`.
///
/// *Passes* → composition is content-addressed end to end. No agent holds a
/// perspective that the graph does not contain.
///
/// *Fails* → an agent's root is outside the witness, which would make its
/// perspective unverifiable and its history unreplayable.
///
/// Not falsifiable from outside the substrate — the type boundary prevents
/// constructing a `Distinction` the engine never produced, which is exactly
/// what `ENG-4` and `tests/trybuild/distinction_has_no_hatches.rs` enforce.
#[test]
fn every_agent_root_is_a_graph_node() {
    let fx = setup();
    let mut agents: Vec<Lca> = fx.roots.iter().map(|&r| Lca { root: r }).collect();
    let mut next = [0usize; 3];

    for &acting in &plan_round_robin() {
        let step = next[acting];
        let _ = agents[acting].synthesize_action(ACTIONS[acting][step], &fx.fold, &fx.engine);
        next[acting] = step + 1;

        for (i, agent) in agents.iter().enumerate() {
            let r = agent.get_current_root();
            assert!(fx.engine.has(r), "agent {i}'s root is not a node in the shared engine");
            assert!(
                reaches_primordial(&fx.engine, r),
                "agent {i}'s root does not reach a primordial through parentage"
            );
        }
    }
    invariant_holds(&fx.engine);
}

// ---------------------------------------------------------------------------
// F4 — DISCOVERY
// ---------------------------------------------------------------------------

/// **F4 — agent order and content. Category: DISCOVERY.**
///
/// Runs agent A's whole history then agent B's, and then the reverse, on
/// two fresh engines. Asserts that the **shared content is identical** —
/// every node either order produces is the same set, with the same
/// parentage.
///
/// The original F4 also claimed the *final roots* would differ between the
/// two orders. They do not, and that is this test's finding rather than its
/// failure: an agent's chain depends on its own root and its own actions
/// only, which is precisely A3 (locality). So for agents on independent
/// roots, composition is commutative at the agent level too — a stronger
/// result than "non-commutative in roots, commutativity-preserving in
/// content". The asymmetry the original F4 expected appears only where
/// agents interact, which is what A7's hand-off covers.
///
/// *Passes* → order of independent agents is unobservable, in roots and in
/// content alike.
///
/// *Fails* → ordering independent agents changes what the graph holds, and
/// the composition would need a scheduler in its trusted base.
#[test]
fn agent_order_matters_content_does_not() {
    let a_then_b: Vec<usize> = [0, 0, 0].into_iter().chain([1, 1, 1]).collect();
    let b_then_a: Vec<usize> = [1, 1, 1].into_iter().chain([0, 0, 0]).collect();

    let fx1 = setup();
    let finals_ab = run_plan(&fx1, &a_then_b);
    let fx2 = setup();
    let finals_ba = run_plan(&fx2, &b_then_a);

    // Content: identical, which is the half the original F4 predicted.
    assert_eq!(
        sorted_parentage(&fx1.engine),
        sorted_parentage(&fx2.engine),
        "A-then-B and B-then-A produced different graphs — content is not \
         order-independent"
    );

    // Roots: also identical. The original expectation was that these would
    // differ; they do not, because each agent's chain is a function of its
    // own root and actions alone.
    assert_eq!(
        finals_ab, finals_ba,
        "independent agents' final roots depended on the order the agents \
         ran in, which would contradict A3's locality result"
    );
    invariant_holds(&fx1.engine);
    invariant_holds(&fx2.engine);
}

// ---------------------------------------------------------------------------
// F5 — GUARD, load-bearing
// ---------------------------------------------------------------------------

/// **F5 — the composition is multi-root and in-graph. Category: GUARD.**
///
/// Three agents on three roots are **not** an LCA. `THEORY.md` names
/// multi-root consumers as a legal shape that does not follow the pattern,
/// and this test states the distinction sharply: the composition has N
/// roots rather than one, every root is a distinct node, every root reaches
/// a primordial, and the set of roots is a subset of the engine's own
/// snapshot.
///
/// *Passes* → the fractal claim, exactly: composition stays in the family —
/// content-addressed, deterministic, in-graph — while **not** being an LCA.
/// A future reader asking "is a composition just an LCA?" gets "no, and
/// here is the precise sense in which it is still in the same family."
///
/// *Fails* → either the composition collapses to a single root (and
/// multi-root would not be legal after all), or a root escapes the graph.
///
/// Not falsifiable from outside the substrate.
#[test]
fn composition_is_multi_root_and_in_graph() {
    let fx = setup();
    let finals = run_plan(&fx, &plan_round_robin());

    // N roots, not 1.
    let unique: HashSet<Distinction> = finals.iter().copied().collect();
    assert_eq!(
        unique.len(),
        SEEDS.len(),
        "the composition collapsed to {} distinct roots for {} agents — it \
         is behaving as a single-root LCA",
        unique.len(),
        SEEDS.len()
    );

    // Each root in-graph and grounded.
    for (i, &r) in finals.iter().enumerate() {
        assert!(fx.engine.has(r), "agent {i}'s final root is not in the graph");
        assert!(
            reaches_primordial(&fx.engine, r),
            "agent {i}'s final root does not reach a primordial"
        );
    }

    // The roots are a subset of the engine's own inventory.
    let mut inventory = fx.engine.snapshot_distinctions();
    inventory.sort_unstable();
    for (i, &r) in finals.iter().enumerate() {
        assert!(
            inventory.binary_search(&r).is_ok(),
            "agent {i}'s root is absent from snapshot_distinctions()"
        );
    }
    invariant_holds(&fx.engine);
}

// ---------------------------------------------------------------------------
// Domain separation
// ---------------------------------------------------------------------------

/// **Different anchors separate the same payload.
/// Category: DOMAIN-SEPARATION.**
///
/// Not A1's negative control. Two agents using different domain anchors are
/// not performing the same operations — they specified different domains —
/// so this does not falsify determinism; it verifies the dual property,
/// that different inputs *do* separate.
///
/// `DEC-4` closed the hash-level-domain-tag branch on theory grounds:
/// separation is an anchor parameter of the fold (`docs/SPEC.md` §4.2),
/// never a tag inside the identifier.
///
/// *Passes* → a consumer that wants separation can have it without the
/// substrate growing a naming layer.
///
/// *Fails* → either the anchor is not load-bearing (two domains collide,
/// and separation is unavailable), or folds differ when they should not.
#[test]
fn distinct_anchors_separate_same_payload() {
    let fx = setup();
    let anchor_a = domain_anchor(&fx.engine, &fx.fold, DomainLabel::new(1));
    let anchor_b = domain_anchor(&fx.engine, &fx.fold, DomainLabel::new(2));
    assert_ne!(anchor_a, anchor_b, "two domain labels produced the same anchor");

    let payload = b"same-payload";
    let lift_a = fx.fold.fold(anchor_a, payload, &fx.engine);
    let lift_b = fx.fold.fold(anchor_b, payload, &fx.engine);
    assert_ne!(
        lift_a, lift_b,
        "the same bytes under two different domain anchors collapsed to one \
         identifier — domain separation is unavailable"
    );

    // And the fold itself is not a source of divergence: a second engine's
    // fold agrees on every tip, which is why the anchor is the only lever.
    let other = Arc::new(DistinctionEngine::new());
    let other_fold = ByteFold::new(&other);
    for b in 0u8..=255 {
        assert_eq!(
            fx.fold.fold(fx.engine.d0(), &[b], &fx.engine),
            other_fold.fold(other.d0(), &[b], &other),
            "two independently built folds disagreed on the tip for byte {b}"
        );
    }
    invariant_holds(&fx.engine);
}

// ---------------------------------------------------------------------------
// A1 under contention lives in concurrent_composition.rs; this keeps the
// single-threaded novelty reading honest about what it observed.
// ---------------------------------------------------------------------------

/// **Novelty is a property of engine state, not of the identifier.
/// Category: GUARD.**
///
/// Supporting assertion for F1′: the first agent to synthesize a pair sees
/// `Novel`, and every later agent synthesizing the same pair sees
/// `Existing` with the identical identifier.
///
/// *Passes* → `docs/SPEC.md` §2.2's reading survives composition.
///
/// *Fails* → novelty is being reported per-agent, which would make it a
/// property of the asker.
#[test]
fn novelty_is_engine_state_not_agent_state() {
    let fx = setup();
    let pair = (fx.roots[0], fx.roots[1]);

    // Read through the accessors rather than matching: `SynthesisOutcome` is
    // `#[non_exhaustive]`, so a match here would need a wildcard arm that
    // silently absorbs any variant added later — exactly the shape that lets
    // a test keep passing after the thing it tests has changed.
    let first = fx.engine.synthesize_novel(pair.0, pair.1);
    assert!(first.is_novel(), "a pair this engine had never seen was not reported Novel");

    let second = fx.engine.synthesize_novel(pair.0, pair.1);
    assert!(
        !second.is_novel(),
        "a pair already witnessed was reported Novel again — novelty is \
         being reported per-asker rather than per-engine"
    );
    assert_eq!(
        first.distinction(),
        second.distinction(),
        "the same pair yielded two identifiers across two agents"
    );
    invariant_holds(&fx.engine);
}
