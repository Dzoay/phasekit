# Mutants speed-ups (2026-10-12)

Asked after a measurement of the M7 loop (by another session, at the user's request): the earlier speed-ups held (prep
7-8 min to about 3, the CI linux job 10-25 min to 5-7), and what stayed slow was mutation testing, run locally and
again in CI. M7.9f's CI mutants job took 165 min: eight mutants of the `gates assertions` tokenizer looped until the
600 s timeout, one after another (80 min), and each xtask mutant took about 50 s to be caught, against 1-5 s in
phasekit-core, because it waited for the whole workspace's tests. Each change is its own PR, measured before and after,
and merged once its checks are green.

| # | Question | Decision | Notes |
|---|---|---|---|
| CI3 | Make the assertions tokenizer always advance? | **Yes** | Its loops step through ranges and iterators, so a mutant that stalls a token fails a test in seconds instead of running to the timeout. |
| CI4 | Lower CI's mutants timeout floor (`minimum_test_timeout`, 600 s) towards the local pre-check's 240 s? | **Yes, after measuring the workspace suite on the CI runner, with a safe margin** | The weekly shards read the same setting. |
| CI5 | Make xtask's mutants cheaper? | **Yes, by whichever option keeps every mutant caught that is caught now** | The options: test them with xtask's tests only, order nextest's tests so the ones that catch them run first, or make the datagen tests' setup (about 3 s each) cheaper. |
