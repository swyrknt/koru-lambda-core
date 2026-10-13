# koru-lambda-core — Substrate Specification

**Version 2.0.0.** This document is normative. It defines the byte-level
behaviour a conforming implementation must reproduce: identifiers, the
synthesis operator, the trust boundary, projections, the canonical wire
format, and the conditions the projection primitive must satisfy.

It is written so that an independent implementation, working from this
document and a SHA-256 library alone, can reproduce the golden vectors in
§10. If it cannot, this document is wrong — that is the intended failure
mode, and it is checked by a test rather than assumed.

Nothing here describes intent or future work. Every statement describes
what the shipped code does.

---

## 1. Distinctions

A **distinction** is a 16-byte identifier. It carries no payload, no
type, no timestamp and no author.

Two distinctions exist by definition, independent of any history. They
are identical in every engine, in every process, on every machine:

| Name | Bytes |
|---|---|
| `d₀` | `00 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00` |
| `d₁` | `01 00 00 00 00 00 00 00 00 00 00 00 00 00 00 00` |

The byte values are conventional: the theory requires only that the two
are distinct.

**Identifier width is 128 bits and is not versioned.** A distinction is
self-describing to nobody: there is no version field inside an id. A
collision search is bounded near 2^64 by the birthday bound. See §11.

## 2. The synthesis operator

`synthesize(a, b)` maps two distinctions to a third:

```
if a == b:            return a
(first, second)   =   (a, b) if a ≤ b else (b, a)      # bytewise lexicographic
digest            =   SHA-256(first ‖ second)          # 32 bytes
result            =   digest[0..16]                    # leading 16 bytes
```

Precisely, and these are the details an independent implementation must
get right:

- **Ordering** is bytewise lexicographic over the raw 16-byte arrays,
  comparing `first[0]` against `second[0]` first. It is not numeric, not
  little-endian, and not a string comparison over hex.
- **Concatenation is bare.** 32 bytes enter the hash: the smaller
  identifier followed by the larger. There is no separator, no length
  prefix, and no domain tag.
- **Truncation takes the leading 16 bytes** of the digest, `digest[0..16]`.

Three properties follow from the definition rather than from any check:

- **Commutative** — `synthesize(a, b) == synthesize(b, a)`, because the
  pair is ordered before hashing.
- **Idempotent** — `synthesize(a, a) == a`, returned before hashing.
  An implementation that hashes `a ‖ a` is not conforming.
- **Deterministic and engine-independent** — the result is a function of
  the two inputs alone.

### 2.1 What an engine is

An engine is a **witness**, not a namespace. It records which
distinctions it has seen produced. Because identity is a pure function
of content, two engines that perform the same operations hold the same
identifiers, and an identifier carries no trace of which engine produced
it.

A consequence worth stating plainly: identifiers are **not scoped to an
application**. Two unrelated systems performing the same structural
operations obtain the same identifiers. That is what makes agreement
without coordination possible; it also means an identifier alone tells
you nothing about provenance.

### 2.2 Novelty

`synthesize_novel(a, b)` returns the same identifier plus whether this
engine had already witnessed it. Under concurrency, exactly one caller
observes `Novel` for a given identifier. Novelty is a property of the
engine's state, not of the identifier.

## 3. The trust boundary

Sixteen bytes arriving from outside are **not** a distinction. They are
an unverified identifier, and the type system distinguishes the two:

```
raw bytes  →  RawDistinctionId  →  verify(engine)  →  Distinction
```

`verify` answers exactly one question: has this engine witnessed these
bytes? It cannot distinguish an identifier produced by another engine
from one that was invented, because content addressing makes them the
same thing.

Passing a distinction that the receiving engine has not witnessed to
`synthesize` is a programmer error, not a runtime condition to handle.

## 4. The lift

The operator combines distinctions. Getting a value that is *not* a
distinction into the graph is the lift, and it is the substrate's only
free parameter: everything downstream of the primordials is forced, so
the lift is the single place where a choice enters.

It is specified here because two implementations that lift differently
agree on nothing, even though both follow §2 exactly.

