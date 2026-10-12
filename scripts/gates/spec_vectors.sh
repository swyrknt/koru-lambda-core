#!/usr/bin/env bash
# Gate: `docs/SPEC.md` §10 is what the fixture says it is.
#
# CONF-6 and CONF-9. §4.3 told the reader that §10 "include[s] byte tips and
# a sequence fold"; §10 contained neither, only the three history values and
# one projection's wire bytes. So an implementer following §10's own
# contract — "reproduce every value below from this document alone" — could
# not check the lift, which is the substrate's one free parameter and the
# thing most worth checking. The promise had also spread: §4.4, written
# later, repeated it and added a composite.
#
# Publishing those values by hand would have made §10 a third source of
# truth next to `conformance/vectors.json` and `src/vectors.rs`. That is the
# drift this project has already paid for twice — vectors living inside the
# crate they pin, and a specification promising vectors it did not publish —
# so §10 is generated instead, and this gate is the tie.
#
# When a vector legitimately changes: edit `conformance/vectors.json`, run
# `scripts/gen_spec_vectors.py --write`, re-pin `conformance/VECTORS.sha256`,
# and commit all three. Three coordinated edits is the point; identity
# moving should read as a decision, not as noise.
set -euo pipefail
cd "$(dirname "$0")/../.."

SPEC="docs/SPEC.md"
GEN="scripts/gen_spec_vectors.py"

for f in "$SPEC" "$GEN" conformance/vectors.json; do
    [ -f "$f" ] || { echo "spec_vectors: $f is missing" >&2; exit 1; }
done

if ! command -v python3 >/dev/null 2>&1; then
    echo "spec_vectors: python3 not available" >&2
    exit 127
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

cp "$SPEC" "$tmp/spec.before"
python3 "$GEN" --write >/dev/null

if ! diff -q "$tmp/spec.before" "$SPEC" >/dev/null; then
    diff -u "$tmp/spec.before" "$SPEC" >"$tmp/drift.diff" || true
    # Put the committed version back: a gate does not edit the tree.
    cp "$tmp/spec.before" "$SPEC"
    echo "spec_vectors: $SPEC §10 does not match conformance/vectors.json" >&2
    sed -n '1,40p' "$tmp/drift.diff" | sed 's/^/    /' >&2
    echo "    (run scripts/gen_spec_vectors.py --write once the change is intended)" >&2
    exit 1
fi

lines=$(sed -n '/BEGIN GENERATED: scripts\/gen_spec_vectors.py/,/END GENERATED: scripts\/gen_spec_vectors.py/p' "$SPEC" | wc -l | tr -d ' ')
echo "spec_vectors: clean — §10's $lines generated lines match conformance/vectors.json"
