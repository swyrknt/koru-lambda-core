use crate::{Distinction, DistinctionEngine};

/// Trait for types that can be canonically represented as distinction structures.
/// This allows arbitrary data to be mapped into the distinction calculus.
///
/// **Concurrency Note**: Uses shared reference to DistinctionEngine since synthesis
/// no longer requires mutable access (interior mutability via DashMap).
pub trait Canonicalizable {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction;
}

/// Utilities for mapping primitive data types to canonical distinction structures.
pub struct ByteMapping;

impl ByteMapping {
    /// Maps a byte to a canonical distinction using MSB-first 8-step binary path encoding.
    ///
    /// Each bit of the byte is encoded as a path through the distinction space,
    /// starting from Δ₀ and synthesizing with either Δ₀ (for 0) or Δ₁ (for 1) at each step.
    ///
    /// This creates a deterministic, content-addressable representation of the byte.
    ///
    /// **Thread-safe**: Can be called concurrently from multiple threads.
    pub fn map_byte_to_distinction(byte: u8, engine: &DistinctionEngine) -> Distinction {
        let mut current_d = engine.d0().clone();

        // MSB-first 8-step binary path encoding
        for i in (0..8).rev() {
            let bit = (byte >> i) & 1;
            let bit_d = if bit == 1 {
                engine.d1().clone()
            } else {
                engine.d0().clone()
            };

            current_d = engine.synthesize(&current_d, &bit_d);
        }

        current_d
    }
}

/// Implement Canonicalizable for u8 using the byte mapping.
impl Canonicalizable for u8 {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        ByteMapping::map_byte_to_distinction(*self, engine)
    }
}
