#!/usr/bin/env bash
# Local mirror of CI. One definition of "green" — if this passes and CI
# fails, that divergence is itself a bug worth fixing.
#
# Usage: ./scripts/check.sh [--fast]
#   --fast skips the slow jobs (loom, miri, wasm, demos) for an inner loop.
set -euo pipefail
cd "$(dirname "$0")/.."

fast=0
[ "${1:-}" = "--fast" ] && fast=1

step() { printf '\n=== %s\n' "$1"; }

# A skip is not a pass. `api snapshot` used to print "skipped:" and fall
# through, so this script reported "all checks passed" for a week while the
# API snapshot job was red in CI — the same defect class as the loom
# sentinels that never ran and the entry-length field nothing probed.
#
# Skips are now recorded and the run exits non-zero at the end. Failing at
# the end rather than at the step is deliberate: it keeps every later step
# running, which is what let `scripts/check-linux.sh` localise the `Tests`
# failure, while still making the exit code tell the truth.
SKIPPED=()
skip() {
    SKIPPED+=("$1")
    printf 'SKIPPED: %s\n' "$1"
}

step "repository gates"
./scripts/gates/no_suppression.sh
./scripts/gates/crate_boundary.sh
./scripts/gates/dep_pins.sh
./scripts/gates/no_process_leaks.sh
./scripts/gates/checklist_evidence.sh
./scripts/gates/doc_anchors.sh
./scripts/gates/roadmap.sh
./scripts/gates/vectors_frozen.sh
./scripts/gates/fixture_coverage.sh
./scripts/gates/no_drop_impls.sh
./scripts/gates/unsafe_forbidden.sh
./scripts/gates/spec_vectors.sh
./scripts/gates/reference_impl.sh
./scripts/gates/no_consumer_names.sh
./scripts/gates/doc_links.sh
./scripts/gates/package_links.sh

step "api snapshot"
if command -v cargo-public-api >/dev/null 2>&1; then
    ./scripts/gates/api_snapshot.sh
else
    skip "api snapshot: cargo-public-api is not installed. Install it with \`cargo install cargo-public-api --locked\` (it needs a nightly toolchain for rustdoc JSON), or run scripts/check-api.sh."
fi

step "format"
cargo fmt --all --check

step "clippy (native)"
cargo clippy --workspace --all-targets --all-features -- -D warnings

step "tests (debug)"
cargo test --workspace --all-features

step "tests (release)"
cargo test --workspace --all-features --release

step "feature matrix"
cargo check --workspace --all-targets --no-default-features
cargo check --workspace --all-targets
cargo check -p koru-lambda-core --all-targets --features serde
cargo check --workspace --all-targets --all-features

step "rustdoc"
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps --all-features

if [ "$fast" -eq 1 ]; then
    printf '\nfast mode: skipped clippy/wasm32, loom, miri, wasm tests, demos\n'
    exit 0
fi

step "clippy (wasm32)"
# The core, not the bindings. The 32-bit codegen coverage is a property of
# the target; routing it through the bindings crate only made it look as
# though the bindings were what provided it.
cargo clippy -p koru-lambda-core --target wasm32-unknown-unknown --all-features -- -D warnings
cargo clippy -p koru-lambda-wasm --target wasm32-unknown-unknown --all-features -- -D warnings

step "loom model check"
RUSTFLAGS="--cfg loom" cargo test --test loom_kernel --release

step "loom regression sentinels"
# The mutant kernels are `#[should_panic]`: each breaks one property of the
# production kernel and must make loom find a violating interleaving. They
# are the proof that the kernels above test something, and until 2026-10-13
# nothing ran them — neither this script nor CI passed `--cfg loom_mutant`,
# so a sentinel that had stopped firing would have looked exactly like one
# that still did.
RUSTFLAGS="--cfg loom --cfg loom_mutant" cargo test --test loom_kernel --release

step "wasm bindings"
# Both commands, because `ci.yml`'s `wasm` job runs both: the `--node` test
# suite and a release build for the bundler target. Running only the test
# meant a build-only failure — a `cdylib` that compiles under test but not
# under `--release --target bundler` — could not be seen locally.
(cd crates/koru-lambda-wasm \
    && wasm-pack test --node \
    && wasm-pack build --release --target bundler)

step "examples"
for ex in 01_hello_synth 02_novelty_and_chain 03_projection_composition 04_wire_format_roundtrip; do
    cargo run --quiet --example "$ex" >/dev/null
done

step "js demos"
# All four commands `ci.yml`'s `examples` job runs, in its order. This step
# used to be `typecheck && demo:all` only, which assumed `pkg-node/` and
# `node_modules/` were already present and correct — so on a clean checkout it
# failed for the wrong reason, and in a Linux container it ran the demos
# against whatever the host had built. `build:pkg` regenerates the wasm the
# demos import; `npm install` resolves the platform's own binaries.
(cd examples/wasm-demos \
    && npm run build:pkg \
    && npm install \
    && npm run typecheck \
    && npm run demo:all >/dev/null)

if [ ${#SKIPPED[@]} -gt 0 ]; then
    printf '\n%d step(s) SKIPPED — that is a failure, not a pass:\n' "${#SKIPPED[@]}"
    for s in "${SKIPPED[@]}"; do printf '  - %s\n' "$s"; done
    exit 1
fi

printf '\nall checks passed\n'
