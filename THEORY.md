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

### Law 10 — Mediated self-reference → unbounded novelty (within ID space)

Direct `synthesize(x, x) = x` (Axiom 3, irreflexivity). A distinction
synthesized with itself produces itself — no new structure. This is
load-bearing: without irreflexivity, the substrate could fall into
trivial recursive loops where every node generates new structure forever
without anchoring to anything.

But `synthesize(synthesize(x, observation), x)` produces a unique
distinction at every depth when `observation` differs at each step
(the warroom experiments verified this for both cycling and constant
`observation` schedules). The mediation through `observation` breaks
the irreflexive collapse: the inner synthesis produces something that's
no longer equal to `x`, so the outer synthesis is no longer reflexive.

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
Every distinction synthesized goes into `parents_of` and `degree_counts`;
no hidden stack growth.

**Bounded by ID space, not by depth.** Distinctions are 16 bytes (128
bits of identity), so the substrate carries at most 2^128 distinct
distinctions and the birthday bound kicks in at ~2^64. The probes
verify uniqueness at depth ≥ 10K (CHECKLIST.md Step 1); above that the
limit is hash-space exhaustion, not the self-reference loophole. "Unbounded"
means "not bounded by the irreflexivity axiom," not literally infinite.

### Law 11 — Fold Law
The two primordials become topological mega-hubs. By construction, byte
folding through `synthesize` routes every byte through `d₀` and `d₁`
multiple times, making them appear on every derivation path. The
"depth ≤ 8" bound comes directly from the `ByteMapping` implementation:
each byte folds through 8 synthesis steps (one per bit), with `d₀` and
`d₁` participating in every step.

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
Beyond the fold layer, degree-centrality tracks usage frequency. The
law has two parts that should not be conflated:

**Structural part (universal, by construction):** `degree(d) = count of
novel synthesis participations` plus the genesis addend. This is
*definitional* — it's what `degree_counts.fetch_add(1, Release)` does
inside the entry-gated `synthesize` closure. It holds under any workload,
including adversarial ones, because it's how the engine is built. There
is no workload where this fails without the engine being broken.

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

## What it means to "use" the substrate

The substrate by itself is timeless. It enforces the axioms; it knows
about distinctions and parents; it does not know about order, history,
identity-of-perspective, or causality.

The substrate's **reference consumption pattern** is the Local Causal
Agent (LCA):

13. An LCA anchors to a **local root distinction** — its perspective.
14. State transitions are **causal syntheses** from the local root + the
    canonical structure of the action being taken.
15. LCAs **update their perspective forward** as their causal chain
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

The trait lives at `src/agent.rs` (substrate level, not subsystems)
because it formalizes the canonical *consumer contract*, not because
it's the only legal way to consume the substrate. Future consumers
that need different shapes are welcome; they'll just write their own
traits and lose the interop benefits the LCA contract provides.

---

## What follows from the theory

The theory's promises, taken together, give the substrate properties
that are difficult or impossible to obtain through other means:

- **Deterministic distributed state without consensus.** Engine
  independence (Law 8) means two machines processing the same operations
  produce identical state. The hash function IS the consensus protocol.

- **Append-only by design choice (not by axiom).** The four axioms
  don't forbid removal — they constrain *what `synthesize` does*, not
  what other operations the engine exposes. We *chose* append-only
  because it's what makes engine-independence (Law 8) and order-
  independent reconstruction (Law 9) hold cleanly: if the graph is
  monotone, replay from any subset converges. Garbage collection by
  partial-reroot replay is theoretically permissible; v2.0 deliberately
  excludes it. Auditability is therefore a *design* property, enabled by
  but not forced by the theory.

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

## Why the theory is small

One operator. Four axioms. Two primordials. Eight structural laws as
named consequences. One reference consumption pattern.

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
an open question; whether it holds for the two we have is what the
warroom experiments and v2.0 release are about.

The 50+ experiments in the warroom record stress-test every claim above
at scales appropriate to each claim — axioms and small-graph invariants
at 10K–1M, the structural laws and Coding/Fold gates at 1M–5M.

**Memory ceiling at ~80M is conservative arithmetic, not extrapolation.**
The per-distinction footprint was measured at 1M scale (~80 B in the
warroom v2.0 attempt; predicted ~180 B in v2.0 with the dhat-honest
gate that includes DashMap shard slack). Available RAM on a 16 GB
laptop after OS overhead is ~12 GB. 12 GB / 180 B ≈ 70M; 12 GB / 80 B
≈ 150M. So ~80M is a round number on the conservative side of the
arithmetic. The math is just division; it doesn't require empirical
validation at 80M to be reliable as a *memory* claim.

**Throughput at scale is the actual untested question.** Single-thread
~500K ops/sec and 8-thread ~12M+ ops/sec were measured at 1M-5M scale.
Whether those numbers hold at 50M+ depends on cache effects past L3
(~8-32 MB), DashMap shard collision rate under sustained load, allocator
paging when the engine consumes most of system RAM, and IdentityHasher
bucket distribution at large N — none of which we've probed. A ceiling
probe at 50M+ is on the Step 4 backlog. Until it runs:

- **Memory ceiling claim:** confident (arithmetic from measured footprint).
- **Throughput at ceiling claim:** unknown (cache effects, allocator paging
  could non-linearize before memory runs out).

Zero exceptions to the axioms or structural laws have been found within
the probed range. Claims at scales beyond what was actually probed are
deductions from the axioms (which hold at all scales) plus extrapolation
of empirical throughput (which doesn't).
