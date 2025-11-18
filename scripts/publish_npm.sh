#!/bin/bash
# Koru Lambda Core - NPM Publish Script
# Builds WASM and publishes to npm

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Koru Lambda Core - NPM Publish                             ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Pre-flight checks
echo "Running pre-flight checks..."
echo ""

# Check if we're on a clean branch
if [ -n "$(git status --porcelain)" ]; then
    echo -e "${YELLOW}⚠️  Warning: You have uncommitted changes${NC}"
    echo "Continue anyway? (y/n)"
    read -r response
    if [[ ! "$response" =~ ^[Yy]$ ]]; then
        echo "Aborting."
        exit 1
    fi
fi

# Check npm authentication
if ! npm whoami &>/dev/null; then
    echo -e "${RED}❌ Not logged in to npm${NC}"
    echo "Run: npm login"
    exit 1
fi

echo -e "${GREEN}✓ npm authentication verified${NC}"
echo ""

# Run code hygiene checks (format, clippy, tests)
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Running Code Hygiene Checks                                ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

./scripts/check.sh

if [ $? -ne 0 ]; then
    echo -e "${RED}❌ Code hygiene checks failed${NC}"
    exit 1
fi

echo -e "${GREEN}✓ All checks passed${NC}"
echo ""

# Build WASM
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Building WASM Artifact                                     ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

./scripts/build_universal.sh

if [ $? -ne 0 ]; then
    echo -e "${RED}❌ WASM build failed${NC}"
    exit 1
fi

echo -e "${GREEN}✓ WASM build successful${NC}"
echo ""

# Verify package.json
echo "Verifying package.json..."
PACKAGE_VERSION=$(node -p "require('./pkg/package.json').version")
echo "Package version: ${PACKAGE_VERSION}"
echo ""

# Publish to npm
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Publishing to npm                                          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

echo "Publishing koru-lambda-core@${PACKAGE_VERSION} to npm..."
echo "Continue? (y/n)"
read -r response

if [[ ! "$response" =~ ^[Yy]$ ]]; then
    echo "Publish cancelled."
    exit 0
fi

cd pkg
npm publish --access public

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}╔══════════════════════════════════════════════════════════════╗${NC}"
    echo -e "${GREEN}║   ✅ Successfully published to npm!                          ║${NC}"
    echo -e "${GREEN}╚══════════════════════════════════════════════════════════════╝${NC}"
    echo ""
    echo "Package: koru-lambda-core@${PACKAGE_VERSION}"
    echo "View at: https://www.npmjs.com/package/koru-lambda-core"
    echo ""
    echo "Install with:"
    echo "  npm install koru-lambda-core"
    echo ""
else
    echo -e "${RED}❌ npm publish failed${NC}"
    exit 1
fi
