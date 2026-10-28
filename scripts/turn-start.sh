#!/usr/bin/env bash
# UserPromptSubmit hook: remembers the state of the working tree at the start of each turn, so
# that the Stop hook (check-docs-updated.sh) can look at the changes made IN THIS TURN only —
# "update the documentation in the same step" (CLAUDE.md §8), including commits made during the
# turn. The snapshot is a git tree id stored in .git/, one file per Claude Code session, so
# parallel sessions do not overwrite each other. Never blocks the prompt: always exits 0.
# Requires: jq, git.

input=$(cat)
cd "$(echo "$input" | jq -r '.cwd')" 2>/dev/null || exit 0
session=$(echo "$input" | jq -r '.session_id // "default"' | tr -cd 'A-Za-z0-9_-')
git_dir=$(git rev-parse --git-dir 2>/dev/null) || exit 0
tree=$(bash "$(dirname "$0")/worktree-snapshot.sh" 2>/dev/null) || exit 0
echo "$tree" > "$git_dir/claude-turn-start-$session"
exit 0
