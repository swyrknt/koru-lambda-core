#!/usr/bin/env python3
"""Differential harness: the Python prototype against the Rust substrate.

WHAT IT DOES. Generates a random *plan* — a line-oriented script of
synthesis steps and reads — executes it against
`experiments/python/engine.py` (which wraps the gated
`conformance/reference/substrate.py`), executes the identical plan against
`experiments/runner/src/bin/diff_runner.rs`, and compares the two record
streams field for field.

The plan is the only shared state. Every command names its operands by
index into a pool both sides build the same way, so neither side is ever
told anything the other computed. A record-for-record difference is
therefore a difference between the two implementations.

WHAT IS COMPARED. Per plan:

  * `synth`   — the identifier and the novelty bit for every pair, including
                self-pairs (§2 idempotence), swapped pairs (§2
                commutativity) and repeats (saturation).
  * `degree`  — per node, not the sum (§6.1).
  * `counts`  — distinction and relationship counts (§9).
  * `parents` — the canonical pair, and the whole canonical-sorted dump.
  * `project` — `canonical_bytes` (§7) *and* `projection_id` (§8) across
                3 signals x 3 directions x {hops 0,1,2,3, saturated}, from
                roots including both primordials (§5's genesis-edge rule).
  * `replay`  — round-trip from sorted, reversed, shuffled, duplicated and
                gapped entry orders.
  * `restore` — each side's own wire bytes fed to the other, plus tampered
                bytes at every §7 header offset.
  * the lift  — all 256 `tip_of_byte` values, the engine state they leave
                (§4.3), `domain_anchor`, `fold_bytes` (§4.2) and
                `fold_composite` (§4.4) including both refusal rules.
  * `cross`   — a distinction from one engine read against another.

WHAT IS NOT COMPARED. One field, `detail`: the free text a refusal carries,
which is an `assert!` message on one side and an exception's `str` on the
other. `ok` and the engine size the refusal left behind are compared.

KNOWN DIVERGENCE, so that a *second* one is not mistaken for this one. The
baseline is not clean. `conformance/reference/substrate.py`'s `restore`
accepts a frame whose boundary tag says saturated (`0x01` at offset 21) and
whose boundary payload (offsets 22..30) is not zero; the Rust engine refuses
it. `docs/SPEC.md` §7 pins that field to "hop count; `0` when saturated", so
such a frame is not a canonical encoding of any projection and §7's restore
rule — "Bytes that do not match what this engine would have produced are
rejected" — requires a refusal. The reference reproduces the tampered payload
instead of normalising it, so its own byte comparison cannot see it. Every
`restore tamper#N@21` and `restore tamper#N@22` difference this harness
reports is that one defect. `conformance/vectors.json` has no reject case for
it, which is why neither side's gate caught it.

A second, smaller one. `cross_project` hands a `Distinction` produced by one
engine to another engine's `project`. The Rust engine materialises a
projection with **zero** entries and emits canonical bytes for it; the
prototype refuses with `NotWitnessed`. §5's root axis admits "any witnessed
distinction" and prescribes nothing for an unwitnessed one, so the
specification does not decide which read is right. What it does decide is
§7: canonical bytes are what *this* engine would produce for that
specification, and the frame the Rust side emits here is accepted by no
engine at all — not by one that witnesses the root (which would emit one
entry) and not by one that does not (which refuses the root). Every
`cross_project` difference is that.

THE HARNESS MUST BE ABLE TO FAIL. `--inject <fault>` perturbs the Python
record stream in a named way so that a green run can be told from a blind
one. Run `--self-test` to drive every fault and confirm each is caught.

Stdlib only, matching `substrate.py`.

    python3 experiments/python/diff_harness.py --self-test
    python3 experiments/python/diff_harness.py --histories 40 --seed 7
"""

import argparse
import hashlib
import json
import os
import pathlib
import random
import re
import subprocess
import sys

_HERE = pathlib.Path(__file__).resolve().parent
_ROOT = _HERE.parents[1]
if str(_HERE) not in sys.path:
    sys.path.insert(0, str(_HERE))

import engine as proto  # noqa: E402
import substrate  # noqa: E402  (engine.py put conformance/reference on the path)

# ------------------------------------------------------------------ plan vocab

DIRS = {"up": substrate.UPSTREAM, "down": substrate.DOWNSTREAM, "undir": substrate.UNDIRECTED}
SIGNALS = {"adj": "adjacency", "deg": "degree", "hop": "hop_distance"}
SIGNAL_OBJS = {"adj": proto.Adjacency, "deg": proto.Degree, "hop": proto.HopDistance}
BOUNDARIES = ["h0", "h1", "h2", "h3", "sat"]

_INVARIANT_KINDS = [
    (proto.BinaryParentageMismatch, "binary_parentage"),
    (proto.UnregisteredParent, "unregistered_parent"),
    (proto.ContentAddressMismatch, "content_address"),
    (proto.DegreeMismatch, "degree"),
    (proto.SelfParentage, "self_parentage"),
]


def _boundary(tok):
    """`h<N>` / `sat` to a prototype `Boundary`."""
    if tok == "sat":
        return proto.Boundary.saturated()
    return proto.Boundary.hops(int(tok[1:]))


def _payload(tok):
    """Decode a plan payload. `.` is the empty sequence.

    A plan is whitespace split, so an empty token cannot survive the trip,
    and a zero-length fold is a case worth running: `docs/SPEC.md` §4.2 folds
    the length first, so the empty sequence still makes eight synthesize
    calls.
    """
    return b"" if tok == "." else bytes.fromhex(tok)


