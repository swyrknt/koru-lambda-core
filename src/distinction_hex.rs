//! Hex display boundary for [`Distinction`] and [`RawDistinctionId`].
//!
//! The substrate is byte-canonical: every distinction's identity is the
//! 16-byte SHA-256 prefix of its canonical parent pair. Hex is the
//! human-readable rendering of those bytes, used for logs, URLs, JSON
//! serialization, and debug output.
//!
//! Hex is NOT theory. It's a display format. Theory says identity is bytes;
//! `to_hex` / `from_hex` are how we put bytes on a page or wire.
//!
//! This module is the ONLY hex-aware part of the substrate. Engine logic
//! operates on bytes exclusively.
//!
//! # Two hex constructors
//!
//! Both [`Distinction::from_hex`] and [`RawDistinctionId::from_hex`]
//! parse the same 32-character lowercase-hex grammar and produce the
//! same 16 bytes. They differ only in what the returned type
//! *claims about engine membership*:
//!
//! - [`RawDistinctionId::from_hex`] — the recommended trust-boundary
//!   path. The returned `RawDistinctionId` is bytes with a type name.
//!   Pass it through [`DistinctionEngine::verify`] to obtain an
//!   engine-witnessed [`Distinction`].
//! - [`Distinction::from_hex`] — a legacy hatch. The returned
//!   `Distinction` is unverified — the parse validated length and
//!   charset, not engine membership. Consumers who use this path assume
//!   the obligation `verify` would otherwise discharge.
//!
//! Under the hood, both route through a single crate-internal helper
//! ([`decode_hex_16`]) so the [`ParseError`] surface stays uniform.
//!
//! [`Distinction`]: crate::Distinction
//! [`RawDistinctionId`]: crate::RawDistinctionId
//! [`DistinctionEngine::verify`]: crate::DistinctionEngine::verify

use crate::engine::RawDistinctionId;
use crate::Distinction;
use std::fmt;
use std::str::FromStr;

/// Errors produced by [`Distinction::from_hex`].
///
/// `#[non_exhaustive]`: future variants will not break consumer semver.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum ParseError {
    /// Input length was not exactly 32 ASCII hex characters.
    #[error("expected 32 hex characters, got {0}")]
    InvalidLength(usize),

    /// Input contained a character outside `[0-9a-f]`.
    ///
    /// Uppercase `[A-F]` is intentionally rejected — the canonical form is
    /// lowercase. Permitting uppercase would mean two strings parse to
    /// the same bytes, which violates the bijection `to_hex` / `from_hex`
    /// callers rely on for content-addressed identity.
    #[error("invalid hex character at position {position}: {found:?}")]
    InvalidChar {
        /// Byte position of the offending character.
        position: usize,
        /// The character found.
        found: char,
    },

    /// Input contained non-ASCII bytes.
    #[error("non-ASCII byte at position {position}")]
    NonAscii {
        /// Byte position of the non-ASCII byte.
        position: usize,
    },
}

impl Distinction {
    /// Parse a 32-character lowercase-hex string into a [`Distinction`].
    ///
    /// **Trust boundary note.** The bytes are validated for length and
    /// charset, but **not** for engine membership. A `Distinction`
    /// returned by `from_hex` is unverified — call
    /// [`DistinctionEngine::has`](crate::DistinctionEngine::has) before
    /// use, or prefer the two-type path:
    /// [`RawDistinctionId::from_hex`] followed by
    /// [`DistinctionEngine::verify`](crate::DistinctionEngine::verify).
    /// The latter is the recommended path per the crate's
    /// `## Trust boundaries` framing.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidLength`] if the input is not exactly
    /// 32 ASCII characters, [`ParseError::NonAscii`] for non-ASCII bytes,
    /// or [`ParseError::InvalidChar`] for characters outside `[0-9a-f]`
    /// (uppercase is explicitly rejected).
    pub fn from_hex(s: &str) -> Result<Self, ParseError> {
        // Route through `RawDistinctionId::from_hex` so the two parallel
        // constructors share a single parse path and error surface. The
        // resulting bytes are wrapped in a `Distinction` directly (no
        // engine verification) — that matches this constructor's
        // pre-S04 semantics and preserves the intentional hatch
        // documented in the trust-boundaries block.
        RawDistinctionId::from_hex(s).map(|r| Distinction::from_bytes_unchecked(r.0))
    }

