#!/usr/bin/env bash
# SPDX-License-Identifier: GPL-3.0-or-later
# Prints the git tree id of the CURRENT working tree (tracked + untracked, honouring .gitignore),
# without touching HEAD, branches, the real index or the stash.
#
# How: copy the real index to a temporary file (so git can reuse its cached file stats and only
# re-hash files that actually changed), `git add -A` into that temporary index, `git write-tree`.
# The blobs written to .git/objects are unreferenced and are garbage-collected by git later.
# Used by turn-start.sh and check-docs-updated.sh. Requires: git, mktemp.
set -euo pipefail
git_dir=$(git rev-parse --git-dir)
tmp_index=$(mktemp)
trap 'rm -f "$tmp_index"' EXIT
if [ -f "$git_dir/index" ]; then cp "$git_dir/index" "$tmp_index"; else rm -f "$tmp_index"; fi
GIT_INDEX_FILE="$tmp_index" git add -A >/dev/null 2>&1
GIT_INDEX_FILE="$tmp_index" git write-tree
