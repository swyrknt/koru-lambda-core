//! wasm-bindgen surface for JS/TS consumers of koru-lambda-core.
//!
//! Behind `#[cfg(feature = "wasm")]`. Zero touches to axiom hot paths
//! in `engine.rs`/`projection.rs`. Every byte-slice input crosses the
//! Axiom-4 trust boundary via [`WasmEngine::verify`] (or an internal
//! `verify` call) before reaching `synthesize_inner`.
//!
//! Ship-set: [`Adjacency`], [`Degree`], [`HopDistance`] (sealed
//! `CoreSignal`). Consumer-defined signals are impossible from JS — the
//! `private::Sealed` bound in [`crate::projection`] blocks external
//! implementations of `CoreSignal`, and JS has no path to define a Rust
//! trait implementation.
//!
//! # Design notes
//!
//! - **POJO outcome** (`WasmOutcomePojo`) instead of a `#[wasm_bindgen]`
//!   handle struct — Cond D′ falsifier's tight synthesis loops
//!   (10³-10⁵ calls) would OOM wasm linear memory without disciplined
//!   `.free()` at every call site. Auto-GC via `serde_wasm_bindgen`
//!   sidesteps that.
//! - **`serde_bytes` bridge** on every `Vec<u8>` field feeding a POJO —
//!   default `serde-wasm-bindgen` serializes `Vec<u8>` as
//!   `Array<number>`, not `Uint8Array`. `serde_bytes` restores the
//!   expected shape.
//! - **`typescript_custom_section`** declares the discriminated-union
//!   type explicitly so TS narrowing on `kind` works — wasm-bindgen's
//!   default would type `kind` as `string`, defeating the union.
//!
//! See warroom `E07-bindings-web/S01-wasm-surface/` for phase-by-phase
//! design decisions.

#![cfg(feature = "wasm")]

use crate::projection::{Adjacency, Boundary, Degree, Direction, HopDistance, Projection, Signal};
use crate::{Distinction, DistinctionEngine, RawDistinctionId, SynthesisOutcome};
use serde::Serialize;
use std::collections::HashSet;
use wasm_bindgen::prelude::*;

// -------------------------------------------------------------------------
// TypeScript custom-section — declares POJO shapes that wasm-bindgen
// cannot infer from Rust types alone. Consumers import these via
// `import type { SynthesisOutcome, AdjacencyEntry, ... } from
// 'koru-lambda-core'`.
// -------------------------------------------------------------------------

#[wasm_bindgen(typescript_custom_section)]
const TS_APPEND: &'static str = r#"
export type SynthesisOutcome =
    | { kind: "novel"; distinction: Uint8Array }
    | { kind: "existing"; distinction: Uint8Array };

export type Parents = { min: Uint8Array; max: Uint8Array };

export type AdjacencyEntry = {
    distinction: Uint8Array;
    parents: Parents | null;
};

export type DegreeEntry = { distinction: Uint8Array; degree: number };

export type HopDistanceEntry = { distinction: Uint8Array; hops: number };

export type ProjectionDirection = "upstream" | "downstream" | "undirected";
"#;

// -------------------------------------------------------------------------
// Shared helpers
// -------------------------------------------------------------------------

/// Pin a JS-supplied `Uint8Array` slice to a 16-byte array or throw a
/// descriptive `JsError`.
fn pin16(bytes: &[u8]) -> Result<[u8; 16], JsError> {
    if bytes.len() != 16 {
        return Err(JsError::new(&format!(
            "invalid distinction length: expected 16 bytes, got {}",
            bytes.len()
        )));
    }
    let mut arr = [0u8; 16];
    arr.copy_from_slice(bytes);
    Ok(arr)
}

/// Parse the JS-side `Direction` string tag.
fn parse_direction(s: &str) -> Result<Direction, JsError> {
    match s {
        "upstream" => Ok(Direction::Upstream),
        "downstream" => Ok(Direction::Downstream),
        "undirected" => Ok(Direction::Undirected),
        _ => Err(JsError::new(&format!(
            "invalid direction: expected 'upstream' | 'downstream' | 'undirected', got '{s}'"
        ))),
    }
}

/// Materialize a `Boundary` from the optional `hops` param. `None` maps
/// to `Boundary::Saturated`; `Some(n)` maps to `Boundary::Hops(n)`.
const fn parse_boundary(hops: Option<u32>) -> Boundary {
    match hops {
        Some(n) => Boundary::Hops(n as usize),
        None => Boundary::Saturated,
    }
}

