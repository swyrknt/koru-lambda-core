#!/usr/bin/env python3
"""Phase 2: run rerun_impl.py against conformance/vectors.json.

No tuning. Failures are the result.
"""

import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

from substrate import (  # noqa: E402
    D0, D1, Engine, Projection, Refused, NotWitnessed,
    UPSTREAM, DOWNSTREAM, UNDIRECTED,
    tip_of_byte, fold_bytes, domain_anchor, fold_composite, restore,
    build_byte_alphabet,
)

VEC = os.path.join(os.path.dirname(os.path.abspath(__file__)), os.pardir, "vectors.json")
V = json.load(open(VEC))

results = {}       # section -> [total, passed, [failure strings]]


def sect(name):
    results.setdefault(name, [0, 0, []])
    return results[name]


def check(section, label, got, exp):
    r = sect(section)
    r[0] += 1
    if got == exp:
        r[1] += 1
    else:
        r[2].append("%s: expected %s, got %s" % (label, exp, got))


# --------------------------------------------------- lift.engine_state (§4.3)
# The one divergence no projection case reaches: whether constructing the
# alphabet witnesses it. This implementation originally computed tips
# privately and reached 41 distinctions where Rust reached 513, while
# reproducing every other value here.
_es = V.get("lift", {}).get("engine_state", {}).get("after_byte_alphabet")
if _es:
    _e = Engine()
    check("lift.engine_state", "fresh engine", len(_e.parents), 2)
    build_byte_alphabet(_e)
    check("lift.engine_state", "distinction_count", len(_e.parents), _es["distinction_count"])
    check("lift.engine_state", "degree(d0)", _e.degree(D0), _es["degree_d0"])
    check("lift.engine_state", "degree(d1)", _e.degree(D1), _es["degree_d1"])

# ------------------------------------------------------------- primordials
check("primordials", "d0", D0.hex(), V["primordials"]["d0"])
check("primordials", "d1", D1.hex(), V["primordials"]["d1"])

# ------------------------------------------------------------- operator
NAMED = {"d0": D0, "d1": D1}
E_OP = Engine()
for c in V["operator"]["cases"]:
    a, b = (NAMED[p] for p in c["parents"])
    got = E_OP.synthesize(a, b)
    NAMED[c["name"]] = got
    check("operator.cases", c["name"], got.hex(), c["id"])

# ------------------------------------------------------------- lift.tips
for c in V["lift"]["tips"]:
    e = Engine()
    check("lift.tips", "byte 0x%02x" % c["byte"],
          tip_of_byte(e, c["byte"]).hex(), c["id"])

# ------------------------------------------------------------- lift.folds
for c in V["lift"]["folds"]:
    e = Engine()
    cc = e.synthesize(D0, D1)
    dd = e.synthesize(cc, D0)
    e.synthesize(dd, D1)
    anchor = bytes.fromhex(c["anchor"])
    try:
        got = fold_bytes(e, anchor, c["bytes"]).hex()
    except (Refused, NotWitnessed) as exc:
        got = "REFUSED(%s)" % exc
    check("lift.folds", c["name"], got, c["id"])

# ------------------------------------------------------------- domain anchors
for c in V["lift"]["domain_anchors"]["cases"]:
    e = Engine()
    check("lift.domain_anchors.cases", "label 0x%02x" % c["label"],
          domain_anchor(e, c["label"]).hex(), c["id"])

# ------------------------------------------------------------- canonical_bytes
for i, c in enumerate(V["lift"]["canonical_bytes"]["cases"]):
    t, val = c["type"], c["value"]
    if t == "bool":
        got = (b"\x01" if val else b"\x00").hex()
    elif t == "usize":
        got = val.to_bytes(8, "little").hex()
    elif t == "Option<ParentPair>":
        if val is None:
            got = "00"
        else:
            p = [bytes.fromhex(x) for x in val]
            lo, hi = (p[0], p[1]) if p[0] <= p[1] else (p[1], p[0])
            got = (b"\x01" + lo + hi).hex()
    else:
        got = "UNSPECIFIED-TYPE"
    check("lift.canonical_bytes.cases", "%s/%s" % (t, val), got, c["bytes"])

