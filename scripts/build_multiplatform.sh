#!/bin/bash
# Koru Lambda Core - Multi-Platform Build Script
# Builds the Rust library for all major platforms

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Koru Lambda Core - Multi-Platform Build                    ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Platforms to build
PLATFORMS=(
    "x86_64-unknown-linux-gnu"      # Linux x64
    "aarch64-unknown-linux-gnu"     # Linux ARM64
    "x86_64-apple-darwin"           # macOS Intel
    "aarch64-apple-darwin"          # macOS ARM (M1/M2)
    # "x86_64-pc-windows-gnu"       # Windows (optional)
)

# Check if cross is installed (for Linux targets on macOS)
if ! command -v cross &> /dev/null; then
    echo -e "${YELLOW}⚠️  'cross' not found. Installing for cross-compilation...${NC}"
    cargo install cross
fi

echo "Building for ${#PLATFORMS[@]} platforms..."
echo ""

# Build for each platform
for target in "${PLATFORMS[@]}"; do
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
    echo -e "${GREEN}Building for: $target${NC}"
    echo -e "${GREEN}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"

    # Add target if not already added
    rustup target add "$target" 2>/dev/null || true

    # Use cross for Linux targets on macOS, regular cargo for native targets
    if [[ "$target" == *"linux"* ]] && [[ "$(uname)" == "Darwin" ]]; then
        cross build --release --target "$target"
    else
        cargo build --release --target "$target"
    fi

    if [ $? -eq 0 ]; then
        echo -e "${GREEN}✓ Build successful${NC}"
    else
        echo -e "${RED}✗ Build failed${NC}"
        exit 1
    fi
    echo ""
done

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Packaging Libraries for Distribution                       ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Create dist directory structure
mkdir -p dist/{include,lib/{linux-amd64,linux-arm64,darwin-amd64,darwin-arm64}}

# Copy header (platform-independent)
cp target/koru.h dist/include/
echo "✓ Copied koru.h"

# Copy libraries to platform-specific directories
for target in "${PLATFORMS[@]}"; do
    case "$target" in
        "x86_64-unknown-linux-gnu")
            if [ -f "target/$target/release/libdistinction_engine.a" ]; then
                cp "target/$target/release/libdistinction_engine.a" dist/lib/linux-amd64/
                cp "target/$target/release/libdistinction_engine.so" dist/lib/linux-amd64/ 2>/dev/null || true
                echo "✓ Copied linux-amd64 libraries"
            fi
            ;;
        "aarch64-unknown-linux-gnu")
            if [ -f "target/$target/release/libdistinction_engine.a" ]; then
                cp "target/$target/release/libdistinction_engine.a" dist/lib/linux-arm64/
                cp "target/$target/release/libdistinction_engine.so" dist/lib/linux-arm64/ 2>/dev/null || true
                echo "✓ Copied linux-arm64 libraries"
            fi
            ;;
        "x86_64-apple-darwin")
            if [ -f "target/$target/release/libdistinction_engine.a" ]; then
                cp "target/$target/release/libdistinction_engine.a" dist/lib/darwin-amd64/
                cp "target/$target/release/libdistinction_engine.dylib" dist/lib/darwin-amd64/ 2>/dev/null || true
                echo "✓ Copied darwin-amd64 libraries"
            fi
            ;;
        "aarch64-apple-darwin")
            if [ -f "target/$target/release/libdistinction_engine.a" ]; then
                cp "target/$target/release/libdistinction_engine.a" dist/lib/darwin-arm64/
                cp "target/$target/release/libdistinction_engine.dylib" dist/lib/darwin-arm64/ 2>/dev/null || true
                echo "✓ Copied darwin-arm64 libraries"
            fi
            ;;
    esac
done

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Creating Release Archives                                  ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Create release archives
VERSION="0.1.0"
mkdir -p releases

for platform_dir in dist/lib/*; do
    platform=$(basename "$platform_dir")
    archive_name="koru-core-v${VERSION}-${platform}.tar.gz"

    tar -czf "releases/$archive_name" \
        -C dist \
        include/koru.h \
        lib/$platform/

    # Generate checksum
    shasum -a 256 "releases/$archive_name" > "releases/$archive_name.sha256"

    echo "✓ Created releases/$archive_name"
done

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   ✅ Multi-Platform Build Complete!                          ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "Distribution structure:"
echo "  dist/"
echo "  ├── include/koru.h"
echo "  └── lib/"
echo "      ├── linux-amd64/"
echo "      ├── linux-arm64/"
echo "      ├── darwin-amd64/"
echo "      └── darwin-arm64/"
echo ""
echo "Release archives created in releases/:"
ls -lh releases/*.tar.gz
echo ""
