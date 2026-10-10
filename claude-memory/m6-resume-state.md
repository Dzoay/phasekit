---
name: m6-resume-state
description: "Exact resume point (2026-10-10 handoff to a new machine): main 4fc512d (M7.1 #89); msrv-1.89 and m7.2-phase-rule pushed without PRs; full notes on the orphan branch `handoff`"
metadata:
  node_type: memory
  type: project
  modified: 2026-10-10T21:40:02.730Z
---

Updated 2026-10-10 ~22:45. The user is moving work on phasekit to another computer. Everything was pushed, and the full
handoff note (state, new-machine setup, next actions, helper scripts, PR drafts, these memories sanitised) lives on the
orphan branch **`handoff`** on GitHub: `git show origin/handoff:README.md`. Delete that branch once it is picked up.

**State:** main = 4fc512d (#89 M7.1, CI green). M6 closed; tag `m6` (on 72e2b3a) still awaits the user's yes.
- `msrv-1.89` (pushed, no PR): d1c1ec6 chore rust-version 1.89 + 451716a refactor (user decision MS1). Next: prep,
  mutants (base origin/main), PR "chore: rust-version 1.89 and the code it allows" (body handoff:pr/pr-msrv.md), land.
- `m7.2-phase-rule` (pushed, no PR), on msrv-1.89: 4c994e3 feat + e412994 PT bench. Local mutants: 20 missed + 1
  timeout (handoff:pr/mutants-m7.2.log) to pin or exclude with reasons. After MSRV lands: `git rebase --onto origin/main
  451716a`, pin mutants, prep, PR (body handoff:pr/pr-m7.2.md, RED and BENCH filled), land.
- Then M7.3 (PH/PS/PU; records the PH bench) onward.

**New machine is slower (user, 2026-10-10):** run `cargo xtask baseline` there (C++ CoolProp baseline keyed by CPU)
and land it in its own PR before recording benches; results files are keyed by CPU too. P3 (perf reference machine =
old i7-8700K; absolute §7 targets checked there from M9) needs the user's call: new machine as reference or not.

**Why:** a restart or a machine move loses the scratchpad and background jobs. **How to apply:** on resume, read the
`handoff` branch README first; on the new machine its §2 lists setup. Related: [[develop-while-ci-lands]],
[[landing-stacked-step-prs]], [[long-run-merge-authorization]].
