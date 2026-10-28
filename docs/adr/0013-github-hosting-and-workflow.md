# ADR-0013 — Code hosting on GitHub, protected `main`, pull-request workflow

- **Status:** Accepted
- **Date:** 2026-10-28

## Context

The project needs remote backup, CI on three operating systems and a reviewable history. A lesson
from a previous project: `main` must exist and be the default branch from the first push, or
branch protection and CI configuration get tangled.

## Decision

1. **Hosting:** GitHub, public repository `HumusAudio/orchestra`. GitHub Actions on **standard
   hosted runners** is free for public repositories; larger runners are billed even for public
   repositories, so we use only `ubuntu-latest`, `windows-latest`, `macos-latest`.
2. **`main` exists from the first push** (initial commit: license + minimal README) and is the
   default branch.
3. **`main` is protected** by a repository ruleset (created 2026-10-28, targeting the default
   branch, enforcement *active*, no bypass actors): pull request required, force pushes blocked,
   deletion blocked, required status checks (CI jobs added once the workflow exists).
4. **Workflow:** all work happens on branches (`feature/…`, `fix/…`), pushed after every
   significant commit (backup), merged through a pull request when CI is green on all three
   systems. Never force-push; never `git reset --hard` / `git clean -fd` without explicit owner
   confirmation. Snapshot tag before large refactorings (`pre-refactor-YYYY-MM-DD`).
5. **Commits** are atomic, descriptive, and carry the `User-visible: yes|no` trailer (ADR-0009).
6. **Transport:** SSH for git (`git@github.com:HumusAudio/orchestra.git`). GitHub's host key
   fingerprints were verified against <https://api.github.com/meta> on 2026-10-28
   (ED25519 `SHA256:+DiY3wvvV6TuJJhbpZisF/zLDA0zPMSvHdkr4UvCOqU`).
7. **Claude Code sandbox:** `git push/fetch/pull/ls-remote` and `gh` are listed in
   `sandbox.excludedCommands` (project `.claude/settings.json`), because SSH (port 22, not via the
   HTTP proxy) and the `gh` token in the system keyring (D-Bus) are unreachable from inside the
   sandbox. Trade-off accepted by the owner: those commands run unsandboxed (still subject to
   Claude Code's permission checks); everything else stays sandboxed. Note: compound commands
   (`cd … && git push`) stay sandboxed, so these commands must be run on their own.

## Alternatives rejected

- **Codeberg/Forgejo:** aligned with free-software values, but no free macOS/Windows CI runners.
- **Private repository:** CI minutes would be limited and billed; inconsistent with the GPL.

## Consequences

- Branch protection makes "never commit to `main`" a real constraint, not a convention.
- Contributors need a GitHub account.
