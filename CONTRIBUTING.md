# Contributing to Koru Lambda Core

This is the **core** of our entire system. The code here must be flawless, performant, and maintainable. Follow these guidelines strictly.

## Core Principles

### 1. The Five Axioms Are Sacred
Every change must preserve the five axioms of distinction calculus:
- **Identity**: A distinction is defined solely by its unique identifier
- **Nontriviality**: The system initializes with two primordial distinctions (Δ₀, Δ₁)
- **Synthesis**: Two distinctions combine deterministically to create a third
- **Symmetry**: Relationships are bidirectional and order-independent
- **Irreflexivity**: A distinction synthesized with itself yields itself

### 2. Zero Tolerance for Borrow Checker Warnings
If the borrow checker complains, **fix it properly**. Never:
- Use `unsafe` without explicit approval
- Clone excessively to work around borrowing issues
- Use `Rc<RefCell<T>>` or similar unless absolutely necessary

## Development Workflow

### Before You Start
```bash
# Update to latest
git pull origin main

# Ensure clean state
cargo clean
cargo check
cargo test
cargo clippy -- -D warnings
```

### Making Changes

#### 1. Understanding Ownership Patterns

**Good**: Return owned values from mutating methods
```rust
pub fn synthesize(&mut self, a: &Distinction, b: &Distinction) -> Distinction {
    // ... mutations ...
    new_distinction  // Return owned value
}
```

**Bad**: Trying to return references from mutating methods
```rust
pub fn synthesize(&mut self, a: &Distinction, b: &Distinction) -> &Distinction {
    // This will cause borrow checker errors!
}
```

#### 2. Cloning Strategy

**When to Clone**:
- Returning values from `HashMap`/`HashSet` lookups after mutation
- When the caller needs to store the value while continuing to use the engine
- For tests that need independent copies

**When NOT to Clone**:
- Just to satisfy the borrow checker (refactor instead)
- In tight loops (use references or iterators)
- For large data structures (consider `Rc` or rethink design)

#### 3. Method Signatures

**Prefer**:
- `&self` for queries
- `&mut self` for mutations
- Return owned `T` for new/synthesized values
- Accept `&T` for inputs

**Avoid**:
- `&mut self` with `&` return types (borrow checker hell)
- Unnecessary lifetimes
- Generic types without clear benefit

### Common Pitfalls

#### Pitfall 1: Holding References Across Mutations
```rust
// BAD - Won't compile
let d0 = engine.d0();  // Immutable borrow
let result = engine.synthesize(d0, d0);  // Mutable borrow - ERROR!

// GOOD - Clone for independence
let d0 = engine.d0().clone();  // Owned value
let result = engine.synthesize(&d0, &d0);  // Works!
```

#### Pitfall 2: Early Returns with Mutations
```rust
// BAD - Borrow checker errors
pub fn synthesize(&mut self, a: &Distinction, b: &Distinction) -> &Distinction {
    if let Some(existing) = self.all_distinctions.get(&id) {
        return existing;  // Immutable borrow
    }
    self.all_distinctions.insert(...);  // Can't mutate!
}

// GOOD - Clone the early return
pub fn synthesize(&mut self, a: &Distinction, b: &Distinction) -> Distinction {
    if let Some(existing) = self.all_distinctions.get(&id) {
        return existing.clone();  // Owned value
    }
    // ... now we can mutate freely
}
```

## Code Quality Standards

### Documentation
Every public item **must** have documentation:
```rust
/// Synthesizes two distinctions to create a third (Axiom: Synthesis).
/// Returns the resulting distinction.
///
/// # Examples
/// ```
/// let mut engine = DistinctionEngine::new();
/// let d0 = engine.d0().clone();
/// let d1 = engine.d1().clone();
/// let result = engine.synthesize(&d0, &d1);
/// ```
pub fn synthesize(&mut self, a: &Distinction, b: &Distinction) -> Distinction {
    // ...
}
```

### Testing
- **Unit tests** in `src/lib.rs` test individual axioms
- **Integration tests** in `tests/` verify system properties
- Every public method needs at least one test
- Test edge cases: empty, single, many

### Error Handling
- Use `Result<T, E>` for operations that can fail
- Use `Option<T>` for values that might not exist
- **Never panic** in library code except for programmer errors
- Document all error conditions

## Pre-Commit Checklist

Before committing, **always** run:

```bash
# Format code
cargo fmt

# Check for errors
cargo check

# Run all tests
cargo test

# Strict clippy check
cargo clippy -- -D warnings

# Check for TODOs
git grep "TODO\\|FIXME" src/
```

All checks must pass. No exceptions.

## Performance Considerations

### This is a Performance-Critical System
- Target: 100,000+ transactions/second
- Every clone has a cost - profile before optimizing
- Use `cargo bench` to verify improvements
- Never sacrifice correctness for performance

### Profiling
```bash
# Run benchmarks
cargo bench

# Profile with flamegraph
cargo install flamegraph
cargo flamegraph --bench performance
```

## Review Process

1. **Self-review**: Read your diff carefully
2. **Test**: Run all checks locally
3. **Document**: Update docs and comments
4. **PR**: Create pull request with clear description
5. **Address feedback**: Iterate quickly

## Common Review Feedback

### "Too many clones"
- Review each `.clone()` - is it necessary?
- Can you restructure to use references?
- Document **why** the clone is needed

### "Missing tests"
- Every new public method needs tests
- Test happy path AND edge cases
- Verify axioms still hold

### "Unclear documentation"
- Explain **why**, not just what
- Include examples
- Link to axioms when relevant

## Getting Help

### Borrow Checker Issues
1. Read the error message carefully
2. Identify which borrow is the problem
3. Consider: Can I return an owned value instead?
4. Check: Am I holding a reference too long?
5. Ask: "Would cloning here be acceptable?"

### Design Questions
- Reference [Design Documentation](docs/DESIGN_DOC.md) for the theoretical foundation
- Discuss architectural changes before implementing
- Preserve the axioms at all costs

## Anti-Patterns to Avoid

❌ **Using `unwrap()` everywhere**
```rust
// BAD
let value = map.get(&key).unwrap();

// GOOD
let value = map.get(&key).expect("key must exist by construction");
// Or better: handle the None case
```

❌ **Excessive generics**
```rust
// BAD - Unnecessary complexity
pub fn process<T: Clone + Debug + Display>(item: T) { }

// GOOD - Simple and clear
pub fn process(item: &Distinction) { }
```

❌ **Mutable global state**
```rust
// BAD - Never do this
static mut GLOBAL_ENGINE: Option<DistinctionEngine> = None;

// GOOD - Pass references
fn do_work(engine: &mut DistinctionEngine) { }
```

## Remember

> "Rust prevents bugs at compile time. Embrace the compiler as your pair programmer."

> "Koru Lambda Core is the foundation. Build it right once, and everything else becomes easier."

When in doubt, ask. Better to get feedback early than to rewrite later.

---

**Happy coding!** 🦀
