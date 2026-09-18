//! # Projection API — the read-dual of `synthesize`
//!
//! A projection is a coherent read of the append-only distinction graph
//! from a chosen `root`, at a chosen `boundary`, in a chosen `direction`,
//! over a chosen `signal`. See `THEORY.md § The synthesis/projection
//! dual` for the theoretical framing and `DESIGN.md` for the API
//! surface rationale.
//!
//! Consumers construct projections through the fluent builder anchored
//! on [`DistinctionEngine::project`]:
//!
//! ```
//! use koru_lambda_core::{Adjacency, DistinctionEngine};
//! use koru_lambda_core::projection::Direction;
//!
//! let engine = DistinctionEngine::new();
//! let child = engine.synthesize(engine.d0(), engine.d1());
//! let proj = engine
//!     .project(child)
//!     .direction(Direction::Upstream)
//!     .hops(2)
//!     .signal(Adjacency)
//!     .materialize();
//! assert!(proj.contains(&child));
//! ```
//!
//! # Cond A-F axiom-condition closures
//!
//! See PROJECTION_SPEC §6. The types below carry the mechanisms:
//!
//! - **Cond A (determinism):** `Signal: Clone + Send + Sync + 'static`
//!   with a documented purity contract on `compute` / `canonical_bytes`.
//! - **Cond B (content addressing):** [`ProjectionId`] is SHA-256 over
//!   a canonically ordered domain-separated input set.
//! - **Cond C (idempotence):** [`ProjectionOutput`] wraps a
//!   `BTreeMap<Distinction, S::Output>` whose iteration order is
//!   canonical (`Distinction::Ord`).
//! - **Cond D (cross-engine byte-equivalence):** all wire-shape
//!   integers use explicit `to_le_bytes()` encoding.
//! - **Cond E (read-only):** [`ReadOnlyEngine`] is sealed and lists no
//!   mutating method.
//! - **Cond F (no new engine fields):** the projection state lives on
//!   `Projection<'e, S>`; the engine gains no field.

use crate::engine::{RawDistinctionId, VerifyError};
use crate::{Distinction, DistinctionEngine, InvariantError, ParentPair};
use sha2::{Digest, Sha256};
use std::any::TypeId;
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::marker::PhantomData;
use std::sync::Arc;

// -------------------------------------------------------------------------
// Precomputed `TYPE_ID<S>` const-table
// -------------------------------------------------------------------------
//
// Hardcoded 16-byte truncations of `SHA-256(b"koru:signal:v1:<name>")`
// for each substrate `CoreSignal` ZST. The wire format uses
// [`signal_identity`] (runtime SHA-256) for the general `S: Signal`
// path; these constants short-circuit the SHA-256 for the fast
// `CoreSignal`-bounded [`type_id_of`] path.
//
// Single source of truth (no `build.rs`, no `include!`, no `sha2`
// build-dep). The `type_id_constants_match_runtime_sha` inline test is
// the drift guard: if any tag string here diverges from the constant
// derivation, `cargo test` fails immediately.

/// `SHA-256(b"koru:signal:v1:adjacency")[..16]`.
pub(crate) const ADJACENCY_TYPE_ID: [u8; 16] =
    [208, 185, 210, 86, 47, 219, 44, 63, 63, 164, 184, 163, 37, 139, 119, 129];
/// `SHA-256(b"koru:signal:v1:degree")[..16]`.
pub(crate) const DEGREE_TYPE_ID: [u8; 16] =
    [62, 0, 168, 0, 141, 229, 206, 163, 228, 104, 15, 112, 202, 14, 181, 102];
/// `SHA-256(b"koru:signal:v1:hop_distance")[..16]`.
pub(crate) const HOPDISTANCE_TYPE_ID: [u8; 16] =
    [239, 98, 9, 69, 48, 159, 195, 83, 226, 216, 248, 203, 177, 211, 144, 118];

// -------------------------------------------------------------------------
// Sealed marker — external crates cannot reach this module.
// -------------------------------------------------------------------------

pub(crate) mod private {
    /// Sealing token — see [`super::CoreSignal`] and
    /// [`super::ReadOnlyEngine`].
    pub trait Sealed {}
}

// -------------------------------------------------------------------------
// CanonicalBytes — byte encoder (complements `Canonicalizable`)
// -------------------------------------------------------------------------

/// Byte-canonical encoding for a value. Complements
/// [`crate::Canonicalizable`], which lifts values *into* the graph;
/// `CanonicalBytes` emits values *out of* memory to bytes for wire
/// transit and hashing.
///
/// # Cond D contract (unenforceable at type level, disclosed here)
///
/// Implementations MUST NOT return bytes derived from interior-mutable
/// state, wall-clock time, thread ids, or any other input that varies
/// across engines with identical synthesis history. This is a
/// consumer-managed guarantee for consumer types; substrate-shipped
/// impls (`usize`, `bool`, `Option<ParentPair>`) close Cond D by
/// construction.
pub trait CanonicalBytes {
    /// Return this value as canonical bytes. Owned `Vec<u8>` because
    /// outputs may require construction (e.g., serializing
    /// `Option<ParentPair>` needs a tag byte).
    #[must_use]
    fn canonical_bytes(&self) -> Vec<u8>;
}

impl CanonicalBytes for usize {
    fn canonical_bytes(&self) -> Vec<u8> {
        // 8-byte little-endian, target-independent (Cond D — a wasm32
        // producer and an x86_64 consumer must agree on width).
        (*self as u64).to_le_bytes().to_vec()
    }
}

impl CanonicalBytes for bool {
    fn canonical_bytes(&self) -> Vec<u8> {
        vec![u8::from(*self)]
    }
}

impl CanonicalBytes for Option<ParentPair> {
    fn canonical_bytes(&self) -> Vec<u8> {
        match self {
            None => vec![0u8],
            Some((min, max)) => {
                let mut v = Vec::with_capacity(1 + 32);
                v.push(1);
                v.extend_from_slice(min.as_bytes());
                v.extend_from_slice(max.as_bytes());
                v
            },
        }
    }
}

// -------------------------------------------------------------------------
// Signal / CoreSignal traits
// -------------------------------------------------------------------------

/// Per-node output map produced by [`CoreSignal::__materialize_special`]
/// and by [`ReadyBuilder::materialize`]'s compute fallback path.
///
/// Naming the shape once avoids repeating `BTreeMap<Distinction,
/// <S as Signal>::Output>` at every substrate dispatch boundary. The
/// alias is exposed under `koru_lambda_core::projection::SignalOutputs`
/// for consumers that need to name the shape in their own type
/// signatures (e.g., wrappers around `__materialize_special`); it is
/// intentionally NOT re-exported at the flat crate root (Option B —
/// implementation-detail alias, not top-level API).
pub type SignalOutputs<S> = BTreeMap<Distinction, <S as Signal>::Output>;

