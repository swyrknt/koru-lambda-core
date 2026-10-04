#!/usr/bin/env bash
# Gate: the published vectors have not moved within this major version.
#
# Before this gate, the published vectors lived inside the crate they pin. That
# meant they certified internal consistency and nothing else: one commit could
# change `synth_bytes` and regenerate the constants together, and every check
# in the repository stayed green. The API snapshot could not help — it records
# `pub const C_HEX: &str`, the type, not the value. The release promised
# "identity never moves for the life of the major version" and nothing
# whatsoever enforced it.
#
# So the vectors moved out of tree to conformance/vectors.json, and this gate
# pins their digest from a separate file. Four things now have to change
# together for a published identifier to move: the engine, the mirror in
# src/vectors.rs, the fixture, and the pin here. Each is visible in review.
#
# Honest about the limit: a contributor who *intends* to change identity can
# still edit all four in one commit, and no in-repo mechanism can stop that.
# What is closed is the silent case — identity moving as a side effect, which
# is the one that actually happened to the lift between 1.x and 2.0.0.
set -euo pipefail
cd "$(dirname "$0")/../.."

FIXTURE="conformance/vectors.json"
PIN="conformance/VECTORS.sha256"

for f in "$FIXTURE" "$PIN"; do
    [ -f "$f" ] || { echo "vectors_frozen: $f is missing"; exit 1; }
done

pinned_sha=$(sed -nE 's/^sha256[[:space:]]*=[[:space:]]*([0-9a-f]{64})$/\1/p' "$PIN")
pinned_major=$(sed -nE 's/^major[[:space:]]*=[[:space:]]*([0-9]+)$/\1/p' "$PIN")
if [ -z "$pinned_sha" ] || [ -z "$pinned_major" ]; then
    echo "vectors_frozen: $PIN must carry 'major = N' and 'sha256 = <64 hex>'"
    exit 1
fi

if command -v shasum >/dev/null 2>&1; then
    actual_sha=$(shasum -a 256 "$FIXTURE" | awk '{print $1}')
elif command -v sha256sum >/dev/null 2>&1; then
    actual_sha=$(sha256sum "$FIXTURE" | awk '{print $1}')
else
    echo "vectors_frozen: no sha256 tool available" >&2
    exit 127
fi

fail=0

if [ "$actual_sha" != "$pinned_sha" ]; then
    echo "vectors_frozen: $FIXTURE has changed within major $pinned_major"
    echo "  pinned: $pinned_sha"
    echo "  actual: $actual_sha"
    echo
    echo "  Every value in that file is a published identifier. If this change is"
    echo "  intended, it is a BREAKING change: update the pin in $PIN, say why in"
    echo "  CHANGELOG.md, and expect every downstream identifier to move. If it is"
    echo "  not intended, something altered the operator or the lift."
    fail=1
fi

# The fixture must declare the major it belongs to, and agree with the crate.
declared=$(sed -nE 's/^[[:space:]]*"frozen_for_major"[[:space:]]*:[[:space:]]*([0-9]+).*/\1/p' "$FIXTURE")
crate_major=$(sed -nE 's/^version[[:space:]]*=[[:space:]]*"([0-9]+)\..*/\1/p' Cargo.toml | head -1)
if [ "$declared" != "$crate_major" ]; then
    echo "vectors_frozen: $FIXTURE declares frozen_for_major = $declared, crate is $crate_major"
    fail=1
fi
if [ "$pinned_major" != "$crate_major" ]; then
    echo "vectors_frozen: $PIN pins major $pinned_major, crate is $crate_major"
    fail=1
fi

if [ "$fail" -eq 0 ]; then
    echo "vectors_frozen: clean — published identity unchanged within major $crate_major"
fi
exit "$fail"
