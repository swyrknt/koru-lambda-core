//! [`SynthesisRecorder`] — chronological observer of novel syntheses.
//!
//! Consumer pattern, **not substrate**. The engine carries no order-bearing
//! state — time is what consumers do. Consumers that want a chronological
//! record of distinctions they've caused construct a `SynthesisRecorder`
//! and route their `synthesize` calls through it.
//!
//! # `!Send + !Sync` by design
//!
//! The recorder uses [`PhantomData<*const ()>`] to make itself
//! non-thread-safe at compile time. Under concurrent use, the
//! novelty-check via `log.contains(...)` would race: another thread's
//! novel synthesis could insert into the engine between this thread's
//! read of the log and its push. The `!Send + !Sync` marker prevents
//! that misuse from compiling.
//!
//! Consumers that need multi-threaded recording wrap a recorder in
//! their own `Mutex<SynthesisRecorder>` and accept the lock cost —
//! the substrate doesn't ship the wrapper because the right discipline
//! is workload-dependent.

use crate::{Distinction, DistinctionEngine, IdentityBuildHasher};
use std::collections::HashSet;
use std::marker::PhantomData;

/// Chronological observer of novel syntheses through an engine.
///
/// # Not an LCA
///
/// `LocalCausalAgent` anchors to a local root and evolves causally
/// (`update_local_root`). `SynthesisRecorder` is a passive observer —
/// it has no perspective of its own, it just records what passed
/// through its [`synthesize`](SynthesisRecorder::synthesize) wrapper.
/// Consumers that want LCA-style chronological evolution should build
/// their own LCA implementation; this is the simpler "diagnostic log"
/// pattern.
///
/// # Single-thread contract
///
/// `!Send + !Sync` enforced at compile time via
/// [`PhantomData<*const ()>`]. Sharing across threads is a build
/// error; concurrent use within a thread is not possible because
/// `synthesize` takes `&mut self`.
pub struct SynthesisRecorder {
    /// Chronological order of novel observations. Each entry appears
    /// exactly once; presence is mirrored in `seen` for O(1) dedup.
    log: Vec<Distinction>,
    /// Shadow set for O(1) dedup. `IdentityBuildHasher` reads the
    /// leading 8 bytes of the 16-byte distinction identity as the
    /// hash key (SHA-256 prefixes are uniformly distributed).
    seen: HashSet<Distinction, IdentityBuildHasher>,
    /// Marker that makes this `!Send + !Sync`. The `*const ()` choice
    /// is the canonical idiom (used internally by `Rc`); negative
    /// impls (`impl !Send`) require nightly.
    _not_thread_safe: PhantomData<*const ()>,
}

impl SynthesisRecorder {
    /// Create an empty recorder.
    #[must_use]
    pub fn new() -> Self {
        Self {
            log: Vec::new(),
            seen: HashSet::with_hasher(IdentityBuildHasher::default()),
            _not_thread_safe: PhantomData,
        }
    }

    /// Synthesize `a` ⊗ `b` through `engine`, recording the result in
    /// the log if it has not been seen by this recorder before.
    ///
    /// Deduplication via a `HashSet<Distinction, IdentityBuildHasher>`
    /// shadow — O(1) per call. The shadow shares the substrate's
    /// `IdentityBuildHasher` so dedup is cheap on dense workloads.
    ///
    /// **Single-thread only** — `!Send + !Sync` enforced at compile
    /// time.
    #[must_use]
    pub fn synthesize(
        &mut self,
        engine: &DistinctionEngine,
        a: Distinction,
        b: Distinction,
    ) -> Distinction {
        let child = engine.synthesize(a, b);
        if self.seen.insert(child) {
            // `insert` returns true when child was NOT previously present.
            self.log.push(child);
        }
        child
    }

    /// Borrow the recorded log in chronological order.
    ///
    /// Each entry appears at most once.
    #[must_use]
    pub fn log(&self) -> &[Distinction] {
        &self.log
    }
}

