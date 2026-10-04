#!/usr/bin/env bash
# Gate: every documented code anchor resolves, and none is a line number.
#
# The docs carried nine `[SHIPPED @ src/engine.rs:NNN]` anchors and four had
# rotted — one pointed at a closing brace, another at an unrelated doc
# comment. They broke because code moved and nothing checked. Line numbers
# cannot be maintained by hand, so they are banned outright, and the symbol
# anchors that replaced them are checked against the committed API snapshots.
set -euo pipefail
cd "$(dirname "$0")/../.."

fail=0
DOCS=$(git ls-files '*.md' | grep -v '^CHECKLIST.md$' || true)

# 1. No line-number anchors. They are unverifiable by construction.
lineno=$(printf '%s\n' "$DOCS" | xargs grep -nE '\[(SHIPPED|TARGET|DEPRECATED)[^]]*@[^]]*\.rs:[0-9]' 2>/dev/null || true)
if [ -n "$lineno" ]; then
    echo "line-number anchors found (use a symbol path instead):"
    printf '%s\n' "$lineno" | sed 's/^/  /'
    fail=1
fi

# 2. Symbol anchors must name something the crates actually export.
#    api/*.txt is the committed public surface, so it is the oracle.
if ! ls api/*.txt >/dev/null 2>&1; then
    echo "doc_anchors: no api/ snapshots to verify against" >&2
    exit 127
fi
# Grep the snapshot files directly rather than piping a variable into
# `grep -q`: the early exit closes the pipe, printf takes SIGPIPE, and
# `pipefail` then reports a successful match as a failure.
SNAPSHOTS=(api/*.txt)

while IFS= read -r ref; do
    [ -z "$ref" ] && continue
    # Last path segment is the item; check the item appears in the surface.
    # Crate-qualified anchors (e.g. "koru-lambda-agents: validator") and
    # version/branch anchors are not symbol paths; skip them.
    case "$ref" in *" "*|*/*) continue ;; esac
    # Match the fully-qualified path, which is how the snapshots spell it.
    # Exact rather than by last segment: a stale anchor whose item still
    # exists under a different module should still fail.
    found=0
    for crate in koru_lambda_core koru_lambda_agents koru_lambda_wasm; do
        if grep -qF "${crate}::${ref}" "${SNAPSHOTS[@]}"; then
            found=1
            break
        fi
    done
    if [ "$found" -eq 0 ]; then
        echo "anchor names an item not in any published surface: $ref"
        fail=1
    fi
done < <(printf '%s\n' "$DOCS" | xargs grep -hoE '\[(SHIPPED|TARGET|DEPRECATED)[^]]*@ [a-z_]+(::[A-Za-z_]+)+\]' 2>/dev/null \
         | sed -E 's/.*@ //; s/\]$//' || true)

if [ "$fail" -eq 0 ]; then
    echo "doc_anchors: clean"
fi
exit "$fail"
