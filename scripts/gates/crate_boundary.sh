#!/usr/bin/env bash
# Gate: the substrate does not know about its consumers.
#
# The dependency arrow points one way: consumers depend on the substrate, and
# the substrate depends on none of them. This is the mechanical form of the
# boundary the architecture describes; without it, "these are not the
# substrate" is a sentence in a document rather than a property of the build.
#
# This asserts over the **crate graph** rather than over grep, because the
# previous version of this gate had three defects that a graph assertion does
# not have:
#
#   1. It read one `sed` range of `[dependencies]` in one manifest, so
#      `[dev-dependencies]`, `[build-dependencies]` and
#      `[target.'cfg(..)'.dependencies]` were invisible — and this repo had
#      already been bitten exactly there, by an `experiments/runner` edge that
#      created a crate-graph cycle and broke rust-analyzer's macro resolution.
#   2. It matched a hardcoded list of crate and type names, so renaming the
#      agent crate would leave the gate passing while enforcing nothing. A
#      gate that expires on its own roadmap is worse than no gate.
#   3. It never inspected the consumer manifests, so "bindings do not depend on
#      agents" — the other half of the claim — was wholly unchecked.
#
# Nothing here is matched by name. The substrate is identified as the package
# whose manifest *is* the workspace root, and a bindings crate as any member
# producing a `cdylib`. Both survive any rename.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! command -v cargo >/dev/null 2>&1; then
    echo "crate_boundary: cargo not available" >&2
    exit 127
fi

cargo metadata --format-version 1 --no-deps 2>/dev/null | python3 -c '
import json, os, sys

meta = json.load(sys.stdin)
root = os.path.realpath(meta["workspace_root"])
packages = {p["name"]: p for p in meta["packages"]}
members = set(packages)
failures = []

def member_deps(pkg):
    """Workspace-member dependencies, every kind and every target."""
    out = set()
    for dep in pkg.get("dependencies", []):
        if dep["name"] in members:
            kind = dep.get("kind") or "normal"
            target = dep.get("target")
            out.add((dep["name"], kind, target))
    return out

# The substrate is whichever member is defined by the workspace root manifest.
substrate = None
for name, pkg in packages.items():
    if os.path.realpath(os.path.dirname(pkg["manifest_path"])) == root:
        substrate = name
        break
if substrate is None:
    print("crate_boundary: no package is defined by the workspace root manifest")
    sys.exit(1)

# 1. The substrate depends on no workspace member, in any dependency kind.
for name, kind, target in sorted(member_deps(packages[substrate])):
    where = f"{kind} dependency" + (f" for target {target}" if target else "")
    failures.append(f"substrate `{substrate}` has a {where} on member `{name}`")

# 2. No cycles among members. A cycle is what actually broke tooling here
#    before, and it is invisible to any single-manifest check.
edges = {n: {d[0] for d in member_deps(p)} for n, p in packages.items()}
state = {}
def walk(node, trail):
    if state.get(node) == "done":
        return
    if state.get(node) == "open":
        cycle = trail[trail.index(node):] + [node]
        failures.append("dependency cycle: " + " -> ".join(cycle))
        return
    state[node] = "open"
    for nxt in sorted(edges.get(node, ())):
        walk(nxt, trail + [node])
    state[node] = "done"
for node in sorted(edges):
    walk(node, [])

# 3. A bindings crate (anything producing a cdylib) depends on the substrate
#    and on no other member. Identified by crate-type, not by name, so the
#    check survives the rename that would have silently emptied the old gate.
for name, pkg in sorted(packages.items()):
    if name == substrate:
        continue
    crate_types = {t for tgt in pkg.get("targets", []) for t in tgt.get("crate_types", [])}
    if "cdylib" not in crate_types:
        continue
    stray = sorted({d[0] for d in member_deps(pkg)} - {substrate})
    for other in stray:
        failures.append(
            f"bindings crate `{name}` depends on member `{other}`; bindings may depend "
            f"only on the substrate `{substrate}`"
        )

if failures:
    print("crate_boundary: dependency direction violated")
    for f in failures:
        print("  " + f)
    sys.exit(1)

consumers = sorted(m for m in members if m != substrate)
print(f"crate_boundary: clean — `{substrate}` depends on no member; "
      f"{len(consumers)} consumer(s) depend on it")
'