/// A signal — a per-node computation over the projection cone.
///
/// Consumers implement this on their own types to project custom
/// per-node values. The substrate ships three implementations
/// (`Adjacency`, `Degree`, `HopDistance` — all also [`CoreSignal`]).
///
/// # Trait-object safety
///
/// `Signal` is **NOT dyn-safe** by design — `compute_aggregate` takes
/// `impl IntoIterator`, which precludes vtable dispatch. Consumer
/// patterns needing heterogeneous signal collections must wrap in an
/// enum whose variants each carry a concrete `S: Signal`. `<S: Signal>`
/// generic bounds everywhere else — no `Box<dyn Signal>` code paths
/// exist in the substrate. The `tests/trybuild/dyn_signal_unsafe.rs`
/// compile-fail probe proves this.
///
/// # `canonical_bytes()` purity — consumer contract
///
/// Consumer-implemented signals whose `canonical_bytes()` is non-pure
/// produce non-deterministic `ProjectionId` values. The type system
/// cannot enforce method purity; only `CoreSignal`-bounded substrate
/// methods carry the Cond D guarantee. If your `Signal` has instance
/// state (e.g. a threshold field), `canonical_bytes()` MUST incorporate
/// that state or `ProjectionId` collisions occur across different
/// instances.
///
/// # Example: consumer signal (PROJECTION_SPEC §13)
///
/// Consumer signals can't close Cond D by construction — that would
/// require substrate-level canonicalization, which sealing forbids.
/// The consumer's job is to document the purity contract of their
/// `compute` and `canonical_bytes` and hold to it in every code path.
///
/// ```
/// use koru_lambda_core::projection::{Boundary, Direction, ReadOnlyEngine, Signal, SignalContext};
/// use koru_lambda_core::{Distinction, DistinctionEngine};
///
/// #[derive(Clone)]
/// struct MyCustomSignal {
///     threshold: usize,
///     /// Owned canonical-bytes buffer computed at construction time.
///     cached_bytes: Vec<u8>,
/// }
///
/// impl MyCustomSignal {
///     fn new(threshold: usize) -> Self {
///         // Instance-state discipline: `canonical_bytes` MUST encode
///         // every field that affects `compute`'s output. `threshold`
///         // is exactly such a field.
///         let mut buf = Vec::with_capacity(64);
///         buf.extend_from_slice(b"my_crate:MyCustomSignal:v1:");
///         buf.extend_from_slice(&(threshold as u64).to_le_bytes());
///         Self { threshold, cached_bytes: buf }
///     }
/// }
///
/// impl Signal for MyCustomSignal {
///     type Output = bool;
///
///     fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> bool {
///         ctx.engine.degree(node) > self.threshold
///     }
///
///     fn canonical_bytes(&self) -> &[u8] {
///         // If `threshold` were omitted here, two instances { threshold: 5 }
///         // and { threshold: 100 } would hash to the same `ProjectionId` —
///         // projection cache collision, wrong output returned.
///         &self.cached_bytes
///     }
/// }
///
/// // MyCustomSignal does NOT (and cannot) impl CoreSignal — private::Sealed
/// // is unreachable from external crates.
/// let engine = DistinctionEngine::new();
/// let child = engine.synthesize(engine.d0(), engine.d1());
/// let proj = engine
///     .project(child)
///     .direction(Direction::Upstream)
///     .hops(2)
///     .signal(MyCustomSignal::new(3))
///     .materialize();
/// assert!(proj.contains(&child));
/// ```
pub trait Signal: Clone + Send + Sync + 'static {
    /// The per-node output of this signal.
    ///
    /// `'static` is required for the [`CoreSignal::__materialize_special`]
    /// substrate reification path — substrate signals whose special
    /// dispatch produces a `BTreeMap<Distinction, Self::Output>` are
    /// reified into the caller's generic slot via `Any` downcast, which
    /// requires the target type to be `'static`. All practical outputs
    /// (`usize`, `bool`, `Option<ParentPair>`, consumer POD) satisfy this.
    type Output: Send + Sync + CanonicalBytes + 'static;

    /// Compute the signal for a specific node in the projection cone.
    ///
    /// `ctx` carries the engine borrow, projection root, direction, and
    /// boundary. `node` is the specific distinction being visited.
    /// Implementations MUST be a pure function of `(self, ctx, node)`.
    #[must_use]
    fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> Self::Output;

    /// Aggregate over the projection cone. Return `None` if this signal
    /// does not aggregate (default).
    ///
    /// The `cone` iterator yields the cone's distinctions in canonical
    /// (BTreeMap) order. `impl IntoIterator<Item = Distinction>`
    /// accepts both `Vec<Distinction>` and iterator-adapter forms. Note
    /// the parameter gives *membership only*, not per-node `S::Output`
    /// values — implementations that need per-node outputs must
    /// recompute them or draw from an external cache.
    ///
    /// Substrate-shipped signals do not override `compute_aggregate` in
    /// v2.0.0; the aggregate surface is reserved for consumer-driven
    /// and future substrate signals (e.g. `NoveltyRate`).
    #[must_use]
    fn compute_aggregate(
        &self,
        _ctx: &SignalContext<'_>,
        _cone: impl IntoIterator<Item = Distinction>,
    ) -> Option<Self::Output> {
        None
    }

    /// Canonical bytes for this signal instance. Feeds `ProjectionId`
    /// hashing (Cond B, §6) and the `signal_identity` wire-format
    /// header (§5).
    ///
    /// Returns a borrowed slice for zero-alloc access on ZST signals;
    /// consumer signals with instance state typically point at a
    /// self-owned buffer.
    #[must_use]
    fn canonical_bytes(&self) -> &[u8];
}

/// Sealed extension of [`Signal`]. Only substrate-shipped ZSTs
/// implement `CoreSignal`. Substrate methods that require verified
/// cross-engine byte-equivalence (Cond D) take `S: CoreSignal`; general
/// projection materialization methods take `S: Signal`.
///
/// Consumer signals cannot impl `CoreSignal` because `private::Sealed`
/// is unreachable from external crates.
pub trait CoreSignal: Signal + private::Sealed {
    /// Materialize-time special dispatch (substrate-internal). Signals
    /// whose per-node output cannot be expressed as a pure function of
    /// `(&self, ctx, node)` — e.g. [`HopDistance`], which needs BFS
    /// state carried across nodes — override this to compute the whole
    /// entry map at once.
    ///
    /// The default returns `None`; substrate signals inheriting the
    /// default fall through to the `compute()`-per-node path in
    /// [`ReadyBuilder::materialize`]. Consumer signals cannot reach
    /// this method (sealing).
    ///
    /// `#[doc(hidden)]` — not part of the stable public surface; the
    /// mechanism exists only to keep [`Signal::compute`] honest as a
    /// pure per-node function on the trait surface while still letting
    /// substrate carry stateful traversals internally.
    #[doc(hidden)]
    #[must_use]
    fn __materialize_special(
        _ctx: &SignalContext<'_>,
        _cone_with_hops: &[(Distinction, usize)],
    ) -> Option<SignalOutputs<Self>> {
        None
    }
}

// -------------------------------------------------------------------------
// Core signal ZSTs
// -------------------------------------------------------------------------

/// Signal: canonical parent pair of a node. `S::Output = Option<ParentPair>`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Adjacency;

/// Signal: degree of a node. `S::Output = usize`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Degree;

/// Signal: hop-distance from `ctx.root` to `node`, bounded by
/// `ctx.boundary`. `S::Output = usize`.
///
/// `HopDistance` is dispatched via a materialize-time special case —
/// it does NOT flow through `Signal::compute` in the usual way. See
/// [`ReadyBuilder::materialize`] for the dispatch decision.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct HopDistance;

impl private::Sealed for Adjacency {}
impl private::Sealed for Degree {}
impl private::Sealed for HopDistance {}