    /// Render this distinction as a 32-character lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
}

impl RawDistinctionId {
    /// Parse a 32-character lowercase-hex string into a
    /// [`RawDistinctionId`].
    ///
    /// The bytes are validated for length and charset. Because
    /// `RawDistinctionId` makes no engine-membership claim by
    /// construction, this constructor is the syntactic-parse-only entry
    /// point at the trust boundary: pass the returned value through
    /// [`DistinctionEngine::verify`](crate::DistinctionEngine::verify)
    /// to obtain an engine-witnessed [`Distinction`].
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidLength`] if the input is not exactly
    /// 32 ASCII characters, [`ParseError::NonAscii`] for non-ASCII bytes,
    /// or [`ParseError::InvalidChar`] for characters outside `[0-9a-f]`
    /// (uppercase is explicitly rejected).
    pub fn from_hex(s: &str) -> Result<Self, ParseError> {
        decode_hex_16(s).map(RawDistinctionId::from_bytes)
    }

    /// Render these bytes as a 32-character lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
}

/// Decode a 32-character lowercase-hex string into 16 raw bytes.
///
/// The shared parse kernel behind [`Distinction::from_hex`] and
/// [`RawDistinctionId::from_hex`]. Extracting the helper collapses ~15
/// lines of duplication and guarantees a single [`ParseError`] surface
/// across both constructors.
///
/// `pub(crate)` because the two public entry points are the intended
/// consumer surface — external callers should pick a return type
/// (verified `Distinction` or unverified `RawDistinctionId`) rather than
/// working with a bare `[u8; 16]`.
pub(crate) fn decode_hex_16(s: &str) -> Result<[u8; 16], ParseError> {
    if s.len() != 32 {
        return Err(ParseError::InvalidLength(s.len()));
    }

    let bytes = s.as_bytes();
    let mut out = [0u8; 16];

    for (i, out_byte) in out.iter_mut().enumerate() {
        let hi_pos = i * 2;
        let lo_pos = hi_pos + 1;
        let hi = decode_nibble(bytes[hi_pos], hi_pos)?;
        let lo = decode_nibble(bytes[lo_pos], lo_pos)?;
        *out_byte = (hi << 4) | lo;
    }

    Ok(out)
}

/// Decode one ASCII byte to its nibble value, with position-aware error.
const fn decode_nibble(byte: u8, position: usize) -> Result<u8, ParseError> {
    if !byte.is_ascii() {
        return Err(ParseError::NonAscii { position });
    }
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(ParseError::InvalidChar { position, found: byte as char }),
    }
}

impl fmt::Display for Distinction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for Distinction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Distinction({})", self.to_hex())
    }
}

impl FromStr for Distinction {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

impl fmt::Display for RawDistinctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}

impl fmt::Debug for RawDistinctionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "RawDistinctionId({})", self.to_hex())
    }
}

impl FromStr for RawDistinctionId {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::from_hex(s)
    }
}

/// Serde adapter — serialize/deserialize a [`Distinction`] as its hex string.
///
/// Use with `#[serde(with = "koru_lambda_core::distinction_hex::serde_adapter")]`
/// on any field of type `Distinction`:
///
/// ```
/// use koru_lambda_core::Distinction;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct TransactionBatch {
///     #[serde(with = "koru_lambda_core::distinction_hex::serde_adapter")]
///     previous_root: Distinction,
/// }
/// ```
pub mod serde_adapter {
    use super::Distinction;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Return type of [`serialize`] — factored out to keep the signature
    /// below the `type-complexity-threshold` set in `clippy.toml`.
    type SerializeResult<S> = Result<<S as Serializer>::Ok, <S as Serializer>::Error>;

