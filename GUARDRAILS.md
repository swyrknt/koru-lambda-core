# Forma Core Guardrails - Developer Safety System

These guardrails prevent common Rust pitfalls and ensure code quality stays high as your team scales.

---

## 🛡️ Layer 1: Automated Code Quality (clippy.toml)

**File**: `clippy.toml`

### What It Does
Clippy is Rust's official linter. It catches:
- Complexity issues (functions too complex to understand)
- Type issues (overly complex types that hurt readability)
- Performance issues (unnecessary allocations, inefficient patterns)
- API stability issues (accidental breaking changes)

### Our Configuration
```toml
cognitive-complexity-threshold = 15    # Max function complexity
type-complexity-threshold = 100        # Max type nesting depth
avoid-breaking-exported-api = true     # Prevent accidental API changes
```

### What It Prevents
```rust
// ❌ BLOCKED: Function too complex (cognitive complexity > 15)
pub fn overly_complex_function() {
    if condition1 {
        if condition2 {
            if condition3 {
                // ... 20 more nested ifs
            }
        }
    }
}

// ❌ BLOCKED: Type too complex
pub fn bad_signature() -> (Vec<&Distinction>, &HashSet<(String, String)>) {
    // clippy: type_complexity - use a type alias instead!
}

// ✅ ALLOWED: Clean type alias
pub type StateSnapshot<'a> = (Vec<&'a Distinction>, &'a HashSet<Relationship>);
pub fn good_signature() -> StateSnapshot { ... }
```

### How to Run
```bash
# Strict mode - fails on ANY warning
cargo clippy --all-targets -- -D warnings

# See what would fail without failing the build
cargo clippy --all-targets
```

---

## 🎨 Layer 2: Consistent Formatting (rustfmt.toml)

**File**: `rustfmt.toml`

### What It Does
Automatically formats code to a consistent style. No more:
- Arguments about tabs vs spaces
- Inconsistent line breaks
- Debates about brace placement

### Our Configuration
```toml
edition = "2021"                      # Use Rust 2021 edition features
max_width = 100                       # 100 chars per line (readable on laptops)
use_field_init_shorthand = true      # Use { x } instead of { x: x }
use_try_shorthand = true             # Use ? instead of try!
match_block_trailing_comma = true    # Consistent match formatting
```

### What It Does
```rust
// BEFORE (inconsistent, hard to read)
pub struct DistinctionEngine{d0:Distinction,d1:Distinction,all_distinctions:HashMap<String,Distinction>,relationships:HashSet<(String,String)>}

// AFTER (formatted automatically)
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    all_distinctions: HashMap<String, Distinction>,
    relationships: HashSet<Relationship>,
}
```

### How to Run
```bash
# Format all code
cargo fmt

# Check if code is formatted (for CI)
cargo fmt -- --check
```

---

## 📚 Layer 3: Developer Education (CONTRIBUTING.md)

**File**: `CONTRIBUTING.md` (273 lines)

### What It Does
This is your **team's Rust bible**. It teaches:

#### 1. The Five Sacred Axioms
Every developer understands that these are non-negotiable:
- Identity, Nontriviality, Synthesis, Symmetry, Irreflexivity

#### 2. Borrow Checker Mastery
**Pitfall 1: Holding References Across Mutations**
```rust
// ❌ BAD - Won't compile
let d0 = engine.d0();  // Immutable borrow
let result = engine.synthesize(d0, d0);  // Mutable borrow - ERROR!

// ✅ GOOD - Clone for independence
let d0 = engine.d0().clone();  // Owned value
let result = engine.synthesize(&d0, &d0);  // Works!
```

**Pitfall 2: Early Returns with Mutations**
```rust
// ❌ BAD - Borrow checker errors
pub fn synthesize(&mut self, a: &Distinction) -> &Distinction {
    if let Some(existing) = self.map.get(&id) {
        return existing;  // Immutable borrow
    }
    self.map.insert(...);  // Can't mutate! COMPILER ERROR
}

// ✅ GOOD - Clone the early return
pub fn synthesize(&mut self, a: &Distinction) -> Distinction {
    if let Some(existing) = self.map.get(&id) {
        return existing.clone();  // Owned value - no borrow
    }
    self.map.insert(...);  // Now we can mutate freely
}
```

#### 3. When to Clone Strategy
**Clone When:**
- Returning from HashMap/HashSet after mutation
- Caller needs independent copy
- Tests need isolated values

**Don't Clone When:**
- Just satisfying borrow checker (refactor instead!)
- Tight loops (use references)
- Large data (rethink design)

#### 4. Pre-Commit Checklist
Mandatory steps before every commit:
```bash
cargo fmt                          # Format code
cargo check                        # Quick compile check
cargo test                         # All tests pass
cargo clippy -- -D warnings       # Zero warnings
git grep "TODO\\|FIXME" src/      # Review TODOs
```

#### 5. Anti-Patterns to Avoid
```rust
// ❌ NEVER: unwrap() everywhere
let value = map.get(&key).unwrap();  // Will panic!

// ✅ ALWAYS: Handle errors properly
let value = map.get(&key).expect("key must exist by construction");

// ❌ NEVER: Mutable global state
static mut GLOBAL_ENGINE: Option<DistinctionEngine> = None;

// ✅ ALWAYS: Pass references
fn do_work(engine: &mut DistinctionEngine) { }
```

---

## 🤖 Layer 4: Continuous Integration (CI Pipeline)

**File**: `.github/workflows/ci.yml`

### What It Does
Runs automatically on every:
- Push to main
- Pull request
- Commit to any branch

### Four Mandatory Gates

#### Gate 1: Tests
```yaml
- name: Run tests
  run: cargo test --verbose
```
**Blocks**: Any PR where tests fail

