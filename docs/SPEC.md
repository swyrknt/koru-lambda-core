# koru-lambda-core — Substrate Specification

**Version 2.0.0.** This document is normative. It defines the byte-level
behaviour a conforming implementation must reproduce: identifiers, the
synthesis operator, the trust boundary, projections, the canonical wire
format, and the conditions the projection primitive must satisfy.

It is written so that an independent implementation, working from this
document and a SHA-256 library alone, can reproduce the golden vectors in
§9. If it cannot, this document is wrong — that is the intended failure
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
collision search is bounded near 2^64 by the birthday bound. See §10.

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
- **Irreflexive** — `synthesize(a, a) == a`, returned before hashing.
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

## 4. Projections

A projection is a read of the graph. Every projection commits to four
axes, and those four axes alone determine its identity:

| Axis | Values |
|---|---|
| `root` | any witnessed distinction |
| `direction` | `Upstream`, `Downstream`, `Undirected` |
| `boundary` | `Hops(n)`, `Saturated` |
| `signal` | a per-node function (see §5) |

`Upstream` walks parent edges, `Downstream` walks child edges,
`Undirected` walks both. `Hops(0)` yields the root alone. `Saturated`
walks until the frontier stops growing.

Output is ordered by the bytewise ordering of §2 — the same ordering the
operator uses — which is what makes the wire form canonical.

## 5. Signals

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

## 6. Canonical wire format

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
| 31 | 16 | signal identity (§5) |
| 47 | 8 | entry count, u64 little-endian |

Then `entry count` entries, in ascending bytewise order of identifier:

| Size | Field |
|---:|---|
| 16 | node identifier |
| 4 | output length in bytes, u32 little-endian |
| *n* | output bytes, per §5 |

Every integer is little-endian. The entry count is `u64` so that a
32-bit target and a 64-bit target produce identical bytes.

Restoring is **verification, not deserialisation**: the receiving engine
re-materialises the projection from its own state and compares the
result byte for byte. Bytes that do not match what this engine would
have produced are rejected, so a well-formed forgery fails as surely as
a malformed one.

## 7. Projection identity

```
projection_id = SHA-256(
      "koru:projection_id:v1"      // 21-byte domain separator, ASCII
    ‖ root                          // 16 bytes
    ‖ boundary_tag                  //  1 byte
    ‖ boundary_payload              //  8 bytes, u64 little-endian
    ‖ direction_tag                 //  1 byte
    ‖ signal.canonical_bytes()      // variable, the tag from §5
)
```

The result is 32 bytes, untruncated.

The identity covers **the specification, not the output**. Two
projections with the same four axes share an identity even if the graph
grew between them, and changing any single axis changes it. A reader who
needs the contents to be identical compares §6 bytes, not this value.

## 8. Conditions

These are the properties the projection primitive must satisfy. They are
named here because the code refers to them.

| | Condition | How it holds |
|---|---|---|
| A | Determinism — a signal is a pure function of its inputs | Required by the signal contract; a consumer signal is responsible for its own purity |
| B | Content addressing — identity is derived from the specification | §7, by construction |
| C | Idempotence — materialising twice yields identical output | Ordered output over engine state |
| D | Cross-engine byte equivalence — two engines with the same history produce identical bytes | §6 fixes every width and order; verified by test |
| D′ | Cross-target byte equivalence — a native build and a WebAssembly build produce identical bytes | §6 fixes widths that would otherwise differ by target; verified by two test suites asserting against §9 |
| E | Read-only — materialising a projection does not modify the engine | The engine interface exposed to signals contains no mutating operation |
| F | No order-bearing engine state — projections add nothing to the engine | Projection state lives on the projection |

Condition D holds **at quiescence**: while writes are in flight, two
engines are not expected to agree, because they have not yet performed
the same operations.

## 9. Golden vectors

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
`signal = Adjacency`, the canonical bytes of §6 are:

```
4b50524a026cc4f0e930b34481d03a4134331852ea00020000000000000000d0b9d256
2fdb2c3f3fa4b8a3258b778103000000000000000000000000000000000000000000000
001000000000100000000000000000000000000000001000000006cc4f0e930b34481d0
3a4134331852ea21000000010000000000000000000000000000000001000000000000
000000000000000000
```

(The line breaks are presentational; the value is one continuous byte
string.)

Reading the first bytes against §6: `4b 50 52 4a` is `KPRJ`, `02` is the
version, and the next 16 bytes are `c`.

## 10. Limits

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
