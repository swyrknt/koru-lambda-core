//! Loom model-checker test for the substrate's Release/Acquire memory
//! ordering kernel — DESIGN.md gate 24.
//!
//! Gates the load-bearing claim from `DistinctionEngine::synthesize`'s
//! happens-before contract: parent `degree.fetch_add(Release)` paired
//! with `degree.load(Acquire)` in `degree()` produces a sequentially-
//! valid happens-before edge even when the writing thread publishes
//! the new child node (via the DashMap shard lock release) BEFORE the
//! parent degree bump.
//!
//! The substrate's actual synthesize hot path is too large for loom to
//! model directly (DashMap uses parking_lot, not loom's mock primitives),
//! so this file models the minimal kernel that captures the contract.
//! If a future refactor weakens the Release on `degree.fetch_add` or
//! the Acquire on `degree.load`, loom should detect a counterexample.
//!
//! # Running
//!
//! ```text
//! # Standard run — kernels 1, 2, 3 (production memory ordering)
//! RUSTFLAGS="--cfg loom" cargo test --test loom_kernel --release
//!
//! # Regression-sentinel run — additionally compiles kernel 4 which
//! # downgrades the writer's fetch_add to Relaxed and is annotated
//! # `#[should_panic]` because loom MUST find an interleaving where
//! # the assertion fails. If kernel 4 passes WITHOUT panicking, the
//! # production kernel's Release/Acquire pair has stopped being
//! # load-bearing — investigate before merging.
//! RUSTFLAGS="--cfg loom --cfg loom_mutant" cargo test --test loom_kernel --release
//! ```
//!
//! Without `--cfg loom`, this file is empty (the entire body is gated)
//! and the test binary compiles to a no-op. The standard `cargo test`
//! workflow is unaffected.

#![cfg(loom)]

use loom::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use loom::sync::Arc;
use loom::thread;

/// Kernel 1 — Release/Acquire publication via parent.degree.
///
/// Models the synthesize hot path's two-step publication:
///   1. Writer publishes the new child node (proxy: `new_child_published`
///      set with Release ordering; in production this is the DashMap
///      shard lock release on `nodes` after `Entry::Vacant::insert`).
///   2. Writer bumps parent.degree (`fetch_add(1, Release)`).
///
/// Reader's load of parent.degree with Acquire ordering MUST establish
/// a happens-before edge to the new-child publication. If the reader
/// observes the bump, it must also observe the child publication.
///
/// Falsifier: if a future change downgrades `parent.degree.fetch_add`
/// to `Ordering::Relaxed`, loom should find an interleaving where the
/// reader sees the bump but not the child.
#[test]
fn parent_degree_release_acquire_publishes_child() {
    loom::model(|| {
        let new_child_published = Arc::new(AtomicBool::new(false));
        let parent_degree = Arc::new(AtomicUsize::new(0));

        let new_w = Arc::clone(&new_child_published);
        let deg_w = Arc::clone(&parent_degree);
        let writer = thread::spawn(move || {
            // Step 1: publish the new child. Production proxy: DashMap
            // shard lock release on the entry insert.
            new_w.store(true, Ordering::Release);
            // Step 2: bump parent.degree. This is the substrate's
            // actual fetch_add — the load-bearing Release.
            deg_w.fetch_add(1, Ordering::Release);
        });

        let new_r = Arc::clone(&new_child_published);
        let deg_r = Arc::clone(&parent_degree);
        let reader = thread::spawn(move || {
            // Acquire load establishes HB with the writer's Release
            // fetch_add, which itself is sequenced-after the Release
            // store of `new_child_published`. So observing the bump
            // (deg >= 1) MUST imply observing the child publication.
            let bumped = deg_r.load(Ordering::Acquire);
            if bumped >= 1 {
                assert!(
                    new_r.load(Ordering::Acquire),
                    "Acquire on parent_degree did not establish HB with new_child publication \
                     (degree.fetch_add Release downgraded to Relaxed?)"
                );
            }
        });

        writer.join().expect("writer joins (invariant)");
        reader.join().expect("reader joins (invariant)");
    });
}

/// Kernel 2 — post-join visibility of the sum invariant.
///
/// Models the substrate's "sum_of_degrees == 2 × non_primordial_count"
/// post-join consistency claim. Two writers each bump the same
/// parent.degree concurrently; after both join, the reader must see
/// degree == 2.
///
/// This is what `concurrent_synth_byte_equivalent_state` and
/// `relaxed_happens_before_post_join_consistent` assert at scale.
/// Loom verifies the kernel under all interleavings.
///
/// Falsifier: any race that drops one of the two fetch_adds would
/// manifest here as degree != 2.
#[test]
fn two_writers_post_join_sum_is_two() {
    loom::model(|| {
        let parent_degree = Arc::new(AtomicUsize::new(0));

        let h1 = {
            let d = Arc::clone(&parent_degree);
            thread::spawn(move || d.fetch_add(1, Ordering::Release))
        };
        let h2 = {
            let d = Arc::clone(&parent_degree);
            thread::spawn(move || d.fetch_add(1, Ordering::Release))
        };

        h1.join().expect("writer 1 joins (invariant)");
        h2.join().expect("writer 2 joins (invariant)");

        // Post-join: both fetch_adds are visible. Acquire load gives
        // us the final state.
        assert_eq!(
            parent_degree.load(Ordering::Acquire),
            2,
            "two writers' fetch_adds must both land in post-join state"
        );
    });
}

