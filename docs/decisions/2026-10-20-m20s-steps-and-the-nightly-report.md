# M20's steps and the nightly report (2026-10-20)

PL2 was asked at M20's start, when the user said to start it; PL3 when M20.4's work began.

| # | Question | Decision | Notes |
|---|---|---|---|
| PL2 | How do M20's steps land? | **M20.3 and M20.4 in one PR, a commit each** ("start m20, bundle 2.3 and .4") | As LM7 did for M25.1-M25.3, to save a CI run. The squash merge makes one commit on `main`, and its body carries both steps' `Plan-Step:` footers, so `git log --grep '^Plan-Step:'` still finds each. An exception to one plan step per PR (AGENTS.md) for these two steps alone; M20.1 and M20.2 land in PRs of their own. |
| PL3 | M20.4, the nightly report, is 3-4× its planned size: how does it land? | **Split it** (offered with the reason: the per-column headroom, the cells asserted and the oracle-gap rows need the five oracle checks, eos, term, saturation, flash and props, instrumented, each recording its results its own way) | M20.4, in the PR of PL2, is the report from what the fixture sets, the arbiter records and the ignored tests say; M20.4a, a PR of its own, instruments the checks and adds what they assert. The cells exempt per DIV id, offered in M20.4, went to M20.4a once the work began: the flash and props checks decide a row's exemption from phasekit's state, which a count from the fixture alone does not have. |
