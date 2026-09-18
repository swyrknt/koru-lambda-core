# Distinction Theory

`[SHIPPED: axioms + Laws 5-12 correspond to code state]`

The substrate `koru-lambda-core` implements is small enough to state on one
page. It has one operator, four axioms, two primordials, and a handful of
structural laws that follow as consequences. Where a claim references
code, an inline `[SHIPPED]` or `[TARGET]` tag records whether the
correspondence is in-tree today or scheduled; theoretical claims carry no
tag. Version binding lives in `DESIGN.md`.

---

## Downstream Discipline

THEORY.md is upstream of `DESIGN.md`, `ARCHITECTURE.md`, `README.md`, and
`docs/**`; every paraphrase is where drift lives.

**Two-anchor policy.** Downstream references to concept-terms in this
document (`synthesize`, `commutativity`, `irreflexivity`,
`content addressing`, `Law 5`–`Law 12`, `Fold Law`, `Coding Law`, `LCA` /
`local causal agent`, `synthesis/projection dual`) resolve to either
`THEORY.md#section-slug` (default — section anchors survive edits) or
`THEORY.md:LINE` (verbatim-quote-only). Matches inside `[SHIPPED]` or
`[TARGET]` tag blocks are exempt — those anchor to code by construction.

---

## The operator

There is one operator:

> **synthesize(a, b) → (c, novel?)**

`a`, `b`, and `c` are *distinctions* — the only kind of thing the theory
talks about. A distinction has no internal structure beyond its identity.
Identity is content-addressed (see Axiom 4). The `novel?` bit
distinguishes a first-time derivation from a saturated repeat (Law 7);
implementations may expose or discard it, but the theory asserts its
existence as a structural output of the operator.

**Two-type discipline (Axiom-4 closure).** Bytes typed as `Distinction`
that were not produced by the operator are structurally illegitimate.
The theory forces a boundary between verified distinctions and raw
byte payloads; how a given implementation closes that boundary is a
design concern (see `DESIGN.md` and `ARCHITECTURE.md`).

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

*The koru substrate is small enough to state on one page, and its
consequences carry domains that normally require their own foundations.
What follows are the four axioms and the eight structural laws they
force.*

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

The d₀↔d₁ genesis edge is a *theory-level* relationship;
implementations may carry it implicitly or materialize it explicitly.
Either choice satisfies Law 6 without violating Law 5; primordials have
no parents by definition. See `ARCHITECTURE.md` for how the current
substrate materializes the addend.

### Law 7 — Saturation
Repeating the same synthesis adds nothing. `synthesize(a, b)` called
twice in succession produces the same distinction and leaves the graph
unchanged on the second call.

### Law 8 — Engine independence
Two engines that process the same operations produce byte-identical
state **at quiescence**, regardless of their independent histories.
Mid-flight the two engines may transiently disagree while writes are
in progress; the claim is post-processing convergence, not
instantaneous equality. Content addressing means identity is the
operation; distinct paths through the operation space converge.

### Law 9 — Order-independent reconstruction
Two engines synthesizing the same parent pairs converge to byte-identical
state regardless of the order in which they apply the pairs. Replay is
content-validated, not chronologically dependent.

### Law 10 — Mediated self-reference → unbounded novelty (within ID space)

Direct `synthesize(x, x) = x` (Axiom 3, irreflexivity). A distinction
synthesized with itself produces itself — no new structure. This is
load-bearing: without irreflexivity, the substrate could fall into
trivial recursive loops where every node generates new structure forever
without anchoring to anything.

But `synthesize(synthesize(x, observation), x)` produces a unique
distinction at every depth when `observation` differs at each step —
whether `observation` cycles through a finite alphabet or holds constant
at some `obs`, the inner synthesis produces something no longer equal to
`x`, so the outer synthesis is no longer reflexive. The mediation
through `observation` breaks the irreflexive collapse.

**Why this matters structurally.** Most computational systems that try
to represent themselves hit one of three failure modes:

