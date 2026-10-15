"""Self-consistency for the prototype engine, plus one tie back to the fixture.

What these tests can and cannot claim is the point of the file. Every
operation `engine.py` adds is in `conformance/MANIFEST.json`'s **exempt**
set — no wire form, unspecified order, or a Rust-specific signature — so
none of it is covered by the 103-value conformance fixture and none of it
inherits that guarantee. What is asserted here instead:

  * the auditor accepts what this engine builds, and **rejects each of the
    five violations it claims to catch** — an auditor nobody has broken is
    an auditor nobody has tested;
  * replay reconstructs identically from a shuffled or reversed dump, and
    raises the right error for each way a dump can be wrong;
  * `r = 2d - 3` and per-node degree hold as the laws state them;
  * and `test_the_subclass_did_not_change_the_operator` ties back to
    `conformance/vectors.json`, because subclassing the gated reference is
    only safe if the operator is still the gated one.

Stdlib only, like the rest of this repository's Python: `certify.py` runs
under a bare `python3` and so does this. No pytest, no virtualenv, nothing
to install.

Run:  python3 -m unittest discover -s experiments/python -t experiments/python
"""

import json
import pathlib
import random
import time
import unittest

# `engine` puts the gated reference on sys.path as a side effect of import,
# so `substrate` is taken from it rather than imported directly — that way
# this file does not depend on import order to resolve.
from engine import substrate  # noqa: F401  (also used directly below)

from engine import (
    D0,
    D1,
    Adjacency,
    BinaryParentageMismatch,
    Boundary,
    Degree,
    Direction,
    HopDistance,
    Projection,
    Refused,
    Signal,
    project,
    ContentAddressMismatch,
    DegreeMismatch,
    Engine,
    Mismatch,
    MissingPrimordial,
    NotWitnessed,
    ParseError,
    SelfParentage,
    Unreachable,
    UnregisteredParent,
    from_hex,
    replay_topological,
    synthesize_pure,
    to_hex,
)

ROOT = pathlib.Path(__file__).resolve().parents[2]


def grow(n=300):
    """A graph wide enough that degree varies and triangles exist."""
    e = Engine()
    nodes = [D0, D1]
    i = 0
    while len(nodes) < n and i + 1 < len(nodes):
        nodes.append(e.synthesize(nodes[i], nodes[i + 1]))
        i += 1
    return e, nodes


def chain(n):
    """The shape an agent produces: every entry depends on the one before it."""
    e = Engine()
    acc = e.synthesize(D0, D1)
    for i in range(n):
        acc = e.synthesize(acc, D1 if i % 2 else D0)
    dump = e.snapshot_parentage()
    dump.reverse()
    return dump


class TheLaws(unittest.TestCase):
    def test_the_auditor_accepts_a_grown_engine(self):
        e, _ = grow()
        self.assertIsNone(e.check_structural_invariant())

    def test_relationship_count_is_two_d_minus_three(self):
        e, _ = grow()
        self.assertEqual(e.relationship_count(), 2 * e.distinction_count() - 3)

    def test_degree_sums_to_twice_the_edge_count(self):
        """Handshake: every edge contributes two endpoints."""
        e, _ = grow()
        total = sum(e.degree(x) for x in e.snapshot_distinctions())
        self.assertEqual(total, 2 * e.relationship_count())

    def test_idempotence_returns_the_input_and_witnesses_nothing(self):
        e, nodes = grow(40)
        before = e.distinction_count()
        for x in nodes[:10]:
            self.assertEqual(e.synthesize(x, x), x)
        self.assertEqual(e.distinction_count(), before)

    def test_commutativity(self):
        e, nodes = grow(40)
        for a, b in zip(nodes, nodes[3:]):
            self.assertEqual(e.synthesize(a, b), e.synthesize(b, a))


