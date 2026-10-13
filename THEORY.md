# Distinction Theory

One operator, four axioms, two primordials, and the structure they force.

This document is rewritten rather than amended. The version it replaces had
accumulated four internal contradictions over two years — it named six
converging domains in one section and two consumers in its closing paragraph,
stated a law flatly and retracted it ten lines later, described the same
construct as both "a pattern, not an axiom" and "a substrate-level construct,"
and cited a probe that did not exist. Those are what amendment produces, and
editing them out one at a time preserves the structure that allowed them. The
previous version remains in version control.

---

## Reading this document

Every claim below carries a status, because the distinction between what the
axioms *force* and what we *chose* is the one that downstream documents get
wrong, and the one a reader needs in order to argue with us.

| Status | Means |
|---|---|
| **Axiom** | Asserted. Not derived, not negotiable. |
| **Derived** | Follows from the axioms. A proof sketch is given. |
| **Encoding** | A property of an encoding we chose. A different encoding changes it. |
| **Probed** | Empirical, with the probe named. Run it; it is in the repository. |
| **Framework** | A commitment the formalism permits and does not establish. |

A claim with no status is a mistake in this document.

## Downstream discipline

This document is upstream of `ARCHITECTURE.md`, `ECOSYSTEM.md`,
`README.md`, and `docs/**`. Every paraphrase is where drift lives.

Downstream references to concept-terms here (`synthesize`, `commutativity`,
`idempotence`, `content addressing`, `Law 5`–`Law 12`, `Fold Law`,
`Coding Law`, `local causal agent`, `synthesis-projection dual`) resolve to
`THEORY.md#section-slug`. Section anchors survive edits; line numbers do not,
and are rejected by `scripts/gates/doc_anchors.sh`.

`docs/SPEC.md` is normative for bytes. Where this document and the
specification disagree about what an implementation must produce, the
specification wins.

---

## The operator

There is one:

> **synthesize(a, b) → (c, novel?)**

`a`, `b` and `c` are *distinctions* — the only kind of thing the theory talks
about. A distinction has no internal structure beyond its identity. The
`novel?` bit distinguishes a first derivation from a saturated repeat;
implementations may expose or discard it, and this one exposes it
`[SHIPPED @ engine::SynthesisOutcome]`.

**Axiom.** Bytes typed as a distinction that the operator did not produce are
structurally illegitimate. The theory forces a boundary between verified
distinctions and raw payloads; how an implementation closes that boundary is a
design concern, and this one closes it at runtime by refusing to synthesize
against a parent the engine never witnessed.

## The primordials

Two distinctions exist by assertion rather than derivation: **d₀** and **d₁**.
Every other distinction is a synthesis of two that already exist. By
convention the edge `d₀ ↔ d₁` is in the graph without being the result of any
synthesis — the one edge not derivable from parentage.

## The four axioms

The substrate is correct if and only if all four hold for every input.

**Axiom 1 — Determinism.** `synthesize(a, b)` always produces the same
distinction. No randomness, no time-dependence, no dependence of the output's
identity on engine state.

**Axiom 2 — Commutativity.** `synthesize(a, b) = synthesize(b, a)`. Argument
order is not part of the structure.

**Axiom 3 — Idempotence.** `synthesize(a, a) = a`. A distinction
synthesized with itself produces itself. This is load-bearing: it forecloses
runaway self-generation, and the cost is that direct self-reference is
degenerate. See *Mediated self-reference*.

*This axiom was called "Irreflexivity" until 2026-10-05, and the name was
wrong. Irreflexivity is a property of a relation — no element stands in it to
itself. Idempotence is the property of a binary operation that `f(a, a) = a`,
which is what is written here. The old name described a **consequence**: the
parentage relation is irreflexive, because no distinction is its own parent.
Spencer-Brown calls exactly this law — `αα ≈ α` — the Law of Calling, which
is the correspondence worth keeping. Downstream documents still carry the old
name; correcting them is tracked.*

**Axiom 4 — Content addressing.** A distinction's identity *is* the hash of
its parents' canonical pair. The bytes of `c` encode how `c` was made. There
is no separate naming layer.