1. **Infinite recursion.** The system tries to model itself as part of
   its state. Each level of self-modeling requires another level
   underneath it. Stack blows up. (Naive self-reference.)
2. **Explicit fixed-point machinery.** Gödel encoding, Y combinator,
   typed reflection. These work, but they require careful construction
   and don't compose naturally with the system's main operations.
3. **Forbid self-reference.** Many systems just don't allow it.
   Restrictive but simple.

Mediated self-reference is a fourth path. The substrate has self-reference
*as a structural primitive*: `synth(synth(self, obs), self)` is a legal
composition of `synthesize` calls with no special-case machinery. The
result is a distinction that genuinely *refers to* `self` while being
distinct from `self`. It composes cleanly with everything else the
substrate does because synthesis is the only operation.

**Why it doesn't infinite-recurse.** Each `(synth(synth(x, obs_n), x))_n`
chain produces a *finite* chain of distinct distinctions, one per
recursion depth, terminated by whatever stopping condition the consumer
chooses. The substrate doesn't enforce termination — that's the
consumer's job — but it doesn't enable runaway recursion either.
Every distinction synthesized goes into the engine's `nodes` map
(with its `parents` and `degree` fields populated); no hidden stack
growth.

**Bounded by ID space, not by depth.** Distinctions are 16 bytes (128
bits of identity), so the substrate carries at most 2^128 distinct
distinctions and the birthday bound kicks in at ~2^64. Uniqueness at
depth ≥ 10K is a probed property (see the mediated-self-reference
uniqueness probe); above that the limit is hash-space exhaustion, not
the self-reference loophole. "Unbounded" means "not bounded by the
irreflexivity axiom," not literally infinite.

### Law 11 — Fold Law
The two primordials become topological mega-hubs. By construction, byte
folding through `synthesize` routes every byte through `d₀` and `d₁`
multiple times, making them appear on every derivation path. The
"depth ≤ 8" bound is **structural**: the canonical byte-folding
construction folds each input byte through exactly 8 `synthesize`
steps (one per bit), with `d₀` and `d₁` participating in every step.
The bound is a direct consequence of that construction, not a workload
artifact.

**Fold Law and Coding Law both operate at all depths.** They're not
sequential — there's no "handoff" at depth 8. Fold Law dominates the
*topology* at shallow depths (≤ 8) because byte-fold structure mechanically
routes paths through d₀/d₁ before usage patterns have had time to
concentrate elsewhere. Coding Law dominates the *content distribution*
beyond the fold layer because most distinctions exist there and usage
concentration drives degree centrality. Both are observable simultaneously
in a mature engine; the "depth" distinction is about which mechanism
explains the most variance in degree-centrality at that depth.

### Law 12 — Coding Law
Across the whole graph, degree-centrality tracks usage frequency. The
law has two parts that should not be conflated:

**Structural part (universal, by construction):** `degree(d) = count of
novel synthesis participations` plus the genesis addend. This is
*definitional* — every axiom-consistent implementation increments the
count on the parents each time a novel synthesis is registered. It
holds under any workload, including adversarial ones, because it's a
property of how the operator writes the graph. There is no workload
where this fails without the engine being broken.

**Empirical part (workload-conditional):** the Spearman rank correlation
between `degree(d)` and `frequency_of_use(d)` — where frequency counts
*all* draws including saturated repeats — settles around ρ ≈ 0.99 with
run-to-run noise of ~0.005 against the workload exp18 implements
(length-N chain pool, Zipf-sampled index pairs at α=1.0). The v2.0 gate
is ρ ≥ 0.985 on this pinned workload.

**Why ρ < 1.0 even on natural workloads:** saturation drift. When a
workload repeatedly samples the same pair, `frequency` keeps
incrementing but `degree` does not (saturation: the second
`synthesize(a, b)` returns the existing child without bumping degree).
Under heavy saturation (small pool, large M, narrow Zipf), ρ drops
below 1. Under low saturation (large pool, small M, broad distribution),
ρ → 1. The structural part is universal; the gap between structural and
empirical is exactly the saturation effect.