def _list(tok):
    """Split a comma-separated plan list; `.` is the empty list."""
    return [] if tok == "." else [t for t in tok.split(",") if t]


def _dump_digest(entries):
    """SHA-256 over a canonical-sorted dump, `child || p0 || p1` per entry.

    The same bytes the Rust side hashes, so a one-value comparison stands in
    for the whole dump when the dump itself is large.
    """
    h = hashlib.sha256()
    for child, p0, p1 in entries:
        h.update(child)
        h.update(p0)
        h.update(p1)
    return h.hexdigest()


def _sorted_dump(eng):
    """`snapshot_parentage` in canonical order.

    `docs/SPEC.md` fixes no order for the dump and both implementations say
    so, which is exactly why the harness has to sort before comparing rather
    than assume either side's iteration order.
    """
    out = [(c, p[0], p[1]) for c, p in eng.snapshot_parentage()]
    out.sort()
    return out


# ------------------------------------------------------------------ python exec


class PythonSide:
    """Executes a plan against the prototype, emitting the record stream."""

    def __init__(self):
        self.engine = proto.Engine()
        self.pool = [proto.D0, proto.D1]
        self.lift = None  # (engine, d, anchors dict, vals list)

    # --- lift refs ------------------------------------------------------

    def _lift_ref(self, ref):
        eng, d, anchors, vals = self.lift
        if ref == "d0":
            return proto.D0
        if ref == "d1":
            return proto.D1
        if ref == "d":
            return d
        if ref.startswith("t:"):
            return substrate.tip_of_byte(eng, int(ref[2:], 16))
        if ref.startswith("dom:"):
            return anchors[int(ref[4:], 16)]
        if ref.startswith("v:"):
            return vals[int(ref[2:])]
        raise ValueError(f"unknown lift ref {ref!r}")

    def _fold_rec(self, op, fn):
        """Run a fold, recording the refusal and the engine size either way."""
        eng = self.lift[0]
        try:
            d = fn()
        except (proto.Refused, proto.NotWitnessed) as exc:
            return {
                "op": op,
                "ok": False,
                "detail": str(exc),
                "distinctions": eng.distinction_count(),
            }
        self.lift[3].append(d)
        return {
            "op": op,
            "ok": True,
            "slot": len(self.lift[3]) - 1,
            "id": d.hex(),
            "distinctions": eng.distinction_count(),
        }

    # --- dispatch -------------------------------------------------------

    def run(self, plan):
        records = []
        for line in plan:
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            records.append(self.step(line))
        return records

    def step(self, line):
        f = line.split()
        op, args = f[0], f[1:]
        eng = self.engine

        if op == "synth":
            a, b = (self.pool[int(args[0])], self.pool[int(args[1])])
            d, novel = eng.synthesize_novel(a, b)
            self.pool.append(d)
            return {"op": "synth", "slot": len(self.pool) - 1, "id": d.hex(), "novel": novel}

        if op == "degree":
            d = self.pool[int(args[0])]
            return {"op": "degree", "id": d.hex(), "value": eng.degree(d)}

        if op == "counts":
            return {
                "op": "counts",
                "distinctions": eng.distinction_count(),
                "relationships": eng.relationship_count(),
            }

        if op == "parents":
            d = self.pool[int(args[0])]
            p = eng.parents_of(d)
            return {
                "op": "parents",
                "id": d.hex(),
                "parents": None if p is None else [p[0].hex(), p[1].hex()],
            }

        if op == "verify_raw":
            raw = bytes.fromhex(args[0])
            return {"op": "verify_raw", "id": args[0], "witnessed": eng.verify(raw) is not None}

        if op == "cross":
            d = self.pool[int(args[0])]
            other = proto.Engine()
            return {
                "op": "cross",
                "id": d.hex(),
                "has": other.witnessed(d),
                "degree": other.degree(d),
                "parents_some": other.parents.get(d) is not None,
            }

        if op == "cross_synth":
            d = self.pool[int(args[0])]
            other = proto.Engine()
            try:
                got = other.synthesize(d, proto.D0)
            except (proto.NotWitnessed, proto.Refused) as exc:
                return {
                    "op": "cross_synth",
                    "id": d.hex(),
                    "ok": False,
                    "detail": str(exc),
                    "distinctions": other.distinction_count(),
                }
            return {
                "op": "cross_synth",
                "id": d.hex(),
                "ok": True,
                "result": got.hex(),
                "distinctions": other.distinction_count(),
            }

        if op == "cross_project":
            d = self.pool[int(args[0])]
            other = proto.Engine()
            rec = {
                "op": "cross_project",
                "root": d.hex(),
                "direction": args[1],
                "boundary": args[2],
                "signal": args[3],
            }
            try:
                built = proto.Projection(
                    d, DIRS[args[1]], _boundary(args[2]), SIGNAL_OBJS[args[3]]
                ).run(other)
            except (proto.NotWitnessed, proto.Refused) as exc:
                return {**rec, "ok": False, "detail": str(exc)}
            return {
                **rec,
                "ok": True,
                "entries": len(built),
                "bytes": built.canonical_bytes().hex(),
                "pid": built.projection_id().hex(),
            }

        if op == "dump":
            entries = _sorted_dump(eng)
            return {
                "op": "dump",
                "len": len(entries),
                "entries": [[c.hex(), a.hex(), b.hex()] for c, a, b in entries],
            }

        if op == "digest":
            return {
                "op": "digest",
                "value": _dump_digest(_sorted_dump(eng)),
                "distinctions": eng.distinction_count(),
            }

        if op == "audit":
            try:
                eng.check_structural_invariant()
            except proto.InvariantError as exc:
                kind = next((n for cls, n in _INVARIANT_KINDS if isinstance(exc, cls)), "other")
                return {"op": "audit", "ok": False, "kind": kind}
            return {"op": "audit", "ok": True}

        if op == "project":
            root = self.pool[int(args[0])]
            built = proto.Projection(
                root, DIRS[args[1]], _boundary(args[2]), SIGNAL_OBJS[args[3]]
            ).run(eng)
            return {
                "op": "project",
                "root": root.hex(),
                "direction": args[1],
                "boundary": args[2],
                "signal": args[3],
                "entries": len(built),
                "bytes": built.canonical_bytes().hex(),
                "pid": built.projection_id().hex(),
            }

        if op == "restore":
            data = _payload(args[1])
            try:
                substrate.restore(eng, data, SIGNALS[args[0]])
                accepted = True
            except (proto.Refused, proto.NotWitnessed, IndexError, ValueError, KeyError):
                accepted = False
            return {"op": "restore", "signal": args[0], "accepted": accepted}

        if op == "replay":
            dump = _sorted_dump(eng)
            idx = [int(t) for t in _list(args[0])]
            entries = [(dump[i][0], (dump[i][1], dump[i][2])) for i in idx]
            try:
                replayed = proto.replay_topological(entries)
            except proto.MissingPrimordial:
                return {"op": "replay", "fed": len(entries), "outcome": "missing_primordial"}
            except proto.Unreachable as exc:
                return {
                    "op": "replay",
                    "fed": len(entries),
                    "outcome": "unreachable",
                    "remaining": exc.remaining,
                }
            except proto.Mismatch:
                return {"op": "replay", "fed": len(entries), "outcome": "mismatch"}
            return {
                "op": "replay",
                "fed": len(entries),
                "outcome": "ok",
                "distinctions": replayed.distinction_count(),
                "relationships": replayed.relationship_count(),
                "digest": _dump_digest(_sorted_dump(replayed)),
            }

        if op == "lift_init":
            lift_eng = proto.Engine()
            substrate.build_byte_alphabet(lift_eng)
            tips = [substrate.tip_of_byte(lift_eng, v).hex() for v in range(256)]
            self.lift = [lift_eng, None, {}, []]
            return {
                "op": "lift_init",
                "tips": tips,
                "distinctions": lift_eng.distinction_count(),
                "relationships": lift_eng.relationship_count(),
                "degree_d0": lift_eng.degree(proto.D0),
                "degree_d1": lift_eng.degree(proto.D1),
            }

        if op == "lift_d":
            lift_eng = self.lift[0]
            c = lift_eng.synthesize(proto.D0, proto.D1)
            d = lift_eng.synthesize(c, proto.D0)
            self.lift[1] = d
            return {"op": "lift_d", "id": d.hex(), "distinctions": lift_eng.distinction_count()}

        if op == "lift_dom":
            lift_eng = self.lift[0]
            label = int(args[0], 16)
            anchor = substrate.domain_anchor(lift_eng, label)
            self.lift[2][label] = anchor
            return {
                "op": "lift_dom",
                "label": args[0],
                "id": anchor.hex(),
                "distinctions": lift_eng.distinction_count(),
            }

        if op == "lift_acc":
            lift_eng = self.lift[0]
            acc = self._lift_ref(args[0])
            for b in _payload(args[1]):
                acc = lift_eng.synthesize(acc, substrate.tip_of_byte(lift_eng, b))
            self.lift[3].append(acc)
            return {
                "op": "lift_acc",
                "slot": len(self.lift[3]) - 1,
                "id": acc.hex(),
                "distinctions": lift_eng.distinction_count(),
            }

        if op == "lift_fold":
            lift_eng = self.lift[0]
            anchor = self._lift_ref(args[0])
            elements = list(_payload(args[1]))
            return self._fold_rec(
                "lift_fold", lambda: substrate.fold_bytes(lift_eng, anchor, elements)
            )

        if op == "lift_comp":
            lift_eng = self.lift[0]
            anchor = self._lift_ref(args[0])
            parts = [self._lift_ref(t) for t in _list(args[1])]
            return self._fold_rec(
                "lift_comp", lambda: substrate.fold_composite(lift_eng, anchor, parts)
            )

        raise ValueError(f"unknown plan command {op!r}")


