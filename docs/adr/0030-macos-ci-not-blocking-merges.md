# ADR-0030 — macOS CI no longer blocks merges; it still decides when work is done

- **Status:** Proposed (asked by the owner on 2026-11-06, option 4 of four).
- **Date:** 2026-11-06
- **Amends:** ADR-0013 §3 (the required status checks of `main`) and §4 ("merged through a pull
  request when CI is green on all three systems"). ADR-0006 §3 ("a red platform means the work is
  not done") is kept unchanged and made precise below.

## Context

GitHub's hosted macOS runners are often unavailable. Of the last 30 macOS jobs of this repository
(2026-11-05 and 2026-11-06), 19 got a runner within about 4 seconds and finished in about 2
minutes; 11 never got a runner and were cancelled by GitHub after up to 15 minutes, in bursts in
the afternoon (from about 14:00 UTC) on both days. The workflow sets no timeout and the
repository is public (unlimited free minutes), so the cause is the runner pool, which we can
neither control nor change without a paid machine, which the owner does not want.

Because `check (macos-latest)` is a required status check of `main`, every such burst stops the
work: a finished pull request waits, often for hours, for a runner that has nothing to do with
the code (PR #22 waited for four cancelled runs; PR #25 for two so far, on its head commit).

## Decision

1. **Required to merge** (the ruleset of `main`): `check (ubuntu-latest)`,
   `check (windows-latest)`, `guides (PDF)`, `bench (instruction counts)`, green on the pull
   request's **head commit**. `check (macos-latest)` is **removed** from the required checks; the
   owner changes the ruleset.
2. **macOS still runs** on every push (every pull request and every merge into `main`, since CI
   runs on every push) and in the weekly scheduled run. Nothing is removed from the workflow.
3. **A macOS job that ran and failed blocks the merge** like any other red check: only a macOS job
   that is still waiting for a runner, or was cancelled without ever getting one, may be left
   behind. Merging over a real macOS failure is never allowed.
4. **"Done" still means green on the three platforms** (ADR-0006 §3, CLAUDE.md §12): after the
   merge, the macOS job of `main`'s merge commit is followed (re-run when it got no runner) until it
   has run. If it fails, the fix (on a `fix/…` branch, as usual) is the next priority, before any
   new work; ROADMAP records a step as complete only once macOS has been green on it, on the pull
   request or on `main`.
5. **Reversible:** if GitHub's macOS runners become reliable again, a later ADR can put the check
   back among the required ones.

## Alternatives rejected

- **Keep waiting for macOS** (the status quo): the cost is hours per pull request, for a cause
  outside the code.
- **A paid or self-hosted macOS machine:** refused by the owner.
- **Pinning another macOS image, or re-running automatically** (options 1 and 2 of the analysis):
  they may shorten the wait but do not remove it; they can still be added later on top of this.
- **Dropping macOS from CI:** would contradict ADR-0006; macOS problems would be found only by
  users.

## Consequences

- A macOS-only problem can reach `main` for the time it takes the post-merge job to run; it is
  then fixed first. The risk is limited: Linux and Windows catch most problems, and the
  platform-specific code lives in `platform/` modules behind traits (ADR-0006).
- CLAUDE.md §11 and §12 are updated to match (merge on Linux and Windows; "done" includes macOS).
- The working rule "confirm the head commit's checks are green before merging" now reads: the
  required checks green, and macOS not red.
