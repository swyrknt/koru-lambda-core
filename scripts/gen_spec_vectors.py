#!/usr/bin/env python3
"""Render `docs/SPEC.md` §10's vector tables from `conformance/vectors.json`.

CONF-6 and CONF-9 are the same problem seen from two sides. §4.3 told the
reader that §10 "include[s] byte tips and a sequence fold" and §10 contained
neither — only the three history values and one projection's wire bytes. So
an implementer following §10's own contract, "reproduce every value below
from this document alone", could not check the lift, which is the
substrate's single free parameter and the one thing most worth checking.

Publishing those values by hand would make §10 a third source of truth
alongside `conformance/vectors.json` and `src/vectors.rs`, which is the
drift this project has already paid for twice. So §10 is generated, and
`scripts/gates/spec_vectors.sh` fails when the committed text and the
fixture disagree.

Scope is deliberate: §10 carries identity and the lift — the values a
reader can check by hand — plus one worked projection. The other seventeen
projections, the rejection cases and the output encodings stay in the
fixture, which §10 points at. A specification that inlines every vector
stops being readable, and readability is what makes a second
implementation possible.

Usage:
    scripts/gen_spec_vectors.py            # print the rendered block
    scripts/gen_spec_vectors.py --write    # splice it into docs/SPEC.md
"""

from __future__ import annotations

import json
import pathlib
import sys

ROOT = pathlib.Path(__file__).resolve().parent.parent
FIXTURE = ROOT / "conformance" / "vectors.json"
SPEC = ROOT / "docs" / "SPEC.md"

BEGIN = "<!-- BEGIN GENERATED: scripts/gen_spec_vectors.py -->"
END = "<!-- END GENERATED: scripts/gen_spec_vectors.py -->"


def wrap(value: str, width: int = 68) -> list[str]:
    """Break a long hex string into fixed-width lines."""
    return [value[i : i + width] for i in range(0, len(value), width)]


