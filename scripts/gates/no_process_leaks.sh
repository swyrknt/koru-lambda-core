#!/usr/bin/env bash
# Gate: no internal process vocabulary in files we publish.
#
# Planning identifiers, audit tags and untracked working paths are meaningful
# to us and meaningless to a reader who installs the package. Worse, they cite
# documents that reader cannot open. This gate keeps the published artifact
# self-contained.
#
# Scope: shipped source, tests, benches, manifests and docs. CHECKLIST.md is
# excluded: it is the working tracker and may name in-repo paths.
set -euo pipefail
cd "$(dirname "$0")/../.."

fail=0
# The gate scripts name the patterns they hunt for, so they exclude
# themselves; CHECKLIST.md is the working tracker, not a published artifact.
files=$(git ls-files '*.rs' '*.toml' '*.md' '*.sh' '*.yml' \
    | grep -v '^CHECKLIST.md$' | grep -v '^scripts/gates/' \
    | grep -v '^BUDGET_LOG.md$' || true)
# BUDGET_LOG.md is an internal review record, excluded from the published
# package (see Cargo.toml `exclude`). Rewriting a historical log to satisfy a
# gate would be worse than not publishing it, so it is exempt here and
# withheld there.
[ -z "$files" ] && { echo "no_process_leaks: no files"; exit 0; }

check() {
    local pattern="$1" label="$2"
    local hits
    hits=$(printf '%s\n' "$files" | xargs grep -nE "$pattern" 2>/dev/null || true)
    if [ -n "$hits" ]; then
        echo "$label:"
        printf '%s\n' "$hits" | sed 's/^/  /' | head -40
        fail=1
    fi
}

check '\.claude/' "untracked working path"
check '\b(E[0-9]{2}-S[0-9]{2}|E[0-9]{2}-[a-z])' "epic/story identifier"
check '\bS0[0-9]\b' "story identifier"
check '\b(Step [0-9][a-e]?|Round [0-9])\b' "internal phase tag"
check 'PLAN_V2|PROJECTION_SPEC' "reference to an unpublished planning document"
check '(qa-sentinel|engine-architect|rust-craftsman|theory-guardian|research-lead|Practical Believer|Lone Wolf|Logic Enforcer|The Contrarian|The Visionary)' "internal reviewer name"

if [ "$fail" -eq 0 ]; then
    echo "no_process_leaks: clean"
fi
exit "$fail"