### 4.1 Why a naive fold does not work

The operator is commutative and idempotent. So for an ordered sequence:

- `synthesize(x, y)` cannot be told from `synthesize(y, x)`, and
- `synthesize(x, x)` returns `x`, dropping an element entirely.

Order therefore has to be carried by **nesting**: fold the running
accumulator with the next element, so an element's position is recorded
by its depth in the chain.

Two rules make that fold injective:

1. **The accumulator starts at an anchor, never at an element.** Seeding
   with the first element makes the first two positions an unordered
   pair, and lets a leading run of equal elements collapse through
   idempotence. This is not hypothetical: it produced identical
   commitments for the integers 1 and 256, whose little-endian bytes are
   reorderings of each other.
2. **The anchor may not be an element of the alphabet.** Otherwise the
   first step can collapse for the same reason.

Beyond the first step an accumulator is a hash-chain node while elements
come from a fixed alphabet, so a later collapse would require landing on
one of finitely many specific 128-bit values: the collision bound of
§11, not a structural hole.

### 4.2 The anchored fold

Over an alphabet `Σ` with a tip function `tip: Σ → Distinction`:

```
fold(anchor, [e₀ … e_{n-1}]):
    require anchor ∉ tip(Σ)
    acc = anchor
    for b in u64_le(n):           # length first
        acc = synthesize(acc, tip_of_byte(b))
    for e in [e₀ … e_{n-1}]:
        acc = synthesize(acc, tip(e))
    return acc
```

Length is folded in before the elements so that a sequence and a prefix
of it cannot coincide. Where the width is fixed and known to both
parties, the length prefix may be omitted; §4.3 is that case.

The anchor is a **parameter**. Two consumers folding identical bytes
from different anchors obtain different distinctions, which is how
domains separate.

A consumer with no natural anchor of its own derives one from a one-byte
**domain label**:

```
domain_anchor(label):
    base = synthesize(d₀, d₁)
    return synthesize(base, tip_of_byte(label))
```

That is a legal anchor by construction — it is a two-step synthesis, so it
is not a byte tip. §10.2 publishes four of these. The rule was missing
from this document entirely: §10.2 listed the values and no section said
how a label became an anchor, which is not recoverable from the values
without searching candidate constructions.

Nothing reserves a label. Two consumers may choose the same one and
collide in a shared engine, and the substrate cannot prevent it: a
registry would be a naming authority, which is what content addressing
replaces. Consumers sharing an engine agree on labels out of band. That separation lives in synthesized structure and
deliberately not in the hash: a domain tag mixed into §2 would be a
naming layer above content addressing, and would end the property that
two engines performing the same operations agree.

### 4.3 The byte lift

The byte lift is §4.2 instantiated one level down, with `Σ = {d₀, d₁}`
and the eight bits of the byte as elements. Width is fixed at eight, so
no length is folded in:

```
tip_of_byte(v):
    acc = synthesize(d₀, d₁)          # the anchor; not an element of Σ
    for i in 0 … 7:                    # least significant bit first
        acc = synthesize(acc, d₁ if (v >> i) & 1 else d₀)
    return acc
```

Nine synthesize calls per byte and 256 distinct tips: the anchoring
`synthesize(d₀, d₁)`, then one step per bit. The anchor is the same value
for every byte, so it is novel once and saturates thereafter — count it
anyway, because an implementation has to make the call.

### 4.4 Composite values

A value with named parts is lifted by lifting each part to a distinction
and folding those, in declared part order, by §4.2 — the alphabet being
the parts' own tips. Length is folded in first, as always, over the byte
alphabet, and it is the **number of parts** — not the byte length of
anything they contain.

Nesting needs no further rule. A composite's fold is a distinction, and a
distinction is an element one level up, so a part that is itself a
composite is folded as its own tip. Grouping is therefore part of the
value: `{{a, b}, c}` and `{a, b, c}` carry the same parts in the same
order and do not coincide.

This is specified because the alternative is worse, not because it is the
only option. A rule laying composites out as **bytes** — fields
concatenated with length prefixes, then byte-folded — would have to say
how a nested composite is framed within those bytes, which is a
serialisation format growing inside this document. Folding distinctions
keeps the specification at one rule.

