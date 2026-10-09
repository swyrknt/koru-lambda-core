# Conformance

What an implementation must reproduce to call itself conforming, and the
fixture to check itself against. Nothing here assumes Rust.

- **`vectors.json`** — every value the substrate publishes, as hex strings and
  integers. This is the source. The Rust crate mirrors it and a test asserts
  the mirror against both the fixture and a live engine.
- **`VECTORS.sha256`** — the fixture's digest, pinned separately so that
  changing a published identifier is a visible two-file diff rather than a
  regeneration that reads as noise.

`../docs/SPEC.md` is normative. Where this document and the specification
differ, the specification wins and this one is a bug.

---

## Why conformance rather than bindings

A complete implementation of the operator and the byte lift is tens of lines
in any language with SHA-256. That makes a *fixture* reach further than a
binding would: anyone can implement the substrate and certify it, in any
language, without our participation and without an ABI.

A conforming *wire* implementation is considerably more than tens of lines —
cone traversal, three signals with their encodings, the header with its widths
and ordering rule, projection identity, restore-as-verification. Both levels
are below, and an implementation may stop after level 1 and say so.

## Level 1 — Identity

Reproduce these and your identifiers agree with every other conforming
implementation, forever, independent of version.

1. **The primordials** (§1). Two 16-byte values that exist by assertion:
   `primordials.d0` and `primordials.d1`.
2. **The operator** (§2). `synthesize(a, b)`:
   - if `a == b`, return `a` **before hashing** — an implementation that
     hashes `a ‖ a` is *not* conforming;
   - otherwise order the pair bytewise and return
     `SHA-256(min ‖ max)[0..16]`.
   Check against `operator.cases`, deriving each from the ones above it.
3. **Identity is 128 bits, unversioned** (§1). No payload, no type, no
   timestamp, no author, and no origin: an identifier does not say which
   implementation made it.

## Level 2 — The lift

The lift is the substrate's **one free parameter**, which makes it the thing
that most needs checking: an implementation can satisfy all of level 1 and
still disagree on every value a consumer folds. Identity at this level is
pinned **per major version**, not forever — `vectors.json` carries
`frozen_for_major` so you know which one you are checking against.

4. **Byte tips** (§4.3). Each byte takes `lift.bits_per_byte` synthesize steps,
   one per bit, least-significant first, folding the accumulator with the
   primordial that bit selects. Check against `lift.tips`.
5. **The anchored sequence fold** (§4.2). Length folded first, elements nested
   in order, and the anchor must **not** be an element tip. The anchor is a
   parameter: two implementations folding identical bytes from different
   anchors agree on nothing, which is how domains separate. Check against
   `lift.folds`.

## Level 3 — Reads

6. **The projection axes** (§5). A read commits to
   `{ root, direction, boundary, signal }`.
7. **The wire format** (§7). Magic, version byte, little-endian integers, and
   the output ordering rule.
8. **Projection identity** (§8). Derived from the specification alone, so it is
   independent of which engine answered.
9. **Restoring is verification, not deserialisation** (§7). A restored
   projection is re-materialised and compared byte for byte.

10. **Refusing bad input** (§3, §7). Reproduction is not conformance: an
    implementation that accepts a well-formed forgery satisfies every value in
    this fixture. Check against `reject.cases`, and note that where
    `expect.kind` is `any_rejection` the specification fixes only that the
    input is refused, not which check refuses it.

Check against `projections.cases` and `reject.cases`.

## What is not yet covered, and matters

Stated plainly, because a suite that implies completeness is worse than one
that admits its gaps:

- ~~One projection wire form of eighteen.~~ **Closed.** All eighteen are
  pinned, each with its `projection_id`. Note the requirement that came with
  them: only four are invariant under appending (`Upstream` × `{Adjacency,
  HopDistance}`), because ancestry is fixed at creation while degree grows and
  the downstream and undirected node sets grow. The other fourteen are
  reproducible only against the engine history the fixture declares — build
  anything else first and you will disagree with them while being correct.
- ~~No rejection cases.~~ **Closed.** `reject.cases` carries eight: bad magic,
  unsupported version, unknown boundary tag, unknown direction tag, truncated
  header, truncated entry array, inflated entry count, and — the one that
  matters most — a **well-formed forgery**, a header with nothing wrong about
  it naming a root this engine never witnessed. Each is the valid wire form
  with one thing changed, so a failure localises to the missing check.

  One case is deliberately loose. For a truncation, which check fires depends
  on where the parse shift lands: the reference implementation reports a
  foreign entry rather than a length error, because the shift makes an entry's
  identifier field read unwitnessed bytes. Both are correct refusals, so the
  fixture asks only that the input be refused. This was found by the suite
  demanding one of them and failing a conforming implementation.
- **The fold's general rule ships as one worked instance.** §4.2 specifies the
  fold over an arbitrary alphabet; only the `{d₀, d₁}` byte case has vectors,
  and composite framing — how a struct containing a variable-length field is
  laid out before folding — is unspecified. Two implementations that pass
  everything here will disagree on the first composite value either one lifts.
- **Domain anchors have vectors now, but still no registry.** Four labels are
  pinned, so anchor *derivation* is checkable. What is not: nothing reserves a
  label, so two consumers can independently choose the same one and collide
  silently in a shared engine.
- **No reference implementation ships yet.** The specification's sufficiency —
  that an implementation can be written from the document alone — is therefore
  untested. Treat it as a claim, not a result.

## Reporting

An implementation that reproduces levels 1 and 2 and says which level it
stopped at is making a precise, checkable claim. One that says "conforming"
without naming a level is not.
