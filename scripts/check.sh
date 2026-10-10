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

step "repository gates"
./scripts/gates/no_suppression.sh
./scripts/gates/crate_boundary.sh
./scripts/gates/no_process_leaks.sh
./scripts/gates/checklist_evidence.sh
./scripts/gates/doc_anchors.sh
./scripts/gates/roadmap.sh
./scripts/gates/vectors_frozen.sh
./scripts/gates/fixture_coverage.sh
./scripts/gates/no_consumer_names.sh
./scripts/gates/doc_links.sh

step "api snapshot"
if command -v cargo-public-api >/dev/null 2>&1; then
    ./scripts/gates/api_snapshot.sh
else
    echo "skipped: cargo-public-api not installed"
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
cargo clippy -p koru-lambda-wasm --target wasm32-unknown-unknown --all-features -- -D warnings

step "loom model check"
RUSTFLAGS="--cfg loom" cargo test --test loom_kernel --release

step "wasm bindings"
(cd crates/koru-lambda-wasm && wasm-pack test --node)

step "examples"
for ex in 01_hello_synth 02_novelty_and_chain 03_projection_composition 04_wire_format_roundtrip; do
    cargo run --quiet --example "$ex" >/dev/null
done

step "js demos"
(cd examples/wasm-demos && npm run typecheck && npm run demo:all >/dev/null)

printf '\nall checks passed\n'