#### Gate 2: Formatting
```yaml
- name: Check formatting
  run: cargo fmt -- --check
```
**Blocks**: Any PR with unformatted code

#### Gate 3: Clippy
```yaml
- name: Run clippy
  run: cargo clippy --all-targets -- -D warnings
```
**Blocks**: Any PR with warnings

#### Gate 4: Coverage (Optional)
```yaml
- name: Generate coverage
  run: cargo tarpaulin --out Xml
```
**Reports**: How much code is covered by tests

### Benefits
- **No bad code merges**: If it doesn't pass CI, it doesn't merge
- **Consistent quality**: Every commit meets the same standards
- **Fast feedback**: Developers know within minutes if something broke
- **Documentation**: CI badges show project health at a glance

---

## 🎯 How Guardrails Work Together

### Scenario: Junior Developer Makes a Change

1. **Developer writes code** with unnecessary complexity
   ```rust
   pub fn complex_function() {
       // 50 lines of nested ifs...
   }
   ```

2. **Pre-commit hook runs** (if installed)
   ```bash
   cargo fmt              # Auto-formats the code
   cargo clippy           # "Error: cognitive complexity too high"
   ```

3. **Developer checks CONTRIBUTING.md**
   - Reads: "Keep functions under 15 complexity"
   - Learns: "Break into smaller functions"
   - Refactors: Splits into 3 focused functions

4. **Developer commits, opens PR**

5. **CI runs automatically**
   - ✓ Tests pass
   - ✓ Formatting correct
   - ✓ Clippy happy
   - ✓ Coverage acceptable

6. **Code reviewer approves** (less to review, CI did the grunt work)

7. **Merges to main** with confidence

### Without Guardrails
1. Developer commits complex code
2. Reviewer has to manually check everything
3. Formatting inconsistencies slip through
4. Code quality degrades over time
5. Months later: "Why is this codebase so hard to maintain?"

---

## 🔧 Setting Up for Your Team

### 1. Install Pre-Commit Hook
Create `.git/hooks/pre-commit`:
```bash
#!/bin/bash
echo "Running pre-commit checks..."

# Format code automatically
cargo fmt

# Check for errors
echo "Checking for errors..."
cargo clippy --all-targets -- -D warnings || {
    echo "❌ Clippy found issues. Fix them before committing."
    exit 1
}

# Run tests
echo "Running tests..."
cargo test --quiet || {
    echo "❌ Tests failed. Fix them before committing."
    exit 1
}

echo "✅ All checks passed!"
```

Make it executable:
```bash
chmod +x .git/hooks/pre-commit
```

### 2. Require CI in GitHub Settings
1. Go to repo Settings → Branches
2. Add branch protection rule for `main`
3. Check "Require status checks to pass"
4. Select: Test, Format, Clippy

Now **nobody** can merge without passing all checks (not even admins!)

### 3. Add Status Badges to README
```markdown
[![CI](https://github.com/you/forma-core/actions/workflows/ci.yml/badge.svg)](https://github.com/you/forma-core/actions)
[![Clippy](https://github.com/you/forma-core/actions/workflows/ci.yml/badge.svg)](https://github.com/you/forma-core/actions)
```

Shows at a glance: "This project maintains high quality"

---

## 📈 Measuring Guardrail Effectiveness

Track these metrics:

### Code Quality Metrics
- **Clippy warnings over time**: Should trend to zero and stay there
- **Test coverage**: Should increase over time
- **Cognitive complexity**: Average should stay low

```bash
# Check clippy warnings
cargo clippy --all-targets 2>&1 | grep "warning:" | wc -l

# Check test coverage
cargo tarpaulin --out Stdout | grep "Coverage:"
```

### Process Metrics
- **PR review time**: Should decrease (CI does grunt work)
- **Bugs caught in CI vs production**: More in CI = guardrails working
- **Developer onboarding time**: CONTRIBUTING.md should speed this up

---

## 🚨 When Guardrails Trigger

### Example 1: Type Complexity
```bash
$ cargo clippy -- -D warnings
error: very complex type used. Consider factoring parts into `type` definitions
  --> src/engine.rs:106:41
   |
106| pub fn get_state_snapshot(&self) -> (Vec<&Distinction>, &HashSet<(String, String)>) {
   |                                     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^
```

**What happened**: Developer created complex return type
**Why it's blocked**: Hard to read and maintain
**Fix**: Use type alias (already done in our code!)

### Example 2: Missing Documentation
```bash
$ cargo clippy -- -W clippy::missing_docs_in_public_items
warning: missing documentation for a public function
  --> src/engine.rs:84:5
   |
84 |     pub fn synthesize(...) {
   |     ^^^^^^^^^^^^^^^^^^^^^^^^
```

**What happened**: Public function without docs
**Why it matters**: Public API must be documented
**Fix**: Add `///` doc comment

---

## 💡 Philosophy

> "Guardrails don't block creativity—they create freedom to innovate safely."

By catching errors early and automating quality checks:
- Developers focus on **solving problems**, not fixing mistakes
- Code reviews focus on **architecture**, not formatting
- The team moves **faster** because they trust the code
- New developers **ramp up quickly** with clear guidelines

---

## 🎓 Key Takeaways

1. **clippy.toml**: Automated code quality enforcement
2. **rustfmt.toml**: Consistent formatting, zero effort
3. **CONTRIBUTING.md**: Team knowledge base for Rust patterns
4. **CI pipeline**: Nothing bad gets merged
5. **Together**: A system that scales as your team grows

The guardrails are **already configured and working**. Your team just needs to:
1. Run `cargo clippy` and `cargo fmt` before committing
2. Read CONTRIBUTING.md when stuck
3. Let CI catch anything that slips through

**Your core is protected.** 🛡️
