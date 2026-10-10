---
name: m6-resume-state
description: "Exact resume point (2026-10-10 handoff to a new machine): main 08a7226 (#90 RM1, after #89 M7.1); tag m6 pushed; msrv-1.89 and m7.2-phase-rule pushed without PRs; full notes on the orphan branch `handoff`"
metadata:
  node_type: memory
  type: project
  modified: 2026-10-10T21:40:02.730Z
---

Updated 2026-10-10 ~22:45. The user is moving work on phasekit to another computer. Everything was pushed, and the full
handoff note (state, new-machine setup, next actions, helper scripts, PR drafts, these memories sanitised) lives on the
orphan branch **`handoff`** on GitHub: `git show origin/handoff:README.md`. Delete that branch once it is picked up.

**State:** main = 08a7226 (#90 records RM1; #89 M7.1 before it; CI green). Signed tag `m6` on 72e2b3a pushed
2026-10-10 (user approved).
- `msrv-1.89` (pushed, no PR): ca27ac4 chore rust-version 1.89 + b124f0d refactor (user decision MS1). Next: prep,
  mutants (base origin/main), PR "chore: rust-version 1.89 and the code it allows" (body handoff:pr/pr-msrv.md), land.
- `m7.2-phase-rule` (pushed, no PR), on msrv-1.89: a9ce1e4 feat + 36ebf67 PT bench. Local mutants: 20 missed + 1
  timeout (handoff:pr/mutants-m7.2.log) to pin or exclude with reasons. After MSRV lands: `git rebase --onto origin/main
  b124f0d`, pin mutants, prep, PR (body handoff:pr/pr-m7.2.md, RED and BENCH filled), land.
- Then M7.3 (PH/PS/PU; records the PH bench) onward.

**New machine is slower (user, 2026-10-10):** run `cargo xtask baseline` there (C++ CoolProp baseline keyed by CPU)
and land it in its own PR before recording benches; results files are keyed by CPU too. User decision RM1 (2026-10-10, #90):
the old i7-8700K stays the perf reference machine "for now"; §7 targets checked there from M9.

**Why:** a restart or a machine move loses the scratchpad and background jobs. **How to apply:** on resume, read the
`handoff` branch README first; on the new machine its §2 lists setup. Related: [[develop-while-ci-lands]],
[[landing-stacked-step-prs]], [[long-run-merge-authorization]].

The user's three private ~/.claude files (CLAUDE.md, settings.json, hooks/guard-email.sh) are on their Ventoy USB stick
under claude-handoff/claude/ (renamed from .claude so Dolphin shows it).
