use crate::{Distinction, DistinctionEngine};

// --------------------------------------------------------------------------------
// BYTE CANONICALIZATION
// --------------------------------------------------------------------------------

/// Computes a byte's canonical distinction by folding the byte's 8 bits MSB-first
/// through the engine. Each bit selects d0 (bit=0) or d1 (bit=1) as the `bit_d`
/// operand; the accumulator starts at `engine.d0()`.
///
/// All 8 intermediate syntheses are performed against `engine`, registering the
/// full chain (8 distinctions, 16 relationships per novel byte) in the calling
/// engine's `all_distinctions` and relationship set.
///
/// `synthesize` is idempotent, so repeat calls for the same byte against the
/// same engine are fast (DashMap hits on the cached intermediate IDs); only the
/// first call for a given byte pays the 8-step SHA256 cost.
fn fold_byte_into_engine(byte: u8, engine: &DistinctionEngine) -> Distinction {
    let mut current_d = engine.d0().clone();

    // MSB-first 8-step binary path encoding (8 synthesis operations)
    for i in (0..8).rev() {
        let bit = (byte >> i) & 1;
        let bit_d = if bit == 1 { engine.d1().clone() } else { engine.d0().clone() };
        current_d = engine.synthesize(&current_d, &bit_d);
    }

    current_d
}

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
    /// Maps a byte to its canonical distinction in the given engine.
    ///
    /// Performs the MSB-first 8-step binary path encoding into `engine`,
    /// registering every intermediate distinction in `engine.all_distinctions`
    /// and adding the corresponding relationships. Subsequent calls for the same
    /// byte against the same engine are fast (idempotent `synthesize` short-circuits
    /// via DashMap lookup).
    ///
    /// # History
    ///
    /// Prior to this fix (Phase 3, CHECKLIST 1.1 #1), `ByteMapping` cached byte
    /// IDs against a throwaway engine and returned them as `Distinction::new(id)`,
    /// leaving the 8-step chain unregistered in the calling engine. This produced
    /// phantom parents in the relationship set and violated `r = 2d - 3`
    /// semantically (Exp 5, qa-sentinel). Phase 1.5 demonstrated that running
    /// the validator over a 256-byte payload produced 259 distinctions but 512
    /// unique parent IDs in relationships, leaving 253 phantoms
    /// (`run_log/exp_validator_audit.log` E).
    ///
    /// Thread-safe: can be called concurrently from multiple threads. The engine's
    /// DashMap handles concurrent inserts; `synthesize`'s idempotency means
    /// duplicate concurrent calls converge on the same Distinction without
    /// inflating distinction or relationship counts.
    pub fn map_byte_to_distinction(byte: u8, engine: &DistinctionEngine) -> Distinction {
        fold_byte_into_engine(byte, engine)
    }
}

/// Implement Canonicalizable for u8 using the byte mapping.
impl Canonicalizable for u8 {
    fn to_canonical_structure(&self, engine: &DistinctionEngine) -> Distinction {
        ByteMapping::map_byte_to_distinction(*self, engine)
    }
}