/// Verify-in-front helper — the Axiom-4 trust boundary funnel. Every
/// JS-supplied byte slice claiming to name a [`Distinction`] passes
/// through here before reaching a hot path.
fn verify_bytes(
    engine: &DistinctionEngine,
    label: &str,
    bytes: &[u8],
) -> Result<Distinction, JsError> {
    let raw = RawDistinctionId::from_bytes(pin16(bytes)?);
    engine.verify(raw).map_err(|e| JsError::new(&format!("VerifyError on {label}: {e}")))
}

/// Adapt `serde_wasm_bindgen::Error` into `JsError` — `serde` errors
/// during POJO conversion are user-visible failures, not silent panics.
fn to_js<T: Serialize>(v: &T) -> Result<JsValue, JsError> {
    serde_wasm_bindgen::to_value(v).map_err(|e| JsError::new(&format!("SerializeError: {e}")))
}

// -------------------------------------------------------------------------
// POJO shapes crossing the JS boundary
// -------------------------------------------------------------------------

/// Discriminated-union outcome. `#[serde(with = "serde_bytes")]` on
/// `distinction` ensures JS receives `Uint8Array`, not `Array<number>`.
/// Auto-GC on the JS side — no manual `.free()` at synthesis call sites.
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
enum WasmOutcomePojo {
    Novel {
        #[serde(with = "serde_bytes")]
        distinction: Vec<u8>,
    },
    Existing {
        #[serde(with = "serde_bytes")]
        distinction: Vec<u8>,
    },
}

/// Canonical `(min, max)` parent pair for `Adjacency` entries and
/// `parentsOf` returns.
#[derive(Serialize)]
struct ParentsPojo {
    #[serde(with = "serde_bytes")]
    min: Vec<u8>,
    #[serde(with = "serde_bytes")]
    max: Vec<u8>,
}

#[derive(Serialize)]
struct AdjacencyEntryPojo {
    #[serde(with = "serde_bytes")]
    distinction: Vec<u8>,
    parents: Option<ParentsPojo>,
}

#[derive(Serialize)]
struct DegreeEntryPojo {
    #[serde(with = "serde_bytes")]
    distinction: Vec<u8>,
    degree: u64,
}

#[derive(Serialize)]
struct HopDistanceEntryPojo {
    #[serde(with = "serde_bytes")]
    distinction: Vec<u8>,
    hops: u64,
}

// -------------------------------------------------------------------------
// WasmEngine
// -------------------------------------------------------------------------

/// JavaScript-facing wrapper around a substrate [`DistinctionEngine`].
///
/// Owns its inner engine by value (no `Arc<>`) — wasm is single-threaded
/// under the default `wasm-pack --target bundler` build. Every method
/// accepting `Uint8Array` bytes routes through [`WasmEngine::verify`] (or
/// an internal `verify_bytes` funnel) before reaching a hot path, closing
/// Axiom-4 foreign-byte injection at the JS boundary.
#[wasm_bindgen]
pub struct WasmEngine {
    inner: DistinctionEngine,
}