# ------------------------------------------------------------------ rust side


def run_rust(binary, plan):
    """Execute a plan in the Rust runner, returning its record stream."""
    text = "\n".join(plan) + "\n"
    proc = subprocess.run(
        [str(binary)], input=text, capture_output=True, text=True, check=False
    )
    if proc.returncode != 0:
        raise RuntimeError(f"runner exited {proc.returncode}:\n{proc.stderr}")
    return [json.loads(line) for line in proc.stdout.splitlines() if line.strip()]


# ------------------------------------------------------------------ comparison

IGNORED_FIELDS = frozenset({"detail"})


def group(diffs):
    """Collapse differences into classes: `<command>: <normalised where>`.

    Printing only the first N differences is how a campaign reports "they all
    look like this" without having looked. The grouping covers every one.
    """
    groups = {}
    for d in diffs:
        where = re.sub(r"#\d+", "#N", re.sub(r"record \d+", "record N", d["where"]))
        head = (d["command"] or "").split()
        key = f"{head[0] if head else '-'}: {where}"
        groups[key] = groups.get(key, 0) + 1
    return sorted(groups.items(), key=lambda kv: -kv[1])


def compare(plan, py, rs):
    """Field-for-field comparison. Returns (differences, fields, per-op counts)."""
    commands = [c for c in (l.strip() for l in plan) if c and not c.startswith("#")]
    diffs = []
    compared = 0
    per_op = {}

    if len(py) != len(rs):
        diffs.append(
            {
                "where": "record count",
                "command": None,
                "python": len(py),
                "rust": len(rs),
            }
        )

    for i, (p, r) in enumerate(zip(py, rs)):
        cmd = commands[i] if i < len(commands) else "<unknown>"
        per_op[cmd.split()[0]] = per_op.get(cmd.split()[0], 0) + 1
        for key in sorted(set(p) | set(r)):
            if key in IGNORED_FIELDS:
                continue
            compared += 1
            pv, rv = p.get(key, "<absent>"), r.get(key, "<absent>")
            if pv != rv:
                diffs.append(
                    {
                        "where": f"record {i} field {key!r}",
                        "command": cmd,
                        "python": pv,
                        "rust": rv,
                    }
                )
    return diffs, compared, per_op


