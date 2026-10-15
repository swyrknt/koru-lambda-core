"""A Python substrate engine, for prototyping the agent layer.

WHAT THIS IS. `conformance/reference/substrate.py` is a second *witness*: it
implements everything `docs/SPEC.md` pins, it was written blind from that
document, and `scripts/gates/reference_impl.sh` gates it at 103 of 103
published values. What it is not is a usable engine — it has no parentage
read, no snapshot, no replay, and no auditor, because none of those have a
wire form for two implementations to agree about.

This module adds them, by importing that file rather than editing it. The
distinction matters: `substrate.py`'s whole value is that its author had no
access to `src/`, so adding conveniences written *with* `src/` in view would
quietly convert the conformance gate from "the document determines these
bytes" into "these two files agree". It stays untouched; everything here is
additive and lives outside the published package.

WHAT THIS IS FOR. The agent layer. An agent in this project is a witness
with a perspective — perspective is projection, which `substrate.py` already
has, but *witnessing a history* is `parents_of`, `snapshot_parentage` and
`replay_topological`, which it does not. Those are the reads an agent
prototype needs on day one.

WHAT IT DOES NOT INHERIT. Every operation added here is in
`conformance/MANIFEST.json`'s **exempt** set, and for a reason that matters:
they have no wire form, their iteration order is deliberately unspecified by
the specification, or their signature is Rust-specific. So there is nothing
for two implementations to agree about, and none of this is covered by the
103-value fixture. `test_engine.py` asserts self-consistency instead — the
auditor accepts what this engine builds, replay round-trips, `r = 2d − 3`
holds. That is a weaker claim than the one `substrate.py` carries, and it is
stated here so nothing downstream mistakes one for the other.

NOT SHIPPED. `experiments/*` is excluded in `Cargo.toml`, so this does not
reach crates.io. It is also not a Python package: the substrate is a rule you
satisfy rather than a service you call, so the polyglot mechanism is the
specification plus `conformance/vectors.json`, not a dependency. See
`crates/koru-lambda-wasm/Cargo.toml` for the same decision about bindings.

Stdlib only, matching `substrate.py`.

Run the tests:  python3 -m pytest experiments/python/ -q
"""

import pathlib
import sys

# Import the gated reference without copying or modifying it.
_REFERENCE = pathlib.Path(__file__).resolve().parents[2] / "conformance" / "reference"
if str(_REFERENCE) not in sys.path:
    sys.path.insert(0, str(_REFERENCE))

import substrate  # noqa: E402  (path must be set first)
from substrate import D0, D1, NotWitnessed, Refused, synthesize_pure  # noqa: E402,F401

__all__ = [
    "D0",
    "D1",
    "Adjacency",
    "Boundary",
    "Degree",
    "Direction",
    "Engine",
    "HopDistance",
    "MaterializedProjection",
    "Projection",
    "ProjectionBuilder",
    "Signal",
    "project",
    "InvariantError",
    "BinaryParentageMismatch",
    "UnregisteredParent",
    "ContentAddressMismatch",
    "DegreeMismatch",
    "SelfParentage",
    "ReplayError",
    "MissingPrimordial",
    "Unreachable",
    "Mismatch",
    "ParseError",
    "NotWitnessed",
    "Refused",
    "replay_topological",
    "to_hex",
    "from_hex",
    "synthesize_pure",
]


# ------------------------------------------------------------------ errors
#
# Mirroring `src/engine.rs`'s `InvariantError` and `src/replay.rs`'s
# `ReplayError` variant for variant. `substrate.py` has two exception types
# because the fixture only ever needs "accepted" or "refused"; a prototype
# needs to know *which* law broke, since that is the whole diagnostic value
# of running the auditor at all.


class InvariantError(Exception):
    """A structural law does not hold. Five subclasses, one per check."""


class BinaryParentageMismatch(InvariantError):
    """Law 5. A non-primordial lacks parentage, or a primordial has it."""

    def __init__(self, total, with_parents_plus_two):
        super().__init__(
            f"binary parentage: {total} distinctions, "
            f"{with_parents_plus_two} carrying parentage (+2 primordials)"
        )
        self.total = total
        self.with_parents_plus_two = with_parents_plus_two


