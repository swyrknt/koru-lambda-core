use crate::{Distinction, DistinctionEngine};
use once_cell::sync::Lazy;
use std::collections::HashMap;

// --------------------------------------------------------------------------------
// BYTE CANONICALIZATION CACHE (O(N) -> O(1))
// --------------------------------------------------------------------------------

/// Internal function: Computes byte distinction using the original 8-step
/// binary path encoding logic (used only once to build the cache).
fn compute_byte_distinction_uncached(byte: u8, engine: &DistinctionEngine) -> Distinction {
    let mut current_d = engine.d0().clone();

    // MSB-first 8-step binary path encoding (8 synthesis operations)
    for i in (0..8).rev() {
        let bit = (byte >> i) & 1;
        let bit_d = if bit == 1 { engine.d1().clone() } else { engine.d0().clone() };
        current_d = engine.synthesize(&current_d, &bit_d);
    }

    current_d
}

/// Pre-computed cache of all 256 possible byte-to-distinction IDs.
/// Built once on first access using thread-safe lazy initialization.
static BYTE_DISTINCTION_CACHE: Lazy<HashMap<u8, String>> = Lazy::new(|| {
    // A temporary engine is created just for this deterministic computation.
    let engine = DistinctionEngine::new();
    let mut cache = HashMap::with_capacity(256);

    for byte in 0u8..=255 {
        let distinction = compute_byte_distinction_uncached(byte, &engine);
        // Store only the resulting ID string for cheap cloning.
        cache.insert(byte, distinction.id().to_string());
    }

    cache
});

// --------------------------------------------------------------------------------
// TRAIT & MAPPING
// --------------------------------------------------------------------------------

/// Trait for types that can be canonically represented as distinction structures.
/// This allows arbitrary data to be mapped into the distinction calculus.
///
/// Concurrency Note: Uses shared reference to DistinctionEngine since synthesis
/// no longer requires mutable access (interior mutability via DashMap).
pub trait Canonicalizable {
    /// Converts the data structure into its canonical distinction representation.
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction;
}

/// Utilities for mapping primitive data types to canonical distinction structures.
pub struct ByteMapping;

impl ByteMapping {
    /// Maps a byte to a canonical distinction using a pre-computed cache.
    ///
    /// The actual distinction is computed once during lazy initialization. Subsequent
    /// calls are O(1) lookups. The `_engine` parameter is kept for API compatibility.
    ///
    /// Thread-safe: Can be called concurrently from multiple threads.
    pub fn map_byte_to_distinction(byte: u8, _engine: &DistinctionEngine) -> Distinction {
        let id = BYTE_DISTINCTION_CACHE
            .get(&byte)
            .expect("BYTE_DISTINCTION_CACHE must contain all 256 bytes");

        Distinction::new(id.clone())
    }
}

/// Implement Canonicalizable for u8 using the byte mapping.
impl Canonicalizable for u8 {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        ByteMapping::map_byte_to_distinction(*self, engine)
    }
}
