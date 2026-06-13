use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// A distinction is the fundamental unit of the system.
///
/// # Representation
///
/// A `Distinction` is identified by its canonical 16-byte ID
/// (`bytes: [u8; 16]`), the first 16 bytes of `SHA256(min_parent || max_parent)`
/// (or `[0; 16]` for the primordial Δ₀ and `[1, 0, ..., 0]` for Δ₁).
///
/// During the foundation sub-branch (steps 3–6) a derived `id: String`
/// field is cached so `.id() -> &str` callers continue to compile. Step 6
/// removes the cached String and `.id()`; consumers use `as_bytes()` for
/// raw bytes or `to_hex()` for display.
#[derive(Clone, PartialEq, Eq, Hash)]
pub struct Distinction {
    /// Cached 32-character lowercase hex of `bytes`. Carried during
    /// steps 3–5 so the legacy `.id() -> &str` accessor keeps working;
    /// removed in step 6.
    id: String,
    /// Canonical 16-byte ID. Authoritative identity field. Step 3 made
    /// this the source of truth for `synthesize()`; step 6 removes the
    /// cached String above; step 7 makes this field `pub(crate)`.
    bytes: [u8; 16],
}

impl Distinction {
    /// Legacy constructor: parses the supplied String back into the
    /// 16-byte canonical representation. Step 7 removes this entirely
    /// (`Distinction::new` is dropped from the public surface).
    pub fn new(id: String) -> Self {
        let bytes = derive_bytes_from_legacy_id(&id);
        // Renormalize id to the canonical 32-char hex form to keep `id`
        // and `bytes` consistent.
        Self { id: hex::encode(bytes), bytes }
    }

    /// Accessor for the cached hex-string ID. Removed in step 6 of the
    /// foundation sub-branch.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the canonical 16-byte ID of this distinction.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8; 16] {
        &self.bytes
    }

    /// Internal constructor from raw bytes. The only callable Distinction
    /// constructor post step 7 (when `Distinction::new` is removed).
    pub(crate) fn from_bytes_internal(bytes: [u8; 16]) -> Self {
        Self { id: hex::encode(bytes), bytes }
    }
}

/// Derive the canonical 16-byte representation from a legacy String id.
///
/// Only invoked by the soon-to-be-removed `Distinction::new(String)` (step 7).
///
/// Handles:
/// - Primordials `"0"` → `[0; 16]`, `"1"` → `[1, 0, ..., 0]`
/// - 32-char hex → decode directly (post-step-3 ids)
/// - 64-char SHA256 hex → first 16 bytes (pre-step-3 ids)
/// - Anything else → SHA256 the string and take first 16 bytes (covers
///   foreign IDs minted via `Distinction::new` until step 7 closes that
///   surface structurally)
fn derive_bytes_from_legacy_id(id: &str) -> [u8; 16] {
    match id {
        "0" => return [0u8; 16],
        "1" => {
            let mut b = [0u8; 16];
            b[0] = 1;
            return b;
        },
        _ => {},
    }
    if id.len() == 32 {
        if let Ok(bytes) = hex::decode(id) {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes);
            return arr;
        }
    }
    if id.len() == 64 {
        if let Ok(bytes) = hex::decode(id) {
            let mut arr = [0u8; 16];
            arr.copy_from_slice(&bytes[..16]);
            return arr;
        }
    }
    // Fallback: hash whatever was supplied. Covers foreign-ID inputs
    // (Exp 9) that step 7 closes structurally.
    let digest = Sha256::digest(id.as_bytes());
    let mut arr = [0u8; 16];
    arr.copy_from_slice(&digest[..16]);
    arr
}

/// Type alias for a canonical relationship between two distinctions.
pub type Relationship = (String, String);

/// Type alias for a complete state snapshot.
pub type StateSnapshot = (Vec<Distinction>, Vec<Relationship>);

/// The core engine implementing the five axioms of distinction calculus:
/// 1. Identity: A distinction is defined solely by its unique identifier
/// 2. Nontriviality: The system initializes with two primordial distinctions (Δ₀, Δ₁)
/// 3. Synthesis: Two distinctions combine deterministically to create a third
/// 4. Symmetry: Relationships are bidirectional and order-independent
/// 5. Irreflexivity: A distinction synthesized with itself yields itself
///
/// Concurrency Model: Uses interior mutability via DashMap to allow concurrent
/// synthesis operations from multiple threads. The engine can be safely shared via
/// Arc<DistinctionEngine> across threads without requiring mutable access.
#[derive(Debug)]
pub struct DistinctionEngine {
    d0: Distinction,
    d1: Distinction,
    all_distinctions: DashMap<String, Distinction>,
    relationships: DashMap<Relationship, ()>,
}

impl DistinctionEngine {
    /// Creates a new engine with the two primordial distinctions.
    pub fn new() -> Self {
        let mut d1_bytes = [0u8; 16];
        d1_bytes[0] = 1;
        let d0 = Distinction::from_bytes_internal([0u8; 16]);
        let d1 = Distinction::from_bytes_internal(d1_bytes);

        let all_distinctions = DashMap::new();
        all_distinctions.insert(d0.id.clone(), d0.clone());
        all_distinctions.insert(d1.id.clone(), d1.clone());

        let relationships = DashMap::new();
        relationships.insert((d0.id.clone(), d1.id.clone()), ());

        Self { d0, d1, all_distinctions, relationships }
    }

    /// Returns a reference to the first primordial distinction (Δ₀).
    pub fn d0(&self) -> &Distinction {
        &self.d0
    }

    /// Returns a reference to the second primordial distinction (Δ₁).
    pub fn d1(&self) -> &Distinction {
        &self.d1
    }

