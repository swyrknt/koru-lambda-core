#!/usr/bin/env python3
"""A second witness: the substrate implemented from `docs/SPEC.md` alone.

PROVENANCE. Written against `docs/SPEC.md` at commit 4b076af, by a reader
with no access to `src/`, to any other document in this repository, or to
`conformance/vectors.json`. The fixture was opened only after this file was
finished, and no value in it was adjusted afterwards. It reproduced 90 of
90 published values on the first execution.

It exists because the substrate's claim is that identity is
engine-independent: a conforming implementation is tens of lines plus
SHA-256 in any language, and agreement needs no shared code. This file is
that claim, executed. `certify.py` runs it against the fixture and
`scripts/gates/reference_impl.sh` runs that in CI.

WHAT THIS PROVES, AND WHAT IT DOES NOT. From here on it proves *agreement*:
two independent implementations, one in Rust and one here, produce the same
published bytes, and a change to either that is not a change to both fails
the gate. It catches a fixture value that only a Rust engine could produce.

It does NOT prove the specification is still sufficient. Now that it is
in-repo and gated, the incentive is to fix it until it matches, at which
point it attests that the two agree rather than that the document
determines them. Sufficiency needs a fresh reader with no access to this
file either, and that experiment has to be repeated whenever the
specification changes materially. It has run twice. The first run found
three defects — `Degree` undefined, the domain-anchor derivation absent,
and `Adjacency`'s parent order deferred to "the order stored" — and the
second found the ambiguity the first run's fix introduced, in whether
§6.1's genesis edge is traversed.

ONE LATER ADDITION, marked where it occurs: `build_byte_alphabet`, because
§4.3 now states that constructing the alphabet witnesses it and this file
predates that sentence. Nothing else changed.

Stdlib only: hashlib.
"""

import hashlib

# ---------------------------------------------------------------- §1 primordials

D0 = bytes(16)
D1 = bytes([0x01]) + bytes(15)


# ---------------------------------------------------------------- §2 operator

def synthesize_pure(a: bytes, b: bytes) -> bytes:
    """§2. Pure identity function; no engine involved."""
    assert len(a) == 16 and len(b) == 16
    if a == b:
        return a                                  # idempotent, returned before hashing
    first, second = (a, b) if a <= b else (b, a)   # bytewise lexicographic
    return hashlib.sha256(first + second).digest()[:16]   # bare concat, leading 16


# ---------------------------------------------------------------- §2.1 engine

class NotWitnessed(Exception):
    pass


class Refused(Exception):
    pass


class Engine:
    """§2.1 A witness. Records which distinctions it has seen produced."""

    def __init__(self):
        # id -> parents tuple (canonical order) or None for a primordial
        self.parents = {D0: None, D1: None}
        self.children = {D0: [], D1: []}

    # --- witnessing -----------------------------------------------------

    def witnessed(self, x: bytes) -> bool:
        return x in self.parents

    def synthesize(self, a: bytes, b: bytes) -> bytes:
        return self.synthesize_novel(a, b)[0]

    def synthesize_novel(self, a: bytes, b: bytes):
        """§2.2 Returns (id, novel-for-this-engine)."""
        if a not in self.parents or b not in self.parents:
            # §3: passing an unwitnessed distinction to synthesize is a
            # programmer error.
            raise NotWitnessed("synthesize on an unwitnessed distinction")
        r = synthesize_pure(a, b)
        if r in self.parents:
            return r, False
        lo, hi = (a, b) if a <= b else (b, a)
        self.parents[r] = (lo, hi)
        self.children[r] = []
        self.children[lo].append(r)
        self.children[hi].append(r)
        return r, True

    # --- §3 trust boundary ---------------------------------------------

    def verify(self, raw: bytes):
        """raw bytes -> Distinction, or None. Answers exactly one question."""
        if len(raw) != 16:
            return None
        return raw if raw in self.parents else None

    # --- §6.1 edge set --------------------------------------------------

    def degree(self, node: bytes) -> int:
        if node not in self.parents:
            return 0          # §6.1 not in the edge set at all
        d = 0
        if self.parents[node] is not None:
            d += 2            # two edges, one to each parent
        d += len(self.children[node])
        if node == D0 or node == D1:
            d += 1            # the genesis edge
        return d

    def relationship_count(self) -> int:
        n = len(self.parents)
        return 2 * (n - 2) + 1


# ---------------------------------------------------------------- §4.3 byte lift

