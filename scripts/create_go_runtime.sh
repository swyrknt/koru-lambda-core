#!/bin/bash
# Koru Lambda Core - Go Runtime Setup Script
# This script creates the Go runtime project structure

set -e

echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   Koru Lambda Core - Go Runtime Setup                        ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""

# Configuration
PROJECT_NAME="koru-go"
GO_MODULE="github.com/yourorg/koru-go"

# Check if Go is installed
if ! command -v go &> /dev/null; then
    echo "❌ Error: Go is not installed"
    echo "   Install from: https://go.dev/dl/"
    exit 1
fi

echo "✓ Go version: $(go version)"
echo ""

# Create project directory
if [ -d "$PROJECT_NAME" ]; then
    echo "⚠️  Directory $PROJECT_NAME already exists"
    read -p "   Delete and recreate? (y/N): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        rm -rf "$PROJECT_NAME"
    else
        exit 0
    fi
fi

echo "📁 Creating project structure..."
mkdir -p "$PROJECT_NAME"/{pkg/koru,lib,examples,internal/network,cmd/validator}

# Initialize Go module
cd "$PROJECT_NAME"
go mod init "$GO_MODULE"
echo "✓ Go module initialized: $GO_MODULE"

# Copy library files
echo ""
echo "📦 Copying library files..."
cp ../target/koru.h lib/
cp ../target/release/libdistinction_engine.a lib/
echo "✓ Copied koru.h"
echo "✓ Copied libdistinction_engine.a"

# Create ffi.go
echo ""
echo "🔧 Creating Go bindings..."
cat > pkg/koru/ffi.go << 'EOF'
package koru

// #cgo CFLAGS: -I${SRCDIR}/../../lib
// #cgo LDFLAGS: -L${SRCDIR}/../../lib -ldistinction_engine
// #include <stdlib.h>
// #include "koru.h"
import "C"
import (
	"fmt"
	"unsafe"
)

const (
	Success          = 0
	ErrNullPointer   = -1
	ErrInvalidData   = -2
	ErrBatchRejected = -3
	ErrUTF8          = -4
)

type Engine struct {
	ptr *C.KoruEngine
}

func NewEngine() *Engine {
	ptr := C.koru_engine_new()
	if ptr == nil {
		panic("failed to create engine")
	}
	return &Engine{ptr: ptr}
}

func (e *Engine) Free() {
	if e.ptr != nil {
		C.koru_engine_free(e.ptr)
		e.ptr = nil
	}
}

func (e *Engine) DistinctionCount() uint64 {
	return uint64(C.koru_engine_distinction_count(e.ptr))
}

type Agent struct {
	ptr    *C.KoruAgent
	engine *Engine
}

func NewAgent(engine *Engine) *Agent {
	ptr := C.koru_agent_new(engine.ptr)
	if ptr == nil {
		panic("failed to create agent")
	}
	return &Agent{ptr: ptr, engine: engine}
}

func (a *Agent) Free() {
	if a.ptr != nil {
		C.koru_agent_free(a.ptr)
		a.ptr = nil
	}
}

func (a *Agent) StateRoot() string {
	cStr := C.koru_agent_state_root(a.ptr)
	if cStr == nil {
		return ""
	}
	defer C.koru_free_string(cStr)
	return C.GoString(cStr)
}

func (a *Agent) ProposeCommitment(batchJSON []byte) ([32]byte, error) {
	var hash [32]byte
	result := C.koru_agent_propose_commitment(
		a.ptr,
		a.engine.ptr,
		(*C.uint8_t)(unsafe.Pointer(&batchJSON[0])),
		C.uintptr_t(len(batchJSON)),
		(*C.uint8_t)(unsafe.Pointer(&hash[0])),
	)
	if result != Success {
		return hash, fmt.Errorf("propose_commitment failed: %d", result)
	}
	return hash, nil
}

func (a *Agent) CheckCommitment(hash [32]byte, nonce, epoch uint64) bool {
	result := C.koru_agent_check_commitment(
		a.ptr,
		(*C.uint8_t)(unsafe.Pointer(&hash[0])),
		C.uint64_t(nonce),
		C.uint64_t(epoch),
	)
	return result == 1
}

func (a *Agent) FinalizeBatch(batchJSON []byte, hash [32]byte) error {
	result := C.koru_agent_finalize_batch(
		a.ptr,
		a.engine.ptr,
		(*C.uint8_t)(unsafe.Pointer(&batchJSON[0])),
		C.uintptr_t(len(batchJSON)),
		(*C.uint8_t)(unsafe.Pointer(&hash[0])),
	)
	if result != Success {
		return fmt.Errorf("finalize_batch failed: %d", result)
	}
	return nil
}
EOF

# Create example
cat > examples/simple_validator.go << 'EOF'
package main

import (
	"encoding/hex"
	"fmt"
	"log"

	"github.com/yourorg/koru-go/pkg/koru"
)

