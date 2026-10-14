//! Loom model-checker for the synthesize() Release/Acquire kernel —
//! DESIGN.md gate 24. DashMap isn't loom-compatible; this models the
//! minimal kernel that captures the contract.
//!
//! Standard: `RUSTFLAGS="--cfg loom" cargo test --test loom_kernel --release`
//! Mutant:   `RUSTFLAGS="--cfg loom --cfg loom_mutant" cargo test --test loom_kernel --release`

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
            // First: publish the new child. Production proxy: DashMap
            // shard lock release on the entry insert.
            new_w.store(true, Ordering::Release);
            // Second: bump parent.degree. This is the substrate's
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

/// Kernel 3 — atomic-write atomicity sanity: the reader's degree load
/// observes only {0, 1}, never a third value. NOT a happens-before
/// falsifier (kernel 1 verifies that; kernel 4 is the mutant regression
/// sentinel).
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
/// Annotated `#[should_panic]`: if loom CANNOT find the violating
/// interleaving (test panics with wrong message, or doesn't panic),
/// the production kernel's Release/Acquire pair has lost its
/// load-bearing status — investigate before merging.
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

/// Kernel 5 — the check-then-insert that decides novelty.
///
/// TST-3a. Kernels 1 to 4 all model one thing: `parent.degree`
/// Release/Acquire publication. None of them modelled the decision that
/// makes the bump happen at all — `nodes.entry(k)` returning `Vacant` to
/// exactly one racing writer, which is what "exactly one caller observes
/// `Novel`" rests on, and what keeps `sum(degree) == 2 × non-primordials`
/// true under contention.
///
/// `DashMap` is not loom-compatible, so the slot is modelled as the thing
/// its `Entry` API provides: an atomic claim. `compare_exchange` is the
/// kernel — the check and the insert are one operation, so exactly one
/// writer can win no matter how the two are interleaved.
///
/// The invariant is not "the degree is at least one". It is **exactly** one:
/// a second bump would be as wrong as none, because it would make the
/// structural law `r = 2d − 3` false while every individual operation
/// looked fine.
#[test]
fn entry_vacant_grants_novelty_to_exactly_one_writer() {
    loom::model(|| {
        // Production proxy: the shard slot for `new_bytes` in `nodes`.
        let claimed = Arc::new(AtomicBool::new(false));
        // One of the two parents whose degree a novel insert bumps.
        let parent_degree = Arc::new(AtomicUsize::new(0));
        // How many writers believed they were the novel one.
        let novel_count = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..2 {
            let claimed = Arc::clone(&claimed);
            let degree = Arc::clone(&parent_degree);
            let novel = Arc::clone(&novel_count);
            handles.push(thread::spawn(move || {
                // The kernel: check and claim are one operation. This is
                // `dashmap::Entry::Vacant` — the shard write-lock makes the
                // test-and-set atomic, and only the winner inserts.
                let won = claimed
                    .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                    .is_ok();
                if won {
                    novel.fetch_add(1, Ordering::Relaxed);
                    // Release, pairing with `degree()`'s Acquire, exactly as
                    // kernel 1 models.
                    degree.fetch_add(1, Ordering::Release);
                }
            }));
        }
        for h in handles {
            h.join().expect("writer joins (invariant)");
        }

        assert_eq!(
            novel_count.load(Ordering::Acquire),
            1,
            "exactly one writer may observe Novel for one child"
        );
        assert_eq!(
            parent_degree.load(Ordering::Acquire),
            1,
            "and the parent is bumped exactly once — twice would make r = 2d - 3 false"
        );
    });
}

/// Kernel 6 — Regression sentinel: check-then-insert split into two steps.
///
/// Compiled only under `--cfg loom_mutant`. The claim is a `load` followed
/// by a separate `store` instead of a `compare_exchange`, which is what a
/// contributor writes when replacing `nodes.entry(k)` with a `contains_key`
/// test and a later `insert`. Under loom's exhaustive scheduler both writers
/// MUST be able to observe `false` and both proceed, bumping the parent
/// twice.
///
/// `#[should_panic]`: if loom cannot find that interleaving, the atomicity
/// of the entry claim has stopped being load-bearing in this model and the
/// kernel above is no longer testing anything.
#[cfg(loom_mutant)]
#[test]
#[should_panic(expected = "bumped exactly once")]
fn mutant_split_check_then_insert_double_counts_the_parent() {
    loom::model(|| {
        let claimed = Arc::new(AtomicBool::new(false));
        let parent_degree = Arc::new(AtomicUsize::new(0));
        let novel_count = Arc::new(AtomicUsize::new(0));

        let mut handles = Vec::new();
        for _ in 0..2 {
            let claimed = Arc::clone(&claimed);
            let degree = Arc::clone(&parent_degree);
            let novel = Arc::clone(&novel_count);
            handles.push(thread::spawn(move || {
                // BUG (intentional): the check and the claim are separate.
                let already = claimed.load(Ordering::Acquire);
                if !already {
                    claimed.store(true, Ordering::Release);
                    novel.fetch_add(1, Ordering::Relaxed);
                    degree.fetch_add(1, Ordering::Release);
                }
            }));
        }
        for h in handles {
            h.join().expect("writer joins (invariant)");
        }

        assert_eq!(
            parent_degree.load(Ordering::Acquire),
            1,
            "the parent must be bumped exactly once"
        );
        let _ = novel_count.load(Ordering::Acquire);
    });
}