impl CoreSignal for Adjacency {}
impl CoreSignal for Degree {}
impl CoreSignal for HopDistance {
    fn __materialize_special(
        _ctx: &SignalContext<'_>,
        cone_with_hops: &[(Distinction, usize)],
    ) -> Option<SignalOutputs<Self>> {
        // Hop counts already computed by `traverse_cone` inside
        // `materialize` — thread them through instead of re-running BFS.
        Some(cone_with_hops.iter().copied().collect())
    }
}

/// Domain-separated byte tag for [`Adjacency`] — feeds the
/// `signal_identity` header and the `ADJACENCY_TYPE_ID` constant.
const ADJACENCY_TAG: &[u8] = b"koru:signal:v1:adjacency";
/// Domain-separated byte tag for [`Degree`].
const DEGREE_TAG: &[u8] = b"koru:signal:v1:degree";
/// Domain-separated byte tag for [`HopDistance`].
const HOPDISTANCE_TAG: &[u8] = b"koru:signal:v1:hop_distance";

impl Signal for Adjacency {
    type Output = Option<ParentPair>;

    fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> Self::Output {
        ctx.engine.parents_of(node)
    }

    fn canonical_bytes(&self) -> &[u8] {
        ADJACENCY_TAG
    }
}

impl Signal for Degree {
    type Output = usize;

    fn compute(&self, ctx: &SignalContext<'_>, node: Distinction) -> Self::Output {
        ctx.engine.degree(node)
    }

    fn canonical_bytes(&self) -> &[u8] {
        DEGREE_TAG
    }
}

impl Signal for HopDistance {
    type Output = usize;

    fn compute(&self, _ctx: &SignalContext<'_>, _node: Distinction) -> Self::Output {
        // `HopDistance` is `CoreSignal`-only. Every materialize path
        // in the substrate dispatches through
        // [`CoreSignal::__materialize_special`] before reaching a
        // per-node `compute` loop, so this arm is unreachable by
        // construction. If it fires, the dispatch in
        // [`ReadyBuilder::materialize`] regressed.
        unreachable!(
            "HopDistance is CoreSignal-only; dispatched via __materialize_special, not compute"
        )
    }

    fn canonical_bytes(&self) -> &[u8] {
        HOPDISTANCE_TAG
    }
}

/// Return the 16-byte precomputed `TYPE_ID` for a `CoreSignal`. Reduces
/// to a compiler-inlined constant lookup (no runtime SHA-256).
///
/// For general `S: Signal`, use [`signal_identity`] instead.
#[must_use]
pub fn type_id_of<S: CoreSignal>() -> [u8; 16] {
    let tid = TypeId::of::<S>();
    if tid == TypeId::of::<Adjacency>() {
        ADJACENCY_TYPE_ID
    } else if tid == TypeId::of::<Degree>() {
        DEGREE_TYPE_ID
    } else if tid == TypeId::of::<HopDistance>() {
        HOPDISTANCE_TYPE_ID
    } else {
        unreachable!("CoreSignal is sealed to Adjacency/Degree/HopDistance")
    }
}

/// Runtime signal identity — SHA-256 truncated to 16 bytes. Used by
/// the wire format ([`Projection::canonical_bytes`]) for general
/// `S: Signal`, and by [`DistinctionEngine::restore_projection`] to
/// enforce the `SignalMismatch` check.
///
/// Note: SHA-256 truncated to 16 bytes has a 2^64 birthday bound. A
/// hostile consumer signal whose `canonical_bytes()` is crafted to
/// collide with a substrate signal identity could pass the
/// `SignalMismatch` check even though the type parameter differs. This
/// is a documented Cond D disclosure for non-`CoreSignal` bytes and is
/// NOT a substrate correctness bug — cross-engine byte-equivalence for
/// consumer signals is a consumer-managed guarantee. Wire-shipped
/// projections requiring Cond D must use `S: CoreSignal`.
#[must_use]
pub fn signal_identity<S: Signal>(signal: &S) -> [u8; 16] {
    let mut h = Sha256::new();
    h.update(signal.canonical_bytes());
    let digest = h.finalize();
    let mut out = [0u8; 16];
    out.copy_from_slice(&digest[..16]);
    out
}

// -------------------------------------------------------------------------
// Axis types
// -------------------------------------------------------------------------

/// Traversal boundary — how far from `root` the projection materializes.
///
/// Wire-format tag `0x02` is reserved for a future `AtSynthesisCount(N)`
/// variant; the substrate carries no per-distinction synthesis-order
/// stamp today, so `AtSynthesisCount` cannot be honored without
/// breaking Law 9. It ships only if the substrate ever gains an
/// order-bearing extension.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Boundary {
    /// Traverse up to N hops from `root`. `N == 0` returns just `root`.
    Hops(usize),
    /// Traverse until the frontier saturates (no novel neighbors).
    Saturated,
}

/// Traversal direction relative to `root`.
#[non_exhaustive]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Direction {
    /// Walk parent edges (ancestors). Backed by `parents_of`.
    Upstream,
    /// Walk child edges (descendants). Backed by a reverse-index built
    /// once from `snapshot_parentage`.
    Downstream,
    /// Walk both — union of both frontiers per hop.
    Undirected,
}

// -------------------------------------------------------------------------
// ProjectionId — content-addressed identity
// -------------------------------------------------------------------------

/// Content-addressed 32-byte identity of a `ProjectionSpec<S>`. SHA-256
/// over the domain-separated, canonically ordered input set specified
/// in PROJECTION_SPEC §6 Cond B.
///
/// Note: `Distinction` is 16 bytes; `ProjectionId` is 32 bytes. The
/// widths differ intentionally.
#[must_use]
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ProjectionId(pub(crate) [u8; 32]);

impl ProjectionId {
    /// Borrow the underlying 32 bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl std::fmt::Debug for ProjectionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ProjectionId(")?;
        for b in &self.0 {
            write!(f, "{b:02x}")?;
        }
        write!(f, ")")
    }
}

// -------------------------------------------------------------------------
// ProjectionSpec / ProjectionOutput / SignalContext
// -------------------------------------------------------------------------

/// The identity of a projection — a `(root, boundary, direction, signal)`
/// tuple.
#[must_use]
#[derive(Clone)]
#[non_exhaustive]
pub struct ProjectionSpec<S: Signal> {
    /// The root distinction anchoring the projection.
    pub root: Distinction,
    /// Traversal boundary.
    pub boundary: Boundary,
    /// Traversal direction.
    pub direction: Direction,
    /// Signal instance driving per-node computation.
    pub signal: S,
}

/// The materialized entry map of a `Projection<'e, S>`. `BTreeMap`
/// gives canonical iteration order by `Distinction::Ord` (lexicographic
/// on the 16 bytes) — this is Cond C closure by construction.
#[must_use]
#[non_exhaustive]
pub struct ProjectionOutput<S: Signal> {
    pub(crate) inner: BTreeMap<Distinction, S::Output>,
}

impl<S: Signal> ProjectionOutput<S> {
    /// Number of entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.inner.len()
    }

    /// True if there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Iterate `(Distinction, &S::Output)` in canonical order.
    pub fn iter(&self) -> impl Iterator<Item = (Distinction, &S::Output)> + '_ {
        self.inner.iter().map(|(d, v)| (*d, v))
    }

    /// True iff `d` has an entry in this output.
    #[must_use]
    pub fn contains(&self, d: &Distinction) -> bool {
        self.inner.contains_key(d)
    }
}