# ------------------------------------------------------------------ fault seeds
#
# A differential harness that cannot fail is worth nothing. Each fault below
# perturbs the PYTHON record stream after the fact, in one named place, so a
# green campaign can be distinguished from a blind one. `--self-test` drives
# all of them and asserts each is reported.

FAULTS = {
    "degree": "add 1 to the first `degree` record's value",
    "projbyte": "flip one byte of the first projection's canonical_bytes",
    "pid": "flip one byte of the first projection's projection_id",
    "tip": "flip one byte of tip_of_byte(0x00)",
    "liftstate": "report 512 distinctions after building the byte alphabet",
    "restore": "invert the first restore acceptance",
    "parents": "swap the first non-null parent pair out of canonical order",
    "replay": "drop one entry from the first replay's distinction count",
    "digest": "flip one hex digit of the first dump digest",
    "dropentry": "delete the last entry of the first dump",
}


def _flip_hex(h, pos):
    """Flip the low bit of one hex digit, keeping the string a valid hex id."""
    digits = list(h)
    digits[pos] = "%x" % (int(digits[pos], 16) ^ 1)
    return "".join(digits)


def inject(fault, records):
    """Apply one named fault to a record stream, in place. Returns True if the
    stream contained anything to perturb."""

    def first(op, pred=lambda _r: True):
        return next((r for r in records if r["op"] == op and pred(r)), None)

    if fault == "degree":
        r = first("degree")
        if r:
            r["value"] += 1
            return True
    elif fault == "projbyte":
        r = first("project")
        if r:
            r["bytes"] = _flip_hex(r["bytes"], 60)
            return True
    elif fault == "pid":
        r = first("project")
        if r:
            r["pid"] = _flip_hex(r["pid"], 0)
            return True
    elif fault == "tip":
        r = first("lift_init")
        if r:
            r["tips"][0] = _flip_hex(r["tips"][0], 5)
            return True
    elif fault == "liftstate":
        r = first("lift_init")
        if r:
            r["distinctions"] -= 1
            return True
    elif fault == "restore":
        r = first("restore")
        if r:
            r["accepted"] = not r["accepted"]
            return True
    elif fault == "parents":
        r = first("parents", lambda r: r["parents"] is not None)
        if r:
            r["parents"] = [r["parents"][1], r["parents"][0]]
            return True
    elif fault == "replay":
        r = first("replay", lambda r: r["outcome"] == "ok")
        if r:
            r["distinctions"] -= 1
            return True
    elif fault == "digest":
        r = first("digest")
        if r:
            r["value"] = _flip_hex(r["value"], 3)
            return True
    elif fault == "dropentry":
        r = first("dump", lambda r: r["len"] > 0)
        if r:
            r["entries"] = r["entries"][:-1]
            r["len"] -= 1
            return True
    else:
        raise ValueError(f"unknown fault {fault!r}")
    return False


# ------------------------------------------------------------------ plan build


def gen_history(rng, steps):
    """A random synthesis history, as plan lines.

    Deliberately not uniform: self-pairs probe §2 idempotence, a swapped
    repeat probes commutativity, and an exact repeat probes saturation. A
    uniform draw over a growing pool produces almost none of the three.
    """
    lines = []
    size = 2
    last = None
    for _ in range(steps):
        roll = rng.random()
        if roll < 0.08:
            i = rng.randrange(size)
            pair = (i, i)
        elif roll < 0.18 and last is not None:
            pair = (last[1], last[0])
        elif roll < 0.26 and last is not None:
            pair = last
        else:
            i = rng.randrange(size)
            j = rng.randrange(size)
            pair = (i, j)
        lines.append(f"synth {pair[0]} {pair[1]}")
        last = pair
        size += 1
    return lines



# The shapes a random draw almost never produces, and which the QA brief
# names: an engine holding nothing but the primordials, an engine holding one
# synthesized distinction, a chain deep enough to find a recursive traversal,
# and a hub with a fan-out far above the average degree of 4.
SHAPES = ("random", "empty", "one", "chain", "fan")


