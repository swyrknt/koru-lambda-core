#!/usr/bin/env bash
# Gate: the substrate does not know about its consumers.
#
# `core` must never name a type from the consensus, wasm or ffi crates, and
# must never depend on them. The dependency arrow points one way: consumers
# depend on the substrate. This is the mechanical form of the boundary the
# architecture describes; without it, "these are not the substrate" is a
# sentence in a document rather than a property of the build.
set -euo pipefail
cd "$(dirname "$0")/../.."

fail=0

offenders=$(git ls-files 'src/*.rs' | xargs grep -nE 'koru_lambda_(consensus|wasm|ffi)|ConsensusValidator|BatchCommitment|CommitmentAgent|NetworkAgent|StructuralCompactor|BatchSynthesizer|WasmEngine' 2>/dev/null || true)
if [ -n "$offenders" ]; then
    echo "core names a consumer-layer type:"
    printf '%s\n' "$offenders" | sed 's/^/  /'
    fail=1
fi

dep_offenders=$(sed -n '/^\[dependencies\]/,/^\[/p' Cargo.toml | grep -E '^koru-lambda-(consensus|wasm|ffi)' || true)
if [ -n "$dep_offenders" ]; then
    echo "core depends on a consumer crate:"
    printf '%s\n' "$dep_offenders" | sed 's/^/  /'
    fail=1
fi

if [ "$fail" -eq 0 ]; then
    echo "crate_boundary: clean"
fi
exit "$fail"
