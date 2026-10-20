# M20's last two steps in one PR (2026-10-20)

Asked at M20's start, when the user said to start it.

| # | Question | Decision | Notes |
|---|---|---|---|
| PL2 | How do M20's steps land? | **M20.3 and M20.4 in one PR, a commit each** ("start m20, bundle 2.3 and .4") | As LM7 did for M25.1-M25.3, to save a CI run. The squash merge makes one commit on `main`, and its body carries both steps' `Plan-Step:` footers, so `git log --grep '^Plan-Step:'` still finds each. An exception to one plan step per PR (AGENTS.md) for these two steps alone; M20.1 and M20.2 land in PRs of their own. |
