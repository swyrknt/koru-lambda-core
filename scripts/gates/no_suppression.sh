#!/usr/bin/env bash
# Gate: no lint suppression anywhere in tracked files.
#
# Covers attribute-level suppression (`#[allow]`, `#[expect]`), crate-level
# suppression (`#![allow]`), manifest-level relaxation (`[lints]` entries that
# allow), tool config (clippy.toml's `avoid-breaking-exported-api`, which hides
# exactly the API-shape lints that freeze at a major version), and the
# JS/TS equivalents.
#
# The rule is: fix the cause. A suppression is never the fix.
set -euo pipefail
cd "$(dirname "$0")/../.."

fail=0
report() { printf '%s\n' "$1"; fail=1; }

# Attribute form only: a comment discussing suppression is not suppression,
# so lines that begin with a comment marker are not hits.
rust_hits=$(git ls-files '*.rs' | xargs grep -nE '^[[:space:]]*#!?\[(allow|expect)\(' 2>/dev/null || true)
if [ -n "$rust_hits" ]; then
    report "Rust lint suppression found:"
    printf '%s\n' "$rust_hits" | sed 's/^/  /'
fi

# `[lints]` tables may declare expected cfgs; they may not downgrade lints.
lint_relax=$(git ls-files '*.toml' | xargs grep -nE '^\s*[A-Za-z_:-]+\s*=\s*"allow"' 2>/dev/null || true)
if [ -n "$lint_relax" ]; then
    report "Manifest lint relaxation found:"
    printf '%s\n' "$lint_relax" | sed 's/^/  /'
fi

# Clippy's default for this is true, so absence is as bad as `= true`:
# require the explicit false.
if ! grep -qE '^[[:space:]]*avoid-breaking-exported-api[[:space:]]*=[[:space:]]*false' clippy.toml 2>/dev/null; then
    report "clippy.toml must set avoid-breaking-exported-api = false; its default (true) hides API-shape lints that become permanent at release."
fi

ts_hits=$(git ls-files '*.ts' '*.js' '*.mjs' | xargs grep -nE '@ts-(ignore|expect-error)|eslint-disable' 2>/dev/null || true)
if [ -n "$ts_hits" ]; then
    report "TypeScript/ESLint suppression found:"
    printf '%s\n' "$ts_hits" | sed 's/^/  /'
fi

if [ "$fail" -eq 0 ]; then
    echo "no_suppression: clean"
fi
exit "$fail"
