#!/usr/bin/env bash
# The API snapshot gate, on the toolchain that can actually run it.
#
# `scripts/gates/api_snapshot.sh` needs `cargo-public-api`, which reads
# rustdoc's JSON output and therefore needs a nightly toolchain. That is why
# `check.sh` could only run it conditionally, and why the conditional used to
# print "skipped:" and fall through — reporting "all checks passed" for a
# week while the `api-surface` job was red.
#
# The nightly is PINNED, and to the same date as `.github/workflows/ci.yml`.
# rustdoc's JSON format drifts between nightlies, so an unpinned toolchain
# regenerates the surface in a different shape and the gate fails against a
# committed artifact that never changed. If the pin here and the pin in
# ci.yml ever differ, fix the divergence — do not let either one lie.
#
# Usage:
#   scripts/check-api.sh                 # host toolchain, pinned nightly
#   CHECK_API_DOCKER=1 scripts/check-api.sh   # in a container
set -euo pipefail
cd "$(dirname "$0")/.."

# Must match `api-surface`'s `toolchain:` input in .github/workflows/ci.yml.
PINNED_NIGHTLY="nightly-2026-06-27"

if [ "${CHECK_API_DOCKER:-0}" = "1" ]; then
    IMAGE="${CHECK_API_IMAGE:-rustlang/rust:nightly}"
    if ! docker info >/dev/null 2>&1; then
        echo "check-api: the docker daemon is not running." >&2
        echo "  Start it with:  open -a Docker" >&2
        exit 127
    fi
    echo "check-api: image=$IMAGE (installs the pinned nightly and cargo-public-api; slow first run)"
    # The image's own nightly is whatever date it was built from, and rustdoc
    # JSON drifts between nightlies — running the gate against it reports a
    # surface change that is really a format change (`-> SynthesisOutcome`
    # became `-> Self` between 2026-06-27 and 2026-10-20). So the container
    # installs the same pin CI uses and makes it the default, which is what
    # `api_snapshot.sh` then resolves. Without this the Docker check cannot
    # mirror the job it exists to reproduce.
    exec docker run --rm \
        -e CARGO_TARGET_DIR=/work/target-linux-api \
        -v "$PWD:/work" -w /work "$IMAGE" \
        bash -c "rustup toolchain install ${PINNED_NIGHTLY} \
            && rustup default ${PINNED_NIGHTLY} \
            && cargo install cargo-public-api --locked \
            && ./scripts/gates/api_snapshot.sh"
fi

if ! command -v rustup >/dev/null 2>&1; then
    echo "check-api: rustup is not available, so the pinned nightly cannot be selected." >&2
    exit 127
fi

# Accept either the dated toolchain by name, or a plain `nightly` that
# happens to BE that date. `rustup toolchain list` names the latter
# `nightly-<host>`, so matching only the dated form reports a false negative
# on a machine whose `nightly` is already correct.
PIN_DATE="${PINNED_NIGHTLY#nightly-}"
TOOLCHAIN=""
if rustup toolchain list | grep -q "^${PINNED_NIGHTLY}"; then
    TOOLCHAIN="$PINNED_NIGHTLY"
elif rustc +nightly --version 2>/dev/null | grep -q "$PIN_DATE"; then
    TOOLCHAIN="nightly"
    echo "check-api: using 'nightly', which is ${PIN_DATE} on this machine"
else
    echo "check-api: no toolchain matching ${PINNED_NIGHTLY} is installed." >&2
    echo "  rustup toolchain install ${PINNED_NIGHTLY}" >&2
    echo "  (this is the toolchain that generated the committed api/*.txt;" >&2
    echo "   rustc +nightly reports: $(rustc +nightly --version 2>/dev/null || echo 'no nightly'))" >&2
    exit 127
fi

if ! command -v cargo-public-api >/dev/null 2>&1; then
    echo "check-api: cargo-public-api is not installed." >&2
    echo "  cargo install cargo-public-api --locked" >&2
    exit 127
fi

echo "check-api: toolchain=${TOOLCHAIN}"
RUSTUP_TOOLCHAIN="$TOOLCHAIN" ./scripts/gates/api_snapshot.sh
