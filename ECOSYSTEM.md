# Ecosystem — layers, projects, and how to consume them

How the pieces fit together and why they are separated the way they are.
`ARCHITECTURE.md` describes the inside of the substrate crate; this document
describes everything around it, and `THEORY.md` remains authoritative wherever
the two touch.

Throughout, claims are marked **Forced** when they follow from the four
axioms, **Chosen** when they are a design decision we could have made
differently, and **Target** when they describe the arrangement 2.0.0 commits
to rather than what is built today. Nothing here is a law of the substrate
unless it says Forced.

---

## The goal

**A substrate almost nobody needs to touch, and an agent layer almost
everybody builds on.**

What the substrate guarantees, precisely:

- **Operator-level identity is permanent** and version-independent. The same
  two identifiers synthesize to the same sixteen bytes in every process, on
  every conforming implementation, forever. *(Forced — determinism,
  commutativity, idempotence and content addressing leave no freedom here.)*
- **Lift-level identity is pinned per major version**, not forever, *because
  the lift is the substrate's one free parameter.* It moved in this release:
  the byte lift went from seventeen synthesize calls per byte to nine, so
  every lifted identifier changed between 1.x and 2.0.0. *(Chosen.)*

Those two sentences are the whole promise, and the second is the honest one.
A substrate with a free parameter cannot promise that *everything* it names
is eternal; it can promise that the parameter is named, published, and fixed
for the life of a major version. See `docs/SPEC.md` §4.

Convenience is what forces change, so the substrate is kept small enough to
freeze and everything that evolves lives one layer up. When a consumer needs
something new, the answer is a new agent, not a new substrate feature.

## The layers

| Layer | Artifact | What it is | Changes |
|---|---|---|---|
| Substrate | `koru-lambda-core` (crate) | The operator; the projection read and its wire form; the lift; the trust boundary | Rarely, and never silently |
| Bindings | `koru-lambda-wasm` (crate) | A second compilation target. **Not published at 2.0.0** — see below | With the agent that needs them |
| Agents | `koru-lambda-agents` (repo) | Worked consumers of one pattern, each advancing its own root | Freely |

Of those four substrate pieces, only the operator is **Forced**. The axioms
force that *coherent, engine-independent reads exist*; they do not force the
four-axis parameterization, the three shipped signals, or the wire layout —
those are **Chosen**. The lift is explicitly the one free parameter, and how
the trust boundary is closed is an implementation concern. Conflating these
is how a convention becomes quoted downstream as a law.

**Target.** The agent layer does not exist as its own repository yet. Today
its two consumers live in `crates/koru-lambda-agents`, unpublished, and
2.0.0 does not ship them — so for the life of 2.0.0 the recommended path is
not yet installable. The status section at the end says what is actually
built.

## Why they are separated

The published 1.2.0 crate is **4,489 lines** of Rust under `src/`, counted as
raw lines including blanks and comments, of which the engine, the crate root
and the lift are **409** — nine percent. The method matters and the claim is
sensitive to it: excluding blanks gives 9.0%, and counting code only gives
8.3%. Under every method the substrate is under a tenth of the artifact. The
rest was consensus machinery, a C ABI and bindings.

Two consequences, both observed rather than predicted:

- **Consumers could not tell which part they were meant to use.** Sixteen
  downstream crates across fifteen repositories depend on the substrate
  directly. At least one re-implemented a published export name-for-name:
  One re-implemented a published export name-for-name — same single
  field, same constructor signature, same method name — and never used
  the engine it held. Two others redefined a shipped three-type cluster
  for genuinely different functions. Meanwhile others found and used the
  shipped types. Nothing signposted which was intended. (Who is not
  recorded here; see the note in `DESIGN.md` on why the substrate does
  not name its consumers.)
- **Anything that wanted to change had to change the crate identity depends
  on.** Release 1.1.0 is one commit that touched `Cargo.toml`, `ffi.rs`,
  `network.rs` and `validator.rs` — no substrate file at all — to add a nonce
  accessor for state restoration. A consensus change produced a minor
  version bump of the identity crate, and a downstream consumer still pins
  `1.1.0` today.

Separating them makes the substrate freezable and the agent layer fast.

## One structure, many witnesses

The sentence worth internalising, and the strongest form the formalism
supports:

> **Every engine is a partial witness of one grow-only structure; agents
> agree on identity by construction and differ only in what they have
> witnessed.**

**Forced.** Two agents that synthesize the same pair obtain the same sixteen
bytes with no communication — there is no naming layer that could differ, so
convergence is not negotiated and cannot be avoided. Agents therefore
interoperate at the level of identity *for free*, which is the real argument
for growing agents independently.

**Forced, and the other half of it.** Novelty and degree are properties of
an engine's state, not of an identifier (`docs/SPEC.md` §2.2). So two agents
necessarily agree on *what a thing is* and may permanently disagree on
whether they were the first to see it and how much it has been used. One
structure, many local views — without metaphor.

Likewise: a projection identifier is derived from the specification alone and
is therefore instance-independent, while the canonical bytes come from the
cone and are instance-dependent. Two agents in different engines agree on
*what question they asked* and may disagree on *the answer*.

**The scope condition, which must not be dropped.** Convergence is
unconditional *at the operator* and anchor-conditional *at the boundary*. Two
fully conforming agents folding identical bytes from different anchors agree
on nothing — that is `docs/SPEC.md` §4.2, and it is how domains separate.
The identity space is one; anchors let agents inhabit disjoint regions of it
by design.

**What is not true.** An agent is not structurally unaware of its siblings.
Within one engine instance an undirected, saturated projection from any
witnessed root reaches every distinction in that engine, because the
children index is built from the whole engine and every distinction roots in
the primordials. Ids, reachability and cardinality are all shared there.
There is no privacy axis among the four, and none could be added without
changing the substrate.

Across **separate** instances the wall is real and enforced: an engine
refuses to synthesize against a parent it never witnessed, in every build
profile. So "a closed system whose participants need not know they share
one structure" is implementable as **many instances plus agreement by
vector**, and is not implementable as one merged instance. That is a design
consequence worth knowing before building either.

**Not a claim of this project.** That logic, information, or the physical
world in general are "emergent from synthesis." Nothing in this crate could
falsify such a statement, so it has no status here. `THEORY.md`'s closing
paragraph states the falsifiable version: a small theory has carried two
domains that usually need their own foundations, and whether it generalises
to a third is open.

## What belongs where

Two different questions, which an earlier draft of this document collapsed
into one bad test:

**What must be normative** — what `docs/SPEC.md` has to fix. *Two
implementations that must exchange bytes: is there a choice here they could
make differently?* If yes, it is normative. The operator, the canonical
ordering, the lift, the wire format, projection identity, and replay's
reconstruction contract and error conditions all pass. So do the published
vectors, as the agreed fixture set.

**What belongs in the substrate crate** — the three constitutive objects and
nothing else: the **witness** (the engine), the **vantage** (the projection),
and the **lift** (the encoding). Replay is a theorem about the sufficiency of
recorded parentage, so it travels with the witness. Convenience does not
qualify: a recorder of call order, a hashing optimisation and a children
index are consumer patterns.

*All three have left. The recorder and the children index ship in the agents
crate; the hashing optimisation went crate-private on 2026-10-09, which
settles a disagreement this document used to record — it said the
optimisation did not belong in the substrate while the crate exported it.
The two names went together because they cannot be split: making the struct
private while keeping `BuildHasherDefault<_>` over it public is a hard error,
measured with a two-crate repro rather than assumed.*

## What makes a new agent

**Chosen, not forced.** `THEORY.md` is explicit that the local-causal-agent
pattern is a pattern and not an axiom, and it names three legal shapes that
do not follow it: multi-root, non-root, and no-perspective consumers.

The rule: **one agent is one causal role** — one root, one discipline for
advancing it. A need with a different advance discipline is a new agent; a
need with the same discipline is a parameter on an existing one.

Half of that is mechanical and half is judgment, and it matters which:

- *One root* is enforced by the trait, which returns a single root rather
  than a collection.
- *One advance discipline* is not expressible as a trait bound, because
  nothing stops an implementation adding its own mutating methods. A gate on
  the entry-point count and a generic forward-monotonicity test are the
  cheapest enforcement, and both are tracked.
- *"A different discipline is a new agent"* is a design-review judgment. No
  gate will ever carry it.

The honest status: the shipped validator already has three advance paths, one
of which moves its root without updating the nonce it validates against. The
rule is the fix, not a description of current practice.