impl Default for SynthesisRecorder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use static_assertions::{assert_impl_all, assert_not_impl_any};

    // Compile-time enforcement of the single-thread contract.
    assert_not_impl_any!(SynthesisRecorder: Send, Sync);
    // DistinctionEngine remains Send + Sync; only the recorder is
    // restricted.
    assert_impl_all!(DistinctionEngine: Send, Sync);

    // ----- Basic behavior ----------------------------------------------

    #[test]
    fn fresh_recorder_log_is_empty() {
        let r = SynthesisRecorder::new();
        assert!(r.log().is_empty());
    }

    #[test]
    fn synthesize_records_novel_child() {
        let e = DistinctionEngine::new();
        let mut r = SynthesisRecorder::new();
        let c = r.synthesize(&e, e.d0(), e.d1());
        assert_eq!(r.log(), &[c]);
    }

    #[test]
    fn repeated_synthesize_dedups() {
        // Same synthesis called multiple times must NOT duplicate the
        // log entry. The recorder's contract is "novel observations,"
        // not "every call." This matches Law 7 (saturation) at the
        // observer layer: a saturated synth contributes nothing new.
        let e = DistinctionEngine::new();
        let mut r = SynthesisRecorder::new();
        let c1 = r.synthesize(&e, e.d0(), e.d1());
        let c2 = r.synthesize(&e, e.d0(), e.d1());
        let c3 = r.synthesize(&e, e.d1(), e.d0()); // commutativity equivalent
        assert_eq!(c1, c2);
        assert_eq!(c1, c3);
        assert_eq!(r.log(), &[c1], "log should contain exactly one entry");
    }

    #[test]
    fn distinct_syntheses_recorded_in_order() {
        let e = DistinctionEngine::new();
        let mut r = SynthesisRecorder::new();
        let a = r.synthesize(&e, e.d0(), e.d1());
        let b = r.synthesize(&e, a, e.d0());
        let c = r.synthesize(&e, a, e.d1());
        assert_eq!(r.log(), &[a, b, c], "order must reflect call sequence");
    }

    #[test]
    fn irreflexive_synthesis_does_not_record() {
        // synth(a, a) == a (Axiom 3). The engine doesn't create a new
        // distinction. The recorder's `log.contains(&child)` check
        // sees `a` already exists (after a was previously logged).
        let e = DistinctionEngine::new();
        let mut r = SynthesisRecorder::new();
        let a = r.synthesize(&e, e.d0(), e.d1());
        let a_again = r.synthesize(&e, a, a); // irreflexive → returns a
        assert_eq!(a, a_again);
        assert_eq!(r.log(), &[a], "irreflexive call should not add to log");
    }

    #[test]
    fn recorder_passes_through_to_engine() {
        // The recorder doesn't shadow the engine — synthesize calls
        // do mutate the underlying engine.
        let e = DistinctionEngine::new();
        let count_before = e.distinction_count();
        let mut r = SynthesisRecorder::new();
        let _ = r.synthesize(&e, e.d0(), e.d1());
        assert!(e.distinction_count() > count_before);
    }

    // ----- Re-replay via parents_of -----------------------------------

    #[test]
    fn log_entries_appear_in_engine_parents_of() {
        // Every novel entry the recorder logged is registered in the
        // engine's parents_of map (since it was produced by
        // engine.synthesize). Falsifies any divergence between
        // recorder log and engine state.
        let e = DistinctionEngine::new();
        let mut r = SynthesisRecorder::new();
        let a = r.synthesize(&e, e.d0(), e.d1());
        let b = r.synthesize(&e, a, e.d0());
        let c = r.synthesize(&e, b, e.d1());
        for entry in r.log() {
            assert!(
                e.parents_of(*entry).is_some(),
                "log entry {entry:?} missing from engine.parents_of"
            );
        }
        // And the count of recorded entries matches the engine's
        // non-primordial count exactly (since every novel synth went
        // through the recorder).
        assert_eq!(r.log().len(), e.distinction_count() - 2);
        let _ = [a, b, c]; // suppress unused-var warnings
    }
}
