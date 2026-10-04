#!/usr/bin/env bash
# Gate: the committed public API snapshots match the current surface.
#
# The snapshots under api/ are the record of what each crate exposes. A change
# to a public signature has to change a committed file, so it appears in the
# diff a reviewer reads instead of arriving silently. That record is also what
# the changelog and the migration guides get derived from at release time —
# the alternative is reconstructing weeks of API churn from memory.
#
# When a change is intentional: run with --update, read the diff, commit it.
set -euo pipefail
cd "$(dirname "$0")/../.."

CRATES=(koru-lambda-core koru-lambda-agents koru-lambda-wasm)
update=0
[ "${1:-}" = "--update" ] && update=1

if ! command -v cargo-public-api >/dev/null 2>&1; then
    echo "api_snapshot: cargo-public-api not installed (cargo install cargo-public-api --locked)" >&2
    exit 127
fi

mkdir -p api
fail=0
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for crate in "${CRATES[@]}"; do
    committed="api/${crate}.txt"
    current="$tmp/${crate}.txt"
    cargo public-api --simplified -p "$crate" >"$current" 2>/dev/null

    if [ "$update" -eq 1 ]; then
        cp "$current" "$committed"
        echo "api_snapshot: updated $committed"
        continue
    fi

    if [ ! -f "$committed" ]; then
        echo "api_snapshot: no committed snapshot for $crate (run with --update)"
        fail=1
        continue
    fi

    if ! diff -q "$committed" "$current" >/dev/null; then
        echo "api_snapshot: $crate surface differs from $committed"
        diff -u "$committed" "$current" | sed -n '1,40p' | sed 's/^/    /'
        echo "    (run scripts/gates/api_snapshot.sh --update once the change is intended)"
        fail=1
    fi
done

if [ "$fail" -eq 0 ] && [ "$update" -eq 0 ]; then
    echo "api_snapshot: all ${#CRATES[@]} crate surfaces match their committed snapshots"
fi
exit "$fail"