An agent may define its own signals, anchors and invariants. It may not:

- **mix a domain tag into identity** — *Forced.* A tag in the hash is the
  separate naming layer content addressing excludes, and it would end
  cross-engine agreement.
- **construct a distinction outside the operator** — *Forced*, and now
  closed at the type level as well as at runtime. Four public paths used to
  produce a `Distinction` with no engine — `from_hex`, `FromStr`, `Pod`,
  `Zeroable` — and all four are gone, each pinned as a compile failure. The
  runtime witness check remains the second line.
- **describe its own invariants as laws of the substrate** — *Chosen.* A
  documentation convention, and the reason this file marks everything.

## What the substrate gives up, and where that is answered

The substrate trades three capabilities for three guarantees, and the trade is
deliberate. Identity, agreement and an immutable past come free; **optimality,
forgetting and dynamics** do not. Each loss buys a theorem, so none is a defect
to be patched — but each has an answer, and every answer is **above** the
substrate.

| Given up | Why | Where it is answered |
|---|---|---|
| **Optimality** — no "current best" | Retracting a previously emitted answer is what forces coordination, so a selection that can change its mind cannot be coordination-free | **Agents.** A node's ancestry is fixed at creation, so it is a permanently sealed set, and a maximum over a sealed set cannot retract. *Optimise over the past, not the present.* No new substrate surface: the read is already `Upstream` + `saturated`. |
| **Forgetting** | Append-only is what makes order-independent reconstruction hold cleanly | **Already available.** An engine is a witness, not a namespace. An agent that wants to forget builds a witness with fewer nodes; surviving identifiers are unchanged, because identity is content-addressed. What it loses — `has` and `degree` — are declared engine-relative rather than identifier properties, so nothing breaks. Separately, relevance-forgetting is free: degree share is zero-sum, so a node that stops participating dilutes at 1/d. |
| **Dynamics** | Timelessness is what makes agreement free; a policy that reads global state makes two partial witnesses diverge | **Agents.** A policy expressed as a *total function* of each node's sealed ancestry — rather than a *selection* over global state — is monotone, and therefore coordination-free. Untested; tracked as research. |

**Nothing in this table adds substrate API**, and that is the point rather than
a coincidence. A dynamics in the substrate would mean the substrate picks *the*
dynamics, when the whole argument is that the policy is a parameter and the
lift is the model. It would also be the exact mistake being unwound: the
previous release put six consumers in the substrate, and the agent contract was
in there until this release removed it.

Two things *are* substrate business, and they are facts rather than functions:
which reads are stable under extension, and that a witness cannot be denied.
Both are stated in `THEORY.md` and `docs/SPEC.md` §11, because two conforming
implementations have to agree on them.

## Choosing a layer

Most consumers will want the agent layer. Use the substrate directly if you
want the operator and nothing else: content-addressed identity, the reads,
and no causal role. Both are legitimate — the agent layer is the default
path, not a mandatory one, precisely because the pattern it encodes is a
convention rather than a consequence of the axioms.

The README does not yet signpost this. Its absence is what produced the
duplicated exports above: there was nowhere else to go, so everyone went to
the substrate.

## Polyglot: publish conformance, not bindings

A conforming *identity* implementation — the operator plus the byte lift — is
tens of lines in any language with SHA-256. A conforming *wire*
implementation is considerably more: cone traversal for three directions and
two boundary kinds, three signals with their encodings and identities, the
header with its widths and ordering rule, projection identity, and
restore-as-verification. `docs/SPEC.md` has eleven sections and the hash is
one of them.

That split is what sets the strategy. Maintaining a binding per language
costs far more than it returns when the thing being agreed on is a hash, and
the hash is the part nearly every consumer wants.

1. **The specification is the primary artifact.** `docs/SPEC.md` is
   normative and self-contained.
2. **The vectors are the conformance suite** — *Target.* Published as
   constants today; the language-neutral fixtures that would let an
   implementation certify itself without our participation are not extracted
   yet, and the current vector set covers one of eighteen projection wire
   forms and no projection identity at all.
3. **Reference implementations should be thin and independent**, each
   asserting the same vectors, none wrapping another — *Target.* None ships
   today.