class UnregisteredParent(InvariantError):
    """A registered distinction names a parent that is not registered.

    The graph is append-only, so a parent can never be removed: a dangling
    parent means a child was recorded before its parent was.
    """

    def __init__(self, child, parent):
        super().__init__(f"unregistered parent: {to_hex(child)} names {to_hex(parent)}")
        self.child = child
        self.parent = parent


class ContentAddressMismatch(InvariantError):
    """Axiom 4. A distinction's identity is not the hash of its parents.

    Re-deriving every node from its recorded parentage is the only check
    that tests the axiom rather than restating it.
    """

    def __init__(self, child, actual):
        super().__init__(
            f"content address: {to_hex(child)} re-derives as {to_hex(actual)}"
        )
        self.child = child
        self.actual = actual


class DegreeMismatch(InvariantError):
    """Per-node degree disagrees with the parentage relation.

    Per-node, not the sum: a mutant that bumps one parent twice and another
    never leaves the total correct.
    """

    def __init__(self, node, stored, recomputed):
        super().__init__(
            f"degree: {to_hex(node)} reports {stored}, parentage gives {recomputed}"
        )
        self.node = node
        self.stored = stored
        self.recomputed = recomputed


class SelfParentage(InvariantError):
    """Axiom 3 residue. A registered node has two equal parents.

    `synthesize(a, a)` returns `a` before hashing, so such a node can only
    exist if something wrote parentage directly.
    """

    def __init__(self, node):
        super().__init__(f"self parentage: {to_hex(node)} has two equal parents")
        self.node = node


class ReplayError(Exception):
    """Replay could not reconstruct an engine from a parentage dump."""


class MissingPrimordial(ReplayError):
    """No entry roots in the primordials, so nothing can bootstrap."""

    def __init__(self):
        super().__init__("no parentage entry roots in primordials")


class Unreachable(ReplayError):
    """Entries remain whose parents never arrived — a cycle or a gap.

    This is what a dump taken from a live engine produces: see
    `snapshot_parentage`'s quiescence note.
    """

    def __init__(self, remaining):
        super().__init__(f"corrupted parentage: {remaining} entries unresolved")
        self.remaining = remaining


class Mismatch(ReplayError):
    """A declared child is not the hash of its declared parents."""

    def __init__(self, child, actual):
        super().__init__(
            f"content-address mismatch: {to_hex(child)} re-derived as {to_hex(actual)}"
        )
        self.child = child
        self.actual = actual


class ParseError(Exception):
    """Hex that is not sixteen bytes of hex."""


# ------------------------------------------------------------------ hex


def to_hex(x: bytes) -> str:
    """The display form. Hex is not theory — it is a boundary concern."""
    return x.hex()


def from_hex(s: str) -> bytes:
    """Parse to *raw* bytes.

    Deliberately not "parse to a distinction". A parse cannot establish
    membership, which is why in the Rust crate this direction lives on
    `RawDistinctionId` and reaches `Distinction` only through `verify`. Here
    the return is plain `bytes` for the same reason: hand it to
    `Engine.verify` to learn whether any engine has witnessed it.
    """
    if len(s) != 32:
        raise ParseError(f"expected 32 hex characters, got {len(s)}")
    try:
        return bytes.fromhex(s)
    except ValueError as exc:
        raise ParseError(str(exc)) from exc


# ------------------------------------------------------------------ engine