/// Context passed to every `Signal::compute` call.
#[non_exhaustive]
pub struct SignalContext<'e> {
    /// Read-only engine borrow.
    pub engine: &'e (dyn ReadOnlyEngine + Send + Sync),
    /// Projection root.
    pub root: Distinction,
    /// Traversal direction.
    pub direction: Direction,
    /// Traversal boundary.
    pub boundary: Boundary,
}

// -------------------------------------------------------------------------
// ReadOnlyEngine trait
// -------------------------------------------------------------------------

/// Read-only projection engine surface. Sealed — external crates
/// cannot implement it. Extends `Send + Sync` so trait-objects cross
/// `.await` without per-use-site bounds.
///
/// Method inventory intentionally excludes `synthesize`,
/// `synthesize_novel`, and any other mutating access (Cond E closed
/// by construction).
///
/// `impl ReadOnlyEngine for DistinctionEngine` is additive: existing
/// `fn foo(engine: &DistinctionEngine)` consumer signatures compile
/// unchanged because inherent methods on `DistinctionEngine` win over
/// trait-method resolution.
pub trait ReadOnlyEngine: private::Sealed + Send + Sync {
    /// First primordial.
    fn d0(&self) -> Distinction;
    /// Second primordial.
    fn d1(&self) -> Distinction;
    /// Total distinction count (including primordials).
    fn distinction_count(&self) -> usize;
    /// Relationship count.
    fn relationship_count(&self) -> usize;
    /// Canonical `(min, max)` parent pair of a non-primordial.
    fn parents_of(&self, d: Distinction) -> Option<ParentPair>;
    /// Degree of a distinction.
    fn degree(&self, d: Distinction) -> usize;
    /// Membership check.
    fn has(&self, d: Distinction) -> bool;
    /// Owned snapshot of every registered distinction. Order
    /// unspecified — sort for determinism.
    fn snapshot_distinctions(&self) -> Vec<Distinction>;
    /// Owned snapshot of every parent-child relationship. Enables
    /// O(N) reverse-index construction for downstream/undirected
    /// traversal.
    fn snapshot_parentage(&self) -> Vec<(Distinction, ParentPair)>;
    /// Verify structural law `r = 2d − 3` holds.
    ///
    /// # Errors
    ///
    /// Returns [`InvariantError::BinaryParentageMismatch`] if the
    /// per-node parent-population count doesn't match the total.
    fn check_structural_invariant(&self) -> Result<(), InvariantError>;
}

// -------------------------------------------------------------------------
// Projection<'e, S>
// -------------------------------------------------------------------------

/// A materialized read of the append-only graph, bound to an engine
/// borrow — the read-dual object named in `THEORY.md`.
///
/// The `'e` lifetime binds the projection to its engine — a projection
/// cannot outlive its engine and cannot be re-associated with a
/// different engine. This closes cross-engine byte-injection at the
/// type level while preserving Cond D by construction.
#[must_use = "Projection carries an owned BTreeMap; drop it explicitly if intentional"]
#[non_exhaustive]
pub struct Projection<'e, S: Signal> {
    pub(crate) engine: &'e (dyn ReadOnlyEngine + Send + Sync),
    pub(crate) spec: ProjectionSpec<S>,
    pub(crate) output: ProjectionOutput<S>,
}

impl<'e, S: Signal> std::fmt::Debug for Projection<'e, S>
where
    S: std::fmt::Debug,
    S::Output: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Projection")
            .field("root", &self.spec.root)
            .field("boundary", &self.spec.boundary)
            .field("direction", &self.spec.direction)
            .field("signal", &self.spec.signal)
            .field("output_len", &self.output.inner.len())
            .finish_non_exhaustive()
    }
}

impl<'e, S: Signal> Projection<'e, S> {
    /// Compute the content-addressed identity per PROJECTION_SPEC §6
    /// Cond B.
    ///
    /// ```text
    /// SHA-256(
    ///   "koru:projection_id:v1"   // 21 bytes domain separator
    ///   || root.as_bytes()        // 16 bytes
    ///   || boundary_tag_u8        //  1 byte
    ///   || boundary_payload_le    //  8 bytes (Hops N; 0 if Saturated)
    ///   || direction_tag_u8       //  1 byte
    ///   || signal.canonical_bytes // variable
    /// )
    /// ```
    ///
    /// Cheap: SHA-256 over the fixed-shape spec header. Does NOT depend
    /// on cone contents — same across engines that produce the same
    /// spec. Order matches PROJECTION_SPEC §6 verbatim; the inline
    /// `projection_id_is_deterministic_across_engines` test is the
    /// drift guard.
    pub fn projection_id(&self) -> ProjectionId {
        let mut h = Sha256::new();
        h.update(b"koru:projection_id:v1");
        h.update(self.spec.root.as_bytes());
        let (btag, bpayload) = boundary_to_wire(self.spec.boundary);
        h.update([btag]);
        h.update(bpayload.to_le_bytes());
        h.update([direction_to_tag(self.spec.direction)]);
        h.update(self.spec.signal.canonical_bytes());
        let digest = h.finalize();
        let mut out = [0u8; 32];
        out.copy_from_slice(&digest);
        ProjectionId(out)
    }

    /// Serialize to canonical wire bytes per PROJECTION_SPEC §5.
    ///
    /// Panics only via `expect` if an entry's `S::Output` exceeds
    /// `u32::MAX` bytes — that path is exercised only by pathological
    /// consumer types. For a fallible version, see [`Self::try_canonical_bytes`].
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        self.try_canonical_bytes()
            .expect("canonical_bytes: entry output exceeds u32::MAX (invariant)")
    }

    /// Fallible wire serialize. Returns [`SerializeError::EntryTooLarge`]
    /// if any per-node output exceeds `u32::MAX` bytes.
    ///
    /// # Errors
    ///
    /// - [`SerializeError::EntryTooLarge`] when a single node's
    ///   `S::Output::canonical_bytes()` returns more than `u32::MAX`
    ///   bytes. The wire format's per-entry `output_len` field is
    ///   `u32 LE`; silently truncating would corrupt the stream.
    pub fn try_canonical_bytes(&self) -> Result<Vec<u8>, SerializeError> {
        let mut out = Vec::new();
        out.extend_from_slice(b"KPRJ");
        out.push(0x02);
        out.extend_from_slice(self.spec.root.as_bytes());
        let (btag, bpayload) = boundary_to_wire(self.spec.boundary);
        out.push(btag);
        out.extend_from_slice(&bpayload.to_le_bytes());
        out.push(direction_to_tag(self.spec.direction));
        out.extend_from_slice(&signal_identity(&self.spec.signal));

        // entry_count as u64 LE — Cond D width-safe on wasm32.
        let entry_count = self.output.inner.len() as u64;
        out.extend_from_slice(&entry_count.to_le_bytes());

        // Entries in `BTreeMap` order (canonical `Distinction::Ord`).
        for (d, v) in &self.output.inner {
            out.extend_from_slice(d.as_bytes());
            let vbytes = v.canonical_bytes();
            let output_len = u32::try_from(vbytes.len()).map_err(|_| {
                SerializeError::EntryTooLarge { distinction: *d, output_len: vbytes.len() }
            })?;
            out.extend_from_slice(&output_len.to_le_bytes());
            out.extend_from_slice(&vbytes);
        }

        Ok(out)
    }

    /// Borrow the underlying map of per-node signal outputs.
    pub const fn output(&self) -> &ProjectionOutput<S> {
        &self.output
    }

    /// Borrow the spec this projection was materialized against.
    pub const fn spec(&self) -> &ProjectionSpec<S> {
        &self.spec
    }

    /// Fast-path membership test — true iff `d` is in the cone.
    #[must_use]
    pub fn contains(&self, d: &Distinction) -> bool {
        self.output.contains(d)
    }

    /// The signal aggregate over the cone (see
    /// [`Signal::compute_aggregate`]). Returns `None` for non-aggregate
    /// signals.
    #[must_use]
    pub fn aggregate(&self) -> Option<S::Output> {
        let ctx = SignalContext {
            engine: self.engine,
            root: self.spec.root,
            direction: self.spec.direction,
            boundary: self.spec.boundary,
        };
        let cone: Vec<Distinction> = self.output.inner.keys().copied().collect();
        self.spec.signal.compute_aggregate(&ctx, cone)
    }
}