    /// Serialize a [`Distinction`] as its 32-character lowercase hex string.
    ///
    /// # Errors
    ///
    /// Propagates any error from the serializer.
    pub fn serialize<S>(d: &Distinction, serializer: S) -> SerializeResult<S>
    where
        S: Serializer,
    {
        serializer.serialize_str(&d.to_hex())
    }

    /// Deserialize a [`Distinction`] from a hex string.
    ///
    /// # Errors
    ///
    /// Returns a deserializer error if the input is not valid 32-character
    /// lowercase hex.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Distinction, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        Distinction::from_hex(s).map_err(serde::de::Error::custom)
    }
}

/// Serde adapter — serialize/deserialize a [`RawDistinctionId`] as its
/// hex string.
///
/// Parallel to [`serde_adapter`] for consumers whose wire schema carries
/// unverified `RawDistinctionId`s (the recommended shape at the trust
/// boundary — see the crate `## Trust boundaries` block). Use with
/// `#[serde(with = "koru_lambda_core::distinction_hex::raw_serde_adapter")]`
/// on any field of type `RawDistinctionId`:
///
/// ```
/// use koru_lambda_core::RawDistinctionId;
/// use serde::{Deserialize, Serialize};
///
/// #[derive(Serialize, Deserialize)]
/// struct Envelope {
///     #[serde(with = "koru_lambda_core::distinction_hex::raw_serde_adapter")]
///     claimed_root: RawDistinctionId,
/// }
///
/// // At the trust boundary in your handler:
/// // let root = engine.verify(envelope.claimed_root).map_err(MyError::from)?;
/// ```
///
/// Deserialization performs only syntactic validation (length +
/// charset). Consumers MUST route the returned `RawDistinctionId`
/// through [`DistinctionEngine::verify`](crate::DistinctionEngine::verify)
/// before treating it as engine-witnessed.
pub mod raw_serde_adapter {
    use super::RawDistinctionId;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Return type of [`serialize`] — factored out to keep the signature
    /// below the `type-complexity-threshold` set in `clippy.toml`.
    type SerializeResult<S> = Result<<S as Serializer>::Ok, <S as Serializer>::Error>;

    /// Serialize a [`RawDistinctionId`] as its 32-character lowercase
    /// hex string.
    ///
    /// # Errors
    ///
    /// Propagates any error from the serializer.
    pub fn serialize<S>(r: &RawDistinctionId, serializer: S) -> SerializeResult<S>
    where
        S: Serializer,
    {
        serializer.serialize_str(&r.to_hex())
    }

    /// Deserialize a [`RawDistinctionId`] from a hex string.
    ///
    /// **Trust boundary reminder.** The returned value is unverified —
    /// only length and charset were checked. Route through
    /// [`DistinctionEngine::verify`](crate::DistinctionEngine::verify)
    /// before use.
    ///
    /// # Errors
    ///
    /// Returns a deserializer error if the input is not valid 32-character
    /// lowercase hex.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<RawDistinctionId, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = <&str>::deserialize(deserializer)?;
        RawDistinctionId::from_hex(s).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex_round_trip_known_value() {
        // All-zero (d0)
        let d = Distinction::from_bytes_unchecked([0u8; 16]);
        let hex = d.to_hex();
        assert_eq!(hex, "00000000000000000000000000000000");
        let parsed = Distinction::from_hex(&hex).expect("valid hex round-trips (invariant)");
        assert_eq!(parsed, d);
    }

    #[test]
    fn hex_round_trip_all_bits() {
        let d = Distinction::from_bytes_unchecked([0xFFu8; 16]);
        let hex = d.to_hex();
        assert_eq!(hex, "ffffffffffffffffffffffffffffffff");
        let parsed = Distinction::from_hex(&hex).expect("valid hex round-trips (invariant)");
        assert_eq!(parsed, d);
    }

