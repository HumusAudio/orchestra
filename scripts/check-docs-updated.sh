#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Stop hook: blocks the end of the turn when source files changed but the two documentation
# files (Italian and English) were not both updated, or when their structure diverges.
# Requires: jq, git, awk, sha256sum.
#
# Which changes count: those made DURING THIS TURN (CLAUDE.md §8: "in the same step"). The
# UserPromptSubmit hook (turn-start.sh) stores a snapshot of the working tree when the turn
# starts; here we compare it with a snapshot of the working tree now, so files committed during
# the turn are seen too. If no snapshot exists (hook not installed, first run), we fall back to
# everything changed on the branch since it diverged from origin/main.

input=$(cat)

# Avoid infinite loops: if a previous Stop hook already blocked once this turn, let it finish.
if [ "$(echo "$input" | jq -r '.stop_hook_active')" = "true" ]; then exit 0; fi

cd "$(echo "$input" | jq -r '.cwd')" || exit 0

session=$(echo "$input" | jq -r '.session_id // "default"' | tr -cd 'A-Za-z0-9_-')
git_dir=$(git rev-parse --git-dir 2>/dev/null) || exit 0
start_file="$git_dir/claude-turn-start-$session"

if [ -s "$start_file" ]; then
  # Turn-scoped: tree at turn start vs tree now.
  now=$(bash "$(dirname "$0")/worktree-snapshot.sh" 2>/dev/null) || exit 0
  changed=$(git diff --name-only "$(cat "$start_file")" "$now" 2>/dev/null | sort -u)
else
  # Fallback, branch-scoped: where this branch diverged from origin/main (or the first commit).
  base=$(git merge-base HEAD origin/main 2>/dev/null || git rev-list --max-parents=0 HEAD | tail -1)
  # Everything touched since then: commits, staged, unstaged and untracked files.
  changed=$( {
    git diff --name-only "$base" HEAD
    git diff --name-only
    git diff --name-only --cached
    git ls-files --others --exclude-standard
  } 2>/dev/null | sort -u )
fi

# What counts as a source (owner's decision, 2026-10-29): everything the two documents explain.
#   - Rust code, manifests and translation catalogs of the program and of xtask;
#   - the workspace configuration: root Cargo.toml, deny.toml, clippy.toml, rustfmt.toml,
#     .cargo/config.toml;
#   - automation: CI workflows, hook scripts, the commit-msg hook.
# Not counted: Cargo.lock (changes by itself) and spikes/ (throw-away prototypes).
src=$(echo "$changed" | grep -E \
  -e '^(crates|xtask|app)/.*\.(rs|toml|ftl)$' \
  -e '^(Cargo\.toml|deny\.toml|clippy\.toml|rustfmt\.toml|\.cargo/config\.toml)$' \
  -e '^\.github/workflows/.*\.ya?ml$' \
  -e '^scripts/.*\.sh$' \
  -e '^\.githooks/')
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