Two constraints are not negotiable at this level:

1. **The anchor may not be a byte tip.** The length prefix folds over the
   byte alphabet, so the first step has the same collapse exposure as a
   byte fold's.
2. **No part may equal the accumulator it folds into.** `synthesize(a, a)`
   returns `a`, so such a step drops the part. With parts drawn from a
   fixed alphabet this is the collision bound of §11; with
   caller-supplied distinctions it is reachable, and an implementation
   must refuse it rather than silently produce a shorter fold.

Every step of a lift goes **through the engine**, so each intermediate is
a witnessed distinction: the 256 byte tips, every bit-step accumulator,
and every stage of a sequence or composite fold. A lift therefore grows
the engine, and the nodes it adds appear in later projections and change
later `Degree` and `Saturated` reads.

This is not an incidental property. An implementation that computes tips
against a throwaway engine and hands the identifiers to the caller's
produces values whose distinctions that engine never witnessed —
identifiers that `verify` must reject while appearing in parentage.

So the substrate has one lift rule applied at three levels: bits fold
over the primordials, bytes fold over the bit tips, composites fold over
their parts' tips. An implementation reproducing §10's vectors — which
include byte tips, a sequence fold, and a nested composite — must
implement this exactly; an encoding that produces distinct tips by some
other route is a different substrate.

## 5. Projections

A projection is a read of the graph. Every projection commits to four
axes, and those four axes alone determine its identity:

| Axis | Values |
|---|---|
| `root` | any witnessed distinction |
| `direction` | `Upstream`, `Downstream`, `Undirected` |
| `boundary` | `Hops(n)`, `Saturated` |
| `signal` | a per-node function (see §6) |

`Upstream` walks parent edges, `Downstream` walks child edges,
`Undirected` walks both. `Hops(0)` yields the root alone. `Saturated`
walks until the frontier stops growing.

Output is ordered by the bytewise ordering of §2 — the same ordering the
operator uses — which is what makes the wire form canonical.

## 6. Signals

A signal computes one value per node in the cone. It must be a pure
function of `(signal, context, node)`.

Three signals ship, each with a domain-separated tag:

| Signal | Per-node output | Tag |
|---|---|---|
| `Adjacency` | the node's parent pair in canonical order, or nothing for a primordial | `koru:signal:v1:adjacency` |
| `Degree` | edges incident to the node, per §6.1 | `koru:signal:v1:degree` |
| `HopDistance` | **shortest** hop count from the root within the cone | `koru:signal:v1:hop_distance` |

`Adjacency` emits the pair in the canonical order of §2 — the smaller
identifier first, compared bytewise — and **not** the order the caller
passed to `synthesize`. The two coincide only when the caller already
passed them in order, so an implementation that emits caller order agrees
on some published projections and not others.

`HopDistance` is the shortest path, not first-visit order. The two differ
for any node reachable by more than one route, which `Undirected` makes
possible on every graph and `Downstream` makes possible on any diamond.

### 6.1 Degree

The degree of a node is the number of **edges incident to it**. The edge
set of an engine is:

- two edges for every non-primordial, one to each of its parents; plus
- one **genesis edge** between `d₀` and `d₁`.

The genesis edge is what makes the primordials' mutual existence an
edge rather than a special case, and it is the `+1` in `r = 2d − 3`: the
edge set has exactly `2(d − 2) + 1` members, which is §9's relationship
count. Summed over all nodes, degree is twice that, because every edge
has two endpoints.

Worked against §10.1's history, where `c = synthesize(d₀, d₁)`,
`d = synthesize(c, d₀)` and `e = synthesize(d, d₁)`:

| Node | Incident edges | Degree |
|---|---|---|
| `d₀` | `c–d₀`, `d–d₀`, genesis | 3 |
| `d₁` | `c–d₁`, `e–d₁`, genesis | 3 |
| `c` | `c–d₀`, `c–d₁`, `d–c` | 3 |
| `d` | `d–c`, `d–d₀`, `e–d` | 3 |
| `e` | `e–d`, `e–d₁` | 2 |