#[wasm_bindgen]
impl WasmEngine {
    /// Construct a fresh engine holding only the two primordials.
    ///
    /// Installs `console_error_panic_hook` on first construction so any
    /// substrate panic surfaces as a JS-side stack trace instead of a
    /// bare `RuntimeError: unreachable executed`.
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        console_error_panic_hook::set_once();
        Self { inner: DistinctionEngine::new() }
    }

    /// First primordial. Returns `Uint8Array` of 16 bytes `[0x00; 16]`.
    ///
    /// Note: v2.0.0 primordials are byte-content-different from v1.2 npm
    /// (which returned UTF-8 of `"0"` / `"1"`). See MIGRATION_JS.md
    /// (S02 scope).
    #[wasm_bindgen(js_name = d0)]
    #[must_use]
    pub fn d0(&self) -> Vec<u8> {
        self.inner.d0().as_bytes().to_vec()
    }

    /// Second primordial. Returns `Uint8Array` of 16 bytes
    /// `[0x01, 0x00, ..., 0x00]`.
    #[wasm_bindgen(js_name = d1)]
    #[must_use]
    pub fn d1(&self) -> Vec<u8> {
        self.inner.d1().as_bytes().to_vec()
    }

    /// **Trust boundary.** Verify raw bytes claim a distinction
    /// registered in this engine.
    ///
    /// Returns the 16 bytes back (echoed) if verified; throws
    /// `JsError` on foreign bytes. JS callers who want the trust-boundary
    /// crossing explicit call this first, then pass the returned bytes
    /// back to other methods.
    #[wasm_bindgen]
    pub fn verify(&self, bytes: &[u8]) -> Result<Vec<u8>, JsError> {
        verify_bytes(&self.inner, "bytes", bytes).map(|d| d.as_bytes().to_vec())
    }

    /// Synthesize `(a, b)` and return the child bytes as `Uint8Array`.
    ///
    /// Both `a` and `b` are verified against this engine before entering
    /// `synthesize_inner`. Foreign-byte injection is impossible from JS.
    #[wasm_bindgen]
    pub fn synthesize(&self, a: &[u8], b: &[u8]) -> Result<Vec<u8>, JsError> {
        let a_d = verify_bytes(&self.inner, "a", a)?;
        let b_d = verify_bytes(&self.inner, "b", b)?;
        Ok(self.inner.synthesize(a_d, b_d).as_bytes().to_vec())
    }

    /// Synthesize `(a, b)` and return a discriminated-union outcome:
    /// `{ kind: "novel" | "existing", distinction: Uint8Array }`.
    ///
    /// The POJO auto-GCs on the JS side — no manual `.free()` required.
    /// TS narrows on `kind` via the `typescript_custom_section` type.
    #[wasm_bindgen(js_name = synthesizeNovel)]
    pub fn synthesize_novel(&self, a: &[u8], b: &[u8]) -> Result<JsValue, JsError> {
        let a_d = verify_bytes(&self.inner, "a", a)?;
        let b_d = verify_bytes(&self.inner, "b", b)?;
        let pojo = match self.inner.synthesize_novel(a_d, b_d) {
            SynthesisOutcome::Novel(d) => {
                WasmOutcomePojo::Novel { distinction: d.as_bytes().to_vec() }
            },
            SynthesisOutcome::Existing(d) => {
                WasmOutcomePojo::Existing { distinction: d.as_bytes().to_vec() }
            },
        };
        to_js(&pojo)
    }

    /// Membership check — true iff `d`'s bytes are registered in this
    /// engine.
    ///
    /// Accepts foreign 16-byte inputs without throwing:
    /// `has(foreign) === false`. Non-16-byte inputs throw a
    /// `pin16`-shaped error (invalid distinction length).
    /// This mirrors the substrate's `has(&self, d: Distinction) -> bool`
    /// semantics — for length-correct inputs the check IS the verify.
    #[wasm_bindgen]
    pub fn has(&self, d: &[u8]) -> Result<bool, JsError> {
        let raw = RawDistinctionId::from_bytes(pin16(d)?);
        Ok(self.inner.verify(raw).is_ok())
    }

    /// Degree of a distinction (Law 12, Coding Law).
    ///
    /// Verify-in-front — foreign bytes throw `JsError`. Returns the
    /// substrate's `degree()` result as `u64` (widened from `usize` for
    /// JS `Number`-safe interop).
    #[wasm_bindgen]
    pub fn degree(&self, d: &[u8]) -> Result<u64, JsError> {
        let d_d = verify_bytes(&self.inner, "d", d)?;
        Ok(self.inner.degree(d_d) as u64)
    }

    /// Canonical `(min, max)` parent pair of a non-primordial
    /// distinction. Returns `null` for `d0` / `d1` / any distinction
    /// without parents.
    ///
    /// Verify-in-front — foreign bytes throw `JsError`.
    #[wasm_bindgen(js_name = parentsOf)]
    pub fn parents_of(&self, d: &[u8]) -> Result<JsValue, JsError> {
        let d_d = verify_bytes(&self.inner, "d", d)?;
        match self.inner.parents_of(d_d) {
            None => Ok(JsValue::NULL),
            Some((min, max)) => {
                to_js(&ParentsPojo { min: min.as_bytes().to_vec(), max: max.as_bytes().to_vec() })
            },
        }
    }

    /// Total distinction count (including primordials). Returns `u64`
    /// for JS-safe range.
    #[wasm_bindgen(js_name = distinctionCount)]
    #[must_use]
    pub fn distinction_count(&self) -> u64 {
        self.inner.distinction_count() as u64
    }

    /// Relationship count (2 per non-primordial + 1 for the genesis
    /// edge).
    #[wasm_bindgen(js_name = relationshipCount)]
    #[must_use]
    pub fn relationship_count(&self) -> u64 {
        self.inner.relationship_count() as u64
    }

    /// Materialize an `Adjacency` projection anchored at `root`.
    ///
    /// `direction` is one of `"upstream"`, `"downstream"`, `"undirected"`.
    /// `hops` = `undefined` means `Boundary::Saturated`; `hops = n`
    /// means `Boundary::Hops(n)`.
    ///
    /// Returns an [`AdjacencyProjection`] JS class carrying an eagerly
    /// materialized copy of the entries + wire bytes + content-addressed
    /// id. The underlying `Projection<'e, Adjacency>` borrow is dropped
    /// before this method returns — no engine borrow held on the
    /// JS-facing handle.
    #[wasm_bindgen(js_name = projectAdjacency)]
    pub fn project_adjacency(
        &self,
        root: &[u8],
        direction: &str,
        hops: Option<u32>,
    ) -> Result<AdjacencyProjection, JsError> {
        let root_d = verify_bytes(&self.inner, "root", root)?;
        let dir = parse_direction(direction)?;
        let bnd = parse_boundary(hops);
        let proj =
            self.inner.project(root_d).direction(dir).boundary(bnd).signal(Adjacency).materialize();
        Ok(AdjacencyProjection::from_projection(&proj))
    }

    /// Materialize a `Degree` projection anchored at `root`.
    ///
    /// See [`WasmEngine::project_adjacency`] for the `direction` / `hops`
    /// semantics.
    #[wasm_bindgen(js_name = projectDegree)]
    pub fn project_degree(
        &self,
        root: &[u8],
        direction: &str,
        hops: Option<u32>,
    ) -> Result<DegreeProjection, JsError> {
        let root_d = verify_bytes(&self.inner, "root", root)?;
        let dir = parse_direction(direction)?;
        let bnd = parse_boundary(hops);
        let proj =
            self.inner.project(root_d).direction(dir).boundary(bnd).signal(Degree).materialize();
        Ok(DegreeProjection::from_projection(&proj))
    }

    /// Materialize a `HopDistance` projection anchored at `root`.
    ///
    /// See [`WasmEngine::project_adjacency`] for the `direction` / `hops`
    /// semantics.
    #[wasm_bindgen(js_name = projectHopDistance)]
    pub fn project_hop_distance(
        &self,
        root: &[u8],
        direction: &str,
        hops: Option<u32>,
    ) -> Result<HopDistanceProjection, JsError> {
        let root_d = verify_bytes(&self.inner, "root", root)?;
        let dir = parse_direction(direction)?;
        let bnd = parse_boundary(hops);
        let proj = self
            .inner
            .project(root_d)
            .direction(dir)
            .boundary(bnd)
            .signal(HopDistance)
            .materialize();
        Ok(HopDistanceProjection::from_projection(&proj))
    }

    /// Restore an `Adjacency` projection from canonical wire bytes.
    ///
    /// Round-trips the substrate's `restore_projection::<Adjacency>` —
    /// see PROJECTION_SPEC §5. Errors surface as `JsError` with the
    /// `RestoreError` variant name preserved for JS-side triage.
    #[wasm_bindgen(js_name = restoreProjectionAdjacency)]
    pub fn restore_projection_adjacency(
        &self,
        bytes: &[u8],
    ) -> Result<AdjacencyProjection, JsError> {
        let proj = self
            .inner
            .restore_projection::<Adjacency>(bytes, Adjacency)
            .map_err(|e| JsError::new(&format!("RestoreError: {e}")))?;
        Ok(AdjacencyProjection::from_projection(&proj))
    }

    /// Restore a `Degree` projection from canonical wire bytes.
    #[wasm_bindgen(js_name = restoreProjectionDegree)]
    pub fn restore_projection_degree(&self, bytes: &[u8]) -> Result<DegreeProjection, JsError> {
        let proj = self
            .inner
            .restore_projection::<Degree>(bytes, Degree)
            .map_err(|e| JsError::new(&format!("RestoreError: {e}")))?;
        Ok(DegreeProjection::from_projection(&proj))
    }

    /// Restore a `HopDistance` projection from canonical wire bytes.
    #[wasm_bindgen(js_name = restoreProjectionHopDistance)]
    pub fn restore_projection_hop_distance(
        &self,
        bytes: &[u8],
    ) -> Result<HopDistanceProjection, JsError> {
        let proj = self
            .inner
            .restore_projection::<HopDistance>(bytes, HopDistance)
            .map_err(|e| JsError::new(&format!("RestoreError: {e}")))?;
        Ok(HopDistanceProjection::from_projection(&proj))
    }
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new()
    }
}

