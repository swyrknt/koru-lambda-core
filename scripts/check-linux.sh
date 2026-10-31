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
# What this covers. All fourteen of `check.sh`'s steps, because the image is
# built from `scripts/check-linux.Dockerfile` rather than taken from
# `rust:` as-is.
#
# It used to cover eleven. A plain `rust:` image has no `wasm-pack`, no
# `wasm32` target, no node and no `cargo-public-api`, so the run died at
# `wasm bindings` with `wasm-pack: command not found` — exit 127, with
# `examples` and `js demos` never reached and `api snapshot` already skipped.
# The first line of this file claimed to mirror ci.yml while three steps could
# not run, so the fix was to provide the tools rather than to narrow the
# claim: narrowing makes the docstring true and the gate weaker, and this gate
# is what caught three CI failures that `check.sh` could not see.
#
# The image is built once and cached by Docker's layer cache. The first build
# installs a nightly toolchain, wasm-pack and cargo-public-api, so expect
# several minutes; after that it is a no-op.
#
# Usage:
#   scripts/check-linux.sh
#   CHECK_LINUX_IMAGE=rust:1.99.1 scripts/check-linux.sh   # skip the build,
#                                        # accepting that 3 steps cannot run
#   PROPTEST_CASES=256 scripts/check-linux.sh
set -euo pipefail
cd "$(dirname "$0")/.."

CASES="${PROPTEST_CASES:-16}"
# Tagged by the Dockerfile's own content, so editing the Dockerfile produces a
# new tag and the stale image is never silently reused.
DOCKERFILE="scripts/check-linux.Dockerfile"
IMAGE_TAG="koru-check-linux:$(cksum "$DOCKERFILE" | cut -d' ' -f1)"
IMAGE="${CHECK_LINUX_IMAGE:-$IMAGE_TAG}"

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

# Build unless the caller named their own image. `docker build` is a no-op
# once the layers are cached, so this costs nothing on a warm machine.
if [ "$IMAGE" = "$IMAGE_TAG" ] && ! docker image inspect "$IMAGE" >/dev/null 2>&1; then
    echo "check-linux: building $IMAGE from $DOCKERFILE (first run installs a"
    echo "  nightly toolchain, wasm-pack and cargo-public-api — several minutes)"
    docker build -f "$DOCKERFILE" -t "$IMAGE" .
    echo
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
# The anonymous volume over `node_modules` is the same problem as
# CARGO_TARGET_DIR, one ecosystem over. The host tree is bind-mounted, so the
# container saw macOS `node_modules` and `js demos` died on
#
#   You installed esbuild for another platform than the one you're currently
#   using. … the "@esbuild/darwin-arm64" package is present but this platform
#   needs the "@esbuild/linux-arm64" package instead.
#
# Shadowing the directory gives the container an empty one, which the step's
# own `npm install` then fills with Linux binaries. Without this, the local
# gate runs the demos against the host's native modules and reports a platform
# mismatch as a code failure.
docker run --rm \
    --cpus=2 \
    -e PROPTEST_CASES="$CASES" \
    -e CARGO_TARGET_DIR=/work/target-linux \
    -e CARGO_TERM_COLOR=always \
    -v "$PWD:/work" \
    -v /work/examples/wasm-demos/node_modules \
    -w /work \
    "$IMAGE" \
    bash scripts/check.sh