    #[test]
    fn hex_round_trip_mixed_bytes() {
        let bytes = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        let d = Distinction::from_bytes_unchecked(bytes);
        let hex = d.to_hex();
        assert_eq!(hex, "0123456789abcdeffedcba9876543210");
        let parsed = Distinction::from_hex(&hex).expect("valid hex round-trips (invariant)");
        assert_eq!(parsed, d);
        assert_eq!(parsed.as_bytes(), &bytes);
    }

    #[test]
    fn from_hex_rejects_empty() {
        assert_eq!(Distinction::from_hex(""), Err(ParseError::InvalidLength(0)));
    }

    #[test]
    fn from_hex_rejects_too_short() {
        assert_eq!(Distinction::from_hex("deadbeef"), Err(ParseError::InvalidLength(8)));
    }

    #[test]
    fn from_hex_rejects_too_long() {
        let s = "0".repeat(33);
        assert_eq!(Distinction::from_hex(&s), Err(ParseError::InvalidLength(33)));
    }

    #[test]
    fn from_hex_rejects_uppercase() {
        // Uppercase is intentionally rejected — canonical form is lowercase.
        let err = Distinction::from_hex("FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF")
            .expect_err("known-bad input must reject (invariant)");
        assert!(matches!(err, ParseError::InvalidChar { .. }));
    }

    #[test]
    fn from_hex_rejects_non_hex_char() {
        let s = "g0000000000000000000000000000000";
        let err = Distinction::from_hex(s).expect_err("known-bad input must reject (invariant)");
        assert_eq!(err, ParseError::InvalidChar { position: 0, found: 'g' });
    }

    #[test]
    fn from_hex_rejects_non_ascii_lead_byte() {
        // `é` is U+00E9, encoded as 0xC3 0xA9 in UTF-8 (2 bytes). Followed
        // by 30 ASCII zeros, the total byte length is exactly 32 — so we
        // pass the length check and reach `decode_nibble`, which sees the
        // lead byte 0xC3 (≥ 128) and rejects via `NonAscii`.
        let s = format!("é{}", "0".repeat(30));
        assert_eq!(s.len(), 32, "constructed input must be 32 bytes (invariant)");
        let err = Distinction::from_hex(&s).expect_err("non-ASCII byte must reject (invariant)");
        assert_eq!(err, ParseError::NonAscii { position: 0 });
    }

    #[test]
    fn from_hex_invalid_char_position_is_accurate() {
        // 16 valid hex chars, then 'z' at position 16, then padding to 32.
        let s = "0123456789abcdefz123456789abcdef";
        let err = Distinction::from_hex(s).expect_err("known-bad input must reject (invariant)");
        assert_eq!(err, ParseError::InvalidChar { position: 16, found: 'z' });
    }

    #[test]
    fn display_uses_hex() {
        let d = Distinction::from_bytes_unchecked([0u8; 16]);
        assert_eq!(format!("{}", d), "00000000000000000000000000000000");
    }

    #[test]
    fn debug_uses_hex_with_wrapper() {
        let d = Distinction::from_bytes_unchecked([0u8; 16]);
        assert_eq!(format!("{:?}", d), "Distinction(00000000000000000000000000000000)");
    }

    #[test]
    fn from_str_works() {
        let s = "0123456789abcdeffedcba9876543210";
        let d: Distinction = s.parse().expect("valid hex parses (invariant)");
        assert_eq!(d.to_hex(), s);
    }

    #[test]
    fn serde_round_trip_via_adapter() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Wrapper {
            #[serde(with = "super::serde_adapter")]
            d: Distinction,
        }

        let original = Wrapper { d: Distinction::from_bytes_unchecked([0x42u8; 16]) };
        let json = serde_json::to_string(&original).expect("serializes (invariant)");
        assert!(json.contains("42424242"));