// -------------------------------------------------------------------------
// Free functions — id_to_hex / id_from_hex
// -------------------------------------------------------------------------

/// Encode 16 distinction bytes as a 32-character lowercase hex string.
///
/// Rejects any input whose length is not exactly 16 bytes.
///
/// # Errors
///
/// Returns `JsError` if `bytes.len() != 16`.
#[wasm_bindgen(js_name = idToHex)]
pub fn id_to_hex(bytes: &[u8]) -> Result<String, JsError> {
    let arr = pin16(bytes)?;
    Ok(hex::encode(arr))
}

/// Parse a 32-character lowercase-hex string into 16 raw bytes.
///
/// The bytes are validated for length and charset only — engine-membership
/// is unverified. Pass the returned `Uint8Array` through
/// [`WasmEngine::verify`] to obtain engine-witnessed identity.
///
/// # Errors
///
/// Returns `JsError` on non-32-char inputs, non-ASCII bytes, or
/// characters outside `[0-9a-f]` (uppercase is rejected).
#[wasm_bindgen(js_name = idFromHex)]
pub fn id_from_hex(hex_str: &str) -> Result<Vec<u8>, JsError> {
    let raw = RawDistinctionId::from_hex(hex_str)
        .map_err(|e| JsError::new(&format!("ParseError: {e}")))?;
    Ok(raw.as_bytes().to_vec())
}