**What "Coding Law" the law refers to:** the structural part — degree
tracks the count of novel participations *by construction*. The empirical
ρ measurement is how we *verify* the substrate is built correctly. Calling
the structural part "workload-conditional" would understate what the
substrate guarantees; calling the empirical part "universal" would
overclaim what the measurement establishes.

High-attention distinctions become high-degree nodes naturally — no
PageRank, no curator, no learning algorithm — *given* the workloads
empirical systems typically produce.

---

## The synthesis/projection dual

The axioms describe `synthesize` directly, but they produce more than one
object. The first is what the axioms name explicitly: the append-only,
content-addressed graph, extended one distinction at a time by the
operator or left unchanged under Law 7. This is the *write dual* — every
axiom-consistent act of the operator adds structure, or none at all.

The second is what the axioms produce by implication: any axiom-consistent
read of the graph from a vantage. Walking parents from a chosen
distinction, counting participations, asking whether one distinction is
downstream of another, materializing children under some synthesis-count
boundary — each is a *projection*, a coherent read from a specified root,
at a specified boundary, in a specified direction, over a specified
signal. The projection is not new structure; it is the graph *as viewed
from somewhere.*

The projection is theory-forced. Axioms 1 and 4 make the graph a pure
function of its inputs; Laws 8 and 9 make it identical across engines
with the same synthesis history; Law 7 makes it stable under repeated
observation. Any read that respects those constraints IS a projection,
whether the reader knows it or not.

The substrate has, until now, exposed only the write dual. The read dual
has lived in six ad-hoc reinventions across the ecosystem (ALIS `Field`,
koru-engine `Field`, koru-spatial adjacency, koru-mesh seen-set,
koru-delta replay-to-point, koru-wave dissolve-on-read). Naming it here
does not add a primitive; it names what the axioms already produce.
First-class API surface for the projection is `[TARGET: first-class API]`;
the theoretical claim itself carries no tag.

**Falsifier.** Cross-engine projection independence probe — two engines
with identical synthesis histories, queried with the same projection
`{ Root, boundary, Direction, Signal }` at quiescence, must produce
byte-identical output. If not, the projection is an engine-side
artifact, not a theory-forced object, and this section is wrong.

---

## What it means to "use" the substrate

The substrate by itself is timeless. It enforces the axioms; it knows
about distinctions and parents; it does not know about order, history,
identity-of-perspective, or causality.

The substrate's **reference consumption pattern** is the Local Causal
Agent (LCA):

- An LCA anchors to a **local root distinction** — its perspective.
- State transitions are **causal syntheses** from the local root + the
  canonical structure of the action being taken.
- LCAs **update their perspective forward** as their causal chain
  advances.

**This is a pattern, not an axiom.** The four axioms don't *require*
consumers to be LCAs; they constrain what `synthesize` does, not how
consumers structure their use of it. A consumer with a different
shape — multi-perspective agents that anchor to several roots
simultaneously, consumers that synthesize from non-root state, or
consumers that ignore perspective entirely and just record raw
synthesis events — could exist and still get the substrate's
correctness guarantees.

The LCA pattern is the *reference* because:

- It's how every consumer we've actually built (ALIS, koru-protocol,
  the reference subsystems) uses the substrate.
- It cleanly captures "time is what consumers do" — the substrate
  stays timeless, the LCA carries the causal chain.
- The trait `LocalCausalAgent` makes the pattern composable, so
  multi-consumer systems can interoperate without re-inventing the
  perspective discipline.

The LCA contract is a substrate-level construct — not a subsystem —
because it formalizes the canonical *consumer contract*, not because
it's the only legal way to consume the substrate. Future consumers
that need different shapes are welcome; they'll simply write their own
contracts and lose the interop benefits the LCA one provides.
Implementation details for the reference LCA trait live in
`ARCHITECTURE.md`.

---

## Implications not yet materialized

The axioms force consequences the substrate does not yet expose. Naming
them here fixes the theoretical status; API surface is downstream.