def render(fx: dict) -> str:
    out: list[str] = []
    add = out.append

    add(BEGIN)
    add("")
    add(
        "*Generated from `conformance/vectors.json` by "
        "`scripts/gen_spec_vectors.py`; `scripts/gates/spec_vectors.sh` fails"
    )
    add(
        "if this block and the fixture disagree. Edit the fixture, not this "
        "section.*"
    )
    add("")

    # --- Identity -------------------------------------------------------
    add("### 10.1 Identity")
    add("")
    add("The two primordials, which exist by assertion:")
    add("")
    add("| Value | Hex |")
    add("|---|---|")
    add(f"| `d₀` | `{fx['primordials']['d0']}` |")
    add(f"| `d₁` | `{fx['primordials']['d1']}` |")
    add("")
    hist = fx["projections"]["history"]["names"]
    add("Then, applying §2 in order against a fresh engine:")
    add("")
    add("```")
    for name, expr in hist.items():
        add(f"{name:<8}= {expr}")
    add("```")
    add("")
    add("| Value | Derivation | Hex |")
    add("|---|---|---|")
    for case in fx["operator"]["cases"]:
        parents = ", ".join(case["parents"])
        add(f"| `{case['name']}` | `synthesize({parents})` | `{case['id']}` |")
    add("")

    # --- The lift -------------------------------------------------------
    add("### 10.2 The lift")
    add("")
    add(
        f"Byte tips (§4.3), each {fx['lift']['bits_per_byte']} synthesize "
        "steps over `{d₀, d₁}` after the anchoring call:"
    )
    add("")
    add("| Byte | Tip |")
    add("|---|---|")
    for tip in fx["lift"]["tips"]:
        add(f"| `0x{tip['byte']:02x}` | `{tip['id']}` |")
    add("")
    add(
        "The fixture carries four of the 256; an implementation that "
        "reproduces these four by the §4.3 rule reproduces all of them, and "
        "`conformance/vectors.json` is the place to check the rest."
    )
    add("")
    add("The anchored sequence fold (§4.2):")
    add("")
    for fold in fx["lift"]["folds"]:
        byts = ", ".join(f"0x{b:02x}" for b in fold["bytes"])
        add(f"- **`{fold['name']}`** — anchor `{fold['anchor']}`,")
        add(f"  elements `[{byts}]`, length folded first.")
        add(f"  Result: `{fold['id']}`")
    add("")
    add("Domain anchors (§4.2), which are how two consumers folding identical")
    add("bytes avoid colliding:")
    add("")
    add("| Label | Anchor |")
    add("|---|---|")
    for case in fx["lift"]["domain_anchors"]["cases"]:
        add(f"| `0x{case['label']:02x}` | `{case['id']}` |")
    add("")
    add("Composites (§4.4), folded over their parts' own tips. The last two")
    add("carry the same parts in the same order and differ only in grouping:")
    add("")
    add("| Case | Parts | Result |")
    add("|---|---|---|")
    for case in fx["lift"]["composites"]["cases"]:
        parts = []
        for el in case["elements"]:
            if "tip_of_byte" in el:
                parts.append(f"tip(0x{el['tip_of_byte']:02x})")
            else:
                parts.append(f"`{el['case']}`")
        anchor = case["anchor"]
        where = (
            f"domain 0x{anchor['domain']:02x}"
            if "domain" in anchor
            else f"`{anchor['case_history']}`"
        )
        add(
            f"| `{case['name']}` | {', '.join(parts)} — anchored at {where} "
            f"| `{case['id']}` |"
        )
    add("")

    # --- A worked projection -------------------------------------------
    add("### 10.3 A worked projection")
    add("")
    first = fx["projections"]["cases"][0]
    boundary = first["boundary"]
    bdesc = (
        f"{boundary['kind']}({boundary['value']})"
        if "value" in boundary
        else boundary["kind"]
    )
    add(
        f"With `root = {first['root'][:8]}…`, `direction = "
        f"{first['direction']}`, `boundary = {bdesc}`,"
    )
    add(f"`signal = {first['signal']}`, the §7 canonical bytes are:")
    add("")
    add("```")
    for line in wrap(first["canonical_bytes"]):
        add(line)
    add("```")
    add("")
    add("(The line breaks are presentational; the value is one continuous")
    add("byte string.) Reading the first bytes against §7: `4b 50 52 4a` is")
    add("`KPRJ`, `02` is the version, and the next 16 bytes are the root.")
    add("")
    add(f"Its §8 identity is `{first['projection_id']}`.")
    add("")

    # --- What is not here ----------------------------------------------
    add("### 10.4 What this section does not carry")
    add("")
    n_proj = len(fx["projections"]["cases"])
    n_reject = len(fx["reject"]["cases"])
    n_enc = len(fx["lift"]["canonical_bytes"]["cases"])
    add(
        f"`conformance/vectors.json` carries all {n_proj} projections with "
        f"their identifiers, the {n_enc} output encodings of §6, the "
        f"remaining byte tips, and {n_reject} inputs an implementation must "
        "**refuse**. `conformance/README.md` names which conformance level "
        "each belongs to."
    )
    add("")
    add("Two of those deserve naming here, because an implementation can")
    add("satisfy everything above and still fail them:")
    add("")
    add(
        "- The rejection cases. Reproduction is not conformance: an "
        "implementation that accepts a well-formed forgery reproduces every "
        "value in this section perfectly."
    )
    add(
        f"- Only four of the {n_proj} projections are invariant under "
        "appending to the engine. The other fourteen are reproducible only "
        "against the history §10.1 declares."
    )
    add("")
    add(END)
    return "\n".join(out)


def main() -> int:
    fx = json.loads(FIXTURE.read_text())
    block = render(fx)

    if "--write" not in sys.argv:
        print(block)
        return 0

    spec = SPEC.read_text()
    if BEGIN in spec:
        start = spec.index(BEGIN)
        end = spec.index(END) + len(END)
        spec = spec[:start] + block + spec[end:]
    else:
        print(
            f"gen_spec_vectors: no generated block in {SPEC}; add the markers "
            "first",
            file=sys.stderr,
        )
        return 1
    SPEC.write_text(spec)
    print(f"gen_spec_vectors: wrote {len(block.splitlines())} lines into {SPEC}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
