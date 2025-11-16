```markdown
# Koru Lambda Core - Cross-Platform Distribution Guide

**Problem Solved:** Build once, run anywhere - truly universal Go runtime

---

## 🎯 The Challenge

**Before:** The original setup only worked on the build machine's architecture:
- Built on macOS M1? Only works on macOS ARM64
- Built on Linux x64? Only works on Linux AMD64
- Every platform needed its own build

**After:** One Go runtime works everywhere:
- ✅ Linux AMD64
- ✅ Linux ARM64
- ✅ macOS Intel
- ✅ macOS Apple Silicon
- ✅ Automatic platform detection
- ✅ Zero configuration

---

## 🚀 Quick Start - Two Commands

### Step 1: Build libraries for all platforms
```bash
./scripts/build_multiplatform.sh
```

**This creates:**
```
dist/
├── include/koru.h
└── lib/
    ├── linux-amd64/
    ├── linux-arm64/
    ├── darwin-amd64/
    └── darwin-arm64/
```

### Step 2: Create universal Go runtime
```bash
./scripts/create_go_runtime_universal.sh
```

**Done!** Your Go runtime now works on any platform.

---

## 🔧 How It Works

### 1. Multi-Platform Rust Builds

The `build_multiplatform.sh` script:

```bash
# Uses `cross` for cross-compilation
cross build --release --target x86_64-unknown-linux-gnu
cross build --release --target aarch64-unknown-linux-gnu
cargo build --release --target x86_64-apple-darwin
cargo build --release --target aarch64-apple-darwin
```

**Result:** Static libraries for all major platforms

### 2. Go Build Tags (Platform Detection)

The Go runtime uses **build tags** to automatically select the right library:

**File: `pkg/koru/ffi_linux_amd64.go`**
```go
//go:build linux && amd64

package koru

// #cgo LDFLAGS: -L${SRCDIR}/../../lib/linux-amd64 -ldistinction_engine
// #include "koru.h"
import "C"
```

**File: `pkg/koru/ffi_darwin_arm64.go`**
```go
//go:build darwin && arm64

package koru

// #cgo LDFLAGS: -L${SRCDIR}/../../lib/darwin-arm64 -ldistinction_engine
// #include "koru.h"
import "C"
```

**When you build:**
- On Linux AMD64 → Uses `ffi_linux_amd64.go`
- On macOS M1 → Uses `ffi_darwin_arm64.go`
- **Automatically!** No manual configuration needed

### 3. Universal Distribution

**Single tarball contains all platforms:**
```bash
koru-go/
└── lib/
    ├── linux-amd64/libdistinction_engine.a
    ├── linux-arm64/libdistinction_engine.a
    ├── darwin-amd64/libdistinction_engine.a
    └── darwin-arm64/libdistinction_engine.a
```

**Anyone can:**
```bash
git clone your-repo/koru-go
cd koru-go
go build ./examples/simple_validator.go
./simple_validator  # Works on their platform!
```

---

## 📦 Distribution Options

### Option 1: GitHub Release (Recommended)

```bash
# 1. Build all platforms
./scripts/build_multiplatform.sh

# 2. Create universal Go runtime
./scripts/create_go_runtime_universal.sh

# 3. Package everything
tar -czf koru-go-universal-v0.1.0.tar.gz koru-go/

# 4. Upload to GitHub Releases
gh release create v0.1.0 koru-go-universal-v0.1.0.tar.gz
```

**Users download and:**
```bash
tar -xzf koru-go-universal-v0.1.0.tar.gz
cd koru-go
go run examples/simple_validator.go  # Just works!
```

### Option 2: Platform-Specific Releases

The `build_multiplatform.sh` script creates platform-specific archives:

```
releases/
├── koru-core-v0.1.0-linux-amd64.tar.gz
├── koru-core-v0.1.0-linux-arm64.tar.gz
├── koru-core-v0.1.0-darwin-amd64.tar.gz
└── koru-core-v0.1.0-darwin-arm64.tar.gz
```

**For minimal downloads:**
- Linux server? Download only `linux-amd64.tar.gz`
- macOS M1? Download only `darwin-arm64.tar.gz`

### Option 3: Docker Multi-Arch

**Dockerfile:**
```dockerfile
FROM --platform=$BUILDPLATFORM golang:1.21-alpine AS builder

ARG TARGETPLATFORM
ARG BUILDPLATFORM

WORKDIR /app

# Copy appropriate library based on target platform
COPY lib/$TARGETPLATFORM/ /usr/local/lib/
COPY lib/koru.h /usr/local/include/

COPY . .
RUN go build -o validator ./examples/simple_validator.go

FROM alpine:latest
COPY --from=builder /app/validator /validator
CMD ["/validator"]
```

**Build for all platforms:**
```bash
docker buildx build --platform linux/amd64,linux/arm64 -t koru-validator:v0.1.0 .
```

---

## 🧪 Testing Cross-Platform

### Local Testing (Same Machine)

```bash
cd koru-go

# Test on current platform
go test ./pkg/koru -v

# Cross-compile (doesn't run, just builds)
GOOS=linux GOARCH=amd64 go build ./examples/simple_validator.go
GOOS=linux GOARCH=arm64 go build ./examples/simple_validator.go
GOOS=darwin GOARCH=amd64 go build ./examples/simple_validator.go
GOOS=darwin GOARCH=arm64 go build ./examples/simple_validator.go
```

### CI/CD Testing (Multiple Platforms)

**GitHub Actions:**
```yaml
name: Multi-Platform Test

