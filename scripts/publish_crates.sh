#!/bin/bash
set -e

# The checklist is the release gate. Without this call the gate is a
# document that describes itself as enforcing, which is worse than no gate:
# it invites trust it has not earned.
"$(dirname "$0")/gates/release_ready.sh" || {
    echo "refusing to publish: the release checklist has open items" >&2
    exit 1
}

echo "Publishing koru-lambda-core to crates.io..."

# Navigate to project root (parent of scripts/)
cd "$(dirname "$0")/.."

# Verify we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Cargo.toml not found."
    exit 1
fi

# Run tests first (release mode for performance tests)
echo "Running tests..."
cargo test --release

# Dry run to catch issues
echo "Dry run..."
cargo publish --dry-run

# Publish
echo "Publishing..."
cargo publish

echo "Published successfully!"