Five nodes, nine endpoints over seven edges, and `2 × 5 − 3 = 7`.

A distinction the engine has not witnessed has degree 0; it is not in the
edge set at all. This definition was previously absent — §6 said only
"the node's degree" — and the omission is not recoverable from the
published vectors, because more than one rule reproduces them on a chain
and they diverge on a branching graph.

Output encodings, which the wire format depends on:

- `usize` encodes as **8 bytes little-endian** — fixed width, so a
  32-bit and a 64-bit target agree.
- `bool` encodes as one byte, `0` or `1`.
- An optional parent pair encodes as `0x00` alone when absent, or
  `0x01` followed by the two 16-byte parents in the **canonical order of
  §2**: the bytewise-smaller identifier first. This said "in the order
  stored", which named an implementation detail rather than a rule and
  left the wire form undetermined.

Signals are resolved statically. The signal type is a parameter of the
projection, not a trait object: a projection's output type is determined
by its signal, and mixing signals within one projection is not
representable. An implementation that erases the signal to a dynamic
object cannot preserve the per-signal output encodings above.

A consumer may define its own signal. Its identity on the wire is
`SHA-256(signal.canonical_bytes())[0..16]`, so a consumer signal must
return stable bytes that do not vary across runs or machines.

## 7. Canonical wire format

`canonical_bytes` serialises a materialised projection. This is the sole
canonical form; any other encoding is not this format.

Header:

| Offset | Size | Field |
|---:|---:|---|
| 0 | 4 | magic, ASCII `KPRJ` |
| 4 | 1 | version, currently `0x02` |
| 5 | 16 | root identifier |
| 21 | 1 | boundary tag: `0x00` hops, `0x01` saturated |
| 22 | 8 | boundary payload, u64 little-endian (hop count; `0` when saturated) |
| 30 | 1 | direction tag: `0x00` upstream, `0x01` downstream, `0x02` undirected |
| 31 | 16 | signal identity (§6) |
| 47 | 8 | entry count, u64 little-endian |

Then `entry count` entries, in ascending bytewise order of identifier:

| Size | Field |
|---:|---|
| 16 | node identifier |
| 4 | output length in bytes, u32 little-endian |
| *n* | output bytes, per §6 |

Every integer is little-endian. The entry count is `u64` so that a
32-bit target and a 64-bit target produce identical bytes.

Restoring is **verification, not deserialisation**: the receiving engine
re-materialises the projection from its own state and compares the
result byte for byte. Bytes that do not match what this engine would
have produced are rejected, so a well-formed forgery fails as surely as
a malformed one.

The **caller supplies the signal**; the receiver does not recover it from
the wire. The header's signal identity is `SHA-256(tag)[0..16]`, which is
truncated and one-way, so it can confirm an expectation and cannot name a
signal. A restore therefore takes the signal as an argument, computes its
identity, and refuses the bytes if the header disagrees.

That is why no signal registry exists, and why a consumer-defined signal
(§6) is restorable: by whoever holds the type. Two parties exchanging a
projection must already agree on which read it is — which they do, since
they agreed on a `{root, direction, boundary, signal}` specification in
order to ask for it.

## 8. Projection identity

```
projection_id = SHA-256(
      "koru:projection_id:v1"      // 21-byte domain separator, ASCII
    ‖ root                          // 16 bytes
    ‖ boundary_tag                  //  1 byte
    ‖ boundary_payload              //  8 bytes, u64 little-endian
    ‖ direction_tag                 //  1 byte
    ‖ signal.canonical_bytes()      // variable, the tag from §6
)
```

The result is 32 bytes, untruncated.

The identity covers **the specification, not the output**. Two
projections with the same four axes share an identity even if the graph
grew between them, and changing any single axis changes it. A reader who
needs the contents to be identical compares §7 bytes, not this value.

## 9. Conditions

These are the properties the projection primitive must satisfy. They are
named here because the code refers to them.