def tip_of_byte(engine: Engine, v: int) -> bytes:
    """§4.3. Nine synthesize calls: the anchoring call, then one per bit."""
    assert 0 <= v <= 255
    acc = engine.synthesize(D0, D1)
    for i in range(8):                      # least significant bit first
        acc = engine.synthesize(acc, D1 if (v >> i) & 1 else D0)
    return acc


def _tip_of_byte_pure(v: int) -> bytes:
    """Same value, computed without touching an engine.

    Used only for the §4.2 `anchor not in tip(Sigma)` precondition, so that
    checking the precondition does not itself grow the engine.
    """
    acc = synthesize_pure(D0, D1)
    for i in range(8):
        acc = synthesize_pure(acc, D1 if (v >> i) & 1 else D0)
    return acc


_BYTE_TIPS_PURE = frozenset(_tip_of_byte_pure(v) for v in range(256))


def build_byte_alphabet(engine: Engine) -> None:
    """§4.3. Materialise all 256 byte tips *through* the engine.

    Added after the specification was amended; this file predates the
    sentence. §4.2's precondition `anchor not in tip(Sigma)` requires the
    tips to exist, and §4.3 states that every step of a lift is witnessed,
    so an implementation that means to fold bytes materialises the whole
    alphabet: 513 distinctions on a fresh engine, `degree(d0) = 257`.

    The pure table below is kept for the precondition test only, which is a
    membership check and not a fold. Computing tips privately *instead* of
    through the engine is the reading this file originally took, and it
    reproduces every published value while leaving the engine an order of
    magnitude smaller — which is why `lift.engine_state` now pins it.
    """
    for v in range(256):
        tip_of_byte(engine, v)


# ---------------------------------------------------------------- §4.2 fold

def u64_le(n: int) -> bytes:
    return n.to_bytes(8, "little")


def fold_bytes(engine: Engine, anchor: bytes, elements) -> bytes:
    """§4.2 anchored fold over the byte alphabet (tip = tip_of_byte)."""
    if anchor in _BYTE_TIPS_PURE:
        raise Refused("anchor is a byte tip (forbidden by §4.2 / §4.4.1)")
    if not engine.witnessed(anchor):
        raise NotWitnessed("anchor not witnessed")
    acc = anchor
    for b in u64_le(len(elements)):         # length first
        acc = engine.synthesize(acc, tip_of_byte(engine, b))
    for e in elements:
        acc = engine.synthesize(acc, tip_of_byte(engine, e))
    return acc


def domain_anchor(engine: Engine, label: int) -> bytes:
    """§4.2."""
    base = engine.synthesize(D0, D1)
    return engine.synthesize(base, tip_of_byte(engine, label))


# ---------------------------------------------------------------- §4.4 composites

def fold_composite(engine: Engine, anchor: bytes, parts) -> bytes:
    """§4.4. Parts are already distinctions (their own tips).

    Length (the NUMBER OF PARTS) is folded first over the byte alphabet.
    """
    if anchor in _BYTE_TIPS_PURE:
        raise Refused("anchor is a byte tip (§4.4 constraint 1)")
    if not engine.witnessed(anchor):
        raise NotWitnessed("anchor not witnessed")
    acc = anchor
    for b in u64_le(len(parts)):
        acc = engine.synthesize(acc, tip_of_byte(engine, b))
    for p in parts:
        if p == acc:
            # §4.4 constraint 2: synthesize(a, a) returns a and drops the part.
            raise Refused("part equals the accumulator it folds into")
        acc = engine.synthesize(acc, p)
    return acc


# ---------------------------------------------------------------- §5/§6 signals

UPSTREAM, DOWNSTREAM, UNDIRECTED = 0x00, 0x01, 0x02

SIGNAL_TAGS = {
    "adjacency": b"koru:signal:v1:adjacency",
    "degree": b"koru:signal:v1:degree",
    "hop_distance": b"koru:signal:v1:hop_distance",
}


def signal_identity(canonical_bytes: bytes) -> bytes:
    """§6 / §7: SHA-256(signal.canonical_bytes())[0..16]."""
    return hashlib.sha256(canonical_bytes).digest()[:16]


def _neighbors(engine: Engine, node: bytes, direction: int):
    """§5. Upstream walks parent edges, Downstream child edges, Undirected both.

    The §6.1 genesis edge is an edge of the engine but not a parent or child
    edge, so it is NOT traversed here (see AMBIGUITY LOG).
    """
    out = []
    if direction in (UPSTREAM, UNDIRECTED):
        p = engine.parents.get(node)
        if p is not None:
            out.extend(p)
    if direction in (DOWNSTREAM, UNDIRECTED):
        out.extend(engine.children.get(node, []))
    return out


