# just recipes (2026-11-01)

Asked by the user, who had been using `just` in another project of theirs: its `justfile` groups that project's common
commands (setup, build, run, test, check and tools), and `just` lists them. phasekit's commands were spread over
PLAN.md §2.4 (G1-G9), VERIFICATION.md §11.2 (G8's subcommands), `scripts/` and the helper scripts sessions keep outside
the repository.

| # | Question | Decision | Notes |
|---|---|---|---|
| JR1 | Recipes for phasekit's common commands, as in the user's other project? | **Yes: a `justfile`** ("Can you put similar tooling into phasekit please") | `just` lists the recipes by group: setup, build, run, test, check, gates (`g1` … `g9`, one a gate), mutants, perf and tools; `just gates` runs every gate but G8's mutants and perf, which CI runs as jobs of their own, with `cargo deny`, `cargo shear` and `reuse lint`. The recipes run the commands PLAN.md §2.4 and VERIFICATION.md §11.2 define and define none. Those that run cargo ask `scripts/target-dir.sh` for the target directory once per run (SP4). CI calls the commands directly and needs no `just`; `scripts/check-toolchain.sh` warns when it is missing. |