| | Condition | How it holds |
|---|---|---|
| A | Determinism — a signal is a pure function of its inputs | Required by the signal contract; a consumer signal is responsible for its own purity |
| B | Content addressing — identity is derived from the specification | §8, by construction |
| C | Idempotence — materialising twice yields identical output | Ordered output over engine state |
| D | Cross-engine byte equivalence — two engines with the same history produce identical bytes | §7 fixes every width and order; verified by test |
| D′ | Cross-target byte equivalence — a native build and a WebAssembly build produce identical bytes | §7 fixes widths that would otherwise differ by target; verified by two test suites asserting against §10 |
| E | Read-only — materialising a projection does not modify the engine | The engine interface exposed to signals contains no mutating operation |
| F | No order-bearing engine state — projections add nothing to the engine | Projection state lives on the projection |

Condition D holds **at quiescence**: while writes are in flight, two
engines are not expected to agree, because they have not yet performed
the same operations.

## 10. Golden vectors

These are the canonical test values. They are mirrored in code as
`koru_lambda_core::vectors`, and both the native and WebAssembly test
suites assert against them. An independent implementation should
reproduce every value below from this document alone — that is this
section's contract, and §10.2 is the part of it that used to be missing.

<!-- BEGIN GENERATED: scripts/gen_spec_vectors.py -->

*Generated from `conformance/vectors.json` by `scripts/gen_spec_vectors.py`; `scripts/gates/spec_vectors.sh` fails
if this block and the fixture disagree. Edit the fixture, not this section.*

### 10.1 Identity

The two primordials, which exist by assertion:

| Value | Hex |
|---|---|
| `d₀` | `00000000000000000000000000000000` |
| `d₁` | `01000000000000000000000000000000` |

Then, applying §2 in order against a fresh engine:

```
c       = synthesize(d0, d1)
d       = synthesize(c, d0)
e       = synthesize(d, d1)
```

| Value | Derivation | Hex |
|---|---|---|
| `c` | `synthesize(d0, d1)` | `6cc4f0e930b34481d03a4134331852ea` |
| `d` | `synthesize(c, d0)` | `3309377e376d6ca3c2e3bf9885900260` |
| `e` | `synthesize(d, d1)` | `5f02c8386c05dc7311e0c73a011e18be` |

### 10.2 The lift

Byte tips (§4.3), each 8 synthesize steps over `{d₀, d₁}` after the anchoring call:

| Byte | Tip |
|---|---|
| `0x00` | `1c1a627159dcf9523304bd261346e876` |
| `0x01` | `d1da7982b838811998a709d6b93c4af8` |
| `0x41` | `f734ffd02c46262fbaa9c5c9d5726538` |
| `0xff` | `214032b0fd17b607a0ed4076a36f16ed` |

The fixture carries four of the 256; an implementation that reproduces these four by the §4.3 rule reproduces all of them, and `conformance/vectors.json` is the place to check the rest.

The anchored sequence fold (§4.2):

- **`fold_hi_anchored_at_d`** — anchor `3309377e376d6ca3c2e3bf9885900260`,
  elements `[0x68, 0x69]`, length folded first.
  Result: `c92ea05207b80ce6bb3ef6995a203c3e`

Domain anchors (§4.2), which are how two consumers folding identical
bytes avoid colliding:

| Label | Anchor |
|---|---|
| `0x01` | `93927b780fa715ffa381b28cee016d0c` |
| `0x02` | `7f557565133618eac3d47991ed1fcda4` |
| `0x7f` | `d9777980de7d16296c3e99c0111663dd` |
| `0xff` | `747f49abc60bafa407beae2089934331` |

Composites (§4.4), folded over their parts' own tips. The last two
carry the same parts in the same order and differ only in grouping:

| Case | Parts | Result |
|---|---|---|
| `flat_1_2_3` | tip(0x01), tip(0x02), tip(0x03) — anchored at `d` | `c9211c7822ef0549b38ae9eed84a0f73` |
| `inner_1_2` | tip(0x01), tip(0x02) — anchored at `d` | `e5602c8da1b26ebdc71b3154778af74d` |
| `nested_1_2_then_3` | `inner_1_2`, tip(0x03) — anchored at `d` | `30abdef870bea83aea4efa221a68b506` |
| `flat_1_2_3_under_domain_01` | tip(0x01), tip(0x02), tip(0x03) — anchored at domain 0x01 | `c5f853eea7642741e32204b59d637a16` |

