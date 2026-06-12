//! Hex serialization layer for `Distinction`.
//!
//! Bytes-on-wire architecture (DECISION 5.5): the engine substrate will be
//! pure `[u8; 16]` (post step 3 of the foundation sub-branch). Hex is a
//! display format that lives at human-facing boundaries (JSON wire formats,
//! FFI C-string returns, WASM JS helpers, log output).
//!
//! This module provides the explicit conversion layer between the byte-canonical
//! representation and the hex-string representation. It deliberately does not
//! live inside `engine.rs` — hex is not load-bearing on the theory; the engine
//! must not depend on it.
//!
//! The full hex layer (`Distinction::to_hex`, `Distinction::from_hex`,
//! `impl Display`, the serde adapter) is wired into this module in step 2
//! of the foundation sub-branch once `Distinction::as_bytes()` exists.
//! Step 1 publishes only the [`ParseError`] type so downstream code can
//! reference it.

/// Error returned by [`crate::Distinction::from_hex`] when the input string
/// is malformed.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParseError {
    /// Wrong length: hex must be exactly 32 characters (16 bytes encoded).
    #[error("invalid hex length: expected 32 characters, got {0}")]
    InvalidLength(usize),
    /// Encountered a non-hex character.
    #[error("invalid hex character: {0:?}")]
    InvalidHex(char),
}