on: [push]

jobs:
  test:
    strategy:
      matrix:
        os: [ubuntu-latest, ubuntu-20.04-arm64, macos-latest, macos-13]

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v3
      - uses: actions/setup-go@v4
        with:
          go-version: '1.21'

      - name: Run tests
        working-directory: koru-go
        run: go test ./pkg/koru -v

      - name: Build example
        working-directory: koru-go
        run: go build ./examples/simple_validator.go
```

---

## 🎓 Technical Details

### Why Build Tags?

**Alternative 1: Runtime detection (❌ Complex)**
```go
// Bad: Runtime library loading
func init() {
    platform := runtime.GOOS + "-" + runtime.GOARCH
    libPath := "lib/" + platform + "/libdistinction_engine.a"
    // Load library dynamically...
}
```

**Alternative 2: Build tags (✅ Simple, Fast)**
```go
//go:build linux && amd64

// Good: Compile-time selection via CGo
// #cgo LDFLAGS: -L${SRCDIR}/../../lib/linux-amd64 -ldistinction_engine
```

**Benefits:**
- ✅ Compile-time linking (faster)
- ✅ No runtime overhead
- ✅ Better error messages
- ✅ Standard Go practice

### Cross-Compilation Tools

**1. cross (Recommended for Linux targets)**
```bash
cargo install cross

# Automatically uses Docker for cross-compilation
cross build --target x86_64-unknown-linux-gnu
```

**2. Native cargo (For macOS targets)**
```bash
rustup target add aarch64-apple-darwin
cargo build --target aarch64-apple-darwin
```

**3. Why both?**
- `cross`: Best for Linux targets (uses Docker containers)
- `cargo`: Best for macOS targets (native toolchain)

---

## 📊 Comparison

### Before (Manual Setup)

```bash
# On macOS M1
cargo build --release
# → Only works on macOS ARM64

# On Linux x64
cargo build --release
# → Only works on Linux AMD64

# Users need to:
# 1. Have Rust installed
# 2. Build for their platform
# 3. Configure Go manually
```

**Result:** ❌ Fragmented, hard to distribute

### After (Universal Setup)

```bash
# Once (maintainer)
./scripts/build_multiplatform.sh
./scripts/create_go_runtime_universal.sh

# Users just:
tar -xzf koru-go-universal.tar.gz
cd koru-go
go run examples/simple_validator.go
```

**Result:** ✅ Simple, works everywhere

---

## 🚀 Deployment Strategies

### Strategy 1: All-In-One (Best for Open Source)

**Pros:**
- ✅ One download works everywhere
- ✅ Easy for users
- ✅ GitHub releases friendly

**Cons:**
- ❌ Larger download (~70MB with all platforms)

**When to use:** Open source projects, easy onboarding priority

### Strategy 2: Platform-Specific (Best for Production)

**Pros:**
- ✅ Smaller downloads (~17MB each)
- ✅ Minimal attack surface
- ✅ Docker-friendly

**Cons:**
- ❌ Users must pick correct platform

**When to use:** Production deployments, Docker containers

### Strategy 3: Hybrid (Best of Both)

**Release both:**
```
Releases:
├── koru-go-universal-v0.1.0.tar.gz       (All platforms)
├── koru-core-v0.1.0-linux-amd64.tar.gz   (Platform-specific)
├── koru-core-v0.1.0-linux-arm64.tar.gz
├── koru-core-v0.1.0-darwin-amd64.tar.gz
└── koru-core-v0.1.0-darwin-arm64.tar.gz
```

**When to use:** Maximum flexibility

---

## ✅ Benefits Summary

### Simplicity
- ✅ Two scripts build everything
- ✅ Automatic platform detection
- ✅ No manual configuration
- ✅ Standard Go tooling

### Universality
- ✅ Works on all major platforms
- ✅ Same code, different architectures
- ✅ Future-proof (easy to add new platforms)

### Cross-Compatibility
- ✅ Build on macOS, run on Linux
- ✅ Build on x64, run on ARM
- ✅ CI/CD friendly
- ✅ Docker multi-arch support

### Developer Experience
- ✅ `go run` just works
- ✅ `go test` just works
- ✅ `go build` just works
- ✅ No weird environment variables

---

## 🎯 Next Steps

1. **Build multi-platform libraries:**
   ```bash
   ./scripts/build_multiplatform.sh
   ```

2. **Create universal Go runtime:**
   ```bash
   ./scripts/create_go_runtime_universal.sh
   ```

3. **Test on your platform:**
   ```bash
   cd koru-go
   go run examples/simple_validator.go
   ```

4. **Distribute:**
   - Option A: Tar the `koru-go/` directory
   - Option B: Use platform-specific archives from `releases/`
   - Option C: Push to GitHub and use CI/CD

---

## 📚 Related Documentation

- [GO_RUNTIME_SETUP.md](GO_RUNTIME_SETUP.md) - Detailed Go setup
- [FFI_LIBRARY_GUIDE.md](FFI_LIBRARY_GUIDE.md) - FFI reference
- [LIBRARY_STATUS.md](LIBRARY_STATUS.md) - Distribution status

---

**Status:** ✅ Production-ready cross-platform distribution system
```