class ParentageReads(unittest.TestCase):
    def test_parents_of_distinguishes_primordial_from_absent(self):
        """`None` means primordial; an unwitnessed id raises.

        Collapsing the two would make an absent node look like a root,
        which is the sort of thing a traversal silently believes.
        """
        e, _ = grow(20)
        self.assertIsNone(e.parents_of(D0))
        self.assertIsNone(e.parents_of(D1))

        c = e.synthesize(D0, D1)
        self.assertEqual(e.parents_of(c), (D0, D1))

        stranger = synthesize_pure(bytes([0xFF] * 16), bytes([0xEE] * 16))
        with self.assertRaises(NotWitnessed):
            e.parents_of(stranger)

    def test_parentage_is_stored_in_canonical_order(self):
        e, _ = grow(60)
        for child, (p0, p1) in e.snapshot_parentage():
            self.assertLess(p0, p1, "canonical order is min, max")
            self.assertEqual(synthesize_pure(p0, p1), child)

    def test_get_distinction_by_id_answers_membership_only(self):
        e, _ = grow(20)
        c = e.synthesize(D0, D1)
        self.assertEqual(e.get_distinction_by_id(c), c)
        self.assertIsNone(e.get_distinction_by_id(bytes([0x42] * 16)))
        self.assertIsNone(e.get_distinction_by_id(b"short"))

    def test_snapshots_agree_with_the_counts(self):
        e, _ = grow()
        self.assertEqual(len(e.snapshot_distinctions()), e.distinction_count())
        # Primordials carry no parentage, so the dump is two shorter.
        self.assertEqual(len(e.snapshot_parentage()), e.distinction_count() - 2)

    def test_hex_round_trip_and_rejection(self):
        e, _ = grow(20)
        c = e.synthesize(D0, D1)
        self.assertEqual(from_hex(to_hex(c)), c)
        with self.assertRaises(ParseError):
            from_hex("abcd")
        with self.assertRaises(ParseError):
            from_hex("g" * 32)


class TheAuditorsFive(unittest.TestCase):
    """One mutant per variant.

    Each corrupts the engine directly, which is the only way to reach these
    states — the operator cannot produce them.
    """

    def test_binary_parentage(self):
        e, _ = grow(20)
        third = bytes([0x01] * 16)
        e.parents[third] = None  # a third "primordial"
        e.children[third] = []
        with self.assertRaises(BinaryParentageMismatch):
            e.check_structural_invariant()

    def test_an_unregistered_parent(self):
        e, _ = grow(20)
        orphan = bytes([0x7A] * 16)
        child = synthesize_pure(D0, orphan)
        e.parents[child] = (min(D0, orphan), max(D0, orphan))
        e.children[child] = []
        with self.assertRaises(UnregisteredParent):
            e.check_structural_invariant()

    def test_a_content_address_mismatch(self):
        """The check that tests Axiom 4 rather than restating it."""
        e, nodes = grow(20)
        e.parents[nodes[5]] = (D0, D1)  # real node, wrong parentage
        with self.assertRaises(ContentAddressMismatch):
            e.check_structural_invariant()

    def test_self_parentage(self):
        e, _ = grow(20)
        fake = bytes([0x5C] * 16)
        e.parents[fake] = (D0, D0)
        e.children[fake] = []
        with self.assertRaises(SelfParentage):
            e.check_structural_invariant()

    def test_a_per_node_degree_error(self):
        """Per-node, not the sum.

        The children index is corrupted while parentage stays correct,
        which is exactly the state a sum-only check confirms as healthy.
        """
        e, nodes = grow(20)
        e.children[nodes[4]].append(nodes[7])
        with self.assertRaises(DegreeMismatch):
            e.check_structural_invariant()