4. **Bindings are an agent-layer construct, not a substrate one.** This
   document already contains the rule that settles it, under *Agreement
   versus a shared instance*, and an earlier draft did not apply it to wasm.
   Agreement needs no binding: the specification and the vectors give it in
   any language, and a conforming implementation is tens of lines plus
   SHA-256. A binding buys exactly one thing a re-implementation cannot —
   access to a particular engine's **witnessed history**, the only state not
   derivable from the rule. A thing holding a history and a perspective is
   this document's definition of an agent. So the binding belongs where the
   shared witness is.

   The wasm crate therefore stops being published at 2.0.0 and keeps its
   other job: a second compilation target, which establishes 32-bit pointer
   width and an independent codegen backend. That coverage is a property of
   the *target*, not of the bindings — `cargo clippy -p koru-lambda-core
   --target wasm32-unknown-unknown` has it on its own — and two compilations
   of one source were never two independent witnesses. The published surface
   also could not reach the lift, so it exposed only the forced part of the
   substrate and withheld its one free parameter.

A conformance suite also needs what a vector set cannot express: the trust
boundary is *rejection* semantics, so "conforming" must include refusing
malformed and forged input, not only agreeing on good input.

### Agreement versus a shared instance

The distinction that decides whether a binding is needed at all:

- **Agreement** — your process computes the same identifiers as mine. The
  specification and the vectors give this, in any language. It is what
  content addressing is for, and what almost every use wants.
- **A shared instance** — your process queries the same in-memory graph as
  mine. This needs an in-process boundary: a foreign-function interface, or
  a service fronting a single engine.

A C ABI is one answer to the second and an expensive answer to the first.
Whether this family ships one is an open owner decision recorded in the
release checklist; this document does not settle it, and an earlier draft
wrongly implied it had.

## Versioning

**Target**, all three — the mechanisms are specified, not running:

- The bindings pin the substrate exactly. They re-export substrate types
  across an ABI boundary, so a substrate patch can change a generated type
  definition; a bindings release per substrate release is the point, not the
  cost. *(True today.)*
- Agents should take a compatible range, so that absorbing substrate patches
  is how the agent layer discovers a patch broke a consumer. *Today the
  agent crate pins `=2.0.0` exactly, so this feedback loop does not run.*
- The agent repository should run against the substrate on a schedule, so
  that a failing job is how anyone finds out. *No such job exists, and no
  such repository exists.*

## Non-goals

- **A framework.** The substrate has no opinion about persistence, transport
  or scheduling. Agents may; the substrate may not.
- **One door for every use.** See "Choosing a layer."
- **Bindings as a growth strategy.** Conformance scales; bindings do not.
- **Hiding the substrate.** A facade pretending the operator were
  inaccessible would be a lie, since any consumer can depend on it directly.
  The intended path is made obvious instead of mandatory.

## Status

| Piece | State |
|---|---|
| Substrate | Specification normative. Public surface **not** settled: a dozen tracked items still change it, six of them removals |
| Bindings | Ship with the substrate; the lift is not reachable from them, and the published npm name changed from 1.2.0 |
| Agents | Two consumers in `crates/koru-lambda-agents`, unpublished, pending their own repository and their wire-facing fixes |
| Vectors | Published in `conformance/vectors.json`, language-neutral, with `src/vectors.rs` as a test-asserted mirror. Coverage is thin: three operator cases, four of 256 byte tips, one fold, **one of eighteen** projection wire forms, and no projection identity at all |
| Conformance fixtures | `conformance/vectors.json` and `conformance/README.md` ship, keyed to the specification's sections in three levels. All eighteen reads are covered, and eight rejection cases are, so "conforming" now includes refusing a well-formed forgery rather than only agreeing on good input. `conformance/MANIFEST.json` plus `scripts/gates/fixture_coverage.sh` make the coverage derived rather than asserted: a public function that produces a published value fails the gate until a vector or a reason is written down. What is still untested is the specification's **sufficiency** — no implementation has been written from the document alone |
| Reference implementations | None in the repository. An independent reimplementation was reported but no artifact survives, so treat the specification's sufficiency as untested until one ships |
| The freeze | Mechanical. The vectors live outside the crate they pin, their digest is pinned in a separate file, and a live engine is asserted against them — so moving a published identifier takes four coordinated, visible edits. The limit, stated in the gate: a deliberate change can still make all four at once. What is closed is identity moving as a *side effect* |
| C ABI | Not built; an open owner decision |