// -------------------------------------------------------------------------
// Errors
// -------------------------------------------------------------------------

/// Reasons `engine.restore_projection::<S>(bytes, signal)` can reject.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum RestoreError {
    /// Bytes malformed — magic, version, or length checks failed.
    #[error("wire bytes malformed at offset {offset}: {reason}")]
    Malformed {
        /// Byte offset where the malformation was detected.
        offset: usize,
        /// Static reason string.
        reason: &'static str,
    },

    /// One or more entry distinctions are not registered in this
    /// engine. Restore is only defined against engines that have
    /// synthesized the same history.
    #[error("distinction bytes not registered in restore engine (foreign entry)")]
    ForeignEngine,

    // NOTE: reserved for a future quiescence-detection variant; re-add
    // via `#[non_exhaustive]` when needed. The substrate cannot detect
    // engine quiescence in v2.0.0, so shipping the variant now would
    // be dead public API surface.
    /// The `S` type parameter passed to `restore_projection::<S>` does
    /// not match the `signal_identity` header in the bytes. Prevents
    /// type-confusion when bytes serialized as `Degree` are restored
    /// under `<Adjacency>` turbofish.
    ///
    /// Note: SHA-256 truncated to 16 bytes has a 2^64 birthday bound
    /// against hostile crafted `canonical_bytes()`. See
    /// [`signal_identity`] doc for the Cond D disclosure.
    #[error("signal type mismatch: expected {expected:?}, got {actual:?}")]
    SignalMismatch {
        /// Expected signal-identity bytes (computed from the caller's
        /// `signal` argument).
        expected: [u8; 16],
        /// Actual signal-identity bytes (parsed from the wire header).
        actual: [u8; 16],
    },

    /// A wire-format integer field exceeds `usize::MAX` on the current
    /// target. Real on `wasm32` where `usize` is 32-bit and a wire
    /// message from a 64-bit producer may legally carry values >
    /// `u32::MAX`.
    #[error("wire {field} value {value} exceeds usize::MAX on this target")]
    IntegerOverflow {
        /// The wire field that overflowed (e.g., `"entry_count"`,
        /// `"boundary_payload"`).
        field: &'static str,
        /// The raw u64 value read from the wire.
        value: u64,
    },
}

/// Explicit `From` mapping from [`VerifyError`] to [`RestoreError`], so
/// [`DistinctionEngine::restore_projection`] can use the `?` operator on
/// [`DistinctionEngine::verify`] and preserve the existing
/// [`RestoreError::ForeignEngine`] semantics.
///
/// The match is **explicit** (not a wildcard or closure) so any future
/// [`VerifyError`] variant landing under `#[non_exhaustive]` triggers a
/// compile error here — the correct place to force the mapping decision,
/// not a silent swallow.
impl From<VerifyError> for RestoreError {
    fn from(err: VerifyError) -> Self {
        match err {
            VerifyError::ForeignBytes => RestoreError::ForeignEngine,
            // Future `VerifyError` variants (e.g. `WrongEngine`,
            // `WrongDomain`) will fail to compile here until this
            // match is extended. Do NOT collapse to a wildcard arm.
        }
    }
}

/// Reasons [`Projection::try_canonical_bytes`] can reject.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[non_exhaustive]
pub enum SerializeError {
    /// A single node's `S::Output::canonical_bytes()` returned more
    /// than `u32::MAX` bytes. The wire format's `output_len` field is
    /// `u32 LE`; silently truncating would corrupt the stream.
    #[error("entry for distinction {distinction:?} exceeds u32::MAX bytes: {output_len}")]
    EntryTooLarge {
        /// The distinction whose entry overflowed.
        distinction: Distinction,
        /// The oversized output length.
        output_len: usize,
    },
}

// -------------------------------------------------------------------------
// Wire tag helpers
// -------------------------------------------------------------------------

/// Encode a `Boundary` to its wire (tag, payload) pair.
const fn boundary_to_wire(b: Boundary) -> (u8, u64) {
    match b {
        Boundary::Hops(n) => (0x00, n as u64),
        Boundary::Saturated => (0x01, 0),
    }
}

/// Encode a `Direction` to its wire tag.
const fn direction_to_tag(d: Direction) -> u8 {
    match d {
        Direction::Upstream => 0x00,
        Direction::Downstream => 0x01,
        Direction::Undirected => 0x02,
    }
}

// -------------------------------------------------------------------------
// Cone traversal
// -------------------------------------------------------------------------

/// Reverse (children) index built once from `snapshot_parentage` during
/// downstream / undirected traversal.
type ChildrenIndex = BTreeMap<Distinction, Vec<Distinction>>;

/// BFS the cone rooted at `ctx.root` in `ctx.direction`, bounded by
/// `ctx.boundary`. Returns `(distinction, hops)` pairs in visit order.
/// The `hops` payload is what [`HopDistance`] surfaces via
/// [`CoreSignal::__materialize_special`]; the [`ReadyBuilder::materialize`]
/// compute-loop callers drop it.
///
/// - `Upstream`: walk `parents_of` recursively.
/// - `Downstream`: build a reverse (children) index once from
///   `snapshot_parentage`, then walk it.
/// - `Undirected`: union of both.
fn traverse_cone(ctx: &SignalContext<'_>) -> Vec<(Distinction, usize)> {
    let mut visited: HashSet<[u8; 16]> = HashSet::new();
    let mut queue: VecDeque<(Distinction, usize)> = VecDeque::new();
    let mut cone: Vec<(Distinction, usize)> = Vec::new();

    // Root must be registered — a foreign root yields an empty cone.
    if !ctx.engine.has(ctx.root) {
        return cone;
    }

    // Build children index only if direction requires it.
    let children_index: Option<ChildrenIndex> = match ctx.direction {
        Direction::Downstream | Direction::Undirected => {
            let mut idx: ChildrenIndex = BTreeMap::new();
            for (child, (p1, p2)) in ctx.engine.snapshot_parentage() {
                idx.entry(p1).or_default().push(child);
                if p1 != p2 {
                    idx.entry(p2).or_default().push(child);
                }
            }
            Some(idx)
        },
        Direction::Upstream => None,
    };

    let max_hops = match ctx.boundary {
        Boundary::Hops(n) => Some(n),
        Boundary::Saturated => None,
    };

    queue.push_back((ctx.root, 0));
    visited.insert(*ctx.root.as_bytes());
    cone.push((ctx.root, 0));

    while let Some((node, hops)) = queue.pop_front() {
        if let Some(mh) = max_hops {
            if hops >= mh {
                continue;
            }
        }

        let mut neighbors: Vec<Distinction> = Vec::new();
        match ctx.direction {
            Direction::Upstream => {
                if let Some((p1, p2)) = ctx.engine.parents_of(node) {
                    neighbors.push(p1);
                    if p1 != p2 {
                        neighbors.push(p2);
                    }
                }
            },
            Direction::Downstream => {
                if let Some(idx) = children_index.as_ref() {
                    if let Some(children) = idx.get(&node) {
                        neighbors.extend(children.iter().copied());
                    }
                }
            },
            Direction::Undirected => {
                if let Some((p1, p2)) = ctx.engine.parents_of(node) {
                    neighbors.push(p1);
                    if p1 != p2 {
                        neighbors.push(p2);
                    }
                }
                if let Some(idx) = children_index.as_ref() {
                    if let Some(children) = idx.get(&node) {
                        neighbors.extend(children.iter().copied());
                    }
                }
            },
        }

        for n in neighbors {
            if visited.insert(*n.as_bytes()) {
                cone.push((n, hops + 1));
                queue.push_back((n, hops + 1));
            }
        }
    }

    cone
}

