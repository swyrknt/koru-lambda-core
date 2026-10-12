#!/usr/bin/env bash
# Gate: the roadmap describes the checklist that actually exists.
#
# A roadmap written in prose drifts exactly the way the theory document did:
# it accumulates phases nobody is working, names sections that were renamed,
# and declares work complete while items sit open. This project's recurring
# defect is a check that exists and does not check, so the roadmap gets one.
#
# The design constraint that matters: ROADMAP.md must NOT restate checklist
# items. Two sources of truth for the same work is the failure we found three
# times this week (vectors inside the crate they pin, the spec promising
# vectors it does not publish, a gate whose allowlist outlived its types).
# So the roadmap assigns checklist *sections* to phases and nothing more, and
# this gate proves the assignment is total, exclusive, and honest.
#
# Two scopes, at two granularities, and both are honoured here. The
# `# Not blocking 2.0.0` line divides whole SECTIONS; an inline
# `*(not blocking)*` marker on an individual item excuses that ITEM inside an
# otherwise-blocking section. Those are not duplicate mechanisms — a section
# below the line ships on another timeline, while a marked item is this
# release's work that was judged optional. The gate used to read the line and
# ignore the markers, so thirteen items counted as blocking against their own
# text, and "P1 has 26 open" overstated the release by ten.
set -euo pipefail
cd "$(dirname "$0")/../.."

ROADMAP="${1:-ROADMAP.md}"
CHECKLIST="${2:-CHECKLIST.md}"
fail=0

[ -f "$ROADMAP" ] || { echo "roadmap: $ROADMAP missing"; exit 1; }
[ -f "$CHECKLIST" ] || { echo "roadmap: $CHECKLIST missing"; exit 1; }

# Only sections above the "Not blocking" line are in scope; everything below
# it ships after 2.0.0 and is deliberately unphased.
blocking=$(awk '/^# Not blocking/{exit} {print}' "$CHECKLIST")

# Checklist sections look like: "## THY — Theory alignment"
checklist_sections=$(printf '%s\n' "$blocking" | sed -nE 's/^## ([A-Z]+) .*/\1/p' | sort -u)

# Roadmap phase lines look like:
#   - [ ] P1 Theory and freeze — sections: THY, PRB, FRZ, CONF
# A phase with no sections declares "sections: none".
phase_lines=$(grep -nE '^- \[[ x]\] P[0-9]+ ' "$ROADMAP" || true)
if [ -z "$phase_lines" ]; then
    echo "roadmap: no phase lines found (expected '- [ ] P<n> <name> — sections: A, B')"
    exit 1
fi

# 1. Every section named by the roadmap must exist in the checklist.
roadmap_sections=$(printf '%s\n' "$phase_lines" \
    | sed -nE 's/.*sections:[[:space:]]*(.*)$/\1/p' \
    | tr ',' '\n' | tr -d ' ' | grep -vx 'none' | grep -v '^$' | sort)

while IFS= read -r sec; do
    [ -z "$sec" ] && continue
    if ! printf '%s\n' "$checklist_sections" | grep -qx "$sec"; then
        echo "roadmap: phase names section '$sec', which is not a blocking section of $CHECKLIST"
        fail=1
    fi
done <<< "$roadmap_sections"

# 2a. No two checklist sections may share a prefix. The prefix is the key
#     this gate assigns phases by, so two `## PKG — …` headings made every
#     per-section count ambiguous to a reader — and `sort -u` below hid it.
#     The counts happened to be right (both fed one assignment), which is
#     exactly why it needed a check rather than a reader noticing. PROC-25
#     credited this gate with the check before it had one.
dup_sections=$(printf '%s\n' "$blocking" | sed -nE 's/^## ([A-Z]+) .*/\1/p' | sort | uniq -d || true)
if [ -n "$dup_sections" ]; then
    echo "roadmap: two or more checklist sections share a prefix:"
    printf '%s\n' "$dup_sections" | sed 's/^/  /'
    echo "  (a prefix is this gate's section key; merge them or rename one)"
    fail=1
fi