    /// Retrieves a cloned Distinction by its unique ID.
    /// O(1) complexity via internal DashMap lookup.
    ///
    /// Returns None if the distinction doesn't exist.
    ///
    /// Thread-safe: Can be called concurrently from multiple threads.
    pub fn get_distinction_by_id(&self, id: &str) -> Option<Distinction> {
        self.all_distinctions.get(id).map(|entry| entry.value().clone())
    }

    /// Adds a canonical relationship between two distinctions.
    ///
    /// Thread-safe via DashMap interior mutability.
    fn add_relationship(&self, id_a: &str, id_b: &str) {
        let (min, max) = if id_a < id_b { (id_a, id_b) } else { (id_b, id_a) };
        self.relationships.insert((min.to_string(), max.to_string()), ());
    }

    /// Synthesizes two distinctions to create a third.
    /// Returns the resulting distinction.
    ///
    /// Implements:
    /// - Irreflexivity - synthesize(a, a) = a
    /// - Symmetry - synthesize(a, b) = synthesize(b, a)
    /// - Timeless consistency - repeated synthesis yields the same result
    ///
    /// Concurrency: This method is thread-safe and can be called concurrently
    /// from multiple threads without requiring mutable access to the engine.
    /// Uses DashMap for lock-free concurrent access.
    pub fn synthesize(&self, a: &Distinction, b: &Distinction) -> Distinction {
        // Irreflexivity - a distinction synthesized with itself yields itself
        if a.bytes == b.bytes {
            return a.clone();
        }

        // Symmetry - canonical ordering on the 16-byte ID ensures order
        // independence. Byte comparison; no String allocation in the hot path.
        let (first, second) =
            if a.bytes <= b.bytes { (&a.bytes, &b.bytes) } else { (&b.bytes, &a.bytes) };

        // Content addressing: SHA256(min || max), truncated to first 16 bytes.
        // Per Exp 13 (2026): 0 collisions in 268M synths at 16-byte truncation.
        let mut hasher = Sha256::new();
        hasher.update(first);
        hasher.update(second);
        let digest = hasher.finalize();
        let mut new_bytes = [0u8; 16];
        new_bytes.copy_from_slice(&digest[..16]);

        let new_id_hex = hex::encode(new_bytes);

        // Return existing if already synthesized (timeless consistency).
        // DashMap key is still the hex String; step 4 switches to bytes.
        if let Some(existing) = self.all_distinctions.get(&new_id_hex) {
            return existing.clone();
        }

        // Create new distinction and establish relationships.
        // DashMap handles concurrent insertion safely.
        let new_distinction = Distinction::from_bytes_internal(new_bytes);
        self.all_distinctions.insert(new_id_hex.clone(), new_distinction.clone());
        self.add_relationship(&new_id_hex, &a.id);
        self.add_relationship(&new_id_hex, &b.id);

        new_distinction
    }

    /// Returns a snapshot of all distinctions as a Vec.
    ///
    /// Note: In production, prefer iterating over distinctions directly
    /// rather than creating snapshots, to avoid collecting all values.
    pub fn get_distinctions_snapshot(&self) -> Vec<Distinction> {
        self.all_distinctions.iter().map(|entry| entry.value().clone()).collect()
    }

    /// Returns a snapshot of all relationships as a Vec.
    ///
    /// Note: In production, prefer iterating over relationships directly
    /// rather than creating snapshots.
    pub fn get_relationships_snapshot(&self) -> Vec<Relationship> {
        self.relationships.iter().map(|entry| entry.key().clone()).collect()
    }

    /// Returns a complete state snapshot.
    ///
    /// # Tearing semantics
    ///
    /// This method performs **two independent DashMap iterations** (one for
    /// distinctions, one for relationships) with no synchronization barrier
    /// between them. Under concurrent writes, the two halves are NOT consistent
    /// with each other: a distinction may appear in the first half while its
    /// relationships have not yet been written, or relationships may reference
    /// a distinction not present in the first half.
    ///
    /// Measured tearing rate: rare (~0.1% under 8 concurrent writers) but
    /// avalanche-sized when it occurs — when a tear happens, thousands of
    /// novel syntheses can land between the two iterations (Exp 6, 2026).
    ///
    /// # When to use
    ///
    /// - Quiescent reads (no concurrent writers): consistent.
    /// - Diagnostic / observability use during writes: tolerable.
    /// - **Persistence under load: NOT SAFE.** A future `get_state_snapshot_quiesced`
    ///   API (gated on a write barrier) will be added if and when a concrete
    ///   persistence consumer needs it.
    ///
    /// The suffix `_unsynchronized` is intentional: every caller acknowledges
    /// at the call site that the snapshot is not atomic across the two halves.
    pub fn get_state_snapshot_unsynchronized(&self) -> StateSnapshot {
        (self.get_distinctions_snapshot(), self.get_relationships_snapshot())
    }

    /// Returns the total number of distinctions in the engine.
    pub fn distinction_count(&self) -> usize {
        self.all_distinctions.len()
    }

    /// Returns the total number of relationships tracked.
    pub fn relationship_count(&self) -> usize {
        self.relationships.len()
    }
}

impl Default for DistinctionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to create an Arc-wrapped engine for concurrent access.
///
/// This is the recommended way to share a DistinctionEngine across threads
/// in production environments.
///
/// Example:
/// ```
/// use std::sync::Arc;
/// use koru_lambda_core::DistinctionEngine;
///
/// let engine = Arc::new(DistinctionEngine::new());
/// // Clone Arc for each thread
/// let engine_clone = Arc::clone(&engine);
/// ```
impl DistinctionEngine {
    pub fn new_shared() -> Arc<Self> {
        Arc::new(Self::new())
    }
}