// -------------------------------------------------------------------------
// AdjacencyProjection
// -------------------------------------------------------------------------

/// JS-facing wrapper for a materialized `Adjacency` projection.
///
/// Holds eagerly extracted entries + canonical wire bytes + 32-byte
/// projection id. `entries()` iteration order is canonical (BTreeMap
/// `Ord` on `Distinction`) but the SOLE canonical form on the wire is
/// [`AdjacencyProjection::canonical_bytes`] — JS-shape entries are
/// display, not authority.
#[wasm_bindgen]
pub struct AdjacencyProjection {
    entries_data: Vec<AdjacencyEntryPojo>,
    contains_set: HashSet<[u8; 16]>,
    canonical_bytes_data: Vec<u8>,
    projection_id_data: Vec<u8>,
}

impl AdjacencyProjection {
    fn from_projection(proj: &Projection<'_, Adjacency>) -> Self {
        let entries_data: Vec<AdjacencyEntryPojo> = proj
            .output()
            .iter()
            .map(|(d, parents)| AdjacencyEntryPojo {
                distinction: d.as_bytes().to_vec(),
                parents: parents.map(|(min, max)| ParentsPojo {
                    min: min.as_bytes().to_vec(),
                    max: max.as_bytes().to_vec(),
                }),
            })
            .collect();
        let contains_set = build_contains_set(proj);
        Self {
            entries_data,
            contains_set,
            canonical_bytes_data: proj.canonical_bytes(),
            projection_id_data: proj.projection_id().as_bytes().to_vec(),
        }
    }
}

#[wasm_bindgen]
impl AdjacencyProjection {
    /// Iterate entries as an array of
    /// `{ distinction: Uint8Array, parents: { min, max } | null }`.
    ///
    /// Iteration order is engine-side canonical (BTreeMap `Ord` on
    /// `Distinction`).
    #[wasm_bindgen]
    pub fn entries(&self) -> Result<JsValue, JsError> {
        to_js(&self.entries_data)
    }

    /// Fast-path membership test — true iff `d`'s bytes are in the cone.
    #[wasm_bindgen]
    pub fn contains(&self, d: &[u8]) -> Result<bool, JsError> {
        let arr = pin16(d)?;
        Ok(self.contains_set.contains(&arr))
    }

    /// Canonical wire bytes per PROJECTION_SPEC §5. The SOLE canonical
    /// form of this projection.
    #[wasm_bindgen(js_name = canonicalBytes)]
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.canonical_bytes_data.clone()
    }

    /// 32-byte content-addressed [`crate::projection::ProjectionId`] per
    /// PROJECTION_SPEC §6 Cond B.
    #[wasm_bindgen(js_name = projectionId)]
    #[must_use]
    pub fn projection_id(&self) -> Vec<u8> {
        self.projection_id_data.clone()
    }
}

