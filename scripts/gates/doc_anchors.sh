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
# Published surfaces only. `api/DELTA-from-1.2.0.txt` records the items
# REMOVED since the previous release, so including it let an anchor resolve
# against the record of its own deletion — which is how `agent::LocalCausalAgent`
# kept passing after the trait left the crate.
SNAPSHOTS=()
for f in api/*.txt; do
    case "$f" in */DELTA-*) continue ;; esac
    SNAPSHOTS+=("$f")
done
if [ "${#SNAPSHOTS[@]}" -eq 0 ]; then
    echo "doc_anchors: no published surface snapshots to verify against" >&2
    exit 127
fi

while IFS= read -r ref; do
    [ -z "$ref" ] && continue
    # Last path segment is the item; check the item appears in the surface.
    # Crate-qualified anchors (e.g. "koru-lambda-agents: validator") and
    # version/branch anchors are not symbol paths; skip them.
    case "$ref" in *" "*|*/*) continue ;; esac
    # Match the fully-qualified path, which is how the snapshots spell it.
    # Exact rather than by last segment: a stale anchor whose item still
    # exists under a different module should still fail.
    # An anchor must resolve in the crate the DOCUMENT claims, not in any of
    # them. Accepting the first match anywhere is how `DESIGN.md` kept a
    # section headed "LocalCausalAgent (substrate-level trait)" passing after
    # the trait moved out of the substrate: the anchor still resolved, against
    # the agents crate. The gate was confirming that a symbol exists
    # somewhere, which is not the claim being made.
    #
    # So: a bare anchor must be in the substrate. Referencing a consumer
    # crate's item requires saying so, by writing the crate into the anchor.
    case "$ref" in
        koru_lambda_*)
            crate="${ref%%::*}"
            path="${ref#*::}"
            if ! grep -qF "${crate}::${path}" "${SNAPSHOTS[@]}"; then
                echo "anchor names an item not in ${crate}'s published surface: $ref"
                fail=1
            fi
            ;;
        *)
            if grep -qF "koru_lambda_core::${ref}" "${SNAPSHOTS[@]}"; then
                : # in the substrate, as a bare anchor claims
            elif grep -qF "::${ref}" "${SNAPSHOTS[@]}"; then
                echo "anchor '$ref' is not in the substrate, but exists in a consumer crate."
                echo "  A bare anchor claims the substrate. If the document means the consumer,"
                echo "  write the crate into the anchor (e.g. koru_lambda_agents::${ref})."
                fail=1
            else
                echo "anchor names an item not in any published surface: $ref"
                fail=1
            fi
            ;;
    esac
done < <(printf '%s\n' "$DOCS" | xargs grep -hoE '\[(SHIPPED|TARGET|DEPRECATED)[^]]*@ [a-z_]+(::[A-Za-z_]+)+\]' 2>/dev/null \
         | sed -E 's/.*@ //; s/\]$//' || true)

if [ "$fail" -eq 0 ]; then
    echo "doc_anchors: clean"
fi
exit "$fail"
