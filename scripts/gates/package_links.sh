#!/usr/bin/env bash
# Gate: a document that ships links only to things that ship with it.
#
# `doc_links.sh` resolves relative links between tracked documents — it
# answers "does this work for someone who cloned the repository". It cannot
# answer "does this work for someone who ran `cargo add`", because the
# package is a subset of the repository and that gate has no idea which
# subset.
#
# The gap is not hypothetical. Excluding `DESIGN.md` (a planning document for
# this release) and `docs/MIGRATION_JS.md` (a guide to an npm package 2.0.0
# does not publish) left seven relative links to them in documents that DO
# ship — README, ARCHITECTURE, ECOSYSTEM, THEORY, CHANGELOG, CONTRIBUTING and
# docs/README. Every one resolved in the repository and dangled in the
# tarball, and `doc_links.sh` reported clean throughout.
#
# So this gate takes `cargo package --list` as its universe. A relative link
# from a shipped markdown file must point at something else in that list.
# Absolute URLs are the correct way to reference repository-only material and
# pass untouched.
#
# Zero exemptions by construction: a link either resolves inside the package
# or it does not. Widening `exclude` cannot silence it — excluding the *target*
# is what breaks it, which is the point.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! command -v cargo >/dev/null 2>&1; then
    echo "package_links: cargo not available" >&2
    exit 127
fi

if ! shipped=$(cargo package --list --allow-dirty -p koru-lambda-core 2>/dev/null); then
    echo "package_links: cargo package --list failed" >&2
    exit 1
fi

printf '%s' "$shipped" | python3 -c '
import os, pathlib, re, sys

shipped = {l.strip() for l in sys.stdin if l.strip()}
docs = sorted(p for p in shipped if p.endswith(".md"))

# [text](target) — skip absolute URLs, mailto, and pure anchors.
LINK = re.compile(r"\[[^\]]*\]\(([^)\s]+)")
failures = []
checked = 0
fence = re.compile(r"^\s*```")

for d in docs:
    inside = False
    for n, line in enumerate(pathlib.Path(d).read_text().splitlines(), 1):
        if fence.match(line):
            inside = not inside
            continue
        if inside:
            continue
        for target in LINK.findall(line):
            if re.match(r"^[a-z][a-z0-9+.-]*:", target) or target.startswith("#"):
                continue
            path = target.split("#", 1)[0]
            if not path:
                continue
            resolved = os.path.normpath(os.path.join(os.path.dirname(d), path))
            checked += 1
            if resolved not in shipped:
                reason = (
                    "is in the repository but excluded from the package"
                    if pathlib.Path(resolved).exists()
                    else "does not exist"
                )
                failures.append(
                    f"{d}:{n}: links to {target}, which {reason}. "
                    f"A reader who installed the crate cannot open it."
                )

if failures:
    print("package_links: shipped documents link outside the package", file=sys.stderr)
    for f in failures:
        print(f"    {f}", file=sys.stderr)
    print("    (use an absolute repository URL for repository-only material)", file=sys.stderr)
    sys.exit(1)

print(f"package_links: clean - {checked} relative links across {len(docs)} shipped documents all resolve inside the package")
'
