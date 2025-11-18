use dashmap::DashMap;
use sha2::{Digest, Sha256};
use std::sync::Arc;

/// A distinction is the fundamental unit of the system.
/// A distinction is defined solely by its unique identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Distinction {
    id: String,
}

impl Distinction {
    pub fn new(id: String) -> Self {
        Self { id }
    }

    pub fn id(&self) -> &str {
        &self.id
    }
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
        let d0 = Distinction::new("0".to_string());
        let d1 = Distinction::new("1".to_string());

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
        if a.id == b.id {
            return a.clone();
        }

        // Symmetry - canonical ordering ensures order independence
        let (first, second) = if a.id < b.id {
            (a.id.as_str(), b.id.as_str())
        } else {
            (b.id.as_str(), a.id.as_str())
        };

        // Deterministic synthesis using SHA256 for content-addressable structure
        let new_id_str = format!("{}:{}", first, second);
        let new_id = format!("{:x}", Sha256::digest(new_id_str.as_bytes()));

        // Return existing if already synthesized (timeless consistency)
        if let Some(existing) = self.all_distinctions.get(&new_id) {
            return existing.clone();
        }

        // Create new distinction and establish relationships
        // Note: DashMap handles concurrent insertion safely
        let new_distinction = Distinction::new(new_id.clone());
        self.all_distinctions.insert(new_id.clone(), new_distinction.clone());
        self.add_relationship(&new_id, &a.id);
        self.add_relationship(&new_id, &b.id);

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

    /// Returns a complete state snapshot (for compatibility with existing tests).
    ///
    /// Warning: This method is less efficient than the individual snapshot
    /// methods as it allocates temporary vectors. Consider using direct iteration
    /// in performance-critical code.
    pub fn get_state_snapshot(&self) -> StateSnapshot {
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
