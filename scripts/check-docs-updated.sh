#!/usr/bin/env bash
# Stop hook: blocks the end of the turn when source files changed but the two documentation
# files (Italian and English) were not both updated, or when their structure diverges.
# Requires: jq, git, awk, sha256sum.

input=$(cat)

# Avoid infinite loops: if a previous Stop hook already blocked once this turn, let it finish.
if [ "$(echo "$input" | jq -r '.stop_hook_active')" = "true" ]; then exit 0; fi

cd "$(echo "$input" | jq -r '.cwd')" || exit 0

# Reference point: where this branch diverged from origin/main (or the first commit).
base=$(git merge-base HEAD origin/main 2>/dev/null || git rev-list --max-parents=0 HEAD | tail -1)

# Everything touched since then: commits, staged, unstaged and untracked files.
changed=$( {
  git diff --name-only "$base" HEAD
  git diff --name-only
  git diff --name-only --cached
  git ls-files --others --exclude-standard
} 2>/dev/null | sort -u )

# Only real sources count: Rust code, manifests, translation catalogs.
src=$(echo "$changed" | grep -E '^(crates|xtask|app)/.*\.(rs|toml|ftl)$')
[ -z "$src" ] && exit 0

block() {
  jq -n --arg r "$1" '{decision: "block", reason: $r}'
  exit 0
}

echo "$changed" | grep -qx 'DOCUMENTAZIONE.md' || block "Source files changed but DOCUMENTAZIONE.md was not updated. Update BOTH DOCUMENTAZIONE.md (Italian) and DOCUMENTATION.md (en-US) in the same step."
echo "$changed" | grep -qx 'DOCUMENTATION.md'  || block "Source files changed but DOCUMENTATION.md was not updated. Update BOTH DOCUMENTAZIONE.md (Italian) and DOCUMENTATION.md (en-US) in the same step."

# Structural signature: sequence of heading levels + the exact content of fenced code blocks.
sig() {
  awk '/^```/{f=!f; print "FENCE"; next} f{print; next} /^#+ /{match($0,/^#+/); print "H" RLENGTH}' "$1" | sha256sum | cut -d' ' -f1
}
if [ "$(sig DOCUMENTAZIONE.md)" != "$(sig DOCUMENTATION.md)" ]; then
  block "DOCUMENTAZIONE.md and DOCUMENTATION.md have different structure (heading levels or code blocks differ). They must say the same things with the same structure, only the language changes."
fi
exit 0
