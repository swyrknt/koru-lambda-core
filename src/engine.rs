//! Substrate engine — the [`Distinction`] type, [`IdentityHasher`], and
//! (in sub-milestone 1b) the [`DistinctionEngine`] itself.
//!
//! This is the heart of the crate. See `THEORY.md` for the axioms it
//! enforces and `ARCHITECTURE.md` for the three-projection engine state.

use std::hash::{BuildHasherDefault, Hasher};

/// A distinction — the unique kind of thing the substrate talks about.
///
/// A distinction is identified solely by its 16-byte content-addressed
/// identity. There is no internal structure beyond these bytes.
///
/// Constructed exclusively by [`DistinctionEngine`]: either by retrieving
/// a primordial (`engine.d0()`, `engine.d1()`), by synthesis
/// (`engine.synthesize(a, b)`), or by parsing bytes the engine has
/// already registered ([`Distinction::from_hex`] of a known ID).
///
/// The field is `pub(crate)` — there is no public constructor. Foreign-byte
/// injection is closed structurally: external code cannot mint a
/// `Distinction` that didn't originate from a `DistinctionEngine`. The
/// only exception is [`Distinction::from_hex`], which parses bytes
/// without validating engine membership; passing such a value to
/// `engine.synthesize` triggers a debug-build panic via the foreign-byte
/// guard.
///
/// `#[repr(transparent)]`: layout-compatible with `[u8; 16]`, enabling
/// zero-copy FFI/WASM transit (`*const Distinction` ↔ `*const [u8; 16]`).
///
/// # Safety justification for `bytemuck::Pod` and `bytemuck::Zeroable`
///
/// `Pod` requires the type to have no padding bytes and all bit patterns
/// to be valid. `[u8; 16]` has no padding (it's just 16 bytes), all 2^128
/// bit patterns are valid (any byte sequence is a syntactically valid
/// Distinction identity — the engine is what decides which ones are
/// "registered"), and `#[repr(transparent)]` preserves the inner array's
/// layout guarantees. Both invariants hold.
///
/// `Zeroable` requires that the all-zero bit pattern is a valid value of
/// the type. `[0u8; 16]` is the primordial `d0`'s identity — a valid
/// distinction by definition.
///
/// [`DistinctionEngine`]: crate::DistinctionEngine
/// [`Distinction::from_hex`]: crate::Distinction::from_hex
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, bytemuck::Pod, bytemuck::Zeroable)]
#[repr(transparent)]
pub struct Distinction(pub(crate) [u8; 16]);

impl Distinction {
    /// Construct a `Distinction` directly from bytes, without any engine
    /// registration check.
    ///
    /// **Crate-internal** — used inside the substrate by
    /// [`from_hex`](Distinction::from_hex) and by the engine itself.
    /// External crates cannot reach this constructor.
    #[must_use]
    pub(crate) const fn from_bytes_unchecked(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Borrow the underlying 16 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

// ---------------------------------------------------------------------------
// IdentityHasher
// ---------------------------------------------------------------------------

/// Hasher used by the substrate's [`DashMap`]s for byte-keyed lookups.
///
/// SHA-256 prefixes are uniformly distributed. The hasher returns the
/// leading 8 bytes of the 16-byte key as a `u64`. No XOR, no rotation,
/// no diffusion math — the input is already uniform, so adding
/// "scrambling" would only cost cycles.
///
/// # Misuse detection
///
/// The substrate uses this hasher exclusively for 16-byte keys (raw
/// distinction identities). Any call to `write_u8` / `write_u16` / etc.
/// triggers `unreachable!()`: those code paths exist only to satisfy
/// the [`Hasher`] trait, and the substrate has no business calling
/// them. The `debug_assert_eq!` on `write` catches non-16-byte slices in
/// debug builds.
///
/// In release builds, the assertion is compiled out; if a non-16-byte
/// slice somehow reaches `write`, only the first 8 bytes are read,
/// which still produces a valid `u64` — just one not derived from the
/// expected 16-byte key. This is the substrate's contract: callers must
/// only hash 16-byte keys.
///
/// [`DashMap`]: dashmap::DashMap
#[derive(Default)]
pub struct IdentityHasher {
    state: u64,
}

impl Hasher for IdentityHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.state
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        debug_assert_eq!(bytes.len(), 16, "IdentityHasher only handles 16-byte keys (invariant)");
        // Leading 8 bytes of the 16-byte key.
        // `[..8].try_into()` is fallible only if the slice is shorter
        // than 8 bytes — which the debug_assert above rules out in
        // debug, and which the substrate contract rules out in release.
        let prefix: [u8; 8] =
            bytes[..8].try_into().expect("IdentityHasher requires ≥8 bytes (invariant)");
        self.state = u64::from_le_bytes(prefix);
    }