Any change that violates one of these is wrong, whatever it enables.

---

## The laws

Eight numbered consequences. Law 11 is numbered but is **not** an independent
law — see its entry. Renumbering would break every downstream reference, so the
number stays and the status travels with it.

### Law 5: Binary parentage

**Derived.** Every non-primordial distinction has exactly two parents. The
operator is binary and Axiom 4 encodes both parents in the child.

### Law 6: r = 2d − 3

**Derived, and probed.** For `d` distinctions and `r` relationships,
`r = 2d − 3` holds exactly at every scale. Each novel synthesis adds one
distinction and two parent edges; the `−3` accounts for the genesis state
(`d = 2`, `r = 1`).

*Probe:* `tests/scale.rs` — zero deviation at 5 million syntheses.

Mean degree therefore converges to 4 at every scale under every workload, so
hub degree is paid for by leaves. The genesis edge is a theory-level
relationship; an implementation may carry it implicitly or materialize it.

### Law 7: Saturation

**Derived.** Repeating a synthesis adds nothing. The second
`synthesize(a, b)` returns the same distinction and leaves the graph
unchanged.

### Law 8: Engine independence

**Derived, and probed.** Two engines that process the same operations produce
byte-identical state **at quiescence**, regardless of their independent
histories. Mid-flight they may transiently disagree while writes are in
progress; the claim is post-processing convergence, not instantaneous
equality.

### Law 9: Order-independent reconstruction

**Derived, and probed.** Two engines synthesizing the same parent pairs
converge to byte-identical state regardless of the order in which they apply
them. Replay is content-validated, not chronological.

*Probe for 8 and 9:* `tests/order_independence.rs`. This is the experiment
this document names as the one that would refute its central claim, and for
most of the project's life nothing ran it — the test that looked like the
falsifier drove both engines from one corpus in one order, which establishes
Axiom 1 and not these two laws.

It now permutes 600 parent pairs into a different *topological* order —
`synthesize` rejects an unwitnessed parent, so the only legal reorderings are
topological — asserts the permutation differs from the original at 595 of 600
positions before comparing anything, and then compares counts, the distinction
set, parentage edge for edge, and `projection_id` plus canonical bytes across
all eighteen axis combinations.

**Consequence worth stating plainly, because the architecture rests on it:**
every engine is a partial witness of one grow-only structure. Agents agree on
identity by construction and differ only in what they have witnessed — novelty
and degree are properties of an engine's state, not of an identifier. Two
agents necessarily agree on *what a thing is* and may permanently disagree on
whether they saw it first.

### Law 10: Mediated self-reference

**Derived, and probed.** Direct self-reference is degenerate by Axiom 3. But
`synthesize(synthesize(x, observation), x)` produces a distinction that
genuinely refers to `x` while being distinct from `x`, for any observation not
equal to `x`. The mediation breaks the collapse Axiom 3 would
otherwise force.

This is self-reference as a structural primitive — a legal composition of the
one operator, with no fixed-point machinery, no encoding trick, and no
prohibition. It composes with everything else because synthesis is the only
operation.

