//! Hex display boundary for [`Distinction`].
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
//! [`Distinction`]: crate::Distinction

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
    /// The bytes are validated for length and charset, but **not** for
    /// engine membership. A `Distinction` returned by `from_hex` is just
    /// bytes; passing it to `engine.synthesize` for an engine that has
    /// not registered those bytes triggers a debug-build panic via the
    /// foreign-byte guard in `synthesize`.
    ///
    /// # Errors
    ///
    /// Returns [`ParseError::InvalidLength`] if the input is not exactly
    /// 32 ASCII characters, [`ParseError::NonAscii`] for non-ASCII bytes,
    /// or [`ParseError::InvalidChar`] for characters outside `[0-9a-f]`
    /// (uppercase is explicitly rejected).
    pub fn from_hex(s: &str) -> Result<Self, ParseError> {
        if s.len() != 32 {
            return Err(ParseError::InvalidLength(s.len()));
        }

        let bytes = s.as_bytes();
        let mut out = [0u8; 16];

        for (i, byte_pair_idx) in (0..16).enumerate() {
            let hi_pos = byte_pair_idx * 2;
            let lo_pos = hi_pos + 1;
            let hi = decode_nibble(bytes[hi_pos], hi_pos)?;
            let lo = decode_nibble(bytes[lo_pos], lo_pos)?;
            out[i] = (hi << 4) | lo;
        }

        Ok(Distinction::from_bytes_unchecked(out))
    }

    /// Render this distinction as a 32-character lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }
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

/// Serde adapter — serialize/deserialize a [`Distinction`] as its hex string.
///
/// Use with `#[serde(with = "distinction_hex")]` on any field of type
/// `Distinction`:
///
/// ```ignore
/// #[derive(Serialize, Deserialize)]
/// struct TransactionBatch {
///     #[serde(with = "distinction_hex")]
///     previous_root: Distinction,
/// }
/// ```
pub mod serde_adapter {
    use super::Distinction;
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serialize a [`Distinction`] as its 32-character lowercase hex string.
    ///
    /// # Errors
    ///
    /// Propagates any error from the serializer.
    #[allow(clippy::type_complexity)] // serde adapter signature is canonical, not factorable
    pub fn serialize<S>(d: &Distinction, serializer: S) -> Result<S::Ok, S::Error>
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
}
