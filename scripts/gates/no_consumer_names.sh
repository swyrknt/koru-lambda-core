#!/usr/bin/env bash
# Gate: the substrate does not name its consumers.
#
# The dependency arrow points one way and `crate_boundary.sh` enforces that in
# the crate graph. This is the same rule for documents, and it was missing.
#
# Why it matters beyond tidiness. A substrate document that names a downstream
# project acquires a maintenance obligation to a repository it does not
# control: the consumer renames something, drops a subsystem, or dies, and the
# substrate's docs are wrong with no signal. That is drift by construction, and
# this project found it twice — a document claiming six converging projects
# while another named a different six, and a consumer list that still described
# subsystems deleted a release ago.
#
# It also removes nothing real. Claims about consumers are unverifiable from
# inside this repository: they are assertions whose evidence lives somewhere
# else. What the axioms force stays provable without them.
#
# Exemptions, and why:
#   CHECKLIST.md   tracks the work, including the work of removing these.
#   (CHANGELOG.md was exempt here and should not have been. It is NOT in
#   Cargo.toml's `exclude` list, so it ships inside the published crate — a
#   file naming downstream projects would have gone to crates.io with 2.0.0.
#   The exemption reasoned that rewriting history to satisfy a gate is worse
#   than the names, which is true of a record nobody distributes and false of
#   one that ships. Both names are gone and the exemption with them.)
#   docs/MIGRATION_JS.md  addressed TO a consumer; naming its audience is the
#                  document's purpose.
set -euo pipefail
cd "$(dirname "$0")/../.."

# Known downstream names. A denylist cannot catch a consumer nobody has
# mentioned yet, so this is partial by construction: it prevents these from
# coming back and states the rule for the rest. The rule is the durable part.
# Word-bounded: a bare "alis" matches "generalisation" and
# "deserialisation", which is how a first draft of this gate reported
# fourteen false hits in documents that name no consumer at all.
NAMES='\balis-ai\b|\bALIS\b|\bkoru-protocol\b|\bkoru-engine\b|\bkoru-spatial\b|\bkoru-mesh\b|\bkoru-delta\b|\bkoru-wave\b|\bgame-studio\b|\bstele\b|\bcanon-cpg\b|\binvar-cli\b|\bvoidrift\b|\bhashy-protocol\b|\bkoru-node\b|\bkoru-liberation\b|\bkoru-server\b|\bkoru-client\b|\bkoru-cli\b'

files=$(git ls-files '*.md' '*.rs' '*.toml' \
    | grep -v '^CHECKLIST.md$' \
    | grep -v '^scripts/gates/' \
    | grep -v '^docs/MIGRATION_JS.md$' || true)
[ -z "$files" ] && { echo "no_consumer_names: no files"; exit 0; }

hits=$(printf '%s\n' "$files" | xargs grep -nE "$NAMES" 2>/dev/null || true)
if [ -n "$hits" ]; then
    echo "substrate files name a downstream consumer:"
    printf '%s\n' "$hits" | sed 's/^/  /' | head -40
    echo
    echo "  The substrate does not depend on its consumers, in code or in prose."
    echo "  State what the axioms force; a claim about who else built something"
    echo "  is not verifiable from this repository and ages without warning."
    exit 1
fi
echo "no_consumer_names: clean"
