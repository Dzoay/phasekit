# The docs' layout and upkeep (2026-10-12)

Asked after a review of the docs (five reviewers, 2026-10-12): about 130 findings, nearly all one fact kept by hand in
several places (the current milestone, the divergence entries, gates, rules, measurements), with the copies drifted
apart; and frozen records beside living ones with nothing to tell them apart. Two decisions shared the id PS1.

| # | Question | Decision | Notes |
|---|---|---|---|
| DL1 | Where do the frozen design records go? | **`docs/history/`, the type sketch included** | Never edited; [history/README.md](../history/README.md) says so and maps the old paths. |
| DL2 | Where do the user's decisions live? | **`docs/DECISIONS.md`** (was `docs/design/04-user-decisions.md`) | Rank 2 of AGENTS.md's sources of truth, unchanged. |
| DL3 | The two decisions named PS1 | **The pseudo-pure series becomes PP1-PP3**; PS1 stays the paper-sources decision | Every citation renamed with it. |
| DL4 | How a step's PR keeps the docs | **Each fact has one home; a finished step's PLAN.md text becomes one ledger row; its measurements go to the PR body (the squash commit's body) and to the test that pins them** | The homes: PLAN.md §2.1. One PR for the whole clean-up, a commit per stage, at the user's request. |
| DL5 | Does a ledger row carry its PR and commit? (asked after #125: a step's PR cannot name itself before it is opened, nor its squash commit before it merges) | **No: the row is the step's ID and what landed; `git log --grep '^Plan-Step: <id>$'` finds the commit and its PR** | Writing the PR number in would cost every step an extra push and CI run. |
