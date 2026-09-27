#!/usr/bin/env bash
# Gate: a ticked checklist item must carry evidence that resolves.
#
# Without this, the checklist is a list of assertions: anyone (or any
# assistant) can tick a box and write a plausible sentence under it, and
# nothing objects. That is the exact failure this project already hit once,
# where "all gates green" meant whatever the last summary said it meant.
#
# A ticked item must be followed by an `evidence:` line, and every reference
# inside that line must resolve:
#
#   - a commit SHA        -> must exist in this repository
#   - a repo-relative path -> must exist on disk
#   - a test path::name   -> checked by scripts/gates/checklist_tests.sh
#                            (separate gate; needs a compiled test list)
#
# Prose in the evidence line is fine — it is the references that are checked.
set -euo pipefail
cd "$(dirname "$0")/../.."

CHECKLIST="${1:-CHECKLIST.md}"
fail=0
ticked=0
checked_refs=0

report() { printf '%s\n' "$1"; fail=1; }

# Walk the file: a `- [x]` line must be followed by an `- evidence:` line.
while IFS= read -r line_no; do
    item_line=$(sed -n "${line_no}p" "$CHECKLIST")
    item_id=$(printf '%s' "$item_line" | sed -nE 's/^- \[x\] ([A-Z]+-[0-9]+[a-b]?).*/\1/p')
    [ -z "$item_id" ] && item_id="(unnamed, line $line_no)"
    ticked=$((ticked + 1))

    next=$(sed -n "$((line_no + 1))p" "$CHECKLIST")
    if ! printf '%s' "$next" | grep -qE '^[[:space:]]*- evidence:'; then
        report "$item_id: ticked with no evidence line"
        continue
    fi

    # Commit SHAs: 7-40 hex chars appearing as a bare word or after '@'.
    for sha in $(printf '%s' "$next" | grep -oE '\b[0-9a-f]{7,40}\b' || true); do
        checked_refs=$((checked_refs + 1))
        if ! git cat-file -e "${sha}^{commit}" 2>/dev/null; then
            report "$item_id: evidence names commit $sha, which does not exist"
        fi
    done

    # Repo paths: backticked tokens containing a '/' and a file extension,
    # or a known bare filename. Globs are expanded before checking.
    for path in $(printf '%s' "$next" | grep -oE '`[A-Za-z0-9_./{},*-]+`' | tr -d '`' || true); do
        case "$path" in
            */*|*.toml|*.md|*.sh|*.rs|*.yml) ;;
            *) continue ;;
        esac
        # Brace expansion, e.g. src/{a,b}.rs
        for expanded in $(eval "printf '%s\n' $path" 2>/dev/null || printf '%s' "$path"); do
            checked_refs=$((checked_refs + 1))
            if [ ! -e "$expanded" ]; then
                report "$item_id: evidence names $expanded, which does not exist"
            fi
        done
    done
done < <(grep -nE '^- \[x\] ' "$CHECKLIST" | cut -d: -f1)

if [ "$fail" -eq 0 ]; then
    echo "checklist_evidence: $ticked ticked, $checked_refs references resolved"
fi
exit "$fail"