# 2. No section may be assigned twice. Two phases owning one section is how a
#    phase silently becomes someone else's problem.
dupes=$(printf '%s\n' "$roadmap_sections" | uniq -d || true)
if [ -n "$dupes" ]; then
    echo "roadmap: sections assigned to more than one phase:"
    printf '%s\n' "$dupes" | sed 's/^/  /'
    fail=1
fi

# 3. Every blocking section must be assigned to some phase. An unphased
#    section is work with no owner and no ordering, which is where a backlog
#    starts rotting.
while IFS= read -r sec; do
    [ -z "$sec" ] && continue
    if ! printf '%s\n' "$roadmap_sections" | grep -qx "$sec"; then
        echo "roadmap: checklist section '$sec' is not assigned to any phase"
        fail=1
    fi
done <<< "$checklist_sections"

# 4. A phase ticked complete must have no open items in its sections. This is
#    the assertion the whole file exists for: it makes "we finished that"
#    into a claim a command can refute.
while IFS= read -r line; do
    [ -z "$line" ] && continue
    case "$line" in *'- [x] '*) ;; *) continue ;; esac
    phase=$(printf '%s\n' "$line" | sed -nE 's/.*(P[0-9]+).*/\1/p')
    secs=$(printf '%s\n' "$line" | sed -nE 's/.*sections:[[:space:]]*(.*)$/\1/p' \
           | tr ',' '\n' | tr -d ' ' | grep -vx 'none' | grep -v '^$' || true)
    open_total=0
    while IFS= read -r sec; do
        [ -z "$sec" ] && continue
        n=$(printf '%s\n' "$blocking" \
            | awk -v s="$sec" '$0 ~ "^## "s" " {inside=1; next} /^## /{inside=0} inside' \
            | grep -E '^- \[ \] ' | grep -cvF '*(not blocking' || true)
        open_total=$((open_total + n))
    done <<< "$secs"
    if [ "$open_total" -ne 0 ]; then
        echo "roadmap: $phase is marked complete but its sections have $open_total open items"
        fail=1
    fi
done <<< "$phase_lines"

if [ "$fail" -ne 0 ]; then
    exit 1
fi

# Report per-phase progress. Printing it here is what keeps the roadmap
# honest between reviews: the numbers come from the checklist, not from
# whoever last edited the prose.
echo "roadmap: phases consistent with $CHECKLIST"
while IFS= read -r line; do
    [ -z "$line" ] && continue
    phase=$(printf '%s\n' "$line" | sed -nE 's/^[0-9]+:- \[[ x]\] (P[0-9]+) .*/\1/p')
    name=$(printf '%s\n' "$line" | sed -nE 's/^[0-9]+:- \[[ x]\] P[0-9]+ (.*) — sections:.*/\1/p')
    secs=$(printf '%s\n' "$line" | sed -nE 's/.*sections:[[:space:]]*(.*)$/\1/p' \
           | tr ',' '\n' | tr -d ' ' | grep -vx 'none' | grep -v '^$' || true)
    open=0; done_count=0; optional=0
    while IFS= read -r sec; do
        [ -z "$sec" ] && continue
        body=$(printf '%s\n' "$blocking" \
            | awk -v s="$sec" '$0 ~ "^## "s" " {inside=1; next} /^## /{inside=0} inside')
        open=$((open + $(printf '%s\n' "$body" | grep -E '^- \[ \] ' | grep -cvF '*(not blocking' || true)))
        optional=$((optional + $(printf '%s\n' "$body" | grep -E '^- \[ \] ' | grep -cF '*(not blocking' || true)))
        done_count=$((done_count + $(printf '%s\n' "$body" | grep -cE '^- \[x\] ' || true)))
    done <<< "$secs"
    if [ "$optional" -ne 0 ]; then
        printf '  %-4s %-26s %3d open  %3d done  (+%d marked not blocking)\n' \
            "$phase" "$name" "$open" "$done_count" "$optional"
    else
        printf '  %-4s %-26s %3d open  %3d done\n' "$phase" "$name" "$open" "$done_count"
    fi
done <<< "$phase_lines"