class Engine(substrate.Engine):
    """The reference engine plus the reads an agent needs.

    Subclassed rather than reimplemented so `synthesize`, `verify`,
    `degree` and `relationship_count` remain exactly the gated
    implementations — there is one operator here, not two.
    """

    # --- parentage reads ------------------------------------------------

    def parents_of(self, x: bytes):
        """The canonical parent pair, or `None` for a primordial.

        Raises `NotWitnessed` rather than returning `None` for an unknown
        distinction, because `None` already means "primordial" and
        collapsing the two would make an absent node look like a root.
        """
        if x not in self.parents:
            raise NotWitnessed(f"not witnessed: {to_hex(x)}")
        return self.parents[x]

    def get_distinction_by_id(self, raw: bytes):
        """Lookup by identity: the bytes back if witnessed, else `None`."""
        return raw if len(raw) == 16 and raw in self.parents else None

    def distinction_count(self) -> int:
        return len(self.parents)

    # --- snapshots ------------------------------------------------------
    #
    # Order is unspecified on purpose, exactly as in the Rust crate and for
    # the same reason: `docs/SPEC.md` does not fix it, so a conforming
    # implementation must not be required to match. Sort by bytes if you
    # need determinism. Python dicts happen to preserve insertion order,
    # which is a stronger guarantee than the specification makes — do not
    # build on it.

    def snapshot_distinctions(self):
        """Every distinction this engine holds."""
        return list(self.parents.keys())

    def snapshot_parentage(self):
        """The canonical persistence dump: `(child, (p0, p1))`, primordials
        excluded since their parentage is `None`.

        Combined with `replay_topological` this reconstructs an identical
        engine — *at quiescence*. Nothing here is thread-safe and this
        prototype is single-threaded, so the tearing the Rust engine
        documents under concurrent writers cannot occur; the precondition is
        noted because an agent prototype that grows threads inherits it.
        """
        return [(c, p) for c, p in self.parents.items() if p is not None]

    # --- the auditor ----------------------------------------------------

    def check_structural_invariant(self):
        """Five checks in one pass. Raises the matching `InvariantError`.

        Mirrors `DistinctionEngine::check_structural_invariant`. Degree is
        recomputed from the parentage relation alone rather than read back
        out of the children index, so a corrupted index is caught instead of
        confirming itself.
        """
        nodes = self.parents

        # 1. Law 5 — binary parentage.
        with_parents = sum(1 for p in nodes.values() if p is not None)
        if len(nodes) != with_parents + 2:
            raise BinaryParentageMismatch(len(nodes), with_parents + 2)

        # Degree accumulator, built from parentage only.
        recomputed = {x: 0 for x in nodes}

        for child, par in nodes.items():
            if par is None:
                continue
            p0, p1 = par

            # 2. Every parent registered.
            for p in (p0, p1):
                if p not in nodes:
                    raise UnregisteredParent(child, p)

            # 3. Axiom 3 residue — no equal parents.
            if p0 == p1:
                raise SelfParentage(child)

            # 4. Axiom 4 — identity is the hash of the parents.
            actual = synthesize_pure(p0, p1)
            if actual != child:
                raise ContentAddressMismatch(child, actual)

            # Two edges from the child, one into each parent.
            recomputed[child] += 2
            recomputed[p0] += 1
            recomputed[p1] += 1

        # The genesis edge, which is an edge like any other for degree.
        recomputed[D0] += 1
        recomputed[D1] += 1

        # 5. Per-node degree.
        for node in nodes:
            stored = self.degree(node)
            if stored != recomputed[node]:
                raise DegreeMismatch(node, stored, recomputed[node])

        return None


# ------------------------------------------------------------------ replay


def replay_topological(parentage) -> Engine:
    """Reconstruct an engine from a parentage dump, in any input order.

    Kahn's algorithm, the same shape as `src/replay.rs`. An entry becomes
    ready when the engine holds both its parents, and resolving it wakes
    exactly the entries waiting on the child it produced — so each entry is
    synthesized once and each waiting edge traversed once. The naive
    alternative is a repeated sweep, which is O(N²) precisely on the shape an
    agent produces: a chain where every entry depends on the one before it.

    Raises `MissingPrimordial` if nothing can bootstrap, `Unreachable` if
    entries remain blocked, and `Mismatch` if a declared child is not the
    hash of its declared parents.
    """
    engine = Engine()

    # Deduplicate: a dump may legitimately repeat an entry, and replaying one
    # twice is harmless but wakes its waiters twice.
    seen = set()
    entries = []
    for child, par in parentage:
        key = (child, par)
        if key not in seen:
            seen.add(key)
            entries.append((child, par))

    if not entries:
        return engine

    blocked_on = [0] * len(entries)
    waiting = {}        # parent id -> entry indices blocked on it
    ready = []

    for i, (_child, (p0, p1)) in enumerate(entries):
        missing = {p for p in (p0, p1) if not engine.witnessed(p)}
        blocked_on[i] = len(missing)
        if missing:
            for p in missing:
                waiting.setdefault(p, []).append(i)
        else:
            ready.append(i)

    if not ready:
        raise MissingPrimordial()

    resolved = 0
    while ready:
        i = ready.pop()
        child, (p0, p1) = entries[i]

        produced = engine.synthesize(p0, p1)
        if produced != child:
            raise Mismatch(child, produced)
        resolved += 1

        # Wake only what was waiting on this child.
        for j in waiting.pop(child, ()):
            blocked_on[j] -= 1
            if blocked_on[j] == 0:
                ready.append(j)

    if resolved != len(entries):
        raise Unreachable(len(entries) - resolved)

    return engine


