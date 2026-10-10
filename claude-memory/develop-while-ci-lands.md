---
name: develop-while-ci-lands
description: Keep developing the next steps locally (stacked) while a background chain lands the queued step PRs on CI; never block waiting on CI
metadata:
  type: feedback
---

While step PRs land on CI one after another, keep developing locally: stack the next milestone's steps on the last
queued branch (M6 on M5.11, M7 on M6, ...) and queue them behind the running landing chain. Don't sit polling CI.

**Why:** the user (2026-10-08) interrupted a CI wait with "can we just keep going developing locally, queue all of 5 up
and then queue 6 on top and so on?" A step takes ~10-15 min of CI; waiting serially wastes the session.

**How to apply:** record the parent branch's tip before stacking (the chain rebases it); when a parent lands, move the
child with `git rebase --onto origin/main <parent's old tip>`; never edit a worktree the chain will rebase. Related:
[[landing-stacked-step-prs]], [[long-run-merge-authorization]].
