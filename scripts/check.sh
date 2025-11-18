#!/bin/bash
# Koru Lambda Core - Status Check
# Run locally or in CI - same checks everywhere

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Koru Lambda Core - Status Check                            ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Format check
echo "Checking formatting..."
cargo fmt -- --check
echo "✓ Formatting OK"
echo ""

# Clippy
echo "Running clippy..."
cargo clippy --all-targets -- -D warnings
echo "✓ Clippy OK"
echo ""

# Tests
echo "Running tests..."
cargo test --release
echo "✓ Tests OK"
echo ""

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   ✅ All checks passed                                       ║"
echo "╚══════════════════════════════════════════════════════════════╝"
