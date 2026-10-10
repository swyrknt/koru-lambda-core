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
domains separate. That separation lives in synthesized structure and
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

Eight synthesize calls per byte, and 256 distinct tips.

### 4.4 Composite values

A value with named parts is lifted by lifting each part to a distinction
and folding those, in declared part order, by §4.2 — the alphabet being
the parts' own tips. Length is folded in first, as always, over the byte
alphabet.

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
| `Adjacency` | the node's parent pair, or nothing for a primordial | `koru:signal:v1:adjacency` |
| `Degree` | the node's degree | `koru:signal:v1:degree` |
| `HopDistance` | hops from the root | `koru:signal:v1:hop_distance` |

Output encodings, which the wire format depends on:

- `usize` encodes as **8 bytes little-endian** — fixed width, so a
  32-bit and a 64-bit target agree.
- `bool` encodes as one byte, `0` or `1`.
- An optional parent pair encodes as `0x00` alone when absent, or
  `0x01` followed by the two 16-byte parents in the order stored.

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

These are the canonical test values. They are published in code as
`koru_lambda_core::vectors`, and both the native and WebAssembly test
suites assert against them. An independent implementation should
reproduce every value below from this document alone.

History:

```
engine  = new()
c       = synthesize(d₀, d₁)
d       = synthesize(c, d₀)
e       = synthesize(d, d₁)
```

| Value | Expected (hex) |
|---|---|
| `c` | `6cc4f0e930b34481d03a4134331852ea` |
| `d` | `3309377e376d6ca3c2e3bf9885900260` |
| `e` | `5f02c8386c05dc7311e0c73a011e18be` |

Then, with `root = c`, `direction = Upstream`, `boundary = Hops(2)`,
`signal = Adjacency`, the canonical bytes of §7 are:

```
4b50524a026cc4f0e930b34481d03a4134331852ea00020000000000000000d0b9d256
2fdb2c3f3fa4b8a3258b778103000000000000000000000000000000000000000000000
001000000000100000000000000000000000000000001000000006cc4f0e930b34481d0
3a4134331852ea21000000010000000000000000000000000000000001000000000000
000000000000000000
```

(The line breaks are presentational; the value is one continuous byte
string.)

Reading the first bytes against §7: `4b 50 52 4a` is `KPRJ`, `02` is the
version, and the next 16 bytes are `c`.

The complete set — byte tips, the sequence fold, the four domain anchors,
the three output encodings of §6, the composite folds of §4.4, all
eighteen projections with their identifiers, and the inputs an
implementation must **refuse** — is `conformance/vectors.json`, with
`conformance/README.md` naming which level each belongs to. This section
is the subset a reader can check by hand; that file is the one to certify
against.

Two of those deserve naming here, because an implementation can satisfy
everything else and still fail them:

- The §4.4 composite cases include a nested fold and its flattened
  counterpart. They carry the same parts in the same order and differ
  only in grouping, so an implementation that flattens structure
  reproduces every other value in the file and disagrees with these.
- Only four of the eighteen projections are invariant under appending to
  the engine. The other fourteen are reproducible only against the
  history the fixture declares.

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
