# Conformance

What an implementation must reproduce to call itself conforming, and the
fixture to check itself against. Nothing here assumes Rust.

- **`vectors.json`** — every value the substrate publishes, as hex strings and
  integers. This is the source. The Rust crate mirrors it and a test asserts
  the mirror against both the fixture and a live engine.
- **`VECTORS.sha256`** — the fixture's digest, pinned separately so that
  changing a published identifier is a visible two-file diff rather than a
  regeneration that reads as noise.
- **`MANIFEST.json`** — which public function each vector covers, and which
  functions are deliberately uncovered, with a reason each.
  `scripts/gates/fixture_coverage.sh` derives the same set from the committed
  API snapshot and fails if the two disagree in either direction, so a
  function added in 2.1 cannot produce an unchecked identifier. It is a
  separate file from the vectors because its digest is not identity: coverage
  bookkeeping changes whenever the surface does.

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
6. **Composite values** (§4.4). The same fold with the caller's own lifted
   distinctions as the alphabet. Check against `lift.composites.cases`, whose
   elements are *derived* — a byte whose tip it is, or an earlier case — so
   that nothing in the block is written down twice. Note the flat/nested
   pair: same parts, same order, different grouping, different value. An
   implementation that flattens structure passes every other case here.

## Level 3 — Reads

7. **The projection axes** (§5). A read commits to
   `{ root, direction, boundary, signal }`.
8. **The wire format** (§7). Magic, version byte, little-endian integers, and
   the output ordering rule.
9. **The output encodings** (§7). Each entry's per-node value is encoded by
   its own shape, independent of which signal produced it. Check against
   `lift.canonical_bytes.cases`: `usize` is 8 bytes little-endian regardless
   of the producer's pointer width, `Option<ParentPair>` is a tag byte then
   min then max, and `bool` is `0x00` or `0x01`. The `bool` case is the reason
   these are pinned separately rather than inferred from a projection — no
   substrate signal has a boolean output, so no projection vector reaches that
   encoding, and a consumer signal with a boolean output serialises through it
   anyway.
10. **Projection identity** (§8). Derived from the specification alone, so it is
   independent of which engine answered.
11. **Restoring is verification, not deserialisation** (§7). A restored
    projection is re-materialised and compared byte for byte.

12. **Refusing bad input** (§3, §7). Reproduction is not conformance: an
    implementation that accepts a well-formed forgery satisfies every value in
    this fixture. Check against `reject.cases`, and note that where
    `expect.kind` is `any_rejection` the specification fixes only that the
    input is refused, not which check refuses it.

Check against `projections.cases`, `lift.canonical_bytes.cases` and
`reject.cases`.

## What is not yet covered, and matters

Stated plainly, because a suite that implies completeness is worse than one
that admits its gaps:

- ~~One projection wire form of eighteen.~~ **Closed.** The full cross
  product is pinned — 3 signals × 3 directions × 2 boundary kinds, each with
  its `projection_id` — plus two cases outside it. Both exist because the
  eighteen all share the root `c`, and an
  implementation written from `docs/SPEC.md` alone showed what that
  uniformity hid: §6.1's genesis edge made it ambiguous whether traversal
  walks it, and a reading that does returns four entries there while
  reproducing every other value in this file under the same projection
  identity.

  The second is `hop_distance_undirected_hops3_from_d0`, which pins
  `HopDistance` as the **shortest** path rather than first-visit order. From
  `d₀`, the node `d₁` sits at distance 2 via `c` and also on a longer route
  of length 3; rooted at `c`, no node is reachable at two different
  distances, so none of the eighteen can tell the rules apart.

  Note the requirement that came with the cross product: only four of the
  eighteen are invariant under appending (`Upstream` × `{Adjacency,
  HopDistance}`), because ancestry is fixed at creation while degree grows
  and the downstream and undirected node sets grow. The other fourteen are
  reproducible only against the engine history the fixture declares — build
  anything else first and you will disagree with them while being correct.
- ~~The lift's effect on engine state is unobservable.~~ **Closed**, in
  `lift.engine_state`. §4.3 says every lift step is witnessed, and no
  projection case can check it because none follows a lift. Two
  implementations reproduced all 88 other values while disagreeing here by
  an order of magnitude — 513 distinctions against 41 — so the count and the
  primordials' degree after building the byte alphabet are published
  directly.
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
- ~~The fold's general rule ships as one worked instance.~~ **Closed.** §4.4
  specifies composites as the §4.2 fold over the parts' own tips, and
  `lift.composites.cases` pins four, including a nested fold. Nesting needed
  no additional rule: a composite's fold is a distinction and a distinction is
  an element one level up. The rejected alternative was a byte-layout rule,
  which would have had to specify how a nested composite is framed *within*
  bytes — a serialisation format growing inside the specification. What
  remains open is narrower: the substrate ships `u8` as its only
  `Canonicalizable` impl, so a consumer lifting a `u64` or a `&str` still
  chooses its own part decomposition, and two consumers may decompose the
  same type differently. That is a consumer-level choice the substrate cannot
  make for them, but it is not the same as the framing being unspecified.
- **Domain anchors have vectors now, but still no registry.** Four labels are
  pinned, so anchor *derivation* is checkable. What is not: nothing reserves a
  label, so two consumers can independently choose the same one and collide
  silently in a shared engine.
- ~~Signal output encodings had no vectors.~~ **Closed**, and worth saying how
  it was found: `scripts/gates/fixture_coverage.sh` derives the set of public
  functions that produce a published value and demands a vector or a reason
  for each. `bool::canonical_bytes` had neither. No shipped signal produces a
  boolean, so no projection case could ever have reached it, and an
  implementation writing `true` as `0xFF` would have passed every value in
  this file. Coverage measured as "what the cases cover" cannot find that;
  only coverage measured against the surface can.
- **No reference implementation ships yet.** The specification's sufficiency —
  that an implementation can be written from the document alone — is therefore
  untested. Treat it as a claim, not a result.

## Reporting

An implementation that reproduces levels 1 and 2 and says which level it
stopped at is making a precise, checkable claim. One that says "conforming"
without naming a level is not.
