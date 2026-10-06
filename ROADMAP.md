# Roadmap to 2.0.0

What order the remaining work happens in, and why that order and not another.

**This file contains no work items.** Items live in `CHECKLIST.md` and nowhere
else. A roadmap that restates them creates two sources of truth for the same
work, which is the defect this release keeps finding: vectors living inside the
crate they pin, a specification promising vectors it does not publish, a gate
whose allowlist outlived the types it named. So this file assigns checklist
**sections** to phases and stops there.

`scripts/gates/roadmap.sh` proves the assignment is total (every blocking
section has a phase), exclusive (no section has two), resolvable (every named
section exists), and honest (a phase marked complete has no open items). It
runs in `scripts/check.sh` and in CI. If this document and the checklist ever
disagree, the build fails rather than the reader guessing.

---

## The phases

- [ ] P0 Enforcement — sections: PROC
- [ ] P1 Theory and freeze — sections: THY, PRB, FRZ, CONF
- [ ] P2 Derived documents — sections: DOC, ENC, TST, ANA
- [ ] P3 Public surface — sections: API, PRJ, SEAL, ENG, TRIM, REP, AGT
- [ ] P4 Product — sections: JS, LAND, DEMO, HYG, PKG
- [ ] P5 Close — sections: DEC, REV, REL

Exit criterion, identical for every phase: **`./scripts/gates/release_ready.sh`
reports zero open items in that phase's sections.** No phase closes on
judgement.

---

## Why this order

**P0 before everything.** The gates are what every later claim is checked
against, and two of them were wrong this week: `crate_boundary.sh` reads one
manifest block and silently empties when the agent crate is renamed, and no
gate covered an untracked document that a tracked README linked to. Fixing a
gate after building on it means re-verifying everything it passed.

**P1A — theory — before P2.** `THEORY.md` declares itself upstream of DESIGN,
ARCHITECTURE, README and `docs/**`, and notes that "every paraphrase is where
drift lives." It currently contradicts itself in four places. Editing the
downstream documents first means writing them twice.

`THEORY.md` is **rewritten, not amended.** Its contradictions — six domains in
one section and two in the closer, Law 11 stated flatly and retracted ten lines
later, the LCA as both "a pattern, not an axiom" and "a substrate-level
construct," a citation to a probe that did not exist — are what two years of
amendment produces. Editing them out one at a time preserves the structure that
allowed them. The old version stays in git history.

**P1B — the freeze — runs parallel to P1A** and gates P3. Today a single commit
can change every published identifier with CI green: the API snapshot records a
constant's *type*, not its value, and the vectors ship inside the crate they
pin. No identity-bearing code may move before that is mechanical, which is why
P3 cannot start first.

**P2 before P3.** The derived documents are what the surface is checked
against. Settling the surface first means the documents describe a target that
has already moved.

**P3 before P4.** The landing page, the demos and the npm package all describe
the public surface. A dozen tracked items still change it, six of them
removals, so P4 written first is P4 written twice.

**P5 last, and it is not a formality.** `DEC-*` are open questions for the
owner and they block like any other item. The C ABI currently has four
inconsistent statuses across the repository; that is a decision, not a bug.

---

## Standing rules

These are not phase-scoped. They apply to every change in every phase, and
each has a gate behind it.

| Rule | Enforced by |
|---|---|
| An unticked box blocks the release | `release_ready.sh` |
| A ticked box carries evidence that resolves | `checklist_evidence.sh` |
| Zero warnings, zero suppression — fix the cause | `no_suppression.sh`, `-D warnings` |
| A test must be able to fail, and be shown to | review; stated in each probe's body |
| Findings become numbered items before the gate that found them can be ticked | `checklist_evidence.sh` |
| No internal process material in a tracked file | `no_process_leaks.sh` |
| Documented code anchors resolve to real symbols | `doc_anchors.sh` |
| Dependencies point one way | `crate_boundary.sh` |

The fourth rule is the one with no gate and the one that caught the most this
week. Two ticked items were claiming what their evidence did not support; a
test named as the theory's falsifier established only determinism; three of the
previous release's headline measurements turned out to be a coin flip, a
selection artefact, and a result the test harness had installed itself. A test
that passes is not evidence until someone has made it fail on purpose.

---

## What 2.0.0 is, and is not

**Ships:** `koru-lambda-core` to crates.io, and `koru-lambda-wasm` to npm as
the conformance surface — the artifact that lets a reader reproduce a published
identifier in a browser console with no toolchain.

**Does not ship:** the agent layer. Its two consumers still have wire-facing
defects, and no identity-bearing code moves across a repository boundary before
P1B makes the freeze mechanical. `koru-lambda-agents` is the next release, with
its own repository, the bindings that make it reachable from other languages,
and the signals that are example vantages rather than substrate.

**After 2.0.0** the sections below the `Not blocking 2.0.0` line in
`CHECKLIST.md` become that release's scope. They are deliberately unphased here
and the gate ignores them.

---

## Research, and where it belongs

The theory makes a **framework** claim — distinction as foundational — and
`THEORY.md` stakes that claim on *fertility* rather than falsification: does
the frame produce results you could not otherwise get? That invites probes
that are genuinely open-ended, and open-ended work is the easiest way for a
release to drift.

So it gets a declared home and three rules.

**Probes live in `experiments/`.** `tests/` asserts; `experiments/` measures.
A probe whose outcome we do not know does not belong in a suite whose job is
to go red when something breaks.

**A research finding is not a release item.** It is recorded below the
`Not blocking 2.0.0` line in `CHECKLIST.md`, which the roadmap gate ignores by
design. No phase above that line may depend on one. The exception is narrow
and already spent: a probe whose result changes a claim *already* in a tracked
document is release work, because the document is wrong until it runs — that
is why the Coding Law control ran before the theory was rewritten.

**A probe result may enter a tracked document only with a status and a
citation.** `THEORY.md` defines the statuses; a probed claim names the file
that runs it. This is the rule that would have stopped the previous release's
three headline measurements — a correlation that was a coin flip, a selection
artefact, and a result the harness installed itself — from being published as
properties of the substrate.

**And every emergence probe needs a null model**, stated before it runs. This
project has twice measured its own harness and reported the result as the
substrate's: the previous release's scale-free topology existed only because
the test driver installed preferential attachment, and Law 11's hub turned out
to track the alphabet rather than the anchor. "Let it run and see what
emerges" measures the draw policy unless something establishes what a policy
that encodes nothing produces.

**When a finding is promoted.** The rules above send research below the line,
and one case needs naming because it has already happened twice. A probe
sometimes produces a **forced property of the substrate** — something two
conforming implementations must agree on. Its absence from `THEORY.md` and
`docs/SPEC.md` is then a *conformance* gap rather than a research direction,
and it becomes release work.

The test is the substrate's own membership test, asked of the finding rather
than of a module: *if this were not written down, could two conforming
implementations disagree, or build incompatible consumers on the same
substrate?* "Only four of the eighteen reads are stable under extension" passes
— a consumer comparing reads across engines depends on it — so it was promoted
to the specification. "A policy reading only sealed state may give confluent
dynamics" does not: it is a design conjecture about something built on top, so
it stays below the line with a test attached.

Everything else stays research, and no phase above the line may wait on it.
