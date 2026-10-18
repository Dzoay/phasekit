# The CI mutants jobs (2026-10-11)

Asked after a review of the M7 loop: the weekly full mutants run (user decision TQ2) had never run. At about 6,700
mutants and 3.8 s each it would take about 7 h, longer than a GitHub job may run (6 h), and its report steps ran only
after the run, so a killed job would report nothing; it also lacked cargo-nextest, which `.cargo/mutants.toml` names, and
the CoolProp checkout. The PR mutants gate runs last in the linux job, after G3 and the other gates.

| # | Question | Decision | Notes |
|---|---|---|---|
| CI1 | Fix the weekly run? | **Yes: 8 shards, each a job with a timeout, reporting also after a timeout** | Before its first scheduled run (Monday 04:17 UTC). |
| CI2 | Run the PR mutants gate as its own job? | **Yes, in parallel with the linux job** | About 4 min off every PR's wall time, the same coverage; its own PR. |
