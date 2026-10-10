---
name: landing-stacked-step-prs
description: How to land several stacked step PRs with squash-only merges, and tooling pitfalls found in M1
metadata:
  type: feedback
---

Stacked step PRs (each branch carrying the unmerged earlier steps) do NOT merge cleanly after a predecessor is
squash-merged: GitHub reports DIRTY, because the branch's combined hunks overlap the squash commit's hunks. Rebase each
branch onto origin/main just before landing (git drops the already-squashed commits by patch id), force-push, then
merge when CI is green. Scratchpad scripts did this: chain2.sh (rebase in a landing worktree, push, land.sh),
land.sh (waits for the 9 required checks, re-runs GitHub-cancelled jobs up to 5 times, squash-merges),
watch-run.sh (same for workflow runs). ci.yml only runs for PRs targeting main, so PRs based on other branches get no CI.

**Why:** found when the first chain stopped with #24 DIRTY (2026-10-05); each rebase costs a fresh CI run, so landing
is sequential.

**How to apply:** open step PRs against main; land strictly in order with rebase-before-merge. Also: run
`cargo mutants` with CARGO_TARGET_DIR unset (sharing the target dir left stale xtask test binaries that pointed at the
deleted mutants copy; fix was `cargo clean -p phasekit-xtask`). Oracle fixtures must be (re)generated in the pinned
image via `gh workflow run nightly.yml --ref <branch> -f regenerate=true`, then `gh run download <id> -n fixtures`.
Local `gates mutants` counts a TIMEOUT as caught, but CI (faster, no timeout) can report the same mutant MISSED (PR #55: 6 survivors in IdealScale::get and a branch boundary); before opening a PR, grep G8.txt for TIMEOUT and pin any timed-out code with a hand test, or exclude a truly equivalent mutant in .cargo/mutants.toml with its reason. Mermaid: `~/.local/bin/mmdc` works (chrome-headless-shell installed in ~/.cache/puppeteer); a node id `data` broke a
flowchart. Related: [[git-identity-and-workflow]], [[long-run-merge-authorization]].

**Mutants and nextest (2026-10-07, user decision TQ3, PRs #64/#65):** every test run uses cargo-nextest (G3/G4/counts/mutants/CI/nightly; doctests via `cargo test --doc`). cargo-mutants times its BASELINE on the mutated packages only but tests each mutant against the whole workspace, so its auto timeout was shorter than one full run and uncaught mutants showed up as TIMEOUT (counted as caught): that was the real cause of "local timeouts hide CI misses". Fixed by `minimum_test_timeout = 600` in .cargo/mutants.toml and nextest `fail-fast = { max-fail = 1, terminate = "immediate" }`. For a branch without those configs, pass `--test-tool nextest --minimum-test-timeout 600 --cargo-test-arg=--max-fail=1:immediate`. Never run two mutants runs at once (oversubscription fakes timeouts). Don't edit a bash script while it runs (bash reads it incrementally). On Windows CI, write tool dirs to GITHUB_PATH with `cygpath -w`. nextest's store goes under the workspace target/ unless redirected (`--tool-config-file`). PR titles: ≤ 72 chars, scopes core|data|compat|verify|xtask|capi|wasm|py|plan|deps|repo.

**M7 lessons (2026-10-10):** a nightly `regenerate=true` run also runs the full-set sweeps after uploading the
fixtures artifact (download it mid-run with `gh api .../artifacts/<id>/zip`); a new sweep can fail there, so
generate the full tier locally first (`LC_ALL=C uv run ... gen.py --kind flash --tier full --jobs 8 --out
crates/phasekit-verify/fixtures-full/coolprop-8.0.0`, gitignored; 14 s) and run the ignored sweep with
`--cargo-profile release --run-ignored only`. Editing gen.py changes every fixture's header (its sha). Run clippy
before prep (tests may not use f64::ln/exp: math::ln/exp, D12). `git checkout <file>` throws away uncommitted edits
in it: commit first. Wasm32 heap sizes differ (pointer width) from x86-64.
