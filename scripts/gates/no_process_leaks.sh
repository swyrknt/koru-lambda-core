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
# Scope is exactly what `cargo package` will ship, asked of cargo rather than
# approximated here. It used to be an extension list — '*.rs' '*.toml' '*.md'
# '*.sh' '*.yml' — with three hand-maintained exemptions, and that was wrong
# in both directions. It could not match a file with no extension, so
# `.gitignore` (tracked, shipped, and naming local tooling directories on
# three lines) was invisible; and it scanned files the package excludes, so
# the exemptions existed to undo that.
#
# Asking cargo collapses both: the gate's header says "files we publish", and
# now that is literally the file list. It also means widening Cargo.toml's
# `exclude` is the way to exempt something, which is the honest mechanism —
# a file is exempt because readers do not receive it, not because a gate was
# told to look away.
if ! files=$(cargo package --list --allow-dirty -p koru-lambda-core 2>/dev/null); then
    echo "no_process_leaks: cargo package --list failed; cannot determine what ships" >&2
    exit 1
fi
# Drop cargo's synthesised entries, keep text only.
files=$(printf '%s\n' "$files" \
    | grep -vE '^(Cargo\.toml\.orig|\.cargo_vcs_info\.json)$' \
    | while IFS= read -r f; do [ -f "$f" ] && printf '%s\n' "$f"; done \
    | tr '\n' '\0' | xargs -0 grep -Il '' 2>/dev/null || true)

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
