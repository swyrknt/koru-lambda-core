#!/usr/bin/env bash
# Gate: every crate in the workspace forbids `unsafe`.
#
# API-8. `#![forbid(unsafe_code)]` is the enforcement — it is a hard error,
# not a lint, and no `#[allow]` inside the crate can reopen it. But it is
# per-crate, and nothing checked that every crate had it. Three of four did;
# `experiments/runner` did not, and nobody noticed, because a missing
# attribute produces no diagnostic anywhere.
#
# The members are read from `cargo metadata` rather than listed here, so a
# new crate is covered the moment it joins the workspace instead of the
# moment somebody remembers to add it to a list.
#
# If a crate ever genuinely needs `unsafe` — the C ABI of DEC-1 is the only
# candidate — this gate fails, and that is the right moment to decide it
# rather than discovering the hole later. API-8 states the terms for that
# exception: `#![deny(unsafe_op_in_unsafe_fn)]`, a `# Safety` contract on
# every function, and no other module gaining `unsafe`. Encoding an
# exemption before anything needs one would be building the hole first.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! command -v cargo >/dev/null 2>&1; then
    echo "unsafe_forbidden: cargo not available" >&2
    exit 127
fi

if ! meta=$(cargo metadata --format-version 1 --no-deps 2>/dev/null); then
    echo "unsafe_forbidden: cargo metadata failed" >&2
    exit 1
fi

printf '%s' "$meta" | python3 -c '
import json, pathlib, re, subprocess, sys

m = json.load(sys.stdin)
root = pathlib.Path(m["workspace_root"])

# 1. Every LIBRARY crate root forbids it. Libraries are what ship; a test or
#    bench is not in a consumer binary, and requiring the attribute on each
#    of thirty-three of them would be noise rather than enforcement.
LIB = {"lib", "rlib", "cdylib", "staticlib", "proc-macro"}
bad, libs = [], []
for pkg in sorted(m["packages"], key=lambda p: p["name"]):
    for t in pkg["targets"]:
        if not (set(t["kind"]) & LIB):
            continue
        src = pathlib.Path(t["src_path"])
        if not src.is_file():
            continue
        rel = src.relative_to(root)
        libs.append(str(rel))
        if not re.search(r"^#!\[forbid\(unsafe_code\)\]", src.read_text(), re.M):
            bad.append(str(rel) + " (" + pkg["name"] + ")")

# 2. And no tracked Rust file contains unsafe at all, which covers the tests,
#    benches and examples the attribute is not required on. `unsafe_code`,
#    `UnsafeCell` and `UnsafeUnpin` are names, not uses.
tracked = subprocess.run(["git", "ls-files", "*.rs"], capture_output=True, text=True,
                         cwd=root).stdout.split()
USE = re.compile(r"\bunsafe\s*(\{|fn\b|impl\b|trait\b|extern\b)")
uses = []
for f in tracked:
    path = root / f
    if not path.is_file():
        continue
    for n, line in enumerate(path.read_text().splitlines(), 1):
        if USE.search(line):
            uses.append(f + ":" + str(n) + ": " + line.strip()[:90])

if bad or uses:
    print("unsafe_forbidden: unsafe is not confined", file=sys.stderr)
    for b in bad:
        print("    missing #![forbid(unsafe_code)]: " + b, file=sys.stderr)
    for u in uses:
        print("    unsafe used: " + u, file=sys.stderr)
    print("    (API-8: the C ABI is the only candidate for an exception, and it", file=sys.stderr)
    print("     would need deny(unsafe_op_in_unsafe_fn) and a # Safety contract)", file=sys.stderr)
    sys.exit(1)

print("unsafe_forbidden: clean - " + str(len(libs)) + " library roots forbid unsafe, and "
      + str(len(tracked)) + " tracked Rust files contain none")
'
