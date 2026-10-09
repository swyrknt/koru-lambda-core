//! Every removed path out of the trust boundary, as a compile error.
//!
//! A runtime test cannot assert the *absence* of a trait impl or an
//! associated function, so these live here. Each line was legal before
//! 2026-10-07, and each produced a `Distinction` with no engine involved
//! while the README claimed foreign-byte injection was closed at the type
//! level.

use koru_lambda_core::Distinction;

fn main() {
    let hex = "0123456789abcdeffedcba9876543210";

    // Was: an "intentional hatch" that parsed straight into the verified type.
    let _ = Distinction::from_hex(hex);

    // Was: `FromStr`, so a string became a distinction by inference alone.
    let _: Distinction = hex.parse().unwrap();

    // Was: `Pod`, so any sixteen bytes could be cast into one.
    let bytes = [7u8; 16];
    let _: Distinction = bytemuck::cast(bytes);

    // Was: `Zeroable`, which hands out an all-zero distinction that collides
    // with d0 without d0 ever having been witnessed.
    let _: Distinction = bytemuck::Zeroable::zeroed();
}