class Replay(unittest.TestCase):
    def test_reconstructs_identically(self):
        e, _ = grow()
        restored = replay_topological(e.snapshot_parentage())

        self.assertEqual(restored.distinction_count(), e.distinction_count())
        self.assertEqual(restored.relationship_count(), e.relationship_count())
        self.assertEqual(
            sorted(restored.snapshot_parentage()), sorted(e.snapshot_parentage())
        )
        self.assertIsNone(restored.check_structural_invariant())

    def test_is_order_independent(self):
        """Law 9. The dump is a set of facts; arrival order is not structure."""
        e, _ = grow()
        want = sorted(e.snapshot_parentage())

        for order in ("shuffled", "reversed"):
            with self.subTest(order=order):
                dump = e.snapshot_parentage()
                if order == "shuffled":
                    random.Random(42).shuffle(dump)
                else:
                    dump.reverse()
                restored = replay_topological(dump)
                self.assertEqual(sorted(restored.snapshot_parentage()), want)

    def test_tolerates_duplicate_entries(self):
        e, _ = grow(80)
        dump = e.snapshot_parentage()
        restored = replay_topological(dump + dump)
        self.assertEqual(restored.distinction_count(), e.distinction_count())
        self.assertIsNone(restored.check_structural_invariant())

    def test_of_an_empty_dump_is_the_bare_engine(self):
        restored = replay_topological([])
        self.assertEqual(restored.distinction_count(), 2)
        self.assertIsNone(restored.check_structural_invariant())

    def test_refuses_a_dump_that_cannot_bootstrap(self):
        """Nothing roots in the primordials."""
        e, _ = grow(40)
        dump = [
            (c, p) for c, p in e.snapshot_parentage() if D0 not in p and D1 not in p
        ]
        self.assertTrue(dump, "the filter left something to replay")
        with self.assertRaises(MissingPrimordial):
            replay_topological(dump)

    def test_refuses_a_dump_with_a_hole(self):
        """An intermediate removed: later entries can never become ready."""
        e, _ = grow(60)
        dump = e.snapshot_parentage()
        del dump[len(dump) // 2]
        with self.assertRaises(Unreachable):
            replay_topological(dump)

    def test_refuses_a_tampered_child(self):
        e, _ = grow(40)
        dump = e.snapshot_parentage()
        _child, parents = dump[10]
        dump[10] = (bytes([0x00] * 15 + [0x99]), parents)  # declared child is a lie
        with self.assertRaises((Mismatch, Unreachable)):
            replay_topological(dump)

    def test_is_linear_not_quadratic(self):
        """Asserting the separation between complexity classes, not a budget.

        A wall-clock threshold would depend on the machine. Four times the
        entries costs a linear algorithm about four times the work and a
        quadratic one about sixteen, so a 9x ceiling is unreachable from
        the wrong side.
        """
        small, large = chain(600), chain(2400)

        t0 = time.perf_counter()
        replay_topological(small)
        t_small = time.perf_counter() - t0

        t0 = time.perf_counter()
        replay_topological(large)
        t_large = time.perf_counter() - t0

        ratio = t_large / max(t_small, 1e-9)
        self.assertLess(ratio, 9, f"4x the entries cost {ratio:.1f}x - not linear")


class TieToTheFixture(unittest.TestCase):
    def test_the_subclass_did_not_change_the_operator(self):
        """Subclassing the gated reference is only safe if the operator is intact.

        The three `/operator/cases` values are published identity. If this
        fails, the prototype has forked from the substrate and nothing else
        in this file means anything.
        """
        fixture = json.loads((ROOT / "conformance" / "vectors.json").read_text())
        e = Engine()
        named = {"d0": D0, "d1": D1}

        for case in fixture["operator"]["cases"]:
            p0, p1 = (
                named[p] if p in named else from_hex(p) for p in case["parents"]
            )
            produced = e.synthesize(p0, p1)
            self.assertEqual(to_hex(produced), case["id"], case["name"])
            named[case["name"]] = produced

        self.assertIsNone(e.check_structural_invariant())


def fixture():
    return json.loads((ROOT / "conformance" / "vectors.json").read_text())


def fixture_history():
    """The §10.1 history every projection case is taken over."""
    f = fixture()
    e = Engine()
    named = {"d0": D0, "d1": D1}
    order = ["c", "d", "e"]
    for i, (a, b) in enumerate(f["projections"]["history"]["pairs"]):
        produced = e.synthesize(named[a], named[b])
        named[order[i]] = produced
    return e, named, f


DIRECTIONS = {
    "Upstream": Direction.UPSTREAM,
    "Downstream": Direction.DOWNSTREAM,
    "Undirected": Direction.UNDIRECTED,
}
SIGNALS = {"Adjacency": Adjacency, "Degree": Degree, "HopDistance": HopDistance}


class TheProjectionSurfaceIsByteTransparent(unittest.TestCase):
    """The new builder and `Signal` objects must not move published identity.

    This is the test that licenses the whole wrapper. The three shipped
    signals delegate to `substrate.signal_output`, and `Projection`
    overrides only `materialize` — so all twenty published projection cases
    must come back bit for bit through the new surface. If they do not, the
    prototype has forked from the substrate.
    """

    def test_all_published_projection_cases_round_trip(self):
        e, named, f = fixture_history()
        cases = f["projections"]["cases"]
        self.assertEqual(len(cases), 20, "the fixture's projection count")

        for case in cases:
            with self.subTest(case=case["name"]):
                root = from_hex(case["root"])
                b = case["boundary"]
                builder = (
                    project(e, root)
                    .direction(DIRECTIONS[case["direction"]])
                    .signal(SIGNALS[case["signal"]])
                )
                if b["kind"] == "Hops":
                    builder = builder.hops(b["value"])
                else:
                    builder = builder.saturated()

                got = builder.run()
                self.assertEqual(to_hex(got.canonical_bytes()), case["canonical_bytes"])
                self.assertEqual(to_hex(got.projection_id()), case["projection_id"])

    def test_the_wrapper_matches_the_reference_projection_directly(self):
        """Belt and braces: compare against `substrate.Projection` itself."""
        e, named, _ = fixture_history()
        root = named["c"]

        for name, sig in (("adjacency", Adjacency), ("degree", Degree)):
            for direction in DIRECTIONS.values():
                with self.subTest(signal=name, direction=direction):
                    reference = substrate.Projection(root, direction, 0x00, 2, name)
                    mine = Projection(root, direction, Boundary.hops(2), sig)
                    self.assertEqual(
                        mine.canonical_bytes(e), reference.canonical_bytes(e)
                    )
                    self.assertEqual(mine.projection_id(), reference.projection_id())


class ConsumerSignals(unittest.TestCase):
    """The extension point the agent layer needs."""

    def test_a_consumer_signal_projects(self):
        class ParityOfDegree(Signal):
            def canonical_bytes(self):
                return b"prototype:signal:parity_of_degree"

            def compute(self, engine, node, dist):
                return bytes([engine.degree(node) % 2])

        e, named, _ = fixture_history()
        got = project(e, named["c"]).undirected().saturated().signal(ParityOfDegree()).run()

        self.assertEqual(len(got), e.distinction_count(), "saturated reaches everything")
        for node, output in got:
            self.assertEqual(output, bytes([e.degree(node) % 2]))

    def test_a_consumer_signal_has_its_own_identity(self):
        """Two signals computing differently must not collide.

        Identity is derived from `canonical_bytes()`, so a consumer who
        returns a constant for two different computations gets one id for
        two meanings — which is why the trait asks for it explicitly.
        """

        class A(Signal):
            def canonical_bytes(self):
                return b"prototype:signal:a"

            def compute(self, engine, node, dist):
                return b"\x01"

        class B(Signal):
            def canonical_bytes(self):
                return b"prototype:signal:b"

            def compute(self, engine, node, dist):
                return b"\x01"

        e, named, _ = fixture_history()
        a = project(e, named["c"]).upstream().hops(1).signal(A()).run()
        b = project(e, named["c"]).upstream().hops(1).signal(B()).run()

        self.assertNotEqual(a.projection_id(), b.projection_id())
        self.assertNotEqual(a.canonical_bytes(), b.canonical_bytes())

    def test_a_consumer_signal_is_distinct_from_the_shipped_ones(self):
        """A custom signal cannot impersonate `Degree` by computing like it."""

        class LooksLikeDegree(Signal):
            def canonical_bytes(self):
                return b"prototype:signal:looks_like_degree"

            def compute(self, engine, node, dist):
                return engine.degree(node).to_bytes(8, "little")

        e, named, _ = fixture_history()
        mine = project(e, named["c"]).undirected().hops(2).signal(LooksLikeDegree()).run()
        real = project(e, named["c"]).undirected().hops(2).signal(Degree).run()

        # Same per-node outputs...
        self.assertEqual([o for _, o in mine], [o for _, o in real])
        # ...different identity, because §8 hashes the specification.
        self.assertNotEqual(mine.projection_id(), real.projection_id())

    def test_a_non_signal_is_refused(self):
        e, named, _ = fixture_history()
        with self.assertRaises(Refused):
            Projection(named["c"], Direction.UPSTREAM, Boundary.hops(1), "degree")


class TheBuilder(unittest.TestCase):
    def test_an_incomplete_projection_names_what_is_missing(self):
        """Python cannot typestate this, so it must fail loudly instead."""
        e, named, _ = fixture_history()

        with self.assertRaises(Refused) as caught:
            project(e, named["c"]).build()
        message = str(caught.exception)
        for axis in ("direction", "boundary", "signal"):
            self.assertIn(axis, message)

        with self.assertRaises(Refused):
            project(e, named["c"]).upstream().build()

    def test_an_unwitnessed_root_is_refused_at_the_builder(self):
        e, _, _ = fixture_history()
        with self.assertRaises(NotWitnessed):
            project(e, bytes([0xAB] * 16))

    def test_an_unknown_direction_is_refused(self):
        e, named, _ = fixture_history()
        with self.assertRaises(Refused):
            project(e, named["c"]).direction(0x09)

    def test_negative_hops_is_refused(self):
        e, named, _ = fixture_history()
        with self.assertRaises(Refused):
            project(e, named["c"]).hops(-1)

    def test_the_materialized_read_answers_membership(self):
        e, named, _ = fixture_history()
        got = project(e, named["c"]).upstream().saturated().signal(Degree).run()

        self.assertTrue(got.contains(D0))
        self.assertTrue(got.contains(named["c"]))
        self.assertIsNone(got.get(bytes([0x11] * 16)))
        self.assertFalse(got.contains(bytes([0x11] * 16)))
        self.assertEqual(got.spec().boundary, Boundary.saturated())


class ProjectingIsReadOnly(unittest.TestCase):
    def test_no_shipped_signal_changes_the_engine(self):
        """Cond E for the three the specification pins."""
        e, named, _ = fixture_history()
        before = (
            e.distinction_count(),
            sorted(e.snapshot_parentage()),
            sorted((n, e.degree(n)) for n in e.snapshot_distinctions()),
        )

        for sig in (Adjacency, Degree, HopDistance):
            for direction in DIRECTIONS.values():
                for saturated in (True, False):
                    builder = project(e, named["c"]).direction(direction).signal(sig)
                    builder = builder.saturated() if saturated else builder.hops(2)
                    got = builder.run()
                    _ = got.canonical_bytes(), got.projection_id(), len(got)

        after = (
            e.distinction_count(),
            sorted(e.snapshot_parentage()),
            sorted((n, e.degree(n)) for n in e.snapshot_distinctions()),
        )
        self.assertEqual(after, before)
        self.assertIsNone(e.check_structural_invariant())

    def test_a_consumer_signal_can_break_the_contract(self):
        """Read-only is enforced for the shipped signals and a contract for yours.

        The same finding as `tests/read_only.rs`: no signature can stop a
        consumer signal holding an engine and synthesizing through it. If
        this ever stops being true, a signature started enforcing what only
        a contract can.
        """

        class Mutating(Signal):
            def __init__(self, engine):
                self._engine = engine

            def canonical_bytes(self):
                return b"prototype:signal:mutating"

            def compute(self, engine, node, dist):
                self._engine.synthesize(node, D1)
                return b"\x00"

        e, named, _ = fixture_history()
        before = e.distinction_count()

        project(e, named["c"]).undirected().saturated().signal(Mutating(e)).run()

        self.assertGreater(e.distinction_count(), before)
        # A contract breach is not corruption: the engine is still lawful.
        self.assertIsNone(e.check_structural_invariant())


class Restore(unittest.TestCase):
    def test_the_wire_form_restores_through_the_reference(self):
        e, named, _ = fixture_history()
        got = project(e, named["c"]).upstream().hops(2).signal(Adjacency).run()
        restored = substrate.restore(e, got.canonical_bytes(), "adjacency")
        self.assertEqual(restored.canonical_bytes(e), got.canonical_bytes())

    def test_a_tampered_entry_length_is_refused(self):
        """Offset 71, not 63 — §7's header is 55 bytes.

        `substrate.py` has had `HEADER_LEN = 55` all along, which is how the
        `47 + 16` error on the Rust side was caught.
        """
        e, named, _ = fixture_history()
        wire = bytearray(
            project(e, named["c"]).upstream().hops(2).signal(Adjacency).run().canonical_bytes()
        )
        self.assertEqual(substrate.HEADER_LEN, 55)
        wire[71] = (wire[71] + 1) & 0xFF
        with self.assertRaises(Refused):
            substrate.restore(e, bytes(wire), "adjacency")


if __name__ == "__main__":
    unittest.main()
