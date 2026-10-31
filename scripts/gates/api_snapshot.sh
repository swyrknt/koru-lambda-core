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

# `cargo public-api` builds rustdoc JSON, which requires a nightly toolchain,
# and with no toolchain named it reaches for one called literally `nightly`.
# That is the bug that kept the `api-surface` job red from the day the branch
# was first gated: CI installs the pinned `nightly-2026-06-27`, whose rustup
# name is *dated*, so no toolchain called `nightly` exists on the runner and
# all three crates failed with
#
#   error: toolchain 'nightly-x86_64-unknown-linux-gnu' is not installed
#   Error: Failed to build rustdoc JSON (see stderr)
#
# before rustdoc ran at all. It reproduced only in a container, because every
# development machine here happens to have a plain `nightly` installed, and
# that was the one dimension four earlier reproduction attempts never varied.
#
# So the toolchain is resolved here rather than left to the tool. Preferring
# the *active default* is what keeps the pin in one place: CI sets the default
# to the dated nightly via `dtolnay/rust-toolchain`, so this uses that pin
# without repeating the date, and `scripts/check-api.sh` still overrides it
# explicitly for a host whose default is stable.
if [ -z "${RUSTUP_TOOLCHAIN:-}" ]; then
    if ! command -v rustup >/dev/null 2>&1; then
        echo "api_snapshot: rustup is not available, so no nightly can be selected" >&2
        exit 127
    fi
    active=$(rustup show active-toolchain 2>/dev/null | head -1 | cut -d' ' -f1 || true)
    if [ "${active#nightly}" != "$active" ]; then
        RUSTUP_TOOLCHAIN="$active"
    else
        RUSTUP_TOOLCHAIN=$(rustup toolchain list 2>/dev/null \
            | cut -d' ' -f1 | grep '^nightly' | head -1 || true)
    fi
    if [ -z "$RUSTUP_TOOLCHAIN" ]; then
        echo "api_snapshot: no nightly toolchain is installed, and rustdoc JSON needs one" >&2
        echo "  rustup toolchain install nightly-2026-06-27" >&2
        echo "  (the date that generated the committed api/*.txt — see scripts/check-api.sh)" >&2
        exit 127
    fi
    export RUSTUP_TOOLCHAIN
fi
echo "api_snapshot: rustdoc JSON via ${RUSTUP_TOOLCHAIN}" >&2

mkdir -p api
fail=0
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

for crate in "${CRATES[@]}"; do
    committed="api/${crate}.txt"
    current="$tmp/${crate}.txt"
    # --all-features, because without it the snapshot is not the surface.
    # `Cargo.toml` sets `docs.rs all-features = true`, so the published
    # surface includes the serde adapters; the gate was computing the
    # default-feature surface instead and missing two public modules and
    # three public functions. A semver break in the serde surface would not
    # have tripped it, and `crates/koru-lambda-agents` depends on that path,
    # so it is live surface rather than dormant.
    # stderr is captured, not discarded. It used to go to /dev/null, which
    # meant a tool failure produced an empty snapshot file and `set -e` killed
    # the script with no message at all — the `api-surface` job failed for days
    # saying nothing. The tool's own diagnostics are the only explanation
    # available when rustdoc JSON cannot be built, so they are printed on
    # failure and stay quiet on success.
    if ! cargo public-api --simplified --all-features -p "$crate" \
            >"$current" 2>"$tmp/${crate}.err"; then
        echo "api_snapshot: cargo public-api failed for $crate" >&2
        sed 's/^/    /' "$tmp/${crate}.err" >&2
        fail=1
        continue
    fi

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
