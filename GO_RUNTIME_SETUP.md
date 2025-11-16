# Koru Lambda Core - Go Runtime Setup Guide

This guide shows you how to distribute the Rust library and build the Go runtime.

---

## 📦 Step 1: Package the Library for Distribution

### Option A: Local Development (Start Here)

The library files you need are already built:

```bash
# Required files for Go runtime:
target/release/libdistinction_engine.a   # Static library
target/koru.h                             # C header file

# Optional (for dynamic linking):
target/release/libdistinction_engine.dylib  # macOS
# or
target/release/libdistinction_engine.so     # Linux
```

**Quick packaging:**
```bash
# Create distribution directory
mkdir -p dist/include dist/lib

# Copy library artifacts
cp target/koru.h dist/include/
cp target/release/libdistinction_engine.a dist/lib/

# Package for sharing
tar -czf koru-core-v0.1.0-darwin-arm64.tar.gz \
  -C dist \
  include/koru.h \
  lib/libdistinction_engine.a
```

### Option B: Cross-Platform Distribution

For production, you'll want to build for multiple platforms:

```bash
# macOS (you're here)
cargo build --release
# → target/release/libdistinction_engine.{a,dylib}

# Linux (via cross-compilation or CI)
cargo build --release --target x86_64-unknown-linux-gnu
# → target/x86_64-unknown-linux-gnu/release/libdistinction_engine.{a,so}

# Windows
cargo build --release --target x86_64-pc-windows-gnu
# → target/x86_64-pc-windows-gnu/release/distinction_engine.{lib,dll}
```

---

## 🚀 Step 2: Create the Go Runtime Project

### 2.1 Project Structure

```bash
# Create Go project
mkdir -p koru-go
cd koru-go

# Initialize Go module
go mod init github.com/yourorg/koru-go

# Create project structure
mkdir -p {pkg/koru,lib,examples,internal}
```

**Recommended structure:**
```
koru-go/
├── go.mod
├── go.sum
├── README.md
├── lib/                          # Rust library artifacts
│   ├── libdistinction_engine.a
│   └── koru.h
├── pkg/
│   └── koru/
│       ├── engine.go             # Engine bindings
│       ├── agent.go              # Agent bindings
│       ├── commitment.go         # Commitment types
│       └── ffi.go                # CGo wrapper
├── examples/
│   ├── simple_validator.go
│   └── commitment_demo.go
└── internal/
    └── network/
        └── libp2p.go             # Network layer
```

### 2.2 Copy Library Files

```bash
# From your Rust project root
cp target/koru.h koru-go/lib/
cp target/release/libdistinction_engine.a koru-go/lib/

# Verify
ls -lh koru-go/lib/
```

---

## 🔧 Step 3: Create Go Bindings

### 3.1 Create `pkg/koru/ffi.go`

This file wraps the C FFI:

```go
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

// Error codes from C
const (
	Success          = 0
	ErrNullPointer   = -1
	ErrInvalidData   = -2
	ErrBatchRejected = -3
	ErrUTF8          = -4
)

// Engine wraps the C KoruEngine
type Engine struct {
	ptr *C.KoruEngine
}

// NewEngine creates a new distinction engine
func NewEngine() *Engine {
	ptr := C.koru_engine_new()
	if ptr == nil {
		panic("failed to create engine")
	}
	return &Engine{ptr: ptr}
}

// Free releases the engine
func (e *Engine) Free() {
	if e.ptr != nil {
		C.koru_engine_free(e.ptr)
		e.ptr = nil
	}
}

// DistinctionCount returns the number of distinctions
func (e *Engine) DistinctionCount() uint64 {
	return uint64(C.koru_engine_distinction_count(e.ptr))
}

// RelationshipCount returns the number of relationships
func (e *Engine) RelationshipCount() uint64 {
	return uint64(C.koru_engine_relationship_count(e.ptr))
}

// Agent wraps the C KoruAgent
type Agent struct {
	ptr    *C.KoruAgent
	engine *Engine
}

// NewAgent creates a new network agent
func NewAgent(engine *Engine) *Agent {
	ptr := C.koru_agent_new(engine.ptr)
	if ptr == nil {
		panic("failed to create agent")
	}
	return &Agent{
		ptr:    ptr,
		engine: engine,
	}
}

// Free releases the agent
func (a *Agent) Free() {
	if a.ptr != nil {
		C.koru_agent_free(a.ptr)
		a.ptr = nil
	}
}

// CurrentEpoch returns the current epoch
func (a *Agent) CurrentEpoch() uint64 {
	return uint64(C.koru_agent_current_epoch(a.ptr))
}

// StateRoot returns the current state root
func (a *Agent) StateRoot() string {
	cStr := C.koru_agent_state_root(a.ptr)
	if cStr == nil {
		return ""
	}
	defer C.koru_free_string(cStr)
	return C.GoString(cStr)
}

// ProposeCommitment proposes a new commitment (Stage 1)
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

// CheckCommitment verifies a commitment (light node check)
func (a *Agent) CheckCommitment(hash [32]byte, expectedNonce, expectedEpoch uint64) bool {
	result := C.koru_agent_check_commitment(
		a.ptr,
		(*C.uint8_t)(unsafe.Pointer(&hash[0])),
		C.uint64_t(expectedNonce),
		C.uint64_t(expectedEpoch),
	)
	return result == 1
}

// FinalizeBatch finalizes a batch after verification (Stage 2)
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

// JoinPeer adds a peer to the validator set
func (a *Agent) JoinPeer(peerID string) error {
	cPeerID := C.CString(peerID)
	defer C.free(unsafe.Pointer(cPeerID))

	result := C.koru_agent_join_peer(a.ptr, a.engine.ptr, cPeerID)
	if result != Success {
		return fmt.Errorf("join_peer failed: %d", result)
	}

	return nil
}

// AdvanceEpoch moves to the next epoch
func (a *Agent) AdvanceEpoch() error {
	result := C.koru_agent_advance_epoch(a.ptr, a.engine.ptr)
	if result != Success {
		return fmt.Errorf("advance_epoch failed: %d", result)
	}
	return nil
}

// GetLeader returns the current leader ID
func (a *Agent) GetLeader() string {
	cStr := C.koru_agent_get_leader(a.ptr)
	if cStr == nil {
		return ""
	}
	defer C.koru_free_string(cStr)
	return C.GoString(cStr)
}
```

### 3.2 Create `pkg/koru/commitment.go`

Higher-level commitment types:

```go
package koru

import (
	"encoding/hex"
	"encoding/json"
	"fmt"
)

// Transaction represents a single transaction
type Transaction struct {
	Nonce uint64   `json:"nonce"`
	Data  []byte   `json:"data"`
}

// Batch represents a batch of transactions
type Batch struct {
	Transactions []Transaction `json:"transactions"`
	PreviousRoot string        `json:"previous_root"`
}

// Commitment represents a batch commitment
type Commitment struct {
	Hash   [32]byte
	Nonce  uint64
	Epoch  uint64
	Leader string
}

// String returns hex representation of commitment hash
func (c *Commitment) String() string {
	return hex.EncodeToString(c.Hash[:])
}

// ProposeCommitmentFromBatch is a helper for the two-stage protocol
func (a *Agent) ProposeCommitmentFromBatch(batch *Batch) (*Commitment, error) {
	// Serialize batch to JSON
	batchJSON, err := json.Marshal(batch)
	if err != nil {
		return nil, fmt.Errorf("marshal batch: %w", err)
	}

	// Propose commitment (Stage 1)
	hash, err := a.ProposeCommitment(batchJSON)
	if err != nil {
		return nil, err
	}

	return &Commitment{
		Hash:   hash,
		Nonce:  a.CurrentEpoch(), // Simplified
		Epoch:  a.CurrentEpoch(),
		Leader: a.GetLeader(),
	}, nil
}

// FinalizeBatchFromCommitment is a helper for Stage 2
func (a *Agent) FinalizeBatchFromCommitment(batch *Batch, commitment *Commitment) error {
	batchJSON, err := json.Marshal(batch)
	if err != nil {
		return fmt.Errorf("marshal batch: %w", err)
	}

	return a.FinalizeBatch(batchJSON, commitment.Hash)
}
```

### 3.3 Create `examples/simple_validator.go`

Example usage:

```go
package main

import (
	"fmt"
	"log"

	"github.com/yourorg/koru-go/pkg/koru"
)

func main() {
	// Create engine and agent
	engine := koru.NewEngine()
	defer engine.Free()

	agent := koru.NewAgent(engine)
	defer agent.Free()

	fmt.Printf("Initial state root: %s\n", agent.StateRoot())
	fmt.Printf("Current epoch: %d\n", agent.CurrentEpoch())

	// Join as a peer
	if err := agent.JoinPeer("validator_0"); err != nil {
		log.Fatal(err)
	}

	// Create a batch
	batch := &koru.Batch{
		Transactions: []koru.Transaction{
			{Nonce: 0, Data: []byte{1, 2, 3}},
			{Nonce: 1, Data: []byte{4, 5, 6}},
		},
		PreviousRoot: agent.StateRoot(),
	}

	// Stage 1: Propose commitment
	commitment, err := agent.ProposeCommitmentFromBatch(batch)
	if err != nil {
		log.Fatal(err)
	}

	fmt.Printf("✓ Commitment proposed: %s\n", commitment)

	// Light node: Check commitment
	isValid := agent.CheckCommitment(commitment.Hash, 0, 0)
	fmt.Printf("✓ Commitment valid: %v\n", isValid)

	// Stage 2: Finalize batch
	if err := agent.FinalizeBatchFromCommitment(batch, commitment); err != nil {
		log.Fatal(err)
	}

	fmt.Printf("✓ Batch finalized\n")
	fmt.Printf("Final state root: %s\n", agent.StateRoot())
	fmt.Printf("Distinctions: %d\n", engine.DistinctionCount())
}
```

---

## 🧪 Step 4: Test the Go Bindings

### 4.1 Build and Run

```bash
cd koru-go

# Build the example
go build -o validator ./examples/simple_validator.go

# Run it
./validator
```

**Expected output:**
```
Initial state root: <some hash>
Current epoch: 0
✓ Commitment proposed: <commitment hash>
✓ Commitment valid: true
✓ Batch finalized
Final state root: <new hash>
Distinctions: <count>
```

### 4.2 Create Tests

Create `pkg/koru/ffi_test.go`:

```go
package koru

import (
	"testing"
)

func TestEngineLifecycle(t *testing.T) {
	engine := NewEngine()
	defer engine.Free()

	if engine.DistinctionCount() != 2 {
		t.Errorf("expected 2 initial distinctions, got %d", engine.DistinctionCount())
	}
}

func TestAgentLifecycle(t *testing.T) {
	engine := NewEngine()
	defer engine.Free()

	agent := NewAgent(engine)
	defer agent.Free()

	if agent.CurrentEpoch() != 0 {
		t.Errorf("expected epoch 0, got %d", agent.CurrentEpoch())
	}

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

	// Create batch
	batch := &Batch{
		Transactions: []Transaction{
			{Nonce: 0, Data: []byte{1, 2, 3}},
		},
		PreviousRoot: agent.StateRoot(),
	}

	// Stage 1: Propose
	commitment, err := agent.ProposeCommitmentFromBatch(batch)
	if err != nil {
		t.Fatalf("propose failed: %v", err)
	}

	// Verify
	if !agent.CheckCommitment(commitment.Hash, 0, 0) {
		t.Error("valid commitment rejected")
	}

	// Stage 2: Finalize
	if err := agent.FinalizeBatchFromCommitment(batch, commitment); err != nil {
		t.Fatalf("finalize failed: %v", err)
	}
}
```

Run tests:
```bash
go test ./pkg/koru -v
```

---

## 🌐 Step 5: Add Network Layer (libp2p)

### 5.1 Add Dependencies

```bash
go get github.com/libp2p/go-libp2p
go get github.com/libp2p/go-libp2p-pubsub
```

### 5.2 Create `internal/network/node.go`