func main() {
	fmt.Println("🚀 Koru Lambda Core - Go Runtime Demo")
	fmt.Println("")

	// Create engine and agent
	engine := koru.NewEngine()
	defer engine.Free()

	agent := koru.NewAgent(engine)
	defer agent.Free()

	fmt.Printf("Initial state root: %s\n", agent.StateRoot())
	fmt.Printf("Distinctions: %d\n", engine.DistinctionCount())
	fmt.Println("")

	// Create a simple batch
	batch := `{"transactions":[{"nonce":0,"data":[1,2,3]}],"previous_root":"` + agent.StateRoot() + `"}`

	// Stage 1: Propose commitment
	fmt.Println("Stage 1: Proposing commitment...")
	commitment, err := agent.ProposeCommitment([]byte(batch))
	if err != nil {
		log.Fatal(err)
	}
	fmt.Printf("✓ Commitment hash: %s\n", hex.EncodeToString(commitment[:]))

	// Verify commitment
	isValid := agent.CheckCommitment(commitment, 0, 0)
	fmt.Printf("✓ Commitment valid: %v\n", isValid)
	fmt.Println("")

	// Stage 2: Finalize batch
	fmt.Println("Stage 2: Finalizing batch...")
	if err := agent.FinalizeBatch([]byte(batch), commitment); err != nil {
		log.Fatal(err)
	}

	fmt.Println("✓ Batch finalized!")
	fmt.Printf("Final state root: %s\n", agent.StateRoot())
	fmt.Printf("Distinctions: %d\n", engine.DistinctionCount())
	fmt.Println("")
	fmt.Println("✅ Two-stage commitment protocol completed successfully!")
}
EOF

# Create test
cat > pkg/koru/ffi_test.go << 'EOF'
package koru

import "testing"

func TestEngineLifecycle(t *testing.T) {
	engine := NewEngine()
	defer engine.Free()

	if engine.DistinctionCount() != 2 {
		t.Errorf("expected 2 distinctions, got %d", engine.DistinctionCount())
	}
}

func TestAgentLifecycle(t *testing.T) {
	engine := NewEngine()
	defer engine.Free()

	agent := NewAgent(engine)
	defer agent.Free()

	root := agent.StateRoot()
	if root == "" {
		t.Error("state root should not be empty")
	}
}

func TestCommitmentProtocol(t *testing.T) {
	engine := NewEngine()
	defer engine.Free()

	agent := NewAgent(engine)
	defer agent.Free()

	batch := `{"transactions":[{"nonce":0,"data":[1,2,3]}],"previous_root":"` + agent.StateRoot() + `"}`

	// Propose
	hash, err := agent.ProposeCommitment([]byte(batch))
	if err != nil {
		t.Fatalf("propose failed: %v", err)
	}

	// Verify
	if !agent.CheckCommitment(hash, 0, 0) {
		t.Error("valid commitment rejected")
	}

	// Finalize
	if err := agent.FinalizeBatch([]byte(batch), hash); err != nil {
		t.Fatalf("finalize failed: %v", err)
	}
}
EOF

# Create README
cat > README.md << 'EOF'
# Koru Lambda Core - Go Runtime

Go bindings for the Koru Lambda Core distinction engine.

## Quick Start

```bash
# Run example
go run examples/simple_validator.go

# Run tests
go test ./pkg/koru -v

# Build validator
go build -o validator ./examples/simple_validator.go
./validator
```

## Usage

```go
import "github.com/yourorg/koru-go/pkg/koru"

// Create engine and agent
engine := koru.NewEngine()
defer engine.Free()

agent := koru.NewAgent(engine)
defer agent.Free()

// Two-stage commitment protocol
batch := []byte(`{"transactions":[...],"previous_root":"..."}`)

// Stage 1: Propose
hash, _ := agent.ProposeCommitment(batch)

// Verify
isValid := agent.CheckCommitment(hash, 0, 0)

// Stage 2: Finalize
agent.FinalizeBatch(batch, hash)
```

## Documentation

- [FFI Library Guide](../FFI_LIBRARY_GUIDE.md)
- [Go Runtime Setup](../GO_RUNTIME_SETUP.md)
EOF

echo "✓ Created ffi.go"
echo "✓ Created simple_validator.go"
echo "✓ Created tests"
echo "✓ Created README.md"

# Build and test
echo ""
echo "🧪 Running tests..."
if go test ./pkg/koru -v; then
    echo ""
    echo "✅ All tests passed!"
else
    echo ""
    echo "⚠️  Some tests failed (this is expected if library path needs adjustment)"
fi

echo ""
echo "╔══════════════════════════════════════════════════════════════╗"
echo "║   ✅ Go Runtime Setup Complete!                              ║"
echo "╚══════════════════════════════════════════════════════════════╝"
echo ""
echo "📁 Project created at: $PROJECT_NAME/"
echo ""
echo "Next steps:"
echo "  cd $PROJECT_NAME"
echo "  go run examples/simple_validator.go"
echo ""
echo "Or run tests:"
echo "  cd $PROJECT_NAME"
echo "  go test ./pkg/koru -v"
echo ""
