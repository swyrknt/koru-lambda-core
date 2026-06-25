# Distinction Theory

The substrate `koru-lambda-core` implements is small enough to state on one
page. It has one operator, four axioms, two primordials, and a handful of
structural laws that follow as consequences. The rest of this document
elaborates each. Nothing here references code or implementation choices.

---

## The operator

There is one operator:

> **synthesize(a, b) → c**

`a`, `b`, and `c` are *distinctions* — the only kind of thing the theory
talks about. A distinction has no internal structure beyond its identity.
Identity is content-addressed (see Axiom 4).

---

## The primordials

There are two distinctions that exist by definition:

- **d₀** — the first primordial
- **d₁** — the second primordial

They are the only distinctions whose existence is asserted rather than
derived. Every other distinction comes from a synthesis of two
previously-existing ones. By convention, d₀ and d₁ have a single
"genesis" relationship between them: the edge `d₀ ↔ d₁` is part of the
graph without being the result of any synthesis.

---

## The four axioms

The behaviour of `synthesize` is constrained by four axioms. The substrate
is correct iff all four hold for every input.

### Axiom 1 — Determinism
`synthesize(a, b)` always produces the same distinction. No randomness,
no time-dependence, no engine-state-dependence on the output's identity.

### Axiom 2 — Commutativity
`synthesize(a, b) = synthesize(b, a)`. Argument order is not part of
the structure. Implementations canonicalize the pair before hashing.

### Axiom 3 — Irreflexivity
`synthesize(a, a) = a`. A distinction synthesized with itself produces
itself — no new structure. (See "Mediated self-reference" below for the
loophole that lets self-aware systems still produce novelty.)

### Axiom 4 — Content addressing
A distinction's identity *is* the canonical hash of its parents' canonical
pair. The bytes of `c` literally encode the way `c` was made. There is
no separate naming layer.

These four are non-negotiable. Any change that violates them is wrong,
regardless of what it enables.

---

## Structural laws (consequences of the axioms)

The axioms produce a graph with predictable properties.

### Law 5 — Binary parentage
Every non-primordial distinction has exactly two parents. Not one, not
three. The synthesize operator is binary, and content addressing
encodes both parents in the child's bytes.

### Law 6 — r = 2d − 3
For a graph with `d` distinctions and `r` relationships, `r = 2d − 3`
holds exactly at every scale. Each novel synthesis adds one new
distinction and two new parent-child edges; the `−3` accounts for the
genesis state (d=2, r=1: only the d₀↔d₁ edge).

**Implementation note:** the d₀↔d₁ genesis edge is a *theory-level*
relationship. Implementations may carry it implicitly (the `+ 2` term in
the invariant check `all_distinctions.len() == parents_of.len() + 2`) or
materialize it explicitly. Either choice satisfies Law 6; neither
violates Law 5 (binary parentage) because the primordials by definition
have no parents.

### Law 7 — Saturation
Repeating the same synthesis adds nothing. `synthesize(a, b)` called
twice in succession produces the same distinction and leaves the graph
unchanged on the second call.

### Law 8 — Engine independence
Two engines that process the same operations produce byte-identical
state, regardless of their independent histories. Content addressing
means identity is the operation; distinct paths through the operation
space converge.

### Law 9 — Order-independent reconstruction
Two engines synthesizing the same parent pairs converge to byte-identical
state regardless of the order in which they apply the pairs. Replay is
content-validated, not chronologically dependent.

### Law 10 — Mediated self-reference → infinite novelty
Direct `synthesize(x, x)` is irreflexive (Axiom 3) and produces no novelty.
But `synthesize(synthesize(x, observation), x)` produces a unique
distinction at every depth when `observation` is a deterministic function
of the recursion depth (the warroom experiments verified this for both
cycling and constant `observation`; random `observation` is also unique
but loses the "self-referential" framing — the loophole is about
*structured* mediation, not about novelty in the abstract). This is the
structural loophole that lets the substrate represent self-aware systems.

