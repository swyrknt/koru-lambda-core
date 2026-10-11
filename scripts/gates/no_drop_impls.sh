#!/usr/bin/env bash
# Gate: no `impl Drop` in tracked Rust.
#
# API-9. The substrate's panics are part of its contract — an unregistered
# parent panics with zero state change, and consumers are told that is
# catchable. A panic inside `Drop` is not: during unwinding it aborts the
# process, and under `panic = "abort"` there is no unwinding to begin with.
# So a `Drop` impl anywhere in this crate converts a recoverable contract
# violation into a dead process, in a consumer's binary, at a point they
# cannot wrap in `catch_unwind`.
#
# Nothing here needs one. Every type is plain data or holds a `DashMap`, an
# `Arc` or a `PhantomData`, each of which cleans itself up. The gate exists
# because that is a property worth keeping rather than rediscovering: a
# cache, a pool, a file handle or a metrics flush are all plausible future
# additions that would bring one along.
#
# This is a real prohibition rather than a pattern that needs exemptions —
# it currently matches zero sites, so a match is news.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Tracked Rust only. `target/` and anything untracked is not ours.
files=$(git ls-files '*.rs')
[ -n "$files" ] || { echo "no_drop_impls: no tracked Rust files found" >&2; exit 1; }

# `impl Drop for X`, and the generic forms `impl<T> Drop for X<T>`.
hits=$(printf '%s\n' "$files" | xargs grep -nE '^[[:space:]]*impl([[:space:]]*<[^>]*>)?[[:space:]]+Drop[[:space:]]+for[[:space:]]' || true)

if [ -n "$hits" ]; then
    echo "no_drop_impls: a Drop impl would make a panic unrecoverable for consumers (API-9)" >&2
    printf '%s\n' "$hits" | sed 's/^/    /' >&2
    echo "    A panic while dropping aborts, and under panic = \"abort\" there is no" >&2
    echo "    unwind at all, so the documented catchable-panic contract stops holding." >&2
    exit 1
fi

count=$(printf '%s\n' "$files" | wc -l | tr -d ' ')
echo "no_drop_impls: clean — no Drop impl in $count tracked Rust files"
