---
name: long-run-merge-authorization
description: "User wants long autonomous runs through plan steps; I merge each step PR myself when CI is green, no pause between steps"
metadata:
  node_type: memory
  type: feedback
  modified: 2026-10-05T18:13:50.009Z
---

From M1 on (decided 2026-10-05), work through a milestone's steps in one long run: one PR per plan step, squash-merged
by me as soon as all required CI checks are green, then straight on to the next step (start step k+1 while step k's
CI runs, rebase after the merge). Do not stop to ask for merges between steps.

**Why:** the user said stopping for each PR "doesn't seem to be the best idea"; they want "a long run". They first
proposed one PR rebase-merged with a commit per step, but GitHub does not sign rebased commits and `main` requires
signed commits, so per-step squash PRs (one signed, CI-verified commit per step) were chosen instead.

**How to apply:** still follow PLAN.md §0.1 per step (red first, G1-G8, docs, ROT ticks, red evidence in the PR body,
Plan-Step footer). Stop only for genuine user decisions (AGENTS.md list, PLAN §6 open items such as P4 paywalled
papers) or PLAN §0.4 stop conditions. Outward actions beyond merging step PRs (tags, ruleset or settings changes,
publishing) still need an explicit OK. Related: [[git-identity-and-workflow]], [[coolprop-rs-plan-status]].
