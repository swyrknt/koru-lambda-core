#![forbid(unsafe_code)]
//! Reference subsystems — consumer patterns built atop the substrate.
//!
//! These are **not substrate**. They demonstrate idiomatic LCA-pattern
//! consumers (see `THEORY.md` §13-15) with the v1.2 audit findings
//! designed away structurally, not runtime-checked. Consumer teams
//! (ALIS, koru-protocol) should read these as canonical patterns and
//! build their own analogous subsystems.
//!
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
pub mod recorder;
pub mod validator;

pub use agent::{synthesize_causal_action, LocalCausalAgent};
pub use children_index::build_children_index;
pub use commitment::{
    BatchCommitment, BatchDataRequest, BatchDataResponse, CommitmentAgent, CommitmentError,
};
pub use recorder::SynthesisRecorder;
pub use validator::{ConsensusValidator, TransactionAction, TransactionBatch, ValidatorError};

/// Domain labels for this crate's anchored folds, so a transaction and a
/// commitment carrying identical bytes fold to different distinctions.
pub(crate) const DOMAIN_TRANSACTION: koru_lambda_core::DomainLabel = 0x01;
/// See [`DOMAIN_TRANSACTION`].
pub(crate) const DOMAIN_COMMITMENT: koru_lambda_core::DomainLabel = 0x02;
