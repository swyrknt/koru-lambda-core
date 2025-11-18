#!/bin/bash
set -e

echo "Publishing koru-lambda-core to crates.io..."

# Verify we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "Error: Cargo.toml not found. Run from project root."
    exit 1
fi

# Run tests first
echo "Running tests..."
cargo test

# Dry run to catch issues
echo "Dry run..."
cargo publish --dry-run

# Publish
echo "Publishing..."
cargo publish

echo "Published successfully!"