*Probe:* `tests/mediated_self_reference.rs` — 100,001 distinct distinctions at
depth 100,000, under three observation regimes (constant, cycling, and drawn
from the chain's own history), asserting at every step that the new
distinction still references the one it came from. Without that last
assertion the chain would be merely novel rather than self-referential.

**Bounded by identity space, not by depth.** Distinctions are 128 bits, so the
birthday bound arrives near 2⁶⁴. "Unbounded" means "not bounded by Axiom 3."

### Law 11: the Fold Law *(a corollary, not an independent law)*

**Encoding, and probed. Independent-law status withdrawn.**

The claim was that the two primordials become topological hubs because the
byte lift routes every byte through them. The measurement says otherwise: the
hub tracks the **alphabet**, not the anchor.

*Probe:* `experiments/runner/src/anchor_hub_probe.rs`. Six anchor levels
crossed with six alphabet levels, varied independently — which the shipped
configuration cannot do, because `domain_anchor` derives the anchor from an
alphabet tip, so the two factors covary in-tree and no measurement on it alone
could separate them. The top-degree node changed in 0 of 90 alphabet-varying
cell pairs and 90 of 90 anchor-varying pairs.

Three exact results:

- The primordials gain **zero** degree from folding data. Their degree of 257
  is one-time alphabet-construction cost that does not grow with the corpus.
- An anchor touched only at a fold's first step takes at most one novel
  participation per distinct first element, so its degree is **ceilinged at
  the alphabet size** regardless of corpus size. This is forced by the
  encoding's shape, which is what makes the demotion structural rather than a
  workload accident.
- `degree = novel participations` held for every node in every cell, so Law
  12's structural part fully accounts for every figure this law quoted.

What survives: the lift concentrates degree wherever it routes, which is Law
12 applied to the lift's routing table. "The primordials are the hubs" is true
only while the folded corpus is smaller than the alphabet — at 64 KiB, d₀ is
rank 42.

### Law 12: Coding Law

Two parts that must not be conflated.

**Structural part — Derived.** `degree(d)` is the count of novel synthesis
participations, plus the genesis addend. Every axiom-consistent implementation
increments the count on the parents when a novel synthesis is registered. It
holds under any workload, adversarial ones included, because it is a property
of how the operator writes the graph.

**Empirical part — Probed.** The Spearman rank correlation between `degree(d)`
and frequency of use — where frequency counts *all* draws including saturated
repeats — is **0.9940** on the pinned workload, gated at ≥ 0.985.

*Probes:* `experiments/runner/tests/coding_law.rs`, three of them, because a
correlation with no null model is not a measurement:

1. **The gate is not clearable by noise.** Holding degree deltas fixed and
   permuting the frequency vector over 200 trials — destroying the pairing
   while preserving both marginals — gives mean ρ = −0.0000, sd 0.0148, and a
   maximum |ρ| of 0.0388 against a gate of 0.985.
2. **The stated mechanism holds.** The shortfall below 1.0 is saturation
   drift: when a workload re-draws the same pair, frequency increments and
   degree does not. Sweeping eight workload shapes from 0.06% to 97.3%
   saturation gives ρ falling monotonically from 1.0000 to 0.9292, with
   ρ(saturation, ρ) = **−1.0000**. Nothing in the construction forces that: ρ
   comes from the engine's degree bookkeeping and the saturation rate from how
   often the engine declined to create anything.
3. **ρ → 1 as saturation → 0**, exactly, at the low-saturation limit.

**The band the gate is valid in.** A *correct* substrate yields ρ = 0.9752 at
72% saturation and 0.9292 at 97%. So 0.985 encodes the pinned corpus's
saturation rate of 36%; it is clearable below roughly 45% and not above.
A failing ρ on a saturation-heavy workload is not evidence of a broken
substrate, and the gate must say which workload it speaks for.

High-attention distinctions become high-degree nodes with no ranking
algorithm, no curator and no training — *given* workloads that do not saturate
heavily.

---

## Locality

**Derived, and probed.** `synthesize(a, b)` creates one distinction adjacent
to exactly `a` and `b`, and touches nothing else. So for two neighbourhoods
that are disjoint *and* non-adjacent, work confined to one perturbs nothing in
the other — not approximately, exactly.

*Probe:* `tests/parentage_topology.rs` — 400 syntheses confined to a 37-node
neighbourhood changed 0 of 12 clustering coefficients in a disjoint,
non-adjacent one, compared bit for bit.

This is why "every distinction is an advance of its own parents" is
substantive: there is no action at a distance. It is also a live regression
gate rather than a philosophical claim, because a reverse index, a degree
cache that rewrites a third party, or a rebalancing pass would each break it,
and each is the sort of thing added for performance by someone not expecting a
structural consequence.

## Order from an order-blind operator

**Derived, and probed.** Axiom 2 means the operator cannot represent an
ordered pair: there is no field in which "a came first" could live, and no
number of calls supplies one. Yet sequences of bytes receive distinct
identifiers.

What produces order is **nesting**. Composing commutative operations is not
commutative, because the intermediate values differ:
`synth(synth(X, a), b)` and `synth(synth(X, b), a)` are syntheses of different
parents, so Axiom 4 separates them even though each individual call ignores
argument order.

*Probe:* `tests/order_emergence.rs`, as a counted prediction rather than an
impression. In a flat left-associated fold `((((a₁,a₂),a₃),a₄),a₅)` the only
transposition commutativity can hide is `a₁ ↔ a₂`, because that is the one
pair handed to a single call as bare arguments — every other position is
separated by a nesting level. So `n!` orderings should collapse to exactly
`n!/2`. Measured: **120 orderings → 60** under the flat fold, and **120 → 120**
under the anchored fold, which recovers every ordering with no position index
stored anywhere.

This is the clearest instance in the substrate of structure that was not put
in. It is also why an anchor must not be an element tip: the anchor is the
single thing breaking the symmetry at the first position.

## Clustering is mediated self-reference

**Derived, and probed.** The only edges in the parentage graph are
child-parent, plus the genesis edge. So a triangle requires one of its three
pairs to be an edge in its own right, which means `synth(a, b)` closes a
triangle exactly when `a` and `b` were already adjacent — when one is the
other's parent.

Unpack that and it is Law 10. For `m = synth(x, obs)` and `r = synth(m, x)`,
the parents of `r` are `(m, x)` and `x` is a parent of `m`, so `{r, m, x}` is
a triangle. Mediated self-reference is not *a* way to close a triangle here.
It is the only way.

The consequence is a structural identity between a standard graph statistic
and a law of this theory: **the local clustering coefficient of the parentage
graph measures the local density of mediated self-reference, and measures
nothing else.**

*Probe:* `tests/parentage_topology.rs` — 546 triangles classified as 545
mediated self-reference, 1 genesis, **0 unexplained**. A closed
classification with no residue.

---

## Which reads survive appending

**Derived, and probed.** A read taken over a partially-built graph sometimes
equals the same read taken later and sometimes does not, and *which* depends on
the axes rather than on the workload.

Ancestry is fixed at creation: a distinction's parents are written once and
nothing is ever removed. So an upstream read of a witnessed root cannot change
however much is appended — not even an unbounded one, since the complete
ancestry closure of a fixed node is itself fixed. Degree is different: it grows
as children appear. Downstream and undirected reads are different again,
because their node set grows.

Of the eighteen axis combinations this implementation ships — three directions,
three signals, two boundary kinds — **exactly four are stable under
extension**: `Upstream` crossed with `{Adjacency, HopDistance}` crossed with
either boundary.

*Probe:* `tests/extension_invariance.rs`, predicted before measuring and exact.
It also asserts that all eighteen *identifiers* are unchanged, which is correct
by specification — identity covers the spec, not the output — and is precisely
why the published surface cannot express this property.

**Why it matters, in two directions.** For a consumer: those four are the only
reads whose answer does not depend on how far the graph has got, so they are
the only ones safe to compare across engines that are still filling. For
anything built on top: by the monotonicity result cited under *Prior work*,
they are also the only reads on which a coordination-free policy could be
built. A policy reading degree is engine-relative, and two partial witnesses
will disagree.

This is an instance of **query stability**, not a new notion — see *Prior work*.
What is contributed is the exhaustive enumeration for this read surface.

## The synthesis-projection dual

The axioms describe `synthesize` directly, and produce two objects.

The first is named explicitly: the append-only, content-addressed graph,
extended one distinction at a time or left unchanged under Law 7. This is the
**write dual** — every axiom-consistent act of the operator adds structure, or
none at all.

The second is produced by implication: any axiom-consistent read of the graph
from a vantage. Walking parents from a chosen distinction, counting
participations, asking whether one distinction is downstream of another — each
is a **projection**, a coherent read from a specified root, at a specified
boundary, in a specified direction, over a specified signal. It is not new
structure; it is the graph *as viewed from somewhere*
`[SHIPPED @ projection::Projection]`.

**Derived.** Axioms 1 and 4 make the graph a pure function of its inputs; Laws
8 and 9 make it identical across engines with the same history; Law 7 makes it
stable under repeated observation. Any read respecting those constraints *is* a
projection, whether the reader knows it or not.

**Encoding.** The four axes, the shipped signals and the wire layout are
chosen. The axioms force that coherent engine-independent reads *exist*; they
do not force this parameterization.

**The boundary axis is not an optimization.** Every distinction roots in the
primordials, so the graph is one connected component and an unbounded vantage
reaches everything — and therefore distinguishes nothing. The boundary is what
makes a tractable observer possible at all.

*Falsifier:* two engines with the same history, queried with the same
`{root, direction, boundary, signal}` at quiescence, must produce
byte-identical output. If not, the projection is an engine-side artifact and
this section is wrong. Run by `tests/cond_d_falsifier.rs` for the single-order
case and `tests/order_independence.rs` across topological reorderings.

## Using the substrate

The substrate is timeless. It knows about distinctions and parents; it does not
know about order, history, identity-of-perspective, or causality. Time is what
consumers do.

The reference pattern is the **local causal agent**: a consumer anchored to a
local root, advancing it by synthesizing the root with the canonical structure
of the action being taken.

**Derived — and this is the correction the previous version needed.** The
pattern is not a type some consumers have. *Every* non-primordial distinction
is an advance of each of its parents: set `root = a` and `action = b` in
`synth(a, b)`, and the advance holds with `a` as the root; set `root = b` and
it holds again. Agent-ness is therefore a **reading of the graph**, available
everywhere, and a trait asserting that only some types have it is the wrong
shape.

That resolves a contradiction the previous version carried without noticing:
it described this construct as both "a pattern, not an axiom" and "a
substrate-level construct." It is neither. The argument was about where to put
a type, and the thing is not a type.

What the substrate keeps is **parentage**
`[SHIPPED @ engine::DistinctionEngine::parents_of]`. Whether a given advance
was forward is a question about the graph, and the graph answers it: the
candidate's parents either include the root it replaces or they do not. The
substrate does not need to name that comparison to make it available, and the
contract that *chooses* to advance belongs to consumers, who ship with it.

**Encoding.** Nothing requires a consumer to carry one root. Multi-root
consumers, consumers that synthesize from non-root state, and consumers that
ignore perspective entirely are all legal and all get the substrate's
guarantees, because those flow from the axioms and not from the consumer's
shape.

---

## The framework layer

**Framework.** Everything in this section is a commitment the formalism
permits and does not establish. It is here because it is why the work is being
done, and marked because no gate can check it.

The commitment: **distinction is foundational.** Logic and information are not
products of synthesis — the operator's own specification presupposes equality,
ordering and a hash function, so nothing could emerge from it in that
direction. They are two aspects of distinction-structure, read from different
sides. You cannot have identity without distinguishing; you cannot have
information without a difference.

A framework claim is not judged by falsification — the Peano axioms are not
falsifiable either. It is judged by **fertility**: does the frame produce
results you could not otherwise get? That is a standard this document can be
held to, and the ledger is above. Order from an order-blind operator, a graph
statistic turning out to *be* a law, self-reference without reflection
primitives, convergence without consensus, and one law lost to its own
measurement.

What the formalism cannot establish, and this document therefore does not
claim: that logic, information, or the physical world *are* distinction. The
formalism is substrate-neutral about what it models, and that generality is
exactly why applicability is not evidence of constitution. Arithmetic models
populations; populations are not made of numbers.

## Prior work

A foundations document that cites nothing reads as either unaware of its field
or claiming more than it holds. This section exists so a reader can check us
against the literature rather than take our word, and because correctly
locating the work is a stronger claim than an uncited one.

### What the formalism inherits

**The operator is hash-consing.** Collapsing structurally identical values so
that equality is pointer comparison goes back to Ershov (*CACM* 1958) and is
named by Goto (Univ. Tokyo TR 74-03, 1974); "maximal sharing" is the standard
term for the invariant. The exact form here — a *commutative* binary operator
whose pair is canonically ordered before hashing, with self-application
short-circuited — is **AIG structural hashing** in hardware verification,
whose published pseudocode is literally `if (n1 == n2) return n1` (Kuehlmann
et al., *IEEE TCAD* 21(12), 2002; Mishchenko et al., Berkeley ERL, 2005). The
same discipline appears as hash-consing modulo associativity and commutativity
in term rewriting.

*What differs:* in those systems the identifier is an index chosen by a local
allocator. Here it is a digest of the ordered pair, so identity is
reproducible without a shared table. Both halves are standard; the combination
does not appear to be named in one place.

**Laws 8 and 9 are an instance of the state-based CRDT theorem** — a state
forming a join-semilattice under a monotonically non-decreasing update is
strongly eventually consistent (Shapiro, Preguiça, Baquero, Zawirski, *SSS*
2011; the grow-only-set catalogue entry is in the companion INRIA RR-7506). A
Merkle structure used as a state-based CRDT is Auvolat & Taïani, *IEEE SRDS*
2019. No primary source states "a grow-only content-addressed DAG is a CvRDT"
as its contribution, so treat that as folklore rather than as an attribution.

**The agreement/selection trade is CALM** — a program has a consistent,
coordination-free distributed implementation if and only if it is monotone
(conjectured in Hellerstein, *SIGMOD Record* 39(1), 2010; stated as Theorem 1
in Hellerstein & Alvaro, *CACM* 63(9), 2020; proven in a transducer-network
model by Ameloot, Neven, Van den Bussche, *JACM* 60(2), 2013). Two
qualifications matter. The *iff* holds relative to a model in which nodes know
nothing of the distribution policy; weaker forms of monotonicity correspond to
larger coordination-free classes once they know more (Ameloot, Ketsman, Neven,
Zinn, *ACM TODS* 40(4), 2015). And the dividing line is **retraction**, not
aggregation: a monotone aggregate or a threshold on one is coordination-free,
while anything that can withdraw a previously emitted answer is not (Alvaro,
Conway, Hellerstein, Maier, *ICDE* 2014; Conway et al., *SoCC* 2012). For
reads taken over convergent state specifically, see Laddad et al., *PVLDB*
16(4), 2022 — CRDT guarantees cover updates, not observations.

**Which reads survive appending is query stability**, a named and studied
decision problem (Savković, Marengo, Nutt, *ICDT* 2016), earlier posed as
irrelevant-update detection (Blakeley, Coburn, Larson, *ACM TODS* 14(3), 1989)
and as independence of a query from an update (Levy & Sagiv, *VLDB* 1993). The
syntactic characterisations are the classical preservation theorems —
Łoś–Tarski, Lyndon, and homomorphism preservation — with the caveat that only
the last survives restriction to finite structures (Rossman, *JACM* 55(3),
2008). Note the distinction from monotonicity: a monotone answer only grows, a
*stable* one does not change, and the four reads identified above are both.

**The nearest research programme with the same shape is causal set theory**
(Bombelli, Lee, Meyer, Sorkin, *PRL* 59(5), 1987; Rideout & Sorkin, *PRD* 61,
1999). Two things are worth taking from it. Its growth dynamics is *derived*
from three stated requirements rather than posited, which is a tighter way to
pin a policy than anything here. And its label-invariant observables are
generated by **past-closed sets** (Brightwell, Dowker, García, Henson, Sorkin,
*PRD* 67, 2003) — the same classification as this document's upstream-stable
reads, arrived at for the same reason: under a growth rule that only appends to
the future, the past of an existing element is fixed at birth and its future is
not. That programme also still lacks a dynamics from first principles after
twenty-five years, which is a useful calibration on how hard the policy
question is.

### What the framework layer inherits

**Distinction as prior to logic: Spencer-Brown, *Laws of Form* (1969)** — and
the correspondence must be stated honestly, because it is looser than it
looks. Axiom 3 above is his **Law of Calling** (`αα ≈ α`), exactly. But his
**Law of Crossing** (`((α)) ≈ α`) has *no analogue here at all*, and crossing
is what supplies an involution, hence negation, hence Boolean algebra, hence
most of the generative content of the book. A substrate with calling and not
crossing inherits the primitive and **may not claim the results.** Nor does
*Laws of Form* have anything resembling content-addressed identity; it has no
identifiers at all.

The second-order cybernetics line is closer to the claim than Spencer-Brown
alone: von Foerster, Luhmann's social-systems theory built on the distinction,
and Varela, "A calculus for self-reference," *Int. J. General Systems*, 1975.

**Information as difference:** Bateson, "Form, Substance and Difference"
(1970), in *Steps to an Ecology of Mind*, 1972 — "a difference which makes a
difference."

**Information as primitive in physics:** Wheeler, "Information, Physics,
Quantum" (1989). A programmatic essay, so it supports framing and not
derivation.

### What is claimed as new

Nothing in the formalism. Two observations, both modest:

1. The read surface's **stable quadrant, enumerated exhaustively** and
   verified — an instance of query stability rather than a new notion.
2. That in this graph family the clustering coefficient **degenerates to one
   interpretable quantity.** Every triangle in an acyclic graph is transitive,
   and with exactly two parents per node the only available shape is the
   feed-forward loop (Milo et al., *Science* 298, 2002); so a triangle exists
   precisely where a node was synthesized with one of its own parents. The
   mathematics is a corollary of acyclicity plus out-degree two. What is worth
   reporting is the census: 546 triangles, **zero unexplained** — a structural
   prediction that could have failed.

## Limits

- **Identifiers carry no origin.** An identifier does not reveal which
  implementation produced it, and `verify` cannot distinguish one produced
  elsewhere from one invented.
- **An identifier does not reveal its own construction.** It is the digest of
  the *immediate* parent pair — one step, one way. "Identity is process" means
  *determined by*, not *recoverable from*. The process lives in the engine's
  witness record, which is per-instance and losable.
- **Append-only is chosen, not forced.** The axioms constrain `synthesize`,
  not what other operations an engine exposes. Append-only is what makes Laws
  8 and 9 hold cleanly. Garbage collection by partial-reroot replay is
  theoretically permissible; this version excludes it. Auditability is
  therefore a design property enabled by the theory, not forced by it.
- **The substrate solves state agreement, and nothing else about consensus.**
  Law 8 removes the state-agreement problem: the hash function *is* the
  mechanism for state and identity. It does not solve inclusion, liveness,
  attribution, or censorship resistance — those require knowing about peers,
  time and actor identity, which the substrate is designed not to know.
- **An agent is not structurally unaware of its siblings.** Within one engine,
  an undirected saturated projection from any witnessed root reaches every
  distinction in that engine. There is no privacy axis among the four and none
  could be added without changing the substrate. Across separate engines the
  wall is real and enforced.
- **The lift is the one free parameter**, and the only thing whose identity is
  pinned per major version rather than permanently. It moved between 1.x and
  2.0.0.

## Why the theory is small

One operator producing a graph and, over it, the projection. Four axioms. Two
primordials. Eight numbered consequences, one of which turned out to be a
corollary. One reference reading.

Smallness is a constraint, not an accident: every implementation claiming to
be this substrate must enforce the four axioms, and there is nowhere for a
violation to hide when the surface is this small.

**The bet.** A small, internally consistent theory produces a substrate strong
enough to carry domains that usually require their own foundations.

The evidence, stated so that its *verification status* is the hedge rather
than its size — which is where the previous version went wrong, claiming six
converging projects in one section and two consumers in its closer:

- **Six independent projects built projection-shaped wrappers under six
  different names**, in cognition, physics, spatial indexing, mesh telemetry,
  wave state and replay. None was trying to validate this theory, and none
  coordinated with the others. Convergence under no coordination is hard to
  fake, and it is the substrate's strongest generalisation evidence.
- **Two of those are exercised end to end** against this version. The rest are
  asserted here and not verified here; compile-testing them against 2.0.0 is
  tracked work.
- **Byte-identical agreement across two compilation targets** is a harder
  result than either consumer domain, and is checked against the published
  vectors in `conformance/vectors.json` rather than against the other target.
  Two compilations of one source are not two independent witnesses, which is
  why the specification plus those vectors — not a binding — is what an
  implementation in another language certifies against.

What would refute the bet: a domain that needs a second primitive. No
exceptions to the axioms or the laws have been found within the probed range,
and one law has been demoted by a probe designed to try.

---

*Evidence: `conformance/vectors.json` for published identity;
`docs/BENCHMARKS.md` for capacity and throughput; and the probes named in each
law above, all runnable from this repository.*