// -------------------------------------------------------------------------
// DegreeProjection
// -------------------------------------------------------------------------

/// JS-facing wrapper for a materialized `Degree` projection.
#[wasm_bindgen]
pub struct DegreeProjection {
    entries_data: Vec<DegreeEntryPojo>,
    contains_set: HashSet<[u8; 16]>,
    canonical_bytes_data: Vec<u8>,
    projection_id_data: Vec<u8>,
}

impl DegreeProjection {
    fn from_projection(proj: &Projection<'_, Degree>) -> Self {
        let entries_data: Vec<DegreeEntryPojo> = proj
            .output()
            .iter()
            .map(|(d, deg)| DegreeEntryPojo {
                distinction: d.as_bytes().to_vec(),
                degree: *deg as u64,
            })
            .collect();
        let contains_set = build_contains_set(proj);
        Self {
            entries_data,
            contains_set,
            canonical_bytes_data: proj.canonical_bytes(),
            projection_id_data: proj.projection_id().as_bytes().to_vec(),
        }
    }
}

#[wasm_bindgen]
impl DegreeProjection {
    /// Iterate entries as `{ distinction: Uint8Array, degree: number }`.
    #[wasm_bindgen]
    pub fn entries(&self) -> Result<JsValue, JsError> {
        to_js(&self.entries_data)
    }

    /// Fast-path membership test.
    #[wasm_bindgen]
    pub fn contains(&self, d: &[u8]) -> Result<bool, JsError> {
        let arr = pin16(d)?;
        Ok(self.contains_set.contains(&arr))
    }

    /// Canonical wire bytes per PROJECTION_SPEC §5.
    #[wasm_bindgen(js_name = canonicalBytes)]
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.canonical_bytes_data.clone()
    }

    /// 32-byte content-addressed projection id.
    #[wasm_bindgen(js_name = projectionId)]
    #[must_use]
    pub fn projection_id(&self) -> Vec<u8> {
        self.projection_id_data.clone()
    }
}

// -------------------------------------------------------------------------
// HopDistanceProjection
// -------------------------------------------------------------------------

/// JS-facing wrapper for a materialized `HopDistance` projection.
#[wasm_bindgen]
pub struct HopDistanceProjection {
    entries_data: Vec<HopDistanceEntryPojo>,
    contains_set: HashSet<[u8; 16]>,
    canonical_bytes_data: Vec<u8>,
    projection_id_data: Vec<u8>,
}

impl HopDistanceProjection {
    fn from_projection(proj: &Projection<'_, HopDistance>) -> Self {
        let entries_data: Vec<HopDistanceEntryPojo> = proj
            .output()
            .iter()
            .map(|(d, hops)| HopDistanceEntryPojo {
                distinction: d.as_bytes().to_vec(),
                hops: *hops as u64,
            })
            .collect();
        let contains_set = build_contains_set(proj);
        Self {
            entries_data,
            contains_set,
            canonical_bytes_data: proj.canonical_bytes(),
            projection_id_data: proj.projection_id().as_bytes().to_vec(),
        }
    }
}

#[wasm_bindgen]
impl HopDistanceProjection {
    /// Iterate entries as `{ distinction: Uint8Array, hops: number }`.
    #[wasm_bindgen]
    pub fn entries(&self) -> Result<JsValue, JsError> {
        to_js(&self.entries_data)
    }

    /// Fast-path membership test.
    #[wasm_bindgen]
    pub fn contains(&self, d: &[u8]) -> Result<bool, JsError> {
        let arr = pin16(d)?;
        Ok(self.contains_set.contains(&arr))
    }

    /// Canonical wire bytes per PROJECTION_SPEC §5.
    #[wasm_bindgen(js_name = canonicalBytes)]
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.canonical_bytes_data.clone()
    }

    /// 32-byte content-addressed projection id.
    #[wasm_bindgen(js_name = projectionId)]
    #[must_use]
    pub fn projection_id(&self) -> Vec<u8> {
        self.projection_id_data.clone()
    }
}

// -------------------------------------------------------------------------
// Small shared helper — dedupe the `.contains()` fast-path index build
// across all three projection classes.
// -------------------------------------------------------------------------

fn build_contains_set<S: Signal>(proj: &Projection<'_, S>) -> HashSet<[u8; 16]> {
    proj.output().iter().map(|(d, _)| *d.as_bytes()).collect()
}
