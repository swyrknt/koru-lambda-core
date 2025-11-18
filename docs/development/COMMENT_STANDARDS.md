# Code Comment Standards

## Principle

Comments should be **minimal, professional, and factual**. Describe what the code does, not philosophical implications or subjective assessments of the approach.

## Rules

### 1. NO Subjective Language
❌ Avoid:
- "revolutionary", "groundbreaking", "amazing"
- "insight", "profound", "elegant"
- "emergent", "transcendent", philosophical claims
- Excessive formatting (bold, italics, special symbols)

✅ Use:
- Simple technical descriptions
- Implementation details
- API contracts and requirements

### 2. File Headers

**BAD:**
```rust
/// Revolutionary P2P consensus layer where the network itself is a distinction
/// that evolves through pure causal synthesis. No traditional gossip, no voting -
/// just structural proof-of-causality (SPoC).
///
/// Groundbreaking Insight:
/// The network doesn't "agree" on state - it IS the state.
```

**GOOD:**
```rust
/// Network consensus using structural proof-of-causality (SPoC).
///
/// Implements deterministic leader election and batch validation without
/// traditional voting rounds. Leader is computed from epoch and validator set.
```

### 3. Struct/Enum Documentation

**BAD:**
```rust
/// Stage 1: Lightweight commitment for fast gossip
///
/// This is what gets broadcast to all nodes. Size: ~80 bytes total.
```

**GOOD:**
```rust
/// Lightweight batch commitment (Stage 1 of two-stage protocol).
/// Broadcast to all nodes for consensus. Approximately 80 bytes.
```

### 4. Function Documentation

**BAD:**
```rust
/// Synthesizes two distinctions to create a third (Axiom: Synthesis).
/// Returns the resulting distinction.
```

**GOOD:**
```rust
/// Combines two distinctions deterministically to create a third.
/// Returns the synthesized distinction.
```

### 5. Inline Comments

**BAD:**
```rust
// LRU eviction handled automatically by put() - beautiful!
self.cache.put(key, value);
```

**GOOD:**
```rust
// LRU eviction handled automatically
self.cache.put(key, value);
```

### 6. Implementation Notes

Focus on technical details, not philosophy:

**BAD:**
```rust
// In distinction calculus, the distinction (hash) IS the truth.
// The data is merely evidence that can be fetched on demand.
```

**GOOD:**
```rust
// Hash serves as commitment. Full data fetched on demand in Stage 2.
```

## Format Guidelines

### Documentation Comments (`///`)

- **Module/file headers**: 1-3 sentences max. Describe purpose and key features.
- **Public items**: Describe what it does, parameters, return values.
- **No examples in production code** unless API is complex.

### Inline Comments (`//`)

- Use sparingly - code should be self-documenting
- Explain WHY, not WHAT (code shows what)
- Keep to one line when possible

### Special Sections

**Acceptable when needed:**
- `SAFETY:` for unsafe code
- `TODO:` for incomplete work (but minimize these)
- `NOTE:` for important implementation details

**Remove:**
- `Axiom:` references
- `Theoretical Foundation:` sections  
- `Design Principles:` with philosophical claims

## Examples

### Before
```rust
/// Commitment Protocol - Two-Stage Gossip System
///
/// Implements the revolutionary insight: **The commitment IS the consensus**.
///
/// Traditional systems broadcast full data to all nodes (wasteful).
/// This system separates concerns:
/// - **Stage 1 (FAST)**: Gossip lightweight commitments (32 bytes) for consensus
/// - **Stage 2 (LAZY)**: Fetch heavy data only when needed
///
/// Theoretical Foundation:
/// In distinction calculus, the distinction (hash) IS the truth.
/// The data is merely evidence that can be fetched on demand.
```

### After
```rust
/// Two-stage batch commitment protocol.
///
/// Stage 1: Broadcast lightweight commitments (32 bytes) for consensus.
/// Stage 2: Fetch full batch data on demand.
///
/// Reduces network overhead by separating consensus from data availability.
```

## Summary

- **Be technical, not philosophical**
- **Be concise, not verbose**
- **Be factual, not promotional**
- **Describe behavior, not beauty**
