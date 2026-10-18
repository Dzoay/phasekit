# User decisions

The user's decisions, dated, second only to [BRIEF.md](BRIEF.md) §1 ([AGENTS.md](../AGENTS.md)); a later decision
overrides older text anywhere. Each has a unique id: other docs cite the id and do not restate the decision.

The decisions live in [decisions/](decisions/), a file for each time the user answered, named `<date>-<slug>.md` for
that day: what was asked, the facts known then, and a table `| # | Question | Decision | Notes |` with one row per
decision, its id in the first cell. A session that records a decision adds a file, so two sessions recording
decisions at once never edit the same one (user decision LM4); a later change to a decision is a new file with its own
date, which says what it replaces. Nothing lists the files: their names sort by date. `cargo xtask gates decisions`
checks that each file is named for the date its heading ends with, that no id is in two rows, that this file holds no
decision, and that each id cited as "user decision …" anywhere outside the frozen records is one.