        let restored: Wrapper = serde_json::from_str(&json).expect("round-trips (invariant)");
        assert_eq!(original, restored);
    }

    // ----- RawDistinctionId hex boundary ---------------------------------

    #[test]
    fn raw_hex_round_trip_mixed_bytes() {
        // Parallel to `hex_round_trip_mixed_bytes` for RawDistinctionId
        // — same 32-hex-char grammar, same 16 bytes, same shared
        // decode_hex_16 kernel.
        let bytes = [
            0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0xfe, 0xdc, 0xba, 0x98, 0x76, 0x54,
            0x32, 0x10,
        ];
        let r = RawDistinctionId::from_bytes(bytes);
        let hex = r.to_hex();
        assert_eq!(hex, "0123456789abcdeffedcba9876543210");
        let parsed = RawDistinctionId::from_hex(&hex).expect("valid hex round-trips (invariant)");
        assert_eq!(parsed, r);
        assert_eq!(parsed.as_bytes(), &bytes);
    }

    #[test]
    fn raw_from_hex_shares_parse_error_surface_with_distinction() {
        // The parallel constructors MUST return the exact same
        // ParseError variants for the exact same inputs — falsifier
        // for accidental drift between the two parse paths.
        let uppercase = "FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF";
        assert_eq!(
            RawDistinctionId::from_hex(uppercase),
            Distinction::from_hex(uppercase).map(|d| RawDistinctionId::from_bytes(*d.as_bytes()))
        );
        // Both should return the same InvalidChar error shape.
        assert!(matches!(
            RawDistinctionId::from_hex(uppercase),
            Err(ParseError::InvalidChar { .. })
        ));

        assert_eq!(RawDistinctionId::from_hex(""), Err(ParseError::InvalidLength(0)));
        assert_eq!(RawDistinctionId::from_hex("deadbeef"), Err(ParseError::InvalidLength(8)));
    }

    #[test]
    fn raw_display_and_debug_are_hex() {
        let r = RawDistinctionId::from_bytes([0u8; 16]);
        assert_eq!(format!("{}", r), "00000000000000000000000000000000");
        assert_eq!(format!("{:?}", r), "RawDistinctionId(00000000000000000000000000000000)");
    }

    #[test]
    fn raw_from_str_works() {
        let s = "0123456789abcdeffedcba9876543210";
        let r: RawDistinctionId = s.parse().expect("valid hex parses (invariant)");
        assert_eq!(r.to_hex(), s);
    }

    #[test]
    fn raw_serde_round_trip_via_adapter() {
        // Falsifier: the raw serde adapter must round-trip byte-for-byte
        // through `serde_json`. Consumers whose wire schema declares
        // `#[serde(with = "raw_distinction_hex")]` (the recommended
        // trust-boundary shape) rely on this.
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Envelope {
            #[serde(with = "super::raw_serde_adapter")]
            claimed_root: RawDistinctionId,
        }

        let original = Envelope { claimed_root: RawDistinctionId::from_bytes([0x42u8; 16]) };
        let json = serde_json::to_string(&original).expect("serializes (invariant)");
        assert!(json.contains("42424242"));

        let restored: Envelope = serde_json::from_str(&json).expect("round-trips (invariant)");
        assert_eq!(original, restored);
    }

    #[test]
    fn raw_serde_deserialize_rejects_bad_hex() {
        // Falsifier: the raw adapter must propagate ParseError as a
        // deserializer error, not silently accept garbage. Uses the
        // same Envelope shape as the round-trip test so the failure
        // path exercises exactly the deserialize side.
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Envelope {
            #[serde(with = "super::raw_serde_adapter")]
            claimed_root: RawDistinctionId,
        }

        let bad_json = r#"{"claimed_root":"not-hex"}"#;
        let result: Result<Envelope, _> = serde_json::from_str(bad_json);
        assert!(result.is_err(), "adapter must reject invalid hex");
    }
}
