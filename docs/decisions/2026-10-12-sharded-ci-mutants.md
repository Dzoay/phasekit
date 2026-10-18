# Sharded CI mutants (2026-10-12)

Asked during M8.9, whose diff has 1 717 mutants against 200-500 for the other M8 steps: after CI3-CI5 a core mutant
costs an incremental build and the tests up to their first failure, about 10-17 a minute here and half that on CI's
runner, so M8.9's CI mutants job would take about four hours.

| # | Question | Decision | Notes |
|---|---|---|---|
| CI6 | Shard the PR mutants job? | **Yes, four shards** | `gates mutants --shard k/4` runs one slice of both passes; a `mutants` job, the required check, passes only when every shard did. The same CI minutes in about a quarter of the wall time. |
