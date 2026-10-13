#!/usr/bin/env bash
# Gate: a second, independent implementation reproduces every published value.
#
# CONF-4 and CONF-5. The substrate's claim is that identity is
# engine-independent — two implementations agree without sharing code,
# because content addressing leaves no freedom. `conformance/reference/` is
# that claim executed: 334 lines of stdlib Python written from
# `docs/SPEC.md` by a reader with no access to `src/`, and
# `conformance/reference/certify.py` runs it against the fixture.
#
# An implementation nobody runs is a claim rather than a check, so it runs
# here on every push.
#
# WHAT THIS GATE PROVES. Agreement. A change to the lift, the wire format or
# a projection that is not made in both implementations fails. It also
# catches a fixture value that only a Rust engine could produce, which a
# Rust-only suite cannot.
#
# WHAT IT DOES NOT PROVE, and CONF-4 said so before it was built:
# sufficiency. Now that this file is in-repo and gated, the incentive is to
# fix it until it matches, at which point it attests that the two agree
# rather than that the document determines them. Sufficiency needs a reader
# with no access to this directory either. That experiment has run twice —
# finding three defects, then the ambiguity the first fix introduced — and
# has to be repeated whenever `docs/SPEC.md` changes materially. It is
# DEMO-5, not this.
set -euo pipefail
cd "$(dirname "$0")/../.."

if ! command -v python3 >/dev/null 2>&1; then
    echo "reference_impl: python3 not available" >&2
    exit 127
fi

out=$(cd conformance/reference && python3 certify.py 2>&1) || {
    echo "reference_impl: the independent implementation disagrees with the fixture" >&2
    printf '%s\n' "$out" | cut -c1-160 | sed 's/^/    /' | head -30 >&2
    exit 1
}

total=$(printf '%s\n' "$out" | sed -nE 's/^TOTAL ([0-9]+)\/([0-9]+)$/\1 of \2/p')
[ -n "$total" ] || { echo "reference_impl: could not read a total from certify.py" >&2; exit 1; }
case "$total" in
    *" of "*)
        got=${total%% of *}; want=${total##* of }
        if [ "$got" != "$want" ]; then
            echo "reference_impl: $total published values reproduced" >&2
            printf '%s\n' "$out" | cut -c1-160 | sed 's/^/    /' | head -30 >&2
            echo "    (run conformance/reference/certify.py for the full diff)" >&2
            exit 1
        fi
        ;;
esac
echo "reference_impl: clean — an independent Python implementation reproduces $total published values"
