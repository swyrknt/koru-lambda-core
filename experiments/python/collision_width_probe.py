#!/usr/bin/env python3
"""ANA-2's falsifier: the birthday model and collision-as-saturation, measured.

WHAT IT DOES. ANA-2 extrapolates from a uniform-draw model of
`SHA-256(min || max)` truncated to 16 bytes. At 128 bits that model cannot be
tested — a collision needs ~2^64 draws. This runs the identical rule at a
width where collisions are reachable and asks two questions the analysis
depends on:

  1. Does the observed first-collision count track `sqrt(pi*N/2)`? If it does
     not, the 128-bit numbers in ANA-2 are wrong.

  2. When a collision happens, does it present the way `docs/SPEC.md` §11
     (DEC-5) says — the synthesis returns non-novel, the stored parentage
     keeps the *first* pair, and recomputing the content address over the
     stored parents still reproduces the identifier, so the engine's own
     audit passes over a collided graph?

WHY IT IS NOT A TEST. It is a measurement of a model, not of this crate: the
engine it simulates has a width no shipped engine has. It lives here with the
other probes rather than in `tests/`, and `experiments/*` is excluded from the
published package.

    python3 experiments/python/collision_width_probe.py          # both parts
    python3 experiments/python/collision_width_probe.py --trials 200

A WORD ON TRIAL COUNTS. The first-collision count is heavy-tailed, with
`SD ~ 0.52*sqrt(N)`. At 12 trials the standard error is `0.15*sqrt(N)`, which
is wide enough to make a correct model look wrong — the first run of this
probe reported a ratio of 0.81 and was dismissing the model on noise. 200
trials at w=32 brings the prediction inside the 95% CI. Default accordingly.
"""

from __future__ import annotations

import argparse
import hashlib
import math
import statistics
import sys

# Both answers are recorded in CHECKLIST.md under ANA-2.
DEFAULT_TRIALS = 200
DEFAULT_WIDTH = 32


def truncated_synthesize(width_bytes: int):
    """The shipped rule at a reduced width.

    Mirrors `synth_bytes` in `src/engine.rs`: the pair is ordered bytewise
    (`if a.0 <= b.0`), concatenated, hashed with SHA-256, and truncated to the
    leading `width_bytes`. Nothing else is consulted.
    """

    def synthesize(a: bytes, b: bytes) -> bytes:
        first, second = (a, b) if a <= b else (b, a)
        return hashlib.sha256(first + second).digest()[:width_bytes]

    return synthesize


def primordials(width_bytes: int) -> tuple[bytes, bytes]:
    """d0 = [0x00; w], d1 = [0x01, 0x00 * (w-1)] — the shipped constants, cut
    to width."""
    return bytes(width_bytes), bytes([1]) + bytes(width_bytes - 1)


def run_to_first_collision(width_bits: int, seed: int):
    """Grow a graph until two *distinct* parent pairs produce one identifier.

    A repeat of the same pair is Law 7 saturation and is not a collision, so
    the stored pair is compared rather than merely tested for presence.

    Returns (distinct_children, syntheses, collision) where `collision` is
    (identifier, pair_recorded_first, pair_that_collided) and `nodes` maps
    identifier -> parent pair (None for the primordials).
    """
    import random

    width_bytes = width_bits // 8
    synthesize = truncated_synthesize(width_bytes)
    d0, d1 = primordials(width_bytes)

    nodes: dict[bytes, tuple[bytes, bytes] | None] = {d0: None, d1: None}
    pool = [d0, d1]
    rng = random.Random(seed)
    syntheses = 0

    while True:
        a = pool[rng.randrange(len(pool))]
        b = pool[rng.randrange(len(pool))]
        if a == b:
            continue  # idempotence: synth(a, a) adds nothing
        first, second = (a, b) if a <= b else (b, a)
        child = synthesize(a, b)
        syntheses += 1

        if child not in nodes:
            nodes[child] = (first, second)
            pool.append(child)
            continue

        stored = nodes[child]
        if stored is not None and stored != (first, second):
            return len(nodes) - 2, syntheses, (child, stored, (first, second)), nodes
        # else: the same pair again, or a primordial — saturation, not a collision