- **The novelty bit as a first-class outcome.** Law 7 makes *novel vs.
  saturated* a structural binary the operator computes on every call;
  the current API discards it, returning a bare `Distinction`. Exposing
  `SynthesisOutcome { child, was_novel }` lets consumers compute
  novelty-rate signals over projections without recomputing state.
  Theory-forced; API `[TARGET]`.

- **Intrinsic degree-frequency coupling.** Coding Law's structural part
  makes the graph *self-attending by construction* — reframing what
  earlier drafts called "emergent attention." The closer's bullet states
  the consequence in its final form; cataloged here so the implication
  is preserved rather than lost in the reframe.

---

## Law → downstream consequence

Three-row legend for contributors carrying theory into downstream docs
(translation, not survey — Coding Law and Fold Law's consumer
implications belong in `DESIGN.md`):

| Law | Consequence for downstream |
|-----|---------------------------|
| Law 8 (engine independence, at quiescence) | State agreement without consensus |
| Projection dual (this document, § between Law 12 and LCA) | One primitive; the ecosystem currently reinvents it once per domain (cognition, physics, spatial, mesh, wave, delta) |
| Law 10 (mediated self-reference) | Self-reference without reflection primitives |

---

## What follows from the theory

The theory's promises, taken together, give the substrate properties
that are difficult or impossible to obtain through other means:

- **Deterministic distributed *state* agreement without a consensus
  protocol.** Engine independence (Law 8) means two machines processing
  the same operations produce identical state. The hash function IS the
  consensus mechanism for state and identity. **The substrate does not
  solve inclusion** (which operations belong in the canonical set),
  **liveness** (when a batch is finalized), **attribution** (who
  performed an operation), or **censorship resistance** (preventing
  withholding) — those concerns require knowing about peers, time, and
  actor identity, which the substrate is designed not to know. Consumer
  protocols (koru-protocol's leader election + commitment schemes) layer
  on top to solve those. The substrate's contribution is removing the
  state-agreement problem entirely; consumers handle the rest.

- **Append-only by design choice (not by axiom).** The four axioms
  don't forbid removal — they constrain *what `synthesize` does*, not
  what other operations the engine exposes. We *chose* append-only
  because it's what makes engine-independence (Law 8) and order-
  independent reconstruction (Law 9) hold cleanly: if the graph is
  monotone, replay from any subset converges. Garbage collection by
  partial-reroot replay is theoretically permissible; v2.0 deliberately
  excludes it. Auditability is therefore a *design* property, enabled by
  but not forced by the theory.

- **Intrinsic degree-frequency coupling.** Coding Law (Law 12) means
  degree-centrality tracks usage frequency by construction, not by
  overlaid learning. The graph *attends to what it uses* — usage
  weighting is intrinsic to topology, produced by the operator rather
  than layered on top. Same phenomenon downstream disciplines call
  "attention," but without a trained layer.

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

## Why the theory is small

One operator that produces the graph and, over it, the projection —
the theory's read-side dual. Four axioms. Two primordials. Eight
structural laws as named consequences. One reference consumption
pattern.

Smallness is a deliberate constraint, not an accident. Every implementation
that claims to be `koru-lambda-core` must enforce the four axioms exactly
— there's nowhere for a violation to hide because the surface is so
small. Every consumer that wants the substrate's correctness guarantees
gets them whether or not they use the LCA pattern, because the
guarantees flow from the axioms, not from the consumer's shape.

The bet: a small, internally-consistent theory produces a substrate
strong enough to carry domains that usually require their own,
domain-specific foundations. The current evidence is two consumers
(ALIS for cognition, koru-protocol for economic consensus) both built
on the same engine. Whether the bet generalizes to a third domain is
an open question; whether it holds for the two we have is the
substrate's ongoing test.

Zero exceptions to the axioms or structural laws have been found within
the probed range. Claims at scales beyond what was actually probed are
deductions from the axioms (which hold at all scales); throughput at
those scales is an open question. Capacity and throughput evidence lives
in `docs/BENCHMARKS.md`.

---

*Empirical evidence: `docs/BENCHMARKS.md`, `tests/coding_law.rs`.*