# ------------------------------------------------------------------ signals
#
# `substrate.signal_output` is a closed dispatch on three names, which is
# right for a conformance reference — the specification pins three signals
# and an unknown one must be refused. It is wrong for a prototype: `Signal`
# is the Rust crate's public extension point, and a consumer's own signal is
# what "an agent is a witness with a perspective" actually means. A
# perspective *is* a custom signal.
#
# The three shipped signals are defined here as objects that **delegate to
# the gated reference** rather than reimplementing it, so there is still one
# definition of the published per-node encodings. `test_the_wrapper_did_not
# _change_the_published_bytes` asserts the wrapper is byte-transparent.


class Signal:
    """A per-node computation over the projection cone.

    Two methods, matching the Rust trait:

    * `canonical_bytes()` — the signal's **identity**, hashed into the
      projection's `ProjectionId` and its wire header. Two signals that
      compute differently must return different bytes here, or two
      projections that mean different things collide.
    * `compute(engine, node, dist)` — the per-node output **bytes**. The
      Rust trait returns a typed `Output` that implements `CanonicalBytes`;
      here it returns the encoded bytes directly, because Python has no
      equivalent of that bound and an intermediate type would buy nothing.

    Read-only is a contract, not something this signature enforces —
    exactly as in Rust. Nothing stops `compute` from calling `synthesize`
    on an engine it closed over. `tests/read_only.rs` makes the same point
    about the Rust trait; the recipe for checking your own signal is to
    snapshot the engine before and after and compare.
    """

    def canonical_bytes(self) -> bytes:
        raise NotImplementedError

    def compute(self, engine: Engine, node: bytes, dist) -> bytes:
        raise NotImplementedError


class _CoreSignal(Signal):
    """One of the three the specification pins, delegating to the reference."""

    def __init__(self, name: str):
        self._name = name
        self._tag = substrate.SIGNAL_TAGS[name]

    def canonical_bytes(self) -> bytes:
        return self._tag

    def compute(self, engine, node, dist) -> bytes:
        return substrate.signal_output(engine, self._name, node, dist)

    def __repr__(self):
        return f"<{self._name}>"


Adjacency = _CoreSignal("adjacency")
Degree = _CoreSignal("degree")
HopDistance = _CoreSignal("hop_distance")


# ------------------------------------------------------------------ axes


class Direction:
    """The three traversal directions. Bare ints in the reference."""

    UPSTREAM = substrate.UPSTREAM
    DOWNSTREAM = substrate.DOWNSTREAM
    UNDIRECTED = substrate.UNDIRECTED

    _NAMES = {UPSTREAM: "upstream", DOWNSTREAM: "downstream", UNDIRECTED: "undirected"}

    @classmethod
    def name(cls, d):
        return cls._NAMES.get(d, f"unknown({d})")


class Boundary:
    """Where the cone stops. `hops(n)` or `saturated()`.

    Named rather than passed as a raw tag plus payload, because raw tags at
    call sites are what produced the one wire-format defect this release
    found: a test tampering offset 63 believing it was the entry length
    field, which `docs/SPEC.md` §7 puts at 71.
    """

    HOPS = 0x00
    SATURATED = 0x01

    def __init__(self, tag, payload):
        self.tag = tag
        self.payload = payload

    @classmethod
    def hops(cls, n):
        if n < 0:
            raise Refused("hop count cannot be negative")
        return cls(cls.HOPS, n)

    @classmethod
    def saturated(cls):
        return cls(cls.SATURATED, 0)

    def __repr__(self):
        return "saturated" if self.tag == self.SATURATED else f"hops({self.payload})"

    def __eq__(self, other):
        return (
            isinstance(other, Boundary)
            and self.tag == other.tag
            and self.payload == other.payload
        )

    def __hash__(self):
        return hash((self.tag, self.payload))