# ------------------------------------------------------------- composites
E_C = Engine()
hc = E_C.synthesize(D0, D1)
hd = E_C.synthesize(hc, D0)
he = E_C.synthesize(hd, D1)
HIST = {"c": hc, "d": hd, "e": he, "d0": D0, "d1": D1}
by_case = {}
for c in V["lift"]["composites"]["cases"]:
    a = c["anchor"]
    anchor = HIST[a["case_history"]] if "case_history" in a \
        else domain_anchor(E_C, a["domain"])
    parts = []
    for el in c["elements"]:
        if "tip_of_byte" in el:
            parts.append(tip_of_byte(E_C, el["tip_of_byte"]))
        elif "fold_of_bytes" in el:
            # A part that is itself a variable-length sequence, folded from
            # this case's own anchor. The shape §4.4 exists for.
            parts.append(fold_bytes(E_C, anchor, bytes(el["fold_of_bytes"])))
        elif "case" in el:
            parts.append(by_case[el["case"]])
        else:
            parts.append(None)
    try:
        got = fold_composite(E_C, anchor, parts)
        by_case[c["name"]] = got
        got = got.hex()
    except (Refused, NotWitnessed) as exc:
        got = "REFUSED(%s)" % exc
    check("lift.composites.cases", c["name"], got, c["id"])

# ------------------------------------------------------------- projections
E_P = Engine()
PN = {"d0": D0, "d1": D1}
for pair in V["projections"]["history"]["pairs"]:
    a, b = (PN[x] for x in pair)
    r = E_P.synthesize(a, b)
    # name it per history.names order of appearance
    for nm, expr in V["projections"]["history"]["names"].items():
        if nm not in PN and expr == "synthesize(%s, %s)" % tuple(pair):
            PN[nm] = r

DIR = {"Upstream": UPSTREAM, "Downstream": DOWNSTREAM, "Undirected": UNDIRECTED}
SIG = {"Adjacency": "adjacency", "Degree": "degree", "HopDistance": "hop_distance"}

for c in V["projections"]["cases"]:
    root = bytes.fromhex(c["root"])
    bnd = c["boundary"]
    btag, bpay = (0x00, bnd["value"]) if bnd["kind"] == "Hops" else (0x01, 0)
    p = Projection(root, DIR[c["direction"]], btag, bpay, SIG[c["signal"]])
    try:
        cb = p.canonical_bytes(E_P).hex()
    except Exception as exc:
        cb = "ERROR(%s)" % exc
    check("projections.cases[canonical_bytes]", c["name"], cb, c["canonical_bytes"])
    check("projections.cases[projection_id]", c["name"],
          p.projection_id().hex(), c["projection_id"])
    # restore-as-verification round trip on the genuine bytes
    try:
        restore(E_P, bytes.fromhex(c["canonical_bytes"]), SIG[c["signal"]])
        rt = "accepted"
    except Exception as exc:
        rt = "REFUSED(%s)" % exc
    check("projections.cases[restore]", c["name"], rt, "accepted")

# ------------------------------------------------------------- reject
# Expectation mapping. My impl raises Refused with a message naming the
# offset where §11 fixes one; the spec names no error-class taxonomy, so
# "Malformed" vs "ForeignEngine" is mapped here, in the checker.
OFFSET_HINT = {0: ("bad magic", "truncated header"), 4: ("bad version",),
               21: ("bad boundary tag",), 30: ("bad direction tag",)}

for c in V["reject"]["cases"]:
    data = bytes.fromhex(c["bytes"])
    exp = c["expect"]
    try:
        restore(E_P, data, "adjacency")
        got = "ACCEPTED"
    except (Refused, NotWitnessed) as exc:
        got = str(exc)
    r = sect("reject.cases")
    r[0] += 1
    ok = got != "ACCEPTED"
    detail = ""
    if ok and exp["kind"] == "Malformed" and "offset" in exp:
        hints = OFFSET_HINT.get(exp["offset"], ())
        if not any(h in got for h in hints):
            ok = False
            detail = " (refused, but not at the offset §11 fixes)"
    if ok and exp["kind"] == "ForeignEngine":
        if "witnessed" not in got and "match" not in got:
            ok = False
            detail = " (refused for the wrong reason)"
    if ok:
        r[1] += 1
    else:
        r[2].append("%s: expected %s, got %r%s" % (c["name"], exp, got, detail))

# additional forgery probe: valid bytes, wrong signal argument
r = sect("reject.extra")
r[0] += 1
try:
    restore(E_P, bytes.fromhex(V["projections"]["cases"][0]["canonical_bytes"]), "degree")
    r[2].append("signal_mismatch: ACCEPTED (should refuse)")
except (Refused, NotWitnessed):
    r[1] += 1

# ------------------------------------------------------------- report
print("=" * 70)
tt = tp = 0
for name, (total, passed, fails) in results.items():
    tt += total
    tp += passed
    print("%-42s %2d/%2d%s" % (name, passed, total, "" if passed == total else "   <-- FAIL"))
    for f in fails:
        print("      ! " + f)
print("=" * 70)
print("TOTAL %d/%d" % (tp, tt))