// -------------------------------------------------------------------------
// Builder typestates
// -------------------------------------------------------------------------

/// Typestate marker: builder needs `direction()` next.
pub struct NeedsDirection;

/// Typestate marker: builder needs `boundary()` next.
pub struct NeedsBoundary;

/// Typestate marker: builder needs `signal()` next.
pub struct NeedsSignal;

/// Fluent projection builder — encodes the (direction, boundary,
/// signal) progression at compile time via typestate. Only a fully
/// specified builder exposes `.materialize()`.
#[must_use]
pub struct ProjectionBuilder<'e, State> {
    pub(crate) engine: &'e (dyn ReadOnlyEngine + Send + Sync),
    pub(crate) root: Distinction,
    pub(crate) direction: Option<Direction>,
    pub(crate) boundary: Option<Boundary>,
    pub(crate) _state: PhantomData<State>,
}

/// Terminal builder state — signal instance is carried by value so
/// `materialize()` can move it into the resulting `Projection`.
#[must_use]
pub struct ReadyBuilder<'e, S: Signal> {
    pub(crate) engine: &'e (dyn ReadOnlyEngine + Send + Sync),
    pub(crate) root: Distinction,
    pub(crate) direction: Direction,
    pub(crate) boundary: Boundary,
    pub(crate) signal: S,
}

impl<'e> ProjectionBuilder<'e, NeedsDirection> {
    /// Set the traversal direction. Advances to the `NeedsBoundary`
    /// typestate.
    pub fn direction(self, direction: Direction) -> ProjectionBuilder<'e, NeedsBoundary> {
        ProjectionBuilder {
            engine: self.engine,
            root: self.root,
            direction: Some(direction),
            boundary: None,
            _state: PhantomData,
        }
    }
}

impl<'e> ProjectionBuilder<'e, NeedsBoundary> {
    /// Set the traversal boundary. Advances to `NeedsSignal`.
    pub fn boundary(self, boundary: Boundary) -> ProjectionBuilder<'e, NeedsSignal> {
        ProjectionBuilder {
            engine: self.engine,
            root: self.root,
            direction: self.direction,
            boundary: Some(boundary),
            _state: PhantomData,
        }
    }

    /// Convenience for `boundary(Boundary::Hops(n))`.
    pub fn hops(self, n: usize) -> ProjectionBuilder<'e, NeedsSignal> {
        self.boundary(Boundary::Hops(n))
    }

    /// Convenience for `boundary(Boundary::Saturated)`.
    pub fn saturated(self) -> ProjectionBuilder<'e, NeedsSignal> {
        self.boundary(Boundary::Saturated)
    }
}

impl<'e> ProjectionBuilder<'e, NeedsSignal> {
    /// Pass the signal by value. Supports parameterized signals as
    /// well as ZST literals (`.signal(Adjacency)`).
    pub fn signal<S: Signal>(self, signal: S) -> ReadyBuilder<'e, S> {
        ReadyBuilder {
            engine: self.engine,
            root: self.root,
            direction: self.direction.expect("NeedsSignal implies direction set (invariant)"),
            boundary: self.boundary.expect("NeedsSignal implies boundary set (invariant)"),
            signal,
        }
    }
}

impl<'e, S: Signal> ReadyBuilder<'e, S> {
    /// Execute the traversal and produce the projection.
    ///
    /// Cost: O(cone size × signal-per-node cost). Downstream and
    /// undirected directions add an O(N) reverse-parent index build
    /// exactly once per materialize — `traverse_cone` runs a single
    /// BFS, and hop counts flow through to `__materialize_special`
    /// rather than triggering a second traversal.
    ///
    /// # Dispatch order
    ///
    /// 1. Compute the cone via [`traverse_cone`], yielding
    ///    `(Distinction, hop)` pairs.
    /// 2. If `S: CoreSignal` and the sealed
    ///    [`CoreSignal::__materialize_special`] returns `Some(map)`,
    ///    use it — the substrate signal owns whole-cone dispatch
    ///    (e.g. [`HopDistance`] reuses the hop counts already
    ///    carried in the pairs).
    /// 3. Otherwise fall through to the `compute()`-per-node loop.
    ///
    /// `SignalContext` is constructed at materialize time — Cond A
    /// holds either way (the context is engine-history-derived, so its
    /// construction site is a policy call, not a correctness one).
    pub fn materialize(self) -> Projection<'e, S> {
        let ctx = SignalContext {
            engine: self.engine,
            root: self.root,
            direction: self.direction,
            boundary: self.boundary,
        };

        let cone_pairs = traverse_cone(&ctx);

        // `Signal` does not itself expose `__materialize_special`;
        // dispatch is via the sealed `CoreSignal` extension. Consumer
        // `S: Signal` (non-CoreSignal) always uses the compute() path.
        // `S: CoreSignal` inheriting the default `None` also falls
        // through. Only `HopDistance`'s override returns `Some`.
        let inner: BTreeMap<Distinction, S::Output> = special_dispatch::<S>(&ctx, &cone_pairs)
            .unwrap_or_else(|| {
                cone_pairs.iter().map(|(d, _)| (*d, self.signal.compute(&ctx, *d))).collect()
            });

        Projection {
            engine: self.engine,
            spec: ProjectionSpec {
                root: self.root,
                boundary: self.boundary,
                direction: self.direction,
                signal: self.signal,
            },
            output: ProjectionOutput { inner },
        }
    }
}

/// Dispatch to [`CoreSignal::__materialize_special`] for the one
/// substrate signal that overrides the default ([`HopDistance`]). All
/// other `S: Signal` fall through to `None`, and materialize takes the
/// `compute()`-per-node path.
///
/// `ReadyBuilder<S>` is generic over `S: Signal` (not `S: CoreSignal`)
/// so the sealed dispatch is gated via runtime `TypeId`. The reify
/// step (`BTreeMap<Distinction, usize>` → `BTreeMap<Distinction,
/// S::Output>`) uses `Any` downcast; `TypeId::of::<S>() ==
/// TypeId::of::<HopDistance>()` proves the target and source types are
/// structurally identical, so the downcast is infallible.
fn special_dispatch<S: Signal>(
    ctx: &SignalContext<'_>,
    cone_with_hops: &[(Distinction, usize)],
) -> Option<SignalOutputs<S>> {
    if TypeId::of::<S>() != TypeId::of::<HopDistance>() {
        return None;
    }
    let map = <HopDistance as CoreSignal>::__materialize_special(ctx, cone_with_hops)?;
    let boxed: Box<dyn std::any::Any> = Box::new(map);
    let downcast: Box<SignalOutputs<S>> =
        boxed.downcast().expect("TypeId proves S == HopDistance downcast (invariant)");
    Some(*downcast)
}