```go
package network

import (
	"context"
	"fmt"

	"github.com/libp2p/go-libp2p"
	pubsub "github.com/libp2p/go-libp2p-pubsub"
	"github.com/libp2p/go-libp2p/core/host"
	"github.com/libp2p/go-libp2p/core/peer"

	"github.com/yourorg/koru-go/pkg/koru"
)

const CommitmentTopic = "/koru/commitments/v1"

// Node represents a Koru network node
type Node struct {
	host      host.Host
	pubsub    *pubsub.PubSub
	topic     *pubsub.Topic
	engine    *koru.Engine
	agent     *koru.Agent
	ctx       context.Context
}

// NewNode creates a new network node
func NewNode(ctx context.Context, port int) (*Node, error) {
	// Create libp2p host
	h, err := libp2p.New(
		libp2p.ListenAddrStrings(fmt.Sprintf("/ip4/0.0.0.0/tcp/%d", port)),
	)
	if err != nil {
		return nil, err
	}

	// Create pubsub
	ps, err := pubsub.NewGossipSub(ctx, h)
	if err != nil {
		return nil, err
	}

	// Join topic
	topic, err := ps.Join(CommitmentTopic)
	if err != nil {
		return nil, err
	}

	// Create engine and agent
	engine := koru.NewEngine()
	agent := koru.NewAgent(engine)

	return &Node{
		host:   h,
		pubsub: ps,
		topic:  topic,
		engine: engine,
		agent:  agent,
		ctx:    ctx,
	}, nil
}

// BroadcastCommitment gossips a commitment to the network
func (n *Node) BroadcastCommitment(commitment *koru.Commitment) error {
	// Serialize commitment (simplified - use protobuf in production)
	data := commitment.Hash[:]
	return n.topic.Publish(n.ctx, data)
}

// SubscribeCommitments listens for commitments from the network
func (n *Node) SubscribeCommitments() (<-chan *koru.Commitment, error) {
	sub, err := n.topic.Subscribe()
	if err != nil {
		return nil, err
	}

	ch := make(chan *koru.Commitment, 10)

	go func() {
		defer close(ch)
		for {
			msg, err := sub.Next(n.ctx)
			if err != nil {
				return
			}

			// Parse commitment
			var hash [32]byte
			copy(hash[:], msg.Data)

			commitment := &koru.Commitment{Hash: hash}
			ch <- commitment
		}
	}()

	return ch, nil
}

// Close shuts down the node
func (n *Node) Close() error {
	n.agent.Free()
	n.engine.Free()
	return n.host.Close()
}
```

---

## 📦 Step 6: Distribution Options

### Option A: GitHub Releases

```bash
# Tag version
git tag v0.1.0

# Create release tarball
tar -czf koru-go-v0.1.0.tar.gz \
  -C koru-go \
  go.mod \
  go.sum \
  pkg/ \
  lib/ \
  examples/ \
  README.md

# Upload to GitHub Releases
gh release create v0.1.0 koru-go-v0.1.0.tar.gz
```

### Option B: Docker Image

Create `Dockerfile`:

```dockerfile
FROM golang:1.21-alpine

WORKDIR /app

# Copy library
COPY lib/ /usr/local/lib/
COPY lib/koru.h /usr/local/include/

# Copy Go code
COPY go.mod go.sum ./
RUN go mod download

COPY . .

# Build
RUN go build -o validator ./examples/simple_validator.go

CMD ["./validator"]
```

Build and run:
```bash
docker build -t koru-validator:v0.1.0 .
docker run koru-validator:v0.1.0
```

### Option C: Go Module

Make it importable:

```bash
# Tag and push
git tag v0.1.0
git push origin v0.1.0

# Others can use:
# go get github.com/yourorg/koru-go@v0.1.0
```

---

## 🎯 Quick Start Checklist

- [ ] Package Rust library (`dist/` directory)
- [ ] Create Go project structure
- [ ] Copy library files to `koru-go/lib/`
- [ ] Create `ffi.go` with CGo bindings
- [ ] Create `commitment.go` with higher-level API
- [ ] Create example validator
- [ ] Test Go bindings
- [ ] Add libp2p network layer
- [ ] Build and run example
- [ ] Distribute via GitHub/Docker/Go module

---

## 💡 Next Steps After Go Runtime

1. **Test multi-node** - Run 3+ Go validators together
2. **Benchmark** - Compare performance to Rust core
3. **Add features**:
   - Prometheus metrics
   - gRPC API
   - Database persistence
   - WebSocket subscriptions
4. **Production hardening**:
   - Error handling
   - Logging
   - Configuration
   - Graceful shutdown

---

## 📚 Resources

- [CGo Documentation](https://pkg.go.dev/cmd/cgo)
- [go-libp2p Examples](https://github.com/libp2p/go-libp2p-examples)
- [Koru FFI Guide](../FFI_LIBRARY_GUIDE.md)

---

**Ready to build!** Start with the simple example, then add networking. 🚀