### Law 11 — Fold Law
The two primordials become topological mega-hubs. By construction, byte
folding through `synthesize` routes every byte through `d₀` and `d₁`
multiple times, making them appear on every derivation path. The
"depth ≤ 8" bound comes directly from the `ByteMapping` implementation:
each byte folds through 8 synthesis steps (one per bit), with `d₀` and
`d₁` participating in every step. Above depth 8, the topology is no
longer fold-determined — Coding Law takes over.

### Law 12 — Coding Law
Beyond the fold layer, degree-centrality tracks usage frequency. The
Spearman rank correlation between a distinction's degree (its total
synthesis participations) and its frequency-of-use settles around
ρ ≈ 0.99 with run-to-run noise of ~0.005 against the workload exp18
implements (length-N chain pool, Zipf-sampled index pairs). The v2.0
gate is ρ ≥ 0.985 — tight enough to falsify any structural regression
that perturbs the degree-counting hot path, loose enough to absorb
measurement noise. High-attention distinctions become high-degree nodes
naturally — no PageRank, no curator, no learning algorithm.

---

## What it means to "use" the substrate

The substrate by itself is timeless. It enforces the axioms; it knows
about distinctions and parents; it does not know about order, history,
identity-of-perspective, or causality.

A productive consumer of the substrate is a **Local Causal Agent (LCA)**:

13. Every LCA anchors to a **local root distinction** — its perspective.
14. State transitions are **causal syntheses** from the local root + the
    canonical structure of the action being taken.
15. LCAs **update their perspective forward** as their causal chain
    advances.

This is the only pattern the substrate sanctions for "using" it. Time is
what LCAs do.

The trait `LocalCausalAgent` in the codebase formalizes this pattern. It
is substrate, not subsystem — the trait IS the consumer contract.

---

## What follows from the theory

The theory's promises, taken together, give the substrate properties
that are difficult or impossible to obtain through other means:

- **Deterministic distributed state without consensus.** Engine
  independence (Law 8) means two machines processing the same operations
  produce identical state. The hash function IS the consensus protocol.

- **Append-only by construction.** There is no `remove_distinction`
  primitive in the theory. The graph is monotone — synthesized
  distinctions exist forever. Auditability is structural.

- **Emergent attention without orchestration.** Coding Law (Law 12) means
  high-usage distinctions become high-degree nodes by physics, not policy.

- **Self-aware systems via mediated self-reference.** Law 10 is the
  structural answer to "how do you have a distinction that refers to
  itself without the irreflexivity axiom collapsing it?" — you mediate
  through observation.

- **Identity is process.** Content addressing (Axiom 4) means a
  distinction's identity literally encodes how it was produced. There is
  no separate naming, schema, or coordination layer.

These properties are not engineered features. They fall out of the four
axioms. The implementation's job is to enforce the axioms at every code
path; everything else follows.

---

## Why this is small

The theory has been deliberately compressed. One operator. Four axioms.
Two primordials. Eight structural laws as named consequences. The LCA
pattern as a single contract.

Smallness is load-bearing. Every implementation must enforce the axioms;
every consumer must conform to the LCA pattern. If the theory's surface
were larger, the implementation surface would be larger, and divergence
between consumers would be inevitable.

The bet is that a small, complete theory produces a substrate strong
enough to carry both a cognitive architecture (ALIS) and an economic
protocol (koru) without either feeling like a hack on top of the other.

The 50+ experiments in the warroom record stress-test every claim above
at scales appropriate to each claim — axioms and small-graph invariants
at 10K–1M, the structural laws and Coding/Fold gates at 1M–5M, with
single-engine memory ceilings on commodity laptops cresting ~80M. Zero
exceptions to the axioms or structural laws have been found within the
probed range. Claims at scales beyond what was actually run are
extrapolations from the four axioms, not empirical observations.
