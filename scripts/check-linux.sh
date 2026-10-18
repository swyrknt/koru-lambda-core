#!/usr/bin/env bash
# Authoritative local gate. Mirrors the command set in
# .github/workflows/ci.yml. If these diverge, fix the divergence — do not
# let either one lie.
#
# Why this exists: `scripts/check.sh` runs on the developer's machine, and
# this repository spent a week with every CI run red while check.sh passed.
# The causes were all environment, not code — a stable toolchain four weeks
# behind the runner's, a nightly four months behind, and a shallow clone the
# gates could not see past. A local gate that cannot reproduce the runner is
# not a gate; it is a second opinion that agrees with itself.
#
# What this covers and what it cannot. Running `check.sh` inside a plain
# `rust:` image reaches the repository gates, format, native clippy, tests in
# both profiles, the feature matrix, rustdoc and loom. It cannot reach:
#
#   * `api snapshot`  — check.sh skips it unless `cargo-public-api` is on
#                       PATH, and that tool needs a nightly toolchain for
#                       rustdoc JSON. See scripts/check-api.sh.
#   * `wasm bindings` — needs `wasm-pack` and the wasm32 target.
#   * `js demos`      — needs node.
#   * `clippy (wasm32)` — needs the wasm32 target, so check.sh fails there.
#
# Those are reported as skipped or failed by check.sh itself rather than
# hidden here, because suppressing them would make this script lie in the
# same way the macOS run did.
#
# Usage:
#   scripts/check-linux.sh
#   CHECK_LINUX_IMAGE=rust:1.100 scripts/check-linux.sh
#   PROPTEST_CASES=256 scripts/check-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."

IMAGE="${CHECK_LINUX_IMAGE:-rust:1.99.1}"
CASES="${PROPTEST_CASES:-16}"

if ! command -v docker >/dev/null 2>&1; then
    echo "check-linux: docker is not installed." >&2
    echo "  Install Docker Desktop, or: brew install colima && colima start" >&2
    exit 127
fi

if ! docker info >/dev/null 2>&1; then
    echo "check-linux: the docker daemon is not running." >&2
    echo "  Start it with:  open -a Docker" >&2
    echo "  or, without Docker Desktop:  brew install colima && colima start" >&2
    exit 127
fi

echo "check-linux: image=$IMAGE cpus=2 PROPTEST_CASES=$CASES"
echo "check-linux: running scripts/check.sh in the container"
echo

# --cpus=2 matches the GitHub runner shape. This is load-bearing rather than
# cosmetic: `replay_is_linear_in_the_number_of_entries` asserts a ratio
# between two timings, and a 12-core host can hide a regression that a
# 2-core runner exposes.
#
# CARGO_TARGET_DIR is redirected because $PWD is mounted read-write and the
# host's `target/` holds macOS artifacts. Sharing one directory between a
# darwin host and a linux container makes both rebuild constantly and can
# leave either with a half-written cache.
docker run --rm \
    --cpus=2 \
    -e PROPTEST_CASES="$CASES" \
    -e CARGO_TARGET_DIR=/work/target-linux \
    -e CARGO_TERM_COLOR=always \
    -v "$PWD:/work" \
    -w /work \
    "$IMAGE" \
    bash scripts/check.sh
