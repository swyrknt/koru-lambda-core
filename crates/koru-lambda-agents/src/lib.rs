#![forbid(unsafe_code)]
//! Reference subsystems — consumer patterns built atop the substrate.
//!
//! These are **not substrate**. They demonstrate idiomatic LCA-pattern
//! consumers — see `THEORY.md#using-the-substrate` — with the v1.2 audit
//! findings designed away structurally, not runtime-checked. Consumer teams
//! should read these as canonical patterns and build their own analogous
//! subsystems.
//!
//! *This paragraph cited `THEORY.md` §13-15 until 2026-10-31. That document
//! has no numbered sections at all; the section numbers belong to a different
//! repository's theory document. `a83b043` removed the same false citation
//! from [`agent`] and touched only that file, so it survived here and in one
//! comment inside the file it fixed. No gate catches it —
//! `scripts/gates/doc_anchors.sh` resolves symbol anchors, not section
//! references.*
//!
//! # The consumer surface
//!
//! Every consumer-facing function in this crate takes an [`Lca`], not an
//! engine. **That is the force, and it is at this boundary.** The substrate's
//! algebra stays public for tests, tools and witnesses — `docs/SPEC.md`
//! specifies the algebra, a conforming implementation in another language
//! implements the algebra, and `conformance/reference/substrate.py` models no
//! consumer type at all. A consumer that would rather call `synthesize`
//! directly still can, and gets no help from this crate.
//!
//! - [`lca`] — the consumer surface: one witness, one root, one discipline.
//! - [`validator`] — consensus batch validation with atomic-failure
//!   semantics (V3/V4/V5/V6/V8 closed by construction).
//! - [`commitment`] — cryptographic batch commitments with leader_id
//!   in the hash (N6/F7 closed by construction) + Stage-2 fetch cache.
//!
//! It also carries the consumer patterns that used to ship inside the
//! substrate. None of them determines an identifier — delete any one and two
//! conforming implementations still agree on every byte — so by the
//! substrate's own membership test they belong here:
//!
//! - [`agent`] — the local-causal-agent contract. The substrate keeps
//!   `advances_from`, which is the *read* that makes a forward advance
//!   checkable; the contract that chooses to advance is a consumer's.
//! - [`recorder`] — a chronological observer of novel syntheses. The engine
//!   carries no order-bearing state, because order is what consumers do.
//! - [`children_index`] — the inverse of `parents_of`, materialized.

pub mod agent;
pub mod children_index;
pub mod commitment;
pub mod lca;
pub mod recorder;
pub mod validator;

pub use agent::{synthesize_causal_action, LocalCausalAgent};
pub use children_index::build_children_index;
pub use commitment::{
    BatchCommitment, BatchDataRequest, BatchDataResponse, CommitmentAgent, CommitmentError,
};
pub use lca::Lca;
pub use recorder::SynthesisRecorder;
pub use validator::{ConsensusValidator, TransactionAction, TransactionBatch, ValidatorError};

/// Domain labels for this crate's anchored folds, so a transaction and a
/// commitment carrying identical bytes fold to different distinctions.
pub(crate) const DOMAIN_TRANSACTION: koru_lambda_core::DomainLabel =
    koru_lambda_core::DomainLabel::new(0x01);
/// See [`DOMAIN_TRANSACTION`].
pub(crate) const DOMAIN_COMMITMENT: koru_lambda_core::DomainLabel =
    koru_lambda_core::DomainLabel::new(0x02);