def part_one(width_bits: int, trials: int) -> bool:
    """Does the observed first-collision count track sqrt(pi*N/2)?"""
    n_space = 2**width_bits
    root = math.sqrt(n_space)
    predicted_mean = math.sqrt(math.pi * n_space / 2)
    predicted_median = 1.1774 * root
    predicted_sd = 0.5234 * root

    observed = [run_to_first_collision(width_bits, seed)[0] for seed in range(trials)]
    mean = statistics.mean(observed)
    median = statistics.median(observed)
    sd = statistics.stdev(observed)
    sem = sd / math.sqrt(trials)
    z = (mean - predicted_mean) / sem

    print(f"1. birthday model at w={width_bits} (N = 2^{width_bits}), {trials} trials\n")
    print(f"   {'statistic':<9} {'predicted':>11} {'observed':>11} {'obs/pred':>9}")
    print(f"   {'mean':<9} {predicted_mean:>11.0f} {mean:>11.0f} {mean / predicted_mean:>9.3f}")
    print(f"   {'median':<9} {predicted_median:>11.0f} {median:>11.0f} {median / predicted_median:>9.3f}")
    print(f"   {'sd':<9} {predicted_sd:>11.0f} {sd:>11.0f} {sd / predicted_sd:>9.3f}")
    inside = abs(z) < 1.96
    print(f"\n   standard error {sem:.0f}, z = {z:+.2f}, 95% CI "
          f"[{mean - 1.96 * sem:.0f}, {mean + 1.96 * sem:.0f}] "
          f"-> prediction {'INSIDE' if inside else 'OUTSIDE'}")
    print(f"   {'MODEL HOLDS' if inside else 'MODEL REJECTED — ANA-2 is wrong'}\n")
    return inside


def part_two(width_bits: int) -> bool:
    """Does a collision present as saturation, with the audit still passing?"""
    width_bytes = width_bits // 8
    _, _, (child, stored_pair, new_pair), nodes = run_to_first_collision(width_bits, 0)
    synthesize = truncated_synthesize(width_bytes)

    recomputed = synthesize(*stored_pair)
    audit_failures = sum(
        1
        for cid, pair in nodes.items()
        if pair is not None and synthesize(*pair) != cid
    )

    print(f"2. how the collision presents (w={width_bits}, seed 0)\n")
    print(f"   identifier                 {child.hex()}")
    print(f"   parent pair recorded first ({stored_pair[0].hex()}, {stored_pair[1].hex()})")
    print(f"   parent pair that collided  ({new_pair[0].hex()}, {new_pair[1].hex()})")
    print(f"   the two pairs are distinct {stored_pair != new_pair}")
    print("   synthesize returned        non-novel — indistinguishable from Law 7")
    print(f"   recompute over stored pair {recomputed.hex()} -> matches: {recomputed == child}")
    print(f"   full-graph audit failures  {audit_failures} of {len(nodes)} nodes\n")

    ok = stored_pair != new_pair and recomputed == child and audit_failures == 0
    if ok:
        print("   The second pair is unrecorded, the audit passes, and nothing in the")
        print("   engine's own state distinguishes this from ordinary saturation.")
        print("   docs/SPEC.md §11 (DEC-5) states this; it is now measured.\n")
    else:
        print("   SPEC §11's description of a collision does not match what happened.\n")
    return ok


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--width", type=int, default=DEFAULT_WIDTH,
                        help="identifier width in bits (must be a multiple of 8)")
    parser.add_argument("--trials", type=int, default=DEFAULT_TRIALS,
                        help="trials for part 1; fewer than ~100 cannot test the model")
    args = parser.parse_args()

    if args.width % 8 != 0 or not 8 <= args.width <= 64:
        parser.error("--width must be a multiple of 8 between 8 and 64")

    print(f"ANA-2 falsifier — truncated SHA-256 at {args.width} bits\n")
    model_holds = part_one(args.width, args.trials)
    presents_as_saturation = part_two(args.width)
    return 0 if (model_holds and presents_as_saturation) else 1


if __name__ == "__main__":
    sys.exit(main())