    fn write_u8(&mut self, _: u8) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u16(&mut self, _: u16) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u32(&mut self, _: u32) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u64(&mut self, _: u64) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_u128(&mut self, _: u128) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_usize(&mut self, _: usize) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i8(&mut self, _: i8) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i16(&mut self, _: i16) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i32(&mut self, _: i32) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i64(&mut self, _: i64) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_i128(&mut self, _: i128) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    fn write_isize(&mut self, _: isize) {
        unreachable!("IdentityHasher only handles 16-byte keys via write()")
    }
    // `write_length_prefix` is unstable (issue #96762); intentionally
    // omitted. If a caller hashes a slice via the default impl, that path
    // routes through `write_usize` (above, unreachable) and `write`
    // (length-checked), which both guard against misuse.
}

/// `BuildHasher` flavor of [`IdentityHasher`], used as the hash builder
/// for the engine's [`DashMap`]s.
///
/// [`DashMap`]: dashmap::DashMap
pub type IdentityBuildHasher = BuildHasherDefault<IdentityHasher>;

// ---------------------------------------------------------------------------
// Placeholder for DistinctionEngine — implemented in sub-milestone 1b
// ---------------------------------------------------------------------------

/// The substrate engine.
///
/// Implemented in sub-milestone 1b. Will hold the three canonical
/// projections (`all_distinctions`, `parents_of`, `degree_counts`) and
/// the entry-gated [`synthesize`](DistinctionEngine::synthesize) hot
/// path enforcing the four axioms.
///
/// This is an empty stub so the crate compiles for sub-milestone 1a.
pub struct DistinctionEngine {
    _private: (),
}

impl DistinctionEngine {
    /// Construct a placeholder engine. Real construction lands in
    /// sub-milestone 1b.
    #[must_use]
    pub const fn new() -> Self {
        Self { _private: () }
    }
}

impl Default for DistinctionEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Compile-time assertions on Distinction
// ---------------------------------------------------------------------------

#[cfg(test)]
mod compile_time_assertions {
    use super::*;
    use static_assertions::{assert_eq_align, assert_eq_size, assert_impl_all};

    // `Distinction` is the size and alignment of a `[u8; 16]` — the
    // whole point of `#[repr(transparent)]`.
    assert_eq_size!(Distinction, [u8; 16]);
    assert_eq_align!(Distinction, [u8; 16]);

    // Threading safety — `Distinction` is `Copy + Send + Sync`.
    assert_impl_all!(Distinction: Copy, Send, Sync);

    // bytemuck plain-old-data + zeroable — enables zero-copy slice views.
    assert_impl_all!(Distinction: bytemuck::Pod, bytemuck::Zeroable);

    fn _assert_send_sync<T: Send + Sync>() {}

    #[test]
    fn distinction_send_sync() {
        _assert_send_sync::<Distinction>();
    }

    #[test]
    fn distinction_size_align() {
        assert_eq!(std::mem::size_of::<Distinction>(), 16);
        assert_eq!(std::mem::align_of::<Distinction>(), 1);
    }
}

#[cfg(test)]
mod identity_hasher_tests {
    use super::IdentityHasher;
    use std::hash::Hasher as _;

    #[test]
    fn reads_leading_8_bytes_as_u64_le() {
        let mut h = IdentityHasher::default();
        let bytes = [
            0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff,
            0xff, 0xff,
        ];
        h.write(&bytes);
        let expected = u64::from_le_bytes([0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08]);
        assert_eq!(h.finish(), expected);
    }

    #[test]
    fn distinct_prefixes_give_distinct_hashes() {
        let mut h1 = IdentityHasher::default();
        let mut h2 = IdentityHasher::default();
        let bytes1 = [0u8; 16];
        let mut bytes2 = [0u8; 16];
        bytes2[0] = 1;
        h1.write(&bytes1);
        h2.write(&bytes2);
        assert_ne!(h1.finish(), h2.finish());
    }

    #[test]
    fn million_distinct_inputs_million_distinct_hashes() {
        use std::collections::HashSet;
        let mut seen = HashSet::with_capacity(1_000_000);
        let mut state = 0xdead_beef_cafe_babe_u64;
        for _ in 0..1_000_000 {
            // Cheap LCG to generate "random" bytes deterministically.
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut full = [0u8; 16];
            full[..8].copy_from_slice(&state.to_le_bytes());
            full[8..].copy_from_slice(&state.swap_bytes().to_le_bytes());
            let mut h = IdentityHasher::default();
            h.write(&full);
            seen.insert(h.finish());
        }
        // Distinct LCG outputs in u64 space ⟹ distinct leading-8-byte
        // prefixes ⟹ distinct hashes. Exact equality verifies the hasher
        // drops zero information from the prefix.
        assert_eq!(seen.len(), 1_000_000);
    }

    #[cfg(debug_assertions)]
    #[test]
    #[should_panic(expected = "IdentityHasher only handles 16-byte keys")]
    fn panics_on_short_slice_in_debug() {
        let mut h = IdentityHasher::default();
        h.write(&[0u8; 8]);
    }
}