### 10.3 A worked projection

With `root = 6cc4f0e9…`, `direction = Upstream`, `boundary = Hops(2)`,
`signal = Adjacency`, the §7 canonical bytes are:

```
4b50524a026cc4f0e930b34481d03a4134331852ea00020000000000000000d0b9d2
562fdb2c3f3fa4b8a3258b7781030000000000000000000000000000000000000000
00000001000000000100000000000000000000000000000001000000006cc4f0e930
b34481d03a4134331852ea2100000001000000000000000000000000000000000100
0000000000000000000000000000
```

(The line breaks are presentational; the value is one continuous
byte string.) Reading the first bytes against §7: `4b 50 52 4a` is
`KPRJ`, `02` is the version, and the next 16 bytes are the root.

Its §8 identity is `347d60094a92c906880c8af79f51066b775eafba250709ccd4d8090c24b82f0c`.

### 10.4 What this section does not carry

`conformance/vectors.json` carries all 18 projections with their identifiers, the 7 output encodings of §6, the remaining byte tips, and 8 inputs an implementation must **refuse**. `conformance/README.md` names which conformance level each belongs to.

Two of those deserve naming here, because an implementation can
satisfy everything above and still fail them:

- The rejection cases. Reproduction is not conformance: an implementation that accepts a well-formed forgery reproduces every value in this section perfectly.
- Only four of the 18 projections are invariant under appending to the engine. The other fourteen are reproducible only against the history §10.1 declares.

<!-- END GENERATED: scripts/gen_spec_vectors.py -->

## 11. Limits

Stated here so that a reader finds them rather than discovers them.

- **Identifier width.** 128 bits, unversioned (§1). A deliberate
  collision search is bounded near 2^64. An in-engine collision would
  present as ordinary saturation and would not be detected.
- **Growth.** The engine is append-only. Nothing is evicted, and the
  substrate provides no persistence of its own.
- **Provenance.** Identifiers carry no origin and are not scoped to an
  application (§2.1).
- **Merge semantics.** Combining two engines' histories is set union
  over append-only parentage. That is a grow-only set, not a general
  conflict-resolution mechanism: there are no registers, sequences or
  deletions.
- **Not a consensus protocol.** Two parties that perform the same
  operations agree on the resulting bytes. Nothing here decides which
  operations should be performed, in what order, or by whom.
- **Not every rejection is offset-fixed.** The magic, the version byte, the
  boundary tag and the direction tag each have a fixed offset and reason, and a
  conforming implementation reports them there. A *truncation* does not: the
  parse shifts, so the failure may surface as a length error or as an entry
  whose identifier is not a witnessed distinction, depending on where the shift
  lands. Both are correct refusals. An implementation must refuse the input; it
  need not agree on which check caught it.
- **A witness cannot be denied.** There is no expungement. An identifier is
  a one-way digest and does not reveal its content, so removing a value from
  an engine is not a leak — but anyone *holding* the content can re-derive the
  identifier and ask whether this engine witnessed it. The guarantee an
  implementation can offer is that the content is not recoverable *from the
  substrate*; it cannot offer that the witnessing is unprovable. Consumers
  with a deletion obligation need to know this before they lift anything into
  the graph, not after.
- **Only four reads are stable under extension.** `Upstream` crossed with
  `{Adjacency, HopDistance}` crossed with either boundary returns the same
  bytes however much is appended afterwards; the other fourteen combinations
  do not, because degree grows and the downstream and undirected node sets
  grow. A conforming implementation must agree on this classification, since
  a consumer that compares reads taken at different times against different
  engines depends on it. Note that §8 identity covers the specification and
  not the output, so two specs differing only in stability have
  indistinguishable identifiers — the classification is a fact about the
  substrate, not something the wire form carries.