// -------------------------------------------------------------------------
// DistinctionEngine inherent extensions
// -------------------------------------------------------------------------

impl DistinctionEngine {
    /// Start a projection anchored at `root`. Returns a builder in the
    /// `NeedsDirection` typestate.
    pub fn project(&self, root: Distinction) -> ProjectionBuilder<'_, NeedsDirection> {
        ProjectionBuilder {
            engine: self,
            root,
            direction: None,
            boundary: None,
            _state: PhantomData,
        }
    }

    /// Alternate constructor for consumers holding an
    /// `Arc<DistinctionEngine>`. Returns a builder that borrows the
    /// engine through the `Arc` for the duration of the projection.
    ///
    /// The signature takes `&'a Arc<Self>` (not `Arc<Self>` by value) —
    /// carrying the `Arc` by value would move it, breaking the
    /// caller's ability to keep it. `&'a Arc<Self>` derefs to
    /// `&'a Self`, which coerces to `&'a (dyn ReadOnlyEngine + Send + Sync)`.
    pub fn project_arc<'a>(
        self: &'a Arc<Self>,
        root: Distinction,
    ) -> ProjectionBuilder<'a, NeedsDirection> {
        ProjectionBuilder {
            engine: &**self,
            root,
            direction: None,
            boundary: None,
            _state: PhantomData,
        }
    }

    /// Restore a projection from wire bytes.
    ///
    /// `signal` is required because parameterized signals cannot be
    /// reconstructed from `S: Default`; ZST core signals accept e.g.
    /// `Adjacency` as a zero-cost literal.
    ///
    /// The current implementation is "restore = re-materialize +
    /// byte-verify": parse the header, re-materialize against `self`,
    /// then compare canonical bytes. This closes both `SignalMismatch`
    /// (via the header check) and `ForeignEngine` (via the byte
    /// comparison) without a full entry-parser — the entries in `bytes`
    /// serve as proof that `self` can reproduce them.
    ///
    /// Wire bytes act as **proof-of-history** — they let a peer engine
    /// verify it can reproduce the same projection. They are NOT
    /// portable data for direct deserialization by a stranger engine;
    /// Cond D provides byte-equivalence only for engines sharing
    /// synthesis history.
    ///
    /// # Errors
    ///
    /// - [`RestoreError::Malformed`] — magic/version/length checks fail.
    /// - [`RestoreError::SignalMismatch`] — the header signal-identity
    ///   differs from `signal_identity(&signal)`.
    /// - [`RestoreError::IntegerOverflow`] — a `u64` wire field
    ///   exceeds `usize::MAX` on this target.
    /// - [`RestoreError::ForeignEngine`] — the root isn't registered,
    ///   or re-materialization produces bytes that differ from `bytes`.
    pub fn restore_projection<S: Signal>(
        &self,
        bytes: &[u8],
        signal: S,
    ) -> Result<Projection<'_, S>, RestoreError> {
        // Header: 55 bytes fixed.
        if bytes.len() < 55 {
            return Err(RestoreError::Malformed {
                offset: 0,
                reason: "wire bytes shorter than 55-byte header",
            });
        }
        if &bytes[0..4] != b"KPRJ" {
            return Err(RestoreError::Malformed { offset: 0, reason: "bad magic" });
        }
        if bytes[4] != 0x02 {
            return Err(RestoreError::Malformed { offset: 4, reason: "unsupported version" });
        }

        let mut root_bytes = [0u8; 16];
        root_bytes.copy_from_slice(&bytes[5..21]);
        // Trust boundary: wire bytes cross into engine-verified
        // `Distinction` here. The `?` operator threads `VerifyError`
        // into `RestoreError::ForeignEngine` via `impl From<VerifyError>
        // for RestoreError` above — preserving the pre-S04
        // `ForeignEngine` mapping while replacing the
        // `from_bytes_unchecked` + guarded-by-`has()` pattern.
        let raw = RawDistinctionId::from_bytes(root_bytes);
        let root = self.verify(raw)?;

        let btag = bytes[21];
        let bpayload_bytes: [u8; 8] = bytes[22..30]
            .try_into()
            .expect("slice-to-array conversion for fixed range (invariant)");
        let bpayload = u64::from_le_bytes(bpayload_bytes);
        let boundary = match btag {
            0x00 => {
                let n = usize::try_from(bpayload).map_err(|_| RestoreError::IntegerOverflow {
                    field: "boundary_payload",
                    value: bpayload,
                })?;
                Boundary::Hops(n)
            },
            0x01 => Boundary::Saturated,
            _ => {
                return Err(RestoreError::Malformed { offset: 21, reason: "unknown boundary tag" })
            },
        };

        let dtag = bytes[30];
        let direction = match dtag {
            0x00 => Direction::Upstream,
            0x01 => Direction::Downstream,
            0x02 => Direction::Undirected,
            _ => {
                return Err(RestoreError::Malformed { offset: 30, reason: "unknown direction tag" })
            },
        };

        let mut sig_id = [0u8; 16];
        sig_id.copy_from_slice(&bytes[31..47]);
        let expected = signal_identity(&signal);
        if sig_id != expected {
            return Err(RestoreError::SignalMismatch { expected, actual: sig_id });
        }

        let entry_count_bytes: [u8; 8] = bytes[47..55]
            .try_into()
            .expect("slice-to-array conversion for fixed range (invariant)");
        let entry_count_u64 = u64::from_le_bytes(entry_count_bytes);
        // Width probe: if the wire's u64 entry_count exceeds usize::MAX
        // on this target (real on wasm32), reject before the
        // re-materialize path. The re-materialize step is what
        // produces the definitive entry count.
        if usize::try_from(entry_count_u64).is_err() {
            return Err(RestoreError::IntegerOverflow {
                field: "entry_count",
                value: entry_count_u64,
            });
        }

        // Foreign-root guard already discharged above via `self.verify(raw)?`
        // — the `Distinction` value here is engine-witnessed by construction.

        // Re-materialize and verify byte-equivalence.
        let proj = ReadyBuilder { engine: self, root, direction, boundary, signal }.materialize();
        let regenerated = proj.canonical_bytes();
        if regenerated != bytes {
            return Err(RestoreError::ForeignEngine);
        }

        Ok(proj)
    }
}

