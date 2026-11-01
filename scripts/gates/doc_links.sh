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
# It also checks the one link class PROC-8 named that nothing covered: a bare
# backticked document path inside a Rust comment, like THEORY.md or
# docs/SPEC.md. Markdown links are checked above, shipped Markdown links by
# package_links.sh, and rustdoc *intra-doc* links by the Rustdoc job's
# RUSTDOCFLAGS=-D warnings — but a backticked filename is none of those. It
# is not a link rustdoc resolves, and it is not in a Markdown file, so the
# scan above never sees it. That is how koru-lambda-agents/src/lib.rs cited a
# THEORY.md section that does not exist for thirteen days after the same
# claim was removed from its sibling.
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

# ---- Rust comments naming a document -------------------------------------
# tracked already holds every tracked path, so the Rust files come from there
# rather than from a third stdin section.
#
# Every comment line is scanned, not only /// and //!. A plain // comment
# citing a document that moved is wrong in the same way and costs nothing
# extra to check.
DOCREF = re.compile(r"`([A-Za-z0-9_][A-Za-z0-9_./-]*[.]md)`")
for rs in sorted(f for f in tracked if f.endswith(".rs")):
    try:
        rtext = pathlib.Path(rs).read_text(errors="replace")
    except OSError:
        continue
    rbase = pathlib.Path(rs).parent
    for n, line in enumerate(rtext.splitlines(), 1):
        if not line.lstrip().startswith("//"):
            continue
        for ref in DOCREF.findall(line):
            # Either spelling a reader would try: relative to the repository
            # root, or relative to the file naming it.
            from_root = os.path.normpath(ref)
            from_here = os.path.normpath(os.path.join(str(rbase), ref))
            if from_root in tracked or from_here in tracked:
                continue
            if pathlib.Path(from_root).exists() or pathlib.Path(from_here).exists():
                failures.append(
                    f"{rs}:{n}: comment names a document that exists locally but is not tracked: {ref}"
                )
            else:
                failures.append(f"{rs}:{n}: comment names a document that does not exist: {ref}")

if failures:
    print("broken document links:")
    for f in failures:
        print("  " + f)
    print()
    print("  A stale fragment is invisible in rendering — the link works and")
    print("  lands at the top of the page. That is why it needs a gate.")
    sys.exit(1)
rust_count = sum(1 for f in tracked if f.endswith(".rs"))
print(f"doc_links: clean — {len(docs)} documents, {rust_count} Rust files scanned for document references")
'