def cone(engine: Engine, root: bytes, direction: int, hops):
    """BFS. hops is None for Saturated, else the max hop count.

    Returns {node: shortest hop distance from root within the cone}.
    """
    if not engine.witnessed(root):
        raise NotWitnessed("projection root not witnessed")
    dist = {root: 0}
    frontier = [root]
    depth = 0
    while frontier:
        if hops is not None and depth >= hops:
            break
        depth += 1
        nxt = []
        for n in frontier:
            for m in _neighbors(engine, n, direction):
                if m not in dist:
                    dist[m] = depth
                    nxt.append(m)
        frontier = nxt
    return dist


def signal_output(engine: Engine, signal: str, node: bytes, dist) -> bytes:
    """§6 per-node output bytes."""
    if signal == "adjacency":
        p = engine.parents.get(node)
        if p is None:
            return b"\x00"
        lo, hi = (p[0], p[1]) if p[0] <= p[1] else (p[1], p[0])
        return b"\x01" + lo + hi
    if signal == "degree":
        return engine.degree(node).to_bytes(8, "little")      # usize, 8 LE
    if signal == "hop_distance":
        return dist[node].to_bytes(8, "little")               # usize, 8 LE
    raise Refused("unknown signal")


# ---------------------------------------------------------------- §7 wire format

MAGIC = b"KPRJ"
VERSION = 0x02


class Projection:
    def __init__(self, root, direction, boundary_tag, boundary_payload, signal):
        self.root = root
        self.direction = direction
        self.boundary_tag = boundary_tag          # 0x00 hops, 0x01 saturated
        self.boundary_payload = boundary_payload  # hop count; 0 when saturated
        self.signal = signal                      # name in SIGNAL_TAGS, or raw bytes tag

    @property
    def tag_bytes(self) -> bytes:
        if isinstance(self.signal, bytes):
            return self.signal
        return SIGNAL_TAGS[self.signal]

    def hops(self):
        return None if self.boundary_tag == 0x01 else self.boundary_payload

    def materialize(self, engine: Engine):
        dist = cone(engine, self.root, self.direction, self.hops())
        entries = []
        for node in sorted(dist):                 # ascending bytewise
            entries.append((node, signal_output(engine, self.signal, node, dist)))
        return entries

    def canonical_bytes(self, engine: Engine) -> bytes:
        entries = self.materialize(engine)
        out = bytearray()
        out += MAGIC
        out += bytes([VERSION])
        out += self.root
        out += bytes([self.boundary_tag])
        out += u64_le(self.boundary_payload)
        out += bytes([self.direction])
        out += signal_identity(self.tag_bytes)
        out += u64_le(len(entries))
        for node, output in entries:
            out += node
            out += len(output).to_bytes(4, "little")
            out += output
        return bytes(out)

    # ------------------------------------------------------------ §8

    def projection_id(self) -> bytes:
        h = hashlib.sha256()
        h.update(b"koru:projection_id:v1")        # 21-byte ASCII separator
        h.update(self.root)
        h.update(bytes([self.boundary_tag]))
        h.update(u64_le(self.boundary_payload))
        h.update(bytes([self.direction]))
        h.update(self.tag_bytes)
        return h.digest()                          # 32 bytes, untruncated


HEADER_LEN = 55


def restore(engine: Engine, data: bytes, signal) -> Projection:
    """§7. Restoring is verification, not deserialisation."""
    if len(data) < HEADER_LEN:
        raise Refused("truncated header")
    if data[0:4] != MAGIC:
        raise Refused("bad magic at offset 0")
    if data[4] != VERSION:
        raise Refused("bad version at offset 4")
    root = data[5:21]
    boundary_tag = data[21]
    if boundary_tag not in (0x00, 0x01):
        raise Refused("bad boundary tag at offset 21")
    boundary_payload = int.from_bytes(data[22:30], "little")
    direction = data[30]
    if direction not in (0x00, 0x01, 0x02):
        raise Refused("bad direction tag at offset 30")
    header_sig = data[31:47]

    tag = signal if isinstance(signal, bytes) else SIGNAL_TAGS[signal]
    if signal_identity(tag) != header_sig:
        raise Refused("signal identity at offset 31 disagrees with the supplied signal")

    if engine.verify(root) is None:
        raise Refused("root is not a witnessed distinction")

    proj = Projection(root, direction, boundary_tag, boundary_payload, signal)
    expected = proj.canonical_bytes(engine)
    if expected != data:
        raise Refused("bytes do not match what this engine would have produced")
    return proj
