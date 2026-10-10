#!/usr/bin/env bash
# Gate: every public function that produces a published value is named in the
# conformance manifest, either as covered by a vector or as exempt with a
# reason.
#
# FRZ-8. The fixture pins the substrate's published identities, and a test
# asserts those against a live engine. Both of those are about *today's*
# surface. Nothing stopped a function added in 2.1 from producing an
# identifier, a projection id or wire bytes that no vector ever checked —
# coverage would still read as complete, because coverage counts what it
# covers and not what it misses.
#
# So the set is derived here instead of maintained. The oracle is the
# committed API snapshot (the same file `api_snapshot.sh` holds to the live
# surface), filtered to the return types that carry a published value. That
# derived set must equal `manifest.covered` + `manifest.exempt` exactly, in
# both directions: an uncovered function fails, and so does a manifest entry
# naming a function that no longer exists. The second half is the one that
# matters six months out, because a manifest that rots is worse than none.
#
# An exemption is a sentence, not a flag. "Returns its receiver" and "no wire
# form, implementations need not agree" are reasons; the gate requires one and
# a reader can disagree with it in review.
#
# This reads the *committed* snapshot, so it is only as current as that file.
# `api_snapshot.sh` is what holds the committed snapshot to the live surface;
# the two together mean the manifest covers what the crate actually publishes.
# Running this one alone proves the manifest covers whatever the snapshot says.
set -euo pipefail
cd "$(dirname "$0")/../.."

SNAPSHOT="api/koru-lambda-core.txt"
FIXTURE="conformance/vectors.json"
MANIFEST="conformance/MANIFEST.json"

for f in "$SNAPSHOT" "$FIXTURE" "$MANIFEST"; do
    [ -f "$f" ] || { echo "fixture_coverage: $f is missing" >&2; exit 1; }
done

python3 - "$SNAPSHOT" "$FIXTURE" "$MANIFEST" <<'PY'
import json, re, sys

snapshot, fixture, manifest = sys.argv[1], sys.argv[2], sys.argv[3]

# Return types that carry a value a second implementation has to reproduce:
# an identity, a projection identifier, a hex form of one, or wire bytes.
# `Projection<` covers materialize, whose whole output is pinned bytes.
CARRIES = re.compile(
    r"-> .*?("
    r"Distinction|SynthesisOutcome|ParentPair|ProjectionId|Projection<"
    r"|Vec<u8>|\[u8; 16\]|\[u8; 32\]|String"
    r")"
)
DECL = re.compile(r"^pub (?:const )?fn ")

def normalise(line):
    """Collapse a snapshot declaration to the name the manifest uses.

    Everything before the argument list, with generic parameters dropped and
    foreign path prefixes shortened. `Projection<'e, S>::canonical_bytes`
    becomes `projection::Projection::canonical_bytes`.
    """
    head = line[len("pub fn "):] if line.startswith("pub fn ") else line[len("pub const fn "):]
    depth = 0
    for i, ch in enumerate(head):
        if ch == "<":
            depth += 1
        elif ch == ">":
            depth -= 1
        elif ch == "(" and depth == 0:
            head = head[:i]
            break
    # Drop generic parameter and argument lists: `Option<ParentPair>` and
    # `Projection<'e, S>` both reduce to the bare path. One impl per shape in
    # this crate, so nothing collides.
    while "<" in head:
        head = re.sub(r"<[^<>]*>", "", head)
    head = head.replace("koru_lambda_core::", "")
    # Foreign module prefixes only: lowercase segments, so `core::option::Option`
    # loses the modules and keeps the type.
    head = re.sub(r"\b(?:core|alloc|std)::(?:[a-z_0-9]+::)*", "", head)
    return head.strip()

raw = {}
for line in open(snapshot):
    line = line.strip()
    if not DECL.match(line) or not CARRIES.search(line):
        continue
    raw.setdefault(normalise(line), line)

# Fold each re-export into its module path. A re-exported item appears in the
# snapshot twice, and the root spelling is a proper path-suffix of the module
# one — `domain_anchor` of `primitives::domain_anchor`. Suffix rather than
# last-segment, so `DistinctionEngine::d0` and `ReadOnlyEngine::d0` stay
# distinct items rather than folding into each other.
derived, ambiguous = {}, []
for name in raw:
    longer = [o for o in raw if o != name and o.endswith("::" + name)]
    if len(longer) > 1:
        ambiguous.append((name, sorted(longer)))
    elif not longer:
        derived[name] = raw[name]

fx = json.load(open(fixture))
man = json.load(open(manifest))

covered = {k: v for k, v in man.get("covered", {}).items()}
exempt = dict(man.get("exempt", {}))

both = set(covered) & set(exempt)
named = set(covered) | set(exempt)
derived_names = set(derived)

fail = []

for t, names in sorted(ambiguous):
    fail.append(
        f"two module paths share the tail `{t}` so the re-export fold cannot "
        f"pick one: {', '.join(names)} — name them apart or extend the fold"
    )

for n in sorted(both):
    fail.append(f"both covered and exempt: {n} — pick one")

# A function the substrate publishes and the manifest does not name.
for n in sorted(derived_names - named):
    fail.append(
        f"produces a published value and is in neither covered nor exempt: {n}\n"
        f"        {derived[n]}"
    )

# A manifest entry for a function that is no longer published.
for n in sorted(named - derived_names):
    where = "covered" if n in covered else "exempt"
    fail.append(f"manifest names a function no longer in the published surface ({where}): {n}")

# Every pointer in `covered` must resolve, or the coverage claim is a label.
def resolve(ptr):
    node = fx
    for seg in [s for s in ptr.split("/") if s]:
        if isinstance(node, dict):
            if seg not in node:
                return None
            node = node[seg]
        elif isinstance(node, list):
            try:
                node = node[int(seg)]
            except (ValueError, IndexError):
                return None
        else:
            return None
    return node

for n, ptrs in sorted(covered.items()):
    if not ptrs:
        fail.append(f"covered with no pointer: {n}")
    for ptr in ptrs:
        target = resolve(ptr)
        if target is None:
            fail.append(f"covered by a pointer that does not resolve: {n} -> {ptr}")
        elif isinstance(target, list) and not target:
            fail.append(f"covered by an empty case list: {n} -> {ptr}")

# An exemption has to say why.
for n, reason in sorted(exempt.items()):
    if not isinstance(reason, str) or len(reason.split()) < 4:
        fail.append(f"exempt without a reason a reader can disagree with: {n}")

if fail:
    print("fixture_coverage: FRZ-8 violations", file=sys.stderr)
    for f in fail:
        print(f"    {f}", file=sys.stderr)
    sys.exit(1)

print(
    f"fixture_coverage: clean — {len(derived_names)} value-producing public "
    f"functions, {len(covered)} covered by vectors, {len(exempt)} exempt with reasons"
)
PY