// -------------------------------------------------------------------------
// Inline tests
// -------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DistinctionEngine;

    // -------------------------------------------------------------------
    // Variance / covariance compile probe (PLAN_V2 item 10)
    // -------------------------------------------------------------------
    //
    // Concrete compile check that `Projection<'e, S>` is covariant in
    // `'e`. If this function compiles AND `projection_is_covariant_in_engine_lifetime`
    // exercises it, projections behave the way references do under
    // lifetime subtyping. Failure to compile at a later refactor is a
    // deliberate variance change and must be reviewed as such. Living in
    // `#[cfg(test)]` with a real caller makes it a compile-time test
    // rather than dead production code — no `#[allow(dead_code)]`
    // suppression required.
    fn narrow_lifetime<'long: 'short, 'short, S: Signal>(
        p: Projection<'long, S>,
    ) -> Projection<'short, S> {
        p
    }

    #[test]
    fn projection_is_covariant_in_engine_lifetime() {
        let e = DistinctionEngine::new();
        let proj = e
            .project(e.d0())
            .direction(Direction::Upstream)
            .hops(0)
            .signal(Adjacency)
            .materialize();
        // Calling `narrow_lifetime` proves `Projection<'e, S>` accepts
        // covariant narrowing of `'e` — the compile is the test. The
        // runtime `contains` check ensures the returned value is still
        // structurally identical after the lifetime narrowing.
        let narrowed = narrow_lifetime(proj);
        assert!(narrowed.contains(&e.d0()));
    }

    #[test]
    fn type_id_constants_match_runtime_sha() {
        assert_eq!(type_id_of::<Adjacency>(), signal_identity(&Adjacency));
        assert_eq!(type_id_of::<Degree>(), signal_identity(&Degree));
        assert_eq!(type_id_of::<HopDistance>(), signal_identity(&HopDistance));
    }

    #[test]
    fn projection_id_is_deterministic_across_engines() {
        let e1 = DistinctionEngine::new();
        let e2 = DistinctionEngine::new();
        let c1 = e1.synthesize(e1.d0(), e1.d1());
        let c2 = e2.synthesize(e2.d0(), e2.d1());
        assert_eq!(c1, c2);
        let p1 =
            e1.project(c1).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
        let p2 =
            e2.project(c2).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
        assert_eq!(p1.projection_id(), p2.projection_id());
    }

    #[test]
    fn canonical_bytes_deterministic_across_engines() {
        let e1 = DistinctionEngine::new();
        let e2 = DistinctionEngine::new();
        let c1 = e1.synthesize(e1.d0(), e1.d1());
        let c2 = e2.synthesize(e2.d0(), e2.d1());
        let p1 =
            e1.project(c1).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
        let p2 =
            e2.project(c2).direction(Direction::Upstream).hops(2).signal(Adjacency).materialize();
        assert_eq!(p1.canonical_bytes(), p2.canonical_bytes());
    }

    #[test]
    fn upstream_cone_contains_root_and_parents() {
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let cc = e.synthesize(c, e.d0());
        let proj = e
            .project(cc)
            .direction(Direction::Upstream)
            .saturated()
            .signal(Adjacency)
            .materialize();
        assert!(proj.contains(&cc));
        assert!(proj.contains(&c));
        assert!(proj.contains(&e.d0()));
    }

    #[test]
    fn downstream_cone_walks_children() {
        let e = DistinctionEngine::new();
        let c1 = e.synthesize(e.d0(), e.d1());
        let c2 = e.synthesize(c1, e.d0());
        let proj = e
            .project(e.d0())
            .direction(Direction::Downstream)
            .saturated()
            .signal(Adjacency)
            .materialize();
        assert!(proj.contains(&e.d0()));
        assert!(proj.contains(&c1));
        assert!(proj.contains(&c2));
    }

    #[test]
    fn hops_zero_returns_just_root() {
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let proj =
            e.project(c).direction(Direction::Upstream).hops(0).signal(Adjacency).materialize();
        assert!(proj.contains(&c));
        assert_eq!(proj.output().len(), 1);
    }

    #[test]
    fn wire_format_header_shape() {
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let proj = e
            .project(c)
            .direction(Direction::Downstream)
            .saturated()
            .signal(Adjacency)
            .materialize();
        let bytes = proj.canonical_bytes();
        assert!(bytes.len() >= 55);
        assert_eq!(&bytes[0..4], b"KPRJ");
        assert_eq!(bytes[4], 0x02);
        assert_eq!(&bytes[5..21], c.as_bytes());
        assert_eq!(bytes[21], 0x01); // Boundary::Saturated
        assert_eq!(bytes[30], 0x01); // Direction::Downstream
    }

    #[test]
    fn signal_mismatch_detected_on_restore() {
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let proj =
            e.project(c).direction(Direction::Upstream).hops(1).signal(Adjacency).materialize();
        let bytes = proj.canonical_bytes();
        // Try to restore under Degree — should fail with SignalMismatch.
        let err = e
            .restore_projection::<Degree>(&bytes, Degree)
            .expect_err("restore under wrong signal must fail");
        assert!(matches!(err, RestoreError::SignalMismatch { .. }));
    }

    #[test]
    fn malformed_bytes_rejected() {
        let e = DistinctionEngine::new();
        let bad: [u8; 4] = [0, 0, 0, 0];
        let err = e
            .restore_projection::<Adjacency>(&bad, Adjacency)
            .expect_err("restore on malformed bytes must fail");
        assert!(matches!(err, RestoreError::Malformed { .. }));
    }

    // Uses a 64-bit-wide literal, which overflows `usize` on wasm32
    // (`usize == u32` there). The wire-format guarantee itself is
    // target-independent — `CanonicalBytes for usize` widens to `u64` LE
    // — and the native test above proves it holds. Gating this out on
    // wasm32 rather than narrowing the literal keeps the 8-byte
    // assertion honest on the platform where `usize` actually is 8
    // bytes.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn canonicalbytes_usize_is_8_bytes_le() {
        let v: usize = 0x0102_0304_0506_0708;
        let bytes = v.canonical_bytes();
        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes, vec![0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01]);
    }

    #[test]
    fn canonicalbytes_bool_is_1_byte() {
        assert_eq!(true.canonical_bytes(), vec![1u8]);
        assert_eq!(false.canonical_bytes(), vec![0u8]);
    }

    #[test]
    fn canonicalbytes_option_parentpair_none_is_1_byte() {
        let none: Option<ParentPair> = None;
        assert_eq!(none.canonical_bytes(), vec![0u8]);
    }

    #[test]
    fn read_only_engine_dyn_object_dispatches() {
        let e = DistinctionEngine::new();
        let engine_dyn: &(dyn ReadOnlyEngine + Send + Sync) = &e;
        assert_eq!(engine_dyn.distinction_count(), 2);
        assert!(engine_dyn.has(e.d0()));
    }

    #[test]
    fn hop_distance_materializes_correct_per_node_hops() {
        // Two-level chain: d0 → c → cc. Upstream from cc: cc is 0
        // hops from itself; c is 1 hop; each parent of c is 2 hops.
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let cc = e.synthesize(c, e.d0());
        let proj = e
            .project(cc)
            .direction(Direction::Upstream)
            .saturated()
            .signal(HopDistance)
            .materialize();
        let map: BTreeMap<Distinction, usize> =
            proj.output().iter().map(|(d, v)| (d, *v)).collect();
        assert_eq!(map.get(&cc).copied(), Some(0));
        assert_eq!(map.get(&c).copied(), Some(1));
        // The primordials are 2 hops from cc — one via c.
        assert_eq!(map.get(&e.d0()).copied(), Some(1)); // also direct parent of cc
        assert_eq!(map.get(&e.d1()).copied(), Some(2));
    }

    #[test]
    fn hop_distance_bounded_by_hops_boundary() {
        let e = DistinctionEngine::new();
        let c = e.synthesize(e.d0(), e.d1());
        let cc = e.synthesize(c, e.d0());
        let proj =
            e.project(cc).direction(Direction::Upstream).hops(1).signal(HopDistance).materialize();
        // At hops=1, cone contains cc (0 hops) and its immediate
        // parents (1 hop). d1 (2 hops away) is excluded.
        assert!(proj.contains(&cc));
        assert!(proj.contains(&c));
        assert!(proj.contains(&e.d0()));
        assert!(!proj.contains(&e.d1()));
    }
}
