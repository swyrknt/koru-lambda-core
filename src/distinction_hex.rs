//! Hex serialization layer for `Distinction`.
//!
//! Bytes-on-wire architecture (DECISION 5.5): the engine substrate is
//! `[u8; 16]` (canonical). Hex is a display format that lives at human-facing
//! boundaries (JSON wire formats, FFI C-string returns, WASM JS helpers, log
//! output).
//!
//! This module provides the explicit conversion layer between the byte-canonical
//! representation and the hex-string representation. It deliberately does not
//! live inside `engine.rs` — hex is not load-bearing on the theory; the engine
//! must not depend on it.
//!
//! # API
//!
//! - [`Distinction::to_hex`] — 32-character lowercase hex string.
//! - [`Distinction::from_hex`] — parses a 32-character hex string back into a
//!   `Distinction` (does NOT register it with any engine — see note).
//! - [`Display`](std::fmt::Display) and [`Debug`](std::fmt::Debug) for
//!   `Distinction` — both use `to_hex`.
//! - Module-level [`serialize`] / [`deserialize`] — adapter usable via
//!   `#[serde(with = "distinction_hex")]` to serialize a `Distinction` as a
//!   hex string in JSON-style formats.
//!
//! # Note on `from_hex`
//!
//! `Distinction::from_hex` validates length and character set but does NOT
//! validate that the resulting 16 bytes correspond to any distinction the
//! local engine has actually synthesized. It is the parsing surface for IDs
//! received over JSON, FFI strings, or WASM helpers. Foreign-ID structural
//! closure (Section 2.1 #2) lives in `Distinction`'s `pub(crate)` field:
//! `from_hex` is allowed to produce a Distinction value, but `engine.synthesize`
//! over a foreign parent still produces a child whose parent is not registered.
//! The closure of practical foreign-ID poisoning is `Distinction::new` going
//! away (step 7) — after that, every Distinction in any consumer's hands
//! either came from an engine call or from `from_hex` (an explicit hex parse
//! the consumer chose to do).

use crate::engine::Distinction;
use std::fmt;

/// Error returned by [`Distinction::from_hex`] when the input string is
/// malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Wrong length: hex must be exactly 32 characters (16 bytes encoded).
    #[error("invalid hex length: expected 32 characters, got {0}")]
    InvalidLength(usize),
    /// Encountered a non-hex character.
    #[error("invalid hex character: {0:?}")]
    InvalidHex(char),
}

impl Distinction {
    /// Returns the 32-character lowercase hex encoding of this distinction's
    /// 16-byte ID.
    #[must_use]
    pub fn to_hex(&self) -> String {
        hex::encode(self.as_bytes())
    }

    /// Parses a 32-character hex string into a `Distinction`.
    ///
    /// Returns `ParseError::InvalidLength` if the input is not exactly 32
    /// characters, and `ParseError::InvalidHex` if any character is not a
    /// valid hex digit.
    ///
    /// Note: this only parses bytes. It does not register the resulting
    /// value with any engine.
    pub fn from_hex(s: &str) -> Result<Self, ParseError> {
        if s.len() != 32 {
            return Err(ParseError::InvalidLength(s.len()));
        }
        let bytes = hex::decode(s).map_err(|e| match e {
            hex::FromHexError::InvalidHexCharacter { c, .. } => ParseError::InvalidHex(c),
            hex::FromHexError::OddLength => ParseError::InvalidLength(s.len()),
            hex::FromHexError::InvalidStringLength => ParseError::InvalidLength(s.len()),
        })?;
        let mut arr = [0u8; 16];
        arr.copy_from_slice(&bytes);
        Ok(Distinction::from_bytes_internal(arr))
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

/// Serde adapter — use as `#[serde(with = "distinction_hex")]` to serialize
/// a `Distinction` as a hex string in JSON-style formats.
///
/// At the call site:
///
/// ```ignore
/// #[derive(Serialize, Deserialize)]
/// struct Entry {
///     #[serde(with = "koru_lambda_core::distinction_hex")]
///     id: Distinction,
/// }
/// ```
///
/// Because `#[serde(with = "...")]` resolves to module functions named
/// `serialize` and `deserialize`, these are exposed at the module top level.
#[allow(clippy::type_complexity)]
pub fn serialize<S>(d: &Distinction, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&d.to_hex())
}

#[allow(clippy::type_complexity)]
pub fn deserialize<'de, D>(deserializer: D) -> Result<Distinction, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::de::Error;
    let s = <String as serde::Deserialize>::deserialize(deserializer)?;
    Distinction::from_hex(&s).map_err(D::Error::custom)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DistinctionEngine;

    #[test]
    fn round_trip_synthesized() {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let d1 = engine.d1().clone();
        let c = engine.synthesize(&d0, &d1);
        let parsed = Distinction::from_hex(&c.to_hex()).expect("round-trips");
        assert_eq!(parsed.as_bytes(), c.as_bytes());
    }

    #[test]
    fn round_trip_random_bytes() {
        for b in 0..=255u8 {
            let mut arr = [0u8; 16];
            arr[0] = b;
            arr[15] = b.wrapping_mul(7);
            let d = Distinction::from_bytes_internal(arr);
            let parsed = Distinction::from_hex(&d.to_hex()).expect("round-trips");
            assert_eq!(parsed.as_bytes(), &arr);
        }
    }

    #[test]
    fn primordial_bytes() {
        let engine = DistinctionEngine::new();
        // d0 -> [0; 16]
        assert_eq!(engine.d0().as_bytes(), &[0u8; 16]);
        // d1 -> [1, 0, ..., 0]
        let mut expected_d1 = [0u8; 16];
        expected_d1[0] = 1;
        assert_eq!(engine.d1().as_bytes(), &expected_d1);
    }

    #[test]
    fn reject_wrong_length_short() {
        let err = Distinction::from_hex("deadbeef").unwrap_err();
        assert_eq!(err, ParseError::InvalidLength(8));
    }

    #[test]
    fn reject_wrong_length_long() {
        let too_long = "a".repeat(64);
        let err = Distinction::from_hex(&too_long).unwrap_err();
        assert_eq!(err, ParseError::InvalidLength(64));
    }

    #[test]
    fn reject_wrong_length_empty() {
        let err = Distinction::from_hex("").unwrap_err();
        assert_eq!(err, ParseError::InvalidLength(0));
    }

    #[test]
    fn reject_non_hex_character() {
        let bad = "zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz";
        match Distinction::from_hex(bad).unwrap_err() {
            ParseError::InvalidHex(c) => assert_eq!(c, 'z'),
            other => panic!("expected InvalidHex, got {other:?}"),
        }
    }

    #[test]
    fn display_uses_hex() {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let s = format!("{d0}");
        assert_eq!(s, d0.to_hex());
        assert_eq!(s, "00000000000000000000000000000000");
    }

    #[test]
    fn debug_uses_hex() {
        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let dbg = format!("{d0:?}");
        assert_eq!(dbg, "Distinction(00000000000000000000000000000000)");
    }

    #[test]
    fn serde_adapter_round_trip_via_json() {
        use serde::{Deserialize, Serialize};

        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct Wrapper {
            #[serde(with = "super")]
            d: Distinction,
        }

        let engine = DistinctionEngine::new();
        let d0 = engine.d0().clone();
        let w = Wrapper { d: d0.clone() };
        let json = serde_json::to_string(&w).expect("serialize");
        assert!(json.contains(&d0.to_hex()), "json missing hex: {json}");

        let back: Wrapper = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.d.as_bytes(), w.d.as_bytes());
    }
}
