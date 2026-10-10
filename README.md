# phasekit handoff, 2026-10-10

This orphan branch moves the work in progress from the old dev box (i7-8700K) to a new machine. It shares no history
with `main` and is never merged. Delete it once the new machine has picked it up: `git push origin --delete handoff`.

Read it with `git fetch origin handoff && git show origin/handoff:README.md`, or check it out beside the clone:
`git worktree add ../phasekit-handoff handoff`.

## 1. Where things stand

- **`main` = `4fc512d`** (#89, M7.1, CI green). M0-M6 are closed; M7 is open. Signed tags `m0`-`m5` are pushed.
- **Tag `m6` is waiting for the user's yes.** It goes on `72e2b3a` (#88, which closed M6), signed and annotated, with a
  message in the style of `m5`'s (`git tag -l --format='%(contents)' m5`).
- **Two branches are pushed, without PRs yet.** Both were rebased onto `4fc512d` on 2026-10-10 with unchanged trees,
  and every commit is signed as `Dzoay <3277116+Dzoay@users.noreply.github.com>`.

  | Branch | Commits on its parent | State | Next |
  |---|---|---|---|
  | `msrv-1.89` (parent `main`) | `d1c1ec6` chore: rust-version 1.89; `451716a` refactor: the code Rust 1.89 allows | user decision MS1; complete; 423 tests passed on 1.89.0 locally | gates, local mutants, PR, land (§3.1) |
  | `m7.2-phase-rule` (parent `msrv-1.89`) | `4c994e3` feat(core): PT at saturation has two roots; a generic VLE below every curve; `e412994` test(verify): the PT bench | code, tests, PT bench, PLAN/VERIFICATION notes done; local mutants run: **20 missed + 1 timeout** | pin the mutants, gates, PR, land (§3.2) |

- No other work is in flight: no open PRs, no uncommitted changes, no fixtures waiting for a pinned-image run. M7.2
  changes no oracle fixtures.

## 2. Set up the new machine

1. **Clone:** `git clone git@github.com:Dzoay/phasekit.git ~/Projects/phasekit`. The scripts assume
   `~/Projects/phasekit` with worktrees `~/Projects/phasekit-<name>`; set `PHASEKIT_ROOT` if the parent directory
   differs.
2. **Identity and signing** (repo-local; CONTRIBUTING.md "Signed commits"):
   ```sh
   git config user.name Dzoay
   git config user.email 3277116+Dzoay@users.noreply.github.com
   git config gpg.format ssh
   git config user.signingkey ~/.ssh/id_ed25519.pub
   git config commit.gpgsign true
   git config tag.gpgsign true
   echo "3277116+Dzoay@users.noreply.github.com $(cat ~/.ssh/id_ed25519.pub)" > .git/allowed_signers
   git config gpg.ssh.allowedSignersFile "$PWD/.git/allowed_signers"
   ```
   If this machine has a new SSH key, add it on GitHub as a **Signing key** too (Settings → SSH and GPG keys). Otherwise
   `main`'s ruleset refuses the squash merge of a PR with unsigned commits. The key has to be loaded in the agent
   (`ssh-add`). If signing fails, load the key; never turn signing off. Never put a personal email address anywhere.
3. **Tools:** rustup stable, plus `rustup toolchain install 1.89.0 --component clippy` for the MSRV branch, then
   `scripts/check-toolchain.sh`. It lists what is missing and how to install it: the four targets, wasmtime,
   cargo-deny, cargo-shear, cargo-mutants and cargo-nextest. Also needed: `uv` (the oracle and `uvx reuse`), `gh`
   (`gh auth login`), `jq`, and cmake with a C++ compiler for the C++ baseline. Optional: `mmdc` for Mermaid.
4. **CoolProp checkout:** `scripts/fetch-coolprop.sh` creates `reference/CoolProp` (v8.0.0, read-only).
5. **Papers:** `reference/papers/` is gitignored and never committed. No test reads it. Copy it across privately only
   if you will transcribe new check values: Fiedler-IJT-2023-THF.pdf, Huber-IJT-2025-R1130E.pdf, IAPWS-R6-95-2018.pdf,
   Lemmon-JCED-2015-PMC13576076.1.xml, NIST.IR.8474.pdf, Thol-IJT-2016-PMC13576062.1.xml.
6. **Worktrees:**
   ```sh
   cd ~/Projects/phasekit
   git worktree add ../phasekit-msrv msrv-1.89
   git worktree add ../phasekit-m7.2 m7.2-phase-rule
   ```
   Each worktree builds into its own `~/.cache/phasekit-target-<name>`. Never share target directories, and keep them
   out of `/tmp`: a tmpfs fills, and a reboot wipes it.
7. **Helper scripts:** copy `scripts/*` and `pr/*` from this branch into one working directory outside `/tmp`, for
   example `~/.cache/phasekit-work`. The scripts write their logs, mutants output and filled PR bodies next to
   themselves. See §4.
8. **Claude Code:**
   - Copy these from the old machine privately. They are not here because the hook contains the email address it
     guards:
     - `~/.claude/CLAUDE.md`: the never-send-the-email rule;
     - `~/.claude/hooks/guard-email.sh`;
     - the hook's `PreToolUse` entry in `~/.claude/settings.json`: matcher `*`, command
       `~/.claude/hooks/guard-email.sh`, timeout 10.
   - Then copy `claude-memory/*.md` into `~/.claude/projects/<the clone's absolute path with every / replaced by
     ->/memory/`, for example `~/.claude/projects/-home-<user>-Projects-phasekit/memory/`. These files are sanitised
     copies of the old machine's memory. Copying the old directory itself works too.
9. **C++ baseline for this CPU, before recording any bench.** The new machine is slower, so its numbers are not
   comparable with the i7-8700K's. Both kinds of file are keyed by CPU model, so the two machines never overwrite each
   other:
   - `cargo xtask baseline` builds CoolProp's C++ library in a scratch directory and writes
     `crates/phasekit-verify/benches/baseline/coolprop-8.0.0-<cpu>.csv`, `.memory.csv` and `.scaling.csv`;
   - `cargo xtask bench --record` writes `benches/results/<milestone>-<cpu>.csv`.

   Land the new baseline in its own small PR (e.g. `test(verify): the C++ baseline on <cpu>`) before the first step that
   records a bench there. Benches still due in M7: PH (M7.3).

   **Open user decision (P3):** PLAN §6 / 04-user-decisions.md names the old dev box as the performance reference
   machine. ARCHITECTURE.md §7's absolute targets (≤ 0.3 µs α^r, ≤ 0.5 µs properties, ≤ 3 µs PT, …) are checked there
   at each milestone close from M9. Ask the user whether the new machine becomes the reference machine (with the targets
   rescaled, or judged as ratios to its own C++ baseline) or the old box stays the reference. Record the answer in
   04-user-decisions.md. Nothing before M9 depends on it.

## 3. Next actions, in order

Follow AGENTS.md and PLAN.md §0 as always. The user has authorised long runs: merge each step PR yourself when its 9
required checks are green. Tags, settings and publishing still need an explicit yes.

### 3.1 The MSRV PR

```sh
W=~/.cache/phasekit-work
$W/prep.sh msrv                          # counts + gates G1-G8, deny, shear, reuse; commits `chore: test counts` if they changed
$W/mut.sh ~/Projects/phasekit-msrv origin/main msrv   # local mutants pre-check of the branch's Rust diff (refactor only)
$W/open.sh msrv msrv-1.89 "chore: rust-version 1.89 and the code it allows" "<N mutants, all caught>"
$W/land.sh <pr>                          # in the background; waits for the 9 checks, squash-merges
```

`pr/pr-msrv.md` is the body. It has no `Plan-Step` footer, because MSRV is not a plan step. On this branch,
`check-toolchain.sh` asserts rustc ≥ 1.89.

### 3.2 M7.2, after the MSRV PR lands

1. `cd ~/Projects/phasekit-m7.2 && git fetch origin && git rebase --onto origin/main 451716a`. `451716a` is
   `msrv-1.89`'s tip as pushed; if prep committed test counts on top of it, use the tip the MSRV PR landed from.
2. **Pin the mutants.** `pr/mutants-m7.2.log` has the full list. Pin each with an exact hand test, or exclude it in
   `.cargo/mutants.toml` with its reason if it is truly equivalent (M7.1 excluded three that way). These are first
   reads, not conclusions:
   - `flash.rs:193` `generic_vle`'s guard `t < t_c` (→ `true`, `<=`): no test calls it at or above the labelling Tc.
   - `flash.rs:334` `RootPolicy::Nearest`'s comparison (`<`→`<=`, `-`→`/`, `-`→`+`): test a density nearer each root
     and the exact midpoint.
   - `flash.rs:438` the liquid bracket's lower end `bubble.rho * (1 - SAT_RELAX)` (`*`→`+`).
   - `vle.rs:199-233` `from_eos`: spinodal stepping, the spinodal pressures' 1e-9 margins, the Maxwell gap. Its result
     only seeds the VLE's Newton, so several may be equivalent unless a test checks the seed itself or a case where the
     margin decides convergence.
   - TIMEOUT `vle.rs:209:57` (`a * factor` → `/`) loops forever: caught.
3. Re-run `mut.sh ~/Projects/phasekit-m7.2 <msrv tip on main> m7.2`, then `prep.sh m7.2`.
4. `open.sh m7.2 m7.2-phase-rule "feat(core): PT at saturation has two roots; a generic VLE below curves" "<note>"`
   (title ≤ 72 characters; check it), then `land.sh <pr>`. `pr/pr-m7.2.md` already has its red evidence and bench
   numbers (recorded on the i7-8700K against that machine's C++ baseline, so they stand).

### 3.3 Then

M7.3 (PH, PS, PU) through M7.10, as PLAN.md §3 lists them. Write each step's failing test first. Develop the next step
stacked on the previous branch while CI lands it (§5).

## 4. The helper scripts

| Script | Use |
|---|---|
| `gates.sh <worktree>` | every local gate with logs; called by `prep.sh` (`TARGET` and `LOG` set) |
| `prep.sh <name>` | `cargo xtask gates counts --update`, then the gates; commits `chore: test counts` only when all pass |
| `mut.sh <worktree> <base> <name>` | cargo-mutants on the Rust diff since `<base>` with main's settings; never two at once |
| `open.sh <name> <branch> <title> <mutants note>` | fills `pr-<name>.md`'s GATES from the logs (`fill.py`), pushes, opens the PR |
| `land.sh <pr>` | waits for the 9 required checks, re-runs GitHub-cancelled jobs (≤ 5), squash-merges, deletes the branch |

## 5. Rules and lessons carried over

- **Stacked PRs and squash merges:** a stacked branch goes DIRTY after its parent is squash-merged. Rebase it with
  `git rebase --onto origin/main <parent's old tip>`, then push with `--force-with-lease`. Record the parent's tip
  before the parent lands. Land strictly in order; each rebase costs a fresh CI run.
- **CI only runs for PRs that target `main`.** A PR based on another branch gets no checks.
- **Oracle fixtures** are generated only in the pinned image:
  `gh workflow run nightly.yml --ref <branch> -f regenerate=true`, then `gh run download <id> -n fixtures`. Such a run
  also runs the full sweeps, so generate the full tier locally first (gitignored; 14 s):
  `LC_ALL=C uv run --no-project --python 3.12 --with CoolProp==8.0.0 python scripts/oracle/gen.py --kind flash --tier full --jobs 8 --out crates/phasekit-verify/fixtures-full/coolprop-8.0.0`.
  Then run the ignored sweep with `--cargo-profile release --run-ignored only`. Editing gen.py changes every fixture
  header.
- **Mutants:** run them with `CARGO_TARGET_DIR` unset, one run at a time, and grep the output for TIMEOUT before
  opening a PR.
- **Clippy before prep.** Tests may not use `f64::ln`/`exp`: use `math::ln`/`exp` (D12).
- **Never stop a script with `pkill -f <pattern>`:** it matches the calling shell. Don't edit a bash script while it
  runs, and don't switch branches in a checkout while gates run there.
- **`git checkout <file>` discards uncommitted edits.** Commit first.
- **PR titles:** ≤ 72 characters, Conventional Commits, scopes core|data|compat|verify|xtask|capi|wasm|py|plan|deps|repo.
  Step PR bodies end with `Plan-Step: Mx.y`.
- **Finished job logs** before the run ends: `gh api repos/Dzoay/phasekit/actions/jobs/<id>/logs`.
- **Paper lookups:** use OpenAlex anonymously. Never use Unpaywall, which needs an email address.