/// Kernel 3 — no-third-value sanity check (NOT a happens-before falsifier).
///
/// This is sanity, not contract. A single writer doing one fetch_add(1)
/// on a usize initialized to 0 cannot produce any value outside {0, 1};
/// the assertion `d <= 1` is satisfied by atomic-write atomicity alone
/// regardless of memory ordering. The test is preserved as documentation
/// that the relaxation window has only two valid observations from a
/// reader's perspective, and would catch a hypothetical regression that
/// somehow produced a third value (e.g. via memory corruption).
///
/// **The actual happens-before contract is verified by kernel 1.** Kernel
/// 4 (cfg-gated `loom_mutant`) is the regression sentinel for the
/// Release/Acquire pair on parent.degree. Together, kernels 1 + 4 are the
/// load-bearing pair; this kernel is documentation.
#[test]
fn no_third_value_sanity_for_relaxed_window_reader() {
    loom::model(|| {
        let new_child_published = Arc::new(AtomicBool::new(false));
        let parent_degree = Arc::new(AtomicUsize::new(0));

        let new_w = Arc::clone(&new_child_published);
        let deg_w = Arc::clone(&parent_degree);
        let writer = thread::spawn(move || {
            new_w.store(true, Ordering::Release);
            deg_w.fetch_add(1, Ordering::Release);
        });

        let new_r = Arc::clone(&new_child_published);
        let deg_r = Arc::clone(&parent_degree);
        let reader = thread::spawn(move || {
            // Reader observes child first (matches the relaxation
            // window the synthesize docstring describes).
            if new_r.load(Ordering::Acquire) {
                // The reader sees the child. The bump may or may not
                // be visible yet — both 0 and 1 are valid per the
                // relaxation contract. We assert ONLY that no third
                // value appears (this is atomic-write atomicity, not
                // happens-before).
                let d = deg_r.load(Ordering::Acquire);
                assert!(
                    d <= 1,
                    "reader observed degree value outside {{0, 1}} — atomic-write atomicity broken"
                );
            }
        });

        writer.join().expect("writer joins (invariant)");
        reader.join().expect("reader joins (invariant)");
    });
}

/// Kernel 4 — Regression sentinel: downgrade-to-Relaxed mutant of kernel 1.
///
/// Compiled only under `--cfg loom_mutant`. The writer's `fetch_add` is
/// intentionally `Ordering::Relaxed` instead of `Ordering::Release`. Under
/// loom's exhaustive scheduler, this MUST produce an interleaving where
/// the reader observes `bumped >= 1` AND `new_child_published == false`,
/// because the Relaxed write of parent_degree does not synchronize-with
/// the writer's earlier Release store of new_child_published.
///
/// Annotated `#[should_panic]`: the assertion failure inside the spawned
/// thread propagates to a panic when the joined reader's panic is
/// unwound. If loom CANNOT find the violating interleaving (i.e. this
/// test panics with the wrong message, or doesn't panic at all), the
/// production kernel's Release/Acquire pair has lost its load-bearing
/// status — investigate before merging.
///
/// Run: `RUSTFLAGS="--cfg loom --cfg loom_mutant" cargo test --test loom_kernel --release`
///
/// This pattern (kernel 1 = positive verification, kernel 4 = mutant
/// regression sentinel) is the qa-sentinel R2 demand: a model-checker
/// test that *can* detect a regression, not just decoration.
#[cfg(loom_mutant)]
#[test]
#[should_panic(expected = "Acquire on parent_degree did not establish HB")]
fn mutant_parent_degree_relaxed_writer_loses_publication_visibility() {
    loom::model(|| {
        let new_child_published = Arc::new(AtomicBool::new(false));
        let parent_degree = Arc::new(AtomicUsize::new(0));

        let new_w = Arc::clone(&new_child_published);
        let deg_w = Arc::clone(&parent_degree);
        let writer = thread::spawn(move || {
            new_w.store(true, Ordering::Release);
            // BUG (intentional): Relaxed instead of Release.
            // Should give loom an interleaving that breaks kernel 1's
            // assertion.
            deg_w.fetch_add(1, Ordering::Relaxed);
        });

        let new_r = Arc::clone(&new_child_published);
        let deg_r = Arc::clone(&parent_degree);
        let reader = thread::spawn(move || {
            let bumped = deg_r.load(Ordering::Acquire);
            if bumped >= 1 {
                assert!(
                    new_r.load(Ordering::Acquire),
                    "Acquire on parent_degree did not establish HB with new_child publication \
                     (degree.fetch_add Release downgraded to Relaxed?)"
                );
            }
        });

        writer.join().expect("writer joins (invariant)");
        reader.join().expect("reader joins (invariant)");
    });
}
