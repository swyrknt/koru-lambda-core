#!/usr/bin/env bash
# Gate: the release checklist has no open items.
#
# This is the mechanical half of "an unticked box blocks the release". It is
# what the release script consults before it is allowed to publish anything,
# so that a blocking finding recorded months earlier cannot be walked past by
# whoever happens to be running the release.
#
# It deliberately reports the open items rather than just a count: the useful
# output at release time is "here is what is left", not "no".
set -euo pipefail
cd "$(dirname "$0")/../.."

CHECKLIST="${1:-CHECKLIST.md}"

# Only items above the "Not blocking 2.0.0" line count. Items marked
# `- [~]` were cut with a written reason and are not blockers either, and
# neither are items carrying an inline `*(not blocking: …)*` marker.
#
# That last clause was missing, and it mattered. `roadmap.sh` has always
# excused the inline marker; this gate counted those items as open, so the
# two gates disagreed by thirty items — and this is the one
# `scripts/publish_crates.sh` consults. A release gate that cannot be
# satisfied is not a safeguard: the only ways past it are to edit the gate
# or to skip the script, and both train whoever is releasing to treat it as
# noise. Deferred items are reported separately instead of silently
# dropped, because "what was deliberately left out" is exactly what someone
# wants to re-read at release time.
blocking=$(awk '/^# Not blocking/{exit} {print}' "$CHECKLIST")
open=$(printf '%s\n' "$blocking" | grep -E '^- \[ \] ' | grep -vF '*(not blocking' || true)
open_count=$(printf '%s\n' "$open" | grep -cE '^- \[ \] ' || true)
deferred_count=$(printf '%s\n' "$blocking" | grep -E '^- \[ \] ' | grep -cF '*(not blocking' || true)
done_count=$(printf '%s\n' "$blocking" | grep -cE '^- \[x\] ' || true)

summary="$done_count done, $deferred_count deferred with a written reason"

if [ "$open_count" -eq 0 ]; then
    echo "release_ready: no blocking items — $summary"
    exit 0
fi

echo "release blocked: $open_count items are still open ($summary)"
echo
printf '%s\n' "$open" \
    | sed -E 's/^- \[ \] ([A-Z]+-[0-9]+[a-b]?) (.{0,72}).*/  \1  \2/' \
    | head -30
if [ "$open_count" -gt 30 ]; then
    echo "  … and $((open_count - 30)) more"
fi
exit 1
