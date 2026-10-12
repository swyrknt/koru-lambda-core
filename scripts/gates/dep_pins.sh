#!/usr/bin/env bash
# Gate: every dependency between workspace members declares a version.
#
# PROC-19. A member depending on another by `path` alone resolves correctly
# inside this workspace and nowhere else: `cargo publish` refuses it, and a
# crate built from a tarball or a git dependency gets whatever the registry
# hands it. `koru-experiments-runner` carried exactly that — `koru-lambda-core
# = { path = "../.." }`, which `cargo metadata` reports as `req = "*"` — while
# the two published members pinned `=2.0.0`. Nothing noticed, because
# `crate_boundary.sh` asserts the *direction* of the dependency graph and
# never looks at the requirement.
#
# The shape asked for is both: `path` so local development uses the working
# tree, and `version` so the published artifact names what it needs. This
# gate checks the version half; `crate_boundary.sh` still owns direction.
#
# It deliberately does NOT rule on `=` against `^`. Which to use is a policy
# question with a real trade-off — an exact pin makes a bindings crate and
# its substrate move together, a caret lets a consumer absorb patches and be
# the thing that notices when one breaks — and PKG-11 records a decision that
# the manifests currently contradict. A gate that picked a side would be
# enforcing an unmade decision. It prints the requirement instead, so the
# shape is visible in CI output rather than only in a manifest nobody opens.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! command -v cargo >/dev/null 2>&1; then
    echo "dep_pins: cargo not available" >&2
    exit 127
fi

if ! meta=$(cargo metadata --format-version 1 --no-deps 2>/dev/null); then
    echo "dep_pins: cargo metadata failed" >&2
    exit 1
fi

printf '%s' "$meta" | python3 -c '
import json, sys

m = json.load(sys.stdin)
members = {p["name"] for p in m["packages"]}
rows, bad = [], []

for p in sorted(m["packages"], key=lambda q: q["name"]):
    for d in p["dependencies"]:
        if d["name"] not in members:
            continue
        req = d["req"]
        has_path = bool(d.get("path"))
        who, dep = p["name"], d["name"]
        rows.append((who, dep, req, has_path))
        if req == "*":
            bad.append(
                who + " depends on " + dep + " by path with no version "
                "requirement. cargo publish refuses this, and outside this "
                "workspace it resolves to whatever the registry offers."
            )
        if not has_path:
            bad.append(
                who + " depends on " + dep + " by version alone, with no "
                "path. Local development would build against the registry "
                "instead of the working tree."
            )

if bad:
    print("dep_pins: intra-workspace dependencies with the wrong shape", file=sys.stderr)
    for b in bad:
        print("    " + b, file=sys.stderr)
    print("    (want both: path = \"...\", version = \"...\")", file=sys.stderr)
    sys.exit(1)

print("dep_pins: clean - " + str(len(rows)) + " intra-workspace dependencies, each with a path and a version")
for a, b, req, _ in rows:
    print("    " + a + " -> " + b + "  " + req)
'
