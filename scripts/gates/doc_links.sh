#!/usr/bin/env bash
# Gate: every relative link between tracked documents resolves — path and
# fragment both.
#
# This is the one class of breakage that got through everything else. The
# THEORY rewrite moved and removed sections, three links in DESIGN.md went
# dead, and nothing noticed; they were found by accident while correcting a
# different item. `doc_anchors.sh` checks `[SHIPPED @ symbol]` code anchors
# against the API snapshots, which is a different thing entirely.
#
# Fragments are the half that matters, because a path typo is obvious on the
# first read and a stale fragment is invisible: the link still works, it just
# lands at the top of the page. The slug rule below is GitHub's — lowercase,
# drop anything that is not alphanumeric, space, hyphen or underscore, then
# spaces to hyphens. Note what that does to an em-dash heading: "Law 5 —
# Binary parentage" becomes "law-5--binary-parentage", with two hyphens,
# because the spaces either side of the dash both survive.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Two lists on stdin, separated by a blank line: the markdown documents whose
# links are checked, then every tracked path a link may legitimately point at.
# The second list used to be absent — `tracked` was built from the markdown
# files alone and the existence check used `Path.exists()`, i.e. "is on this
# disk" rather than "is in this repository". The header says tracked, and a
# link to an untracked local file would have passed while being broken for
# everyone who clones.
{ git ls-files '*.md'; echo; git ls-files; } | python3 -c '
import os, re, sys, pathlib

raw = sys.stdin.read().split("\n\n", 1)
docs = [l.strip() for l in raw[0].splitlines() if l.strip()]
tracked = {l.strip() for l in raw[1].splitlines() if l.strip()} if len(raw) > 1 else set(docs)
doc_set = set(docs)

def slug(heading):
    s = heading.strip().lower()
    s = re.sub(r"[^a-z0-9 _-]", "", s)
    return s.replace(" ", "-")

# Headings per document, including duplicates — GitHub suffixes repeats, and
# a link to a duplicated heading is ambiguous rather than broken, so this
# only needs the set.
slugs = {}
for d in docs:
    text = pathlib.Path(d).read_text(errors="replace")
    found = set()
    fenced = False
    for line in text.splitlines():
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if fenced:
            continue
        m = re.match(r"^(#{1,6})\s+(.*)$", line)
        if m:
            found.add(slug(m.group(2)))
    slugs[d] = found

LINK = re.compile(r"\[(?:[^\]]*)\]\(([^)\s]+)\)")
failures = []

for d in docs:
    base = pathlib.Path(d).parent
    text = pathlib.Path(d).read_text(errors="replace")
    fenced = False
    for n, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            fenced = not fenced
            continue
        if fenced:
            continue
        for target in LINK.findall(line):
            if target.startswith(("http://", "https://", "mailto:")):
                continue
            path, _, frag = target.partition("#")
            if not path:
                # Same-document fragment.
                if frag and frag not in slugs[d]:
                    failures.append(f"{d}:{n}: fragment #{frag} is not a heading in this document")
                continue
            # normpath, not a string replace: stripping "./" out of
            # "docs/../CHANGELOG.md" turns it into "docs/.CHANGELOG.md",
            # which is how a first draft of this gate reported eleven
            # non-existent targets that all existed.
            resolved = os.path.normpath(os.path.join(str(base), path))
            real = resolved
            if resolved not in tracked:
                if pathlib.Path(resolved).exists():
                    failures.append(
                        f"{d}:{n}: link target exists locally but is not tracked: {path}"
                    )
                else:
                    failures.append(f"{d}:{n}: link target does not exist: {path}")
                continue
            if frag:
                if real in slugs:
                    if frag not in slugs[real]:
                        failures.append(f"{d}:{n}: #{frag} is not a heading in {real}")
                elif resolved.endswith(".md") and resolved not in doc_set:
                    failures.append(f"{d}:{n}: {path} is not a tracked document, cannot check #{frag}")

if failures:
    print("broken document links:")
    for f in failures:
        print("  " + f)
    print()
    print("  A stale fragment is invisible in rendering — the link works and")
    print("  lands at the top of the page. That is why it needs a gate.")
    sys.exit(1)
print(f"doc_links: clean — {len(docs)} documents")
'
