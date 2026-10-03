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
            | grep -cE '^- \[ \] ' || true)
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
    open=0; done_count=0
    while IFS= read -r sec; do
        [ -z "$sec" ] && continue
        body=$(printf '%s\n' "$blocking" \
            | awk -v s="$sec" '$0 ~ "^## "s" " {inside=1; next} /^## /{inside=0} inside')
        open=$((open + $(printf '%s\n' "$body" | grep -cE '^- \[ \] ' || true)))
        done_count=$((done_count + $(printf '%s\n' "$body" | grep -cE '^- \[x\] ' || true)))
    done <<< "$secs"
    printf '  %-4s %-26s %3d open  %3d done\n' "$phase" "$name" "$open" "$done_count"
done <<< "$phase_lines"
