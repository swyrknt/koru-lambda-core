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
# `- [~]` were cut with a written reason and are not blockers either.
blocking=$(awk '/^# Not blocking/{exit} {print}' "$CHECKLIST")
open_count=$(printf '%s\n' "$blocking" | grep -cE '^- \[ \] ' || true)
done_count=$(printf '%s\n' "$blocking" | grep -cE '^- \[x\] ' || true)

if [ "$open_count" -eq 0 ]; then
    echo "release_ready: all $done_count items complete"
    exit 0
fi

echo "release blocked: $open_count of $((open_count + done_count)) items are still open"
echo
printf '%s\n' "$blocking" | grep -E '^- \[ \] ' \
    | sed -E 's/^- \[ \] ([A-Z]+-[0-9]+[a-b]?) (.{0,72}).*/  \1  \2/' \
    | head -30
if [ "$open_count" -gt 30 ]; then
    echo "  … and $((open_count - 30)) more"
fi
exit 1