# ------------------------------------------------------------------ projection


class Projection(substrate.Projection):
    """The reference projection, extended to accept a `Signal` object.

    Subclassed so the §7 frame and the §8 identity stay the gated code: the
    only override is `materialize`, which asks the signal object instead of
    the closed dispatch. `canonical_bytes` and `projection_id` are
    inherited untouched, so a custom signal gets the same framing and the
    same identity derivation as a shipped one.
    """

    def __init__(self, root, direction, boundary, signal):
        if not isinstance(signal, Signal):
            raise Refused("signal must be a Signal instance")
        # The parent stores the identity bytes; `tag_bytes` returns them
        # directly when the signal is bytes rather than a known name.
        super().__init__(
            root, direction, boundary.tag, boundary.payload, signal.canonical_bytes()
        )
        self.signal_obj = signal
        self.boundary = boundary

    def materialize(self, engine: Engine):
        dist = substrate.cone(engine, self.root, self.direction, self.hops())
        return [
            (node, self.signal_obj.compute(engine, node, dist))
            for node in sorted(dist)  # ascending bytewise, §7
        ]

    def run(self, engine: Engine):
        """Materialize into an object that answers questions about the read."""
        return MaterializedProjection(self, engine)


class MaterializedProjection:
    """A materialized read: the entries, plus what the Rust type answers.

    `substrate.Projection.materialize` returns a list of pairs, which is
    everything the conformance fixture needs and less than a caller wants.
    """

    def __init__(self, projection: Projection, engine: Engine):
        self._projection = projection
        self._entries = projection.materialize(engine)
        self._by_node = dict(self._entries)
        self._bytes = projection.canonical_bytes(engine)

    def spec(self):
        """The `{root, boundary, direction, signal}` this was taken against."""
        return self._projection

    def output(self):
        """`(node, output_bytes)` pairs in canonical order."""
        return list(self._entries)

    def contains(self, node: bytes) -> bool:
        return node in self._by_node

    def get(self, node: bytes):
        return self._by_node.get(node)

    def canonical_bytes(self) -> bytes:
        return self._bytes

    def projection_id(self) -> bytes:
        return self._projection.projection_id()

    def __len__(self):
        return len(self._entries)

    def __iter__(self):
        return iter(self._entries)


class ProjectionBuilder:
    """`project(engine, root).direction(...).hops(2).signal(Degree).run()`.

    The Rust builder is a typestate: omitting an axis is a compile error.
    Python cannot do that, so the axes are checked at `run()` and the error
    names what is missing. Worth stating plainly rather than implying the
    prototype carries the same guarantee — this catches the mistake one
    stage later than the crate does.
    """

    def __init__(self, engine: Engine, root: bytes):
        if not engine.witnessed(root):
            raise NotWitnessed(f"root not witnessed: {to_hex(root)}")
        self._engine = engine
        self._root = root
        self._direction = None
        self._boundary = None
        self._signal = None

    def direction(self, d):
        if d not in (Direction.UPSTREAM, Direction.DOWNSTREAM, Direction.UNDIRECTED):
            raise Refused(f"unknown direction: {d}")
        self._direction = d
        return self

    def upstream(self):
        return self.direction(Direction.UPSTREAM)

    def downstream(self):
        return self.direction(Direction.DOWNSTREAM)

    def undirected(self):
        return self.direction(Direction.UNDIRECTED)

    def hops(self, n):
        self._boundary = Boundary.hops(n)
        return self

    def saturated(self):
        self._boundary = Boundary.saturated()
        return self

    def signal(self, s):
        self._signal = s
        return self

    def build(self) -> Projection:
        missing = [
            name
            for name, value in (
                ("direction", self._direction),
                ("boundary (hops or saturated)", self._boundary),
                ("signal", self._signal),
            )
            if value is None
        ]
        if missing:
            raise Refused("projection is missing: " + ", ".join(missing))
        return Projection(self._root, self._direction, self._boundary, self._signal)

    def run(self) -> MaterializedProjection:
        return self.build().run(self._engine)


def project(engine: Engine, root: bytes) -> ProjectionBuilder:
    """Entry point, mirroring `engine.project(root)`."""
    return ProjectionBuilder(engine, root)