def gen_shape(shape, rng, steps):
    """A synthesis history of a named structural shape, as plan lines."""
    if shape == "random":
        return gen_history(rng, steps)
    if shape == "empty":
        return []
    if shape == "one":
        return ["synth 0 1"]
    if shape == "chain":
        # Each step folds the running tip with d1, so the upstream cone from
        # the last slot is `steps` deep. An implementation that walks it
        # recursively overflows here; both of these walk it iteratively, and
        # that is the claim being checked.
        lines = ["synth 0 1"]
        for k in range(steps - 1):
            lines.append(f"synth {2 + k} 1")
        return lines
    if shape == "fan":
        # A spoke chain first, then one hub synthesized against every spoke:
        # degree(hub) reaches `spokes + 2` while no other node passes 4.
        spokes = max(2, steps // 2)
        lines = ["synth 0 1"]
        for k in range(spokes):
            lines.append(f"synth {2 + k} 1")
        for k in range(1, spokes + 1):
            lines.append(f"synth 2 {2 + k}")
        return lines
    raise ValueError(f"unknown shape {shape!r}")


def gen_queries(rng, pool_ids):
    """Reads over the engine the history built."""
    lines = ["counts", "audit", "dump", "digest"]
    n = len(pool_ids)
    # Degree and parentage per node. Pool slots repeat (a saturating synth
    # pushes a slot for an identifier already present), so both are asked of
    # every slot — the repeats are free and they probe the read twice.
    for i in range(n):
        lines.append(f"degree {i}")
    for i in range(n):
        lines.append(f"parents {i}")
    # The trust boundary, from both sides: identifiers this engine holds, and
    # sixteen bytes it does not.
    for i in rng.sample(range(n), min(4, n)):
        lines.append(f"verify_raw {pool_ids[i]}")
    for _ in range(3):
        lines.append("verify_raw " + "".join(rng.choice("0123456789abcdef") for _ in range(32)))
    for i in rng.sample(range(n), min(3, n)):
        lines.append(f"cross {i}")
    # Cross-engine use of a `Distinction`: the identifier carries no engine
    # tag (§2.1), so a value from this engine can be handed to another. Slots
    # 0 and 1 are the primordials, which every engine witnesses (§1) — they
    # are the control, and the synthesized slots are the probe.
    for i in [0, 1] + list(rng.sample(range(n), min(2, n))):
        lines.append(f"cross_synth {i}")
        for direction in ("up", "down", "undir"):
            for boundary in ("h0", "h2", "sat"):
                lines.append(f"cross_project {i} {direction} {boundary} adj")
    return lines


def gen_projections(rng, pool_ids):
    """Every projection, from roots that include both primordials.

    Slot 0 and slot 1 are d0 and d1, and `docs/SPEC.md` §5 says the genesis
    edge is not traversed — a rule that only a projection rooted at a
    primordial can see, and the one §10 never exercises.
    """
    n = len(pool_ids)
    roots = [0, 1, n - 1]
    roots += rng.sample(range(n), min(3, n))
    lines = []
    for root in sorted(set(roots)):
        for direction in ("up", "down", "undir"):
            for boundary in BOUNDARIES:
                for signal in ("adj", "deg", "hop"):
                    lines.append(f"project {root} {direction} {boundary} {signal}")
    return lines


def gen_replays(rng, dump_len):
    """Replay from orders a live engine would never hand you in sequence."""
    if dump_len == 0:
        return ["replay ."]
    order = list(range(dump_len))
    lines = ["replay " + ",".join(map(str, order))]
    lines.append("replay " + ",".join(map(str, reversed(order))))
    for _ in range(2):
        shuffled = order[:]
        rng.shuffle(shuffled)
        lines.append("replay " + ",".join(map(str, shuffled)))
    # Duplicated entries: legal in a dump, and the two implementations
    # deduplicate differently (one by value, one not at all).
    duped = order + rng.sample(order, min(3, dump_len))
    rng.shuffle(duped)
    lines.append("replay " + ",".join(map(str, duped)))
    # A gap. Dropping an entry other than the last leaves its descendants
    # unreachable; dropping a leaf may leave the rest intact.
    if dump_len > 2:
        gapped = order[:]
        del gapped[dump_len // 2]
        lines.append("replay " + ",".join(map(str, gapped)))
    return lines


def gen_lift(rng):
    """The lift plan with `v:` slots counted out explicitly.

    A slot is consumed by `lift_acc` and by every `lift_fold` / `lift_comp`
    that *succeeds*; the two deliberate-refusal lines consume none. Counting
    is done here rather than inferred so the plan stays a plain list of lines
    that either side can execute without knowing the other's state.
    """
    lines = ["lift_init", "lift_d"]
    for label in ("00", "01", "02", "7f", "ff"):
        lines.append(f"lift_dom {label}")

    slot = 0

    def take():
        nonlocal slot
        slot += 1
        return slot - 1

    lines.append("lift_fold d 6869")
    take()
    for _ in range(4):
        width = rng.randrange(0, 6)
        payload = "".join("%02x" % rng.randrange(256) for _ in range(width)) or "."
        anchor = rng.choice(["d", "dom:01", "dom:ff"])
        lines.append(f"lift_fold {anchor} {payload}")
        take()
    lines.append("lift_fold t:41 6869")  # refused: anchor is a byte tip
    lines.append("lift_comp t:41 t:01")  # refused: same rule, §4.4 constraint 1

    lines.append("lift_comp d t:01,t:02,t:03")
    take()
    lines.append("lift_comp d t:01,t:02")
    inner = take()
    lines.append(f"lift_comp d v:{inner},t:03")
    take()
    lines.append("lift_comp dom:01 t:01,t:02,t:03")
    take()
    lines.append("lift_fold d 01")
    a = take()
    lines.append("lift_fold d 0203")
    b = take()
    lines.append(f"lift_comp d v:{a},v:{b}")
    take()
    lines.append("lift_fold d 0102")
    c = take()
    lines.append("lift_fold d 03")
    d = take()
    lines.append(f"lift_comp d v:{c},v:{d}")
    take()
    lines.append("lift_acc d 0100000000000000")
    acc = take()
    lines.append(f"lift_comp d v:{acc}")  # refused: part == accumulator
    return lines


# ------------------------------------------------------------------ fixture

_FIXTURE = json.loads((_ROOT / "conformance" / "vectors.json").read_text())


def check_against_fixture(records):
    """Third witness: the lift records against `conformance/vectors.json`.

    Two implementations can agree and both be wrong. The fixture is
    digest-pinned and gated, so the lift values it publishes are the one
    place this harness can check agreement against something neither side
    computed.
    """
    problems = []
    checked = 0
    init = next((r for r in records if r["op"] == "lift_init"), None)
    if init is None:
        return problems, checked

    for case in _FIXTURE["lift"]["tips"]:
        checked += 1
        if init["tips"][case["byte"]] != case["id"]:
            problems.append(f"tip_of_byte(0x{case['byte']:02x}): {init['tips'][case['byte']]} != {case['id']}")

    state = _FIXTURE["lift"]["engine_state"]["after_byte_alphabet"]
    for key, field in (
        ("distinction_count", "distinctions"),
        ("degree_d0", "degree_d0"),
        ("degree_d1", "degree_d1"),
    ):
        checked += 1
        if init[field] != state[key]:
            problems.append(f"after_byte_alphabet.{key}: {init[field]} != {state[key]}")

    doms = {r["label"]: r["id"] for r in records if r["op"] == "lift_dom"}
    for case in _FIXTURE["lift"]["domain_anchors"]["cases"]:
        label = "%02x" % case["label"]
        if label in doms:
            checked += 1
            if doms[label] != case["id"]:
                problems.append(f"domain_anchor(0x{label}): {doms[label]} != {case['id']}")

    # The fixture's fold and composite ids, matched positionally against the
    # fixed lines `gen_lift` emits for them.
    ok_folds = [r["id"] for r in records if r["op"] in ("lift_fold", "lift_comp") and r["ok"]]
    expected = [
        ("fold_hi_anchored_at_d", _FIXTURE["lift"]["folds"][0]["id"], 0),
    ]
    comp_names = [
        "flat_1_2_3",
        "inner_1_2",
        "nested_1_2_then_3",
        "flat_1_2_3_under_domain_01",
    ]
    by_name = {c["name"]: c["id"] for c in _FIXTURE["lift"]["composites"]["cases"]}
    # 1 (fold_hi) + 4 random folds = indices 0..4; composites start at 5.
    for offset, name in enumerate(comp_names):
        expected.append((name, by_name[name], 5 + offset))
    expected.append(("composite_of_variable_length_parts_1_23", by_name["composite_of_variable_length_parts_1_23"], 11))
    expected.append(("composite_of_variable_length_parts_12_3", by_name["composite_of_variable_length_parts_12_3"], 14))
    for name, want, index in expected:
        if index < len(ok_folds):
            checked += 1
            if ok_folds[index] != want:
                problems.append(f"{name}: {ok_folds[index]} != {want}")
    return problems, checked


# ------------------------------------------------------------------ restore leg

# §7 header offsets, plus the first entry and the last byte. Flipping each is
# the "well-formed forgery fails as surely as a malformed one" claim, checked
# on both sides at once: what must agree is accept/reject, not which check
# caught it (§11 says an implementation need not agree on that).
TAMPER_OFFSETS = [0, 4, 5, 20, 21, 22, 30, 31, 46, 47, 54, 55, 70]


def _flip_byte(data, offset):
    if offset >= len(data):
        return None
    out = bytearray(data)
    out[offset] ^= 0x01
    return bytes(out)


def restore_leg(binary, history, py_records, rs_records, rng, tamper_per_history, fault=None):
    """Feed each side's wire bytes, and tampered variants, to the other.

    Returns (differences, cases compared).
    """
    py_projs = [r for r in py_records if r["op"] == "project"]
    rs_projs = [r for r in rs_records if r["op"] == "project"]
    if not py_projs:
        return [], 0

    # Which bytes go to whom: Python's own bytes to Rust, Rust's own bytes to
    # Python. Identical by construction when the two agree, which is the
    # point — a restore that accepts its own bytes and refuses the other's is
    # the divergence this leg exists to find.
    cases = []  # (signal, hex, label)
    for i, (p, r) in enumerate(zip(py_projs, rs_projs)):
        cases.append((p["signal"], p["bytes"], f"python#{i}"))
        cases.append((r["signal"], r["bytes"], f"rust#{i}"))

    # The random sample alone under-covers the boundary field: flipping the
    # tag on an `h0` frame yields `saturated, payload 0`, which is canonical,
    # and flipping the payload on an `h<n>` frame yields another legal
    # `h<n±1>`. The two shapes that actually probe §7's "0 when saturated"
    # are a non-zero hop count (tag flip) and a saturated frame (payload
    # flip), so one of each is always included.
    chosen = set(rng.sample(range(len(py_projs)), min(tamper_per_history, len(py_projs))))
    for pred in (
        lambda p: p["boundary"] not in ("h0", "sat"),
        lambda p: p["boundary"] == "sat",
    ):
        pool = [i for i, p in enumerate(py_projs) if pred(p)]
        if pool and not set(pool) & chosen:
            chosen.add(rng.choice(pool))
    chosen = sorted(chosen)
    for i in chosen:
        p = py_projs[i]
        raw = bytes.fromhex(p["bytes"])
        for offset in TAMPER_OFFSETS:
            flipped = _flip_byte(raw, offset)
            if flipped is not None:
                cases.append((p["signal"], flipped.hex(), f"tamper#{i}@{offset}"))
        # §11: a truncation is the one refusal whose *offset* is not fixed,
        # because the parse shifts. Both implementations must refuse it; they
        # need not agree on which check caught it, and the harness only
        # compares accept/reject, so the clause is testable.
        for keep in (0, 1, 54, len(raw) - 1, len(raw) // 2):
            if 0 <= keep < len(raw):
                cases.append((p["signal"], raw[:keep].hex() or ".", f"truncate#{i}@{keep}"))
        # Trailing bytes past the last entry. The frame carries no total
        # length, so a decoder that stops at `entry_count` accepts these; a
        # byte-for-byte comparison does not.
        cases.append((p["signal"], (raw + b"\x00").hex(), f"trailing#{i}+1"))
        cases.append((p["signal"], (raw + raw[:8]).hex(), f"trailing#{i}+8"))

    plan = list(history) + [f"restore {sig} {h}" for sig, h, _ in cases]
    rs = run_rust(binary, plan)
    py = PythonSide().run(plan)

    diffs, compared = [], 0
    rs_restores = [r for r in rs if r["op"] == "restore"]
    py_restores = [r for r in py if r["op"] == "restore"]
    if fault == "restore" and py_restores:
        py_restores[0]["accepted"] = not py_restores[0]["accepted"]
    for (sig, _h, label), pr, rr in zip(cases, py_restores, rs_restores):
        compared += 1
        if pr["accepted"] != rr["accepted"]:
            diffs.append(
                {
                    "where": f"restore {label} (signal {sig})",
                    "command": "restore",
                    "python": pr["accepted"],
                    "rust": rr["accepted"],
                }
            )
    return diffs, compared


# ------------------------------------------------------------------ invariants


def local_invariants(records):
    """Properties each side must satisfy on its own, not just jointly.

    Two implementations agreeing on a wrong answer is the failure mode a
    differential harness cannot see, so the structural laws are checked
    directly against the record stream as well.
    """
    problems = []
    counts = next((r for r in records if r["op"] == "counts"), None)
    dump = next((r for r in records if r["op"] == "dump"), None)
    degrees = [r for r in records if r["op"] == "degree"]
    if counts:
        d, r = counts["distinctions"], counts["relationships"]
        if r != 2 * d - 3:
            problems.append(f"r = 2d - 3 broken: d={d}, r={r}")
        if dump is not None and dump["len"] != d - 2:
            problems.append(f"binary parentage: {dump['len']} entries for {d} distinctions")
        # Average degree -> 4.0: the edge set has 2(d-2)+1 members and every
        # edge has two endpoints, so the degrees sum to 2r = 4d - 6.
        if degrees:
            by_id = {x["id"]: x["value"] for x in degrees}
            if len(by_id) == d and sum(by_id.values()) != 4 * d - 6:
                problems.append(
                    f"degree sum: {sum(by_id.values())} != 2r = {4 * d - 6} over all {d} nodes"
                )
    primordials = {proto.D0.hex(), proto.D1.hex()}
    for rec in records:
        # d0 and d1 exist by definition in every engine (§1), so a fresh
        # engine witnessing them is the rule, not a leak. Any *synthesized*
        # identifier must be foreign to an engine that never produced it.
        if rec["op"] == "cross" and rec["id"] not in primordials:
            if rec["has"] or rec["degree"] != 0 or rec["parents_some"]:
                problems.append(f"cross-engine read leaked: {rec}")
        if rec["op"] == "audit" and not rec["ok"]:
            problems.append(f"auditor rejected the engine it built: {rec}")
    return problems


# ------------------------------------------------------------------ campaign


def find_binary(explicit):
    if explicit:
        return pathlib.Path(explicit)
    for profile in ("release", "debug"):
        candidate = _ROOT / "target" / profile / "diff_runner"
        if candidate.exists():
            return candidate
    raise SystemExit(
        "diff_runner not built. Run:\n"
        "  cargo build --release -p koru-experiments-runner --bin diff_runner"
    )


def one_history(binary, seed, steps, tamper_per_history, fault=None, shape="random"):
    """Build, run and compare one history. Returns a result dict."""
    rng = random.Random(seed)
    history = gen_shape(shape, rng, steps)

    # The plan is built with the Python side as an oracle for *plan
    # construction only* — it needs concrete identifiers for `verify_raw` and
    # a dump length for `replay`. Nothing computed here is compared; both
    # sides then execute the finished plan from scratch.
    scout = PythonSide()
    scout.run(history)
    pool_ids = [d.hex() for d in scout.pool]
    dump_len = len(_sorted_dump(scout.engine))

    plan = (
        history
        + gen_queries(rng, pool_ids)
        + gen_projections(rng, pool_ids)
        + gen_replays(rng, dump_len)
        + gen_lift(rng)
    )

    py = PythonSide().run(plan)
    rs = run_rust(binary, plan)
    if fault and fault != "restore":
        if not inject(fault, py):
            raise RuntimeError(f"fault {fault!r} found nothing to perturb")

    diffs, compared, per_op = compare(plan, py, rs)
    fixture_problems, fixture_checked = check_against_fixture(rs)
    invariant_problems = local_invariants(rs) + local_invariants(py)

    rdiffs, rcompared = restore_leg(binary, history, py, rs, rng, tamper_per_history, fault=fault)

    return {
        "seed": seed,
        "shape": shape,
        "commands": len(plan),
        "fields_compared": compared + rcompared + fixture_checked,
        "restore_cases": rcompared,
        "per_op": per_op,
        "diffs": diffs + rdiffs,
        "fixture": fixture_problems,
        "invariants": invariant_problems,
    }


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--histories", type=int, default=20)
    ap.add_argument("--seed", type=int, default=1)
    ap.add_argument("--steps", type=int, default=24, help="synthesis steps per history")
    ap.add_argument("--tampers", type=int, default=2, help="projections tampered per history")
    ap.add_argument("--runner", default=os.environ.get("KORU_DIFF_RUNNER"))
    ap.add_argument("--inject", choices=sorted(FAULTS), help="seed one divergence")
    ap.add_argument("--self-test", action="store_true", help="drive every fault, expect each caught")
    ap.add_argument("--quiet", action="store_true")
    ap.add_argument(
        "--shape",
        default="random",
        choices=SHAPES + ("all",),
        help="history shape; `all` cycles through every shape",
    )
    args = ap.parse_args(argv)

    binary = find_binary(args.runner)

    if args.self_test:
        print(f"harness self-test — runner: {binary}")
        baseline = one_history(binary, args.seed, args.steps, args.tampers)
        base_keys = {d["where"] for d in baseline["diffs"]}
        print(
            f"  baseline (no fault): {len(baseline['diffs'])} difference(s) over "
            f"{baseline['fields_compared']} compared fields"
        )
        for key, n in group(baseline["diffs"]):
            print(f"      {n:5d}  {key}")
        # The baseline is reported rather than asserted empty: a real
        # disagreement is a finding, and a self-test that demanded a clean
        # baseline would have to be edited away the moment one was found —
        # which is how a test stops being able to fail.
        ok = True
        for fault in sorted(FAULTS):
            got = one_history(binary, args.seed, args.steps, args.tampers, fault=fault)
            fresh = [d for d in got["diffs"] if d["where"] not in base_keys]
            caught = bool(fresh)
            ok &= caught
            print(
                f"  {'PASS' if caught else 'FAIL'}  fault {fault!r:12} "
                f"({FAULTS[fault]}): {len(fresh)} new difference(s)"
                + ("" if caught else "  <-- NOT CAUGHT")
            )
            if caught and not args.quiet:
                first = fresh[0]
                print(f"            first: {first['where']} "
                      f"python={str(first['python'])[:40]!r} rust={str(first['rust'])[:40]!r}")
        print("self-test:", "PASS — every seeded divergence was reported" if ok else "FAIL")
        return 0 if ok else 1

    total_fields = 0
    total_cmds = 0
    total_restores = 0
    all_diffs = []
    all_fixture = []
    all_invariants = []
    shapes_run = {}
    op_counts = {}
    for k in range(args.histories):
        seed = args.seed * 1_000_003 + k
        shape = SHAPES[k % len(SHAPES)] if args.shape == "all" else args.shape
        res = one_history(binary, seed, args.steps, args.tampers, fault=args.inject, shape=shape)
        total_fields += res["fields_compared"]
        total_cmds += res["commands"]
        total_restores += res["restore_cases"]
        shapes_run[res["shape"]] = shapes_run.get(res["shape"], 0) + 1
        for op, n in res["per_op"].items():
            op_counts[op] = op_counts.get(op, 0) + n
        for d in res["diffs"]:
            all_diffs.append({**d, "seed": seed, "shape": res["shape"]})
        for p in res["fixture"]:
            all_fixture.append({"seed": seed, "problem": p})
        for p in res["invariants"]:
            all_invariants.append({"seed": seed, "problem": p})
        if not args.quiet:
            mark = "." if not (res["diffs"] or res["fixture"] or res["invariants"]) else "X"
            sys.stdout.write(mark)
            sys.stdout.flush()
    if not args.quiet:
        sys.stdout.write("\n")

    print(f"runner:           {binary}")
    print(f"histories:        {args.histories} (seed base {args.seed}, {args.steps} synth steps each)")
    print(f"shapes:           {', '.join(f'{k}x{v}' for k, v in sorted(shapes_run.items()))}")
    print(f"plan commands:    {total_cmds}")
    print(f"restore cases:    {total_restores}")
    print(f"fields compared:  {total_fields}")
    print("records compared by command:")
    for op, n in sorted(op_counts.items(), key=lambda kv: -kv[1]):
        print(f"  {n:8d}  {op}")
    print(f"  {total_restores:8d}  restore (second leg: own bytes, peer bytes, tampered, truncated)")
    print(f"differences:      {len(all_diffs)}")
    print(f"fixture problems: {len(all_fixture)}")
    print(f"invariant breaks: {len(all_invariants)}")
    # Grouped first, then a capped sample. Printing only the first N
    # differences is how a campaign reports "all of them look like this" when
    # it has not actually looked: the grouping covers every difference found.
    if all_diffs:
        print("differences by class (every difference, grouped):")
        for key, n in group(all_diffs):
            print(f"  {n:6d}  {key}")
    for d in all_diffs[:40]:
        print(f"  DIFF seed={d['seed']} shape={d['shape']} {d['where']}")
        print(f"       command: {d['command']}")
        print(f"       python:  {str(d['python'])[:160]}")
        print(f"       rust:    {str(d['rust'])[:160]}")
    for p in all_fixture[:20]:
        print(f"  FIXTURE seed={p['seed']} {p['problem']}")
    for p in all_invariants[:20]:
        print(f"  INVARIANT seed={p['seed']} {p['problem']}")
    return 1 if (all_diffs or all_fixture or all_invariants) else 0


if __name__ == "__main__":
    sys.exit(main())
